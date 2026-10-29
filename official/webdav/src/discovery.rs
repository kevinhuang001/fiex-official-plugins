//! RFC 3744 current-user-privilege-set. OPTIONS/Allow only describes methods,
//! so it must never be treated as the current account's write authorization.
use crate::{client, fs};
use fiex_plugin_sdk::{self as sdk, filesystem::FsOperation, RpcError};
use quick_xml::events::Event;

pub fn discover(ctx: &sdk::RequestContext) -> Result<Vec<FsOperation>, RpcError> {
    let cfg: client::StoredConnection = serde_json::from_value(ctx.params()["connection"].clone())
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
    let node = ctx.params()["nodeId"]
        .as_str()
        .or_else(|| ctx.params()["parentId"].as_str())
        .unwrap_or("");
    fs::safe_relative(node)?;
    let mut ops = resource_privileges(ctx, &cfg, node)?;
    ops.retain(|op| !matches!(op, FsOperation::Rename | FsOperation::Remove));
    if !node.is_empty() {
        let parent = node
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        let parent_ops = resource_privileges(ctx, &cfg, parent)?;
        // RFC 3744: bind/unbind are checked on the containing collection.
        if parent_ops.contains(&FsOperation::Rename) {
            ops.extend([FsOperation::Rename, FsOperation::Remove]);
        }
    }
    Ok(ops)
}

fn resource_privileges(
    ctx: &sdk::RequestContext,
    cfg: &client::StoredConnection,
    node: &str,
) -> Result<Vec<FsOperation>, RpcError> {
    let url = client::webdav_url(cfg, node);
    let response = client::request(&ctx.http(), cfg, "PROPFIND", &url)
        .header("Depth", "0")
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(r#"<d:propfind xmlns:d="DAV:"><d:prop><d:current-user-privilege-set/></d:prop></d:propfind>"#)
        .send().map_err(|e| RpcError::internal(e.to_string()))?;
    if matches!(response.status, 401 | 403) {
        return Ok(Vec::new());
    }
    // Servers without ACL support remain usable for reading. No speculative
    // write probes or optimistic write grants on an absent property.
    if matches!(response.status, 400 | 404 | 405 | 501) {
        return Ok(vec![
            FsOperation::List,
            FsOperation::Stat,
            FsOperation::Read,
        ]);
    }
    let xml = response
        .error_for_status()
        .and_then(|r| r.text())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    privileges(&xml)
}

fn privileges(xml: &str) -> Result<Vec<FsOperation>, RpcError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut in_privileges = false;
    let mut in_privilege = false;
    let mut status = String::new();
    let mut in_status = false;
    let mut block = Vec::<String>::new();
    let mut granted = Vec::<String>::new();
    let mut present = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"propstat" => {
                    block.clear();
                    status.clear();
                }
                b"current-user-privilege-set" => {
                    in_privileges = true;
                    present = true;
                }
                b"privilege" if in_privileges => in_privilege = true,
                b"status" => in_status = true,
                name if in_privilege => block.push(String::from_utf8_lossy(name).into_owned()),
                _ => {}
            },
            Ok(Event::Empty(e)) if in_privilege => {
                block.push(String::from_utf8_lossy(e.local_name().as_ref()).into_owned())
            }
            Ok(Event::Text(e)) if in_status => status.push_str(&e.unescape().unwrap_or_default()),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"privilege" => in_privilege = false,
                b"current-user-privilege-set" => in_privileges = false,
                b"status" => in_status = false,
                b"propstat" if status.split_whitespace().nth(1) == Some("200") => {
                    granted.append(&mut block)
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(RpcError::internal(e.to_string())),
            _ => {}
        }
    }
    if !present {
        return Ok(vec![
            FsOperation::List,
            FsOperation::Stat,
            FsOperation::Read,
        ]);
    }
    let has = |name| granted.iter().any(|p| p == name || p == "all");
    let mut ops = vec![FsOperation::Stat];
    if has("read") {
        ops.extend([FsOperation::List, FsOperation::Read]);
    }
    if has("write") || has("write-content") {
        ops.push(FsOperation::Write);
    }
    if has("write") || has("bind") {
        ops.push(FsOperation::Mkdir);
    }
    // Deletion/renaming require privileges on the parent as well; server
    // enforcement remains authoritative for each mutation.
    if has("write") || (has("bind") && has("unbind")) {
        ops.extend([FsOperation::Rename, FsOperation::Remove]);
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_propstat_never_grants_writes() {
        let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:propstat><d:prop><d:current-user-privilege-set><d:privilege><d:all/></d:privilege></d:current-user-privilege-set></d:prop><d:status>HTTP/1.1 403 Forbidden</d:status></d:propstat></d:response></d:multistatus>"#;
        assert_eq!(privileges(xml).unwrap(), vec![FsOperation::Stat]);
        let success = xml.replace("403 Forbidden", "200 OK");
        assert!(privileges(&success).unwrap().contains(&FsOperation::Write));
    }
}
