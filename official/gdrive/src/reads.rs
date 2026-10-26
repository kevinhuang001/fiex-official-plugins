use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::io::Read;

pub(super) fn dispatch(
    ctx: &sdk::RequestContext,
    state: &Mutex<writes::State>,
) -> Result<Value, RpcError> {
    let p = ctx.params();
    let method = ctx.method();
    if method == "fs.closeRead" {
        state
            .lock()
            .map_err(error)?
            .reads
            .remove(text(p, "streamId")?);
        return Ok(Value::Null);
    }
    if method == "fs.readStream" {
        let (config, id) = state
            .lock()
            .map_err(error)?
            .reads
            .get(text(p, "streamId")?)
            .cloned()
            .ok_or_else(|| error("unknown read stream"))?;
        let token = token(ctx, &config)?;
        let offset = p["offset"].as_u64().unwrap_or(0);
        let max = p["maxBytes"].as_u64().unwrap_or(262144).clamp(1, 262144);
        let end = offset
            .checked_add(max - 1)
            .ok_or_else(|| error("range overflow"))?;
        let mut response = request(
            ctx,
            &token,
            "GET",
            &format!("{API}/files/{id}?alt=media&supportsAllDrives=true"),
        )
        .header("Range", format!("bytes={offset}-{end}"))
        .send()
        .map_err(error)?;
        if response.status == 416 {
            return Ok(json!({"base64":"","eof":true}));
        }
        if response.status != 206 && !(response.status == 200 && offset == 0) {
            return Err(error(format!("Drive read HTTP {}", response.status)));
        }
        let eof = if response.status == 206 {
            let range = response
                .headers
                .get("content-range")
                .ok_or_else(|| error("missing Content-Range"))?;
            let (bounds, total) = range
                .strip_prefix("bytes ")
                .and_then(|s| s.split_once('/'))
                .ok_or_else(|| error("invalid range"))?;
            let (first, last) = bounds
                .split_once('-')
                .ok_or_else(|| error("invalid range"))?;
            let first: u64 = first.parse().map_err(error)?;
            let last: u64 = last.parse().map_err(error)?;
            let total: u64 = total.parse().map_err(error)?;
            if first != offset || last < first || last - first >= max || last >= total {
                return Err(error("unexpected range"));
            }
            last + 1 == total
        } else {
            true
        };
        let mut bytes = Vec::new();
        (&mut response)
            .take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        if bytes.len() as u64 > max {
            return Err(error("Drive ignored range"));
        }
        return Ok(json!({"base64":STANDARD.encode(bytes),"eof":eof}));
    }
    let cfg = &p["connection"];
    let token = token(ctx, cfg)?;
    if method == "connection.validate" {
        request(
            ctx,
            &token,
            "GET",
            &format!("{API}/about?fields=user(displayName)"),
        )
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(error)?;
        return Ok(json!({"valid":true}));
    }
    if method == "fs.list" {
        let id = p["directoryId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("root");
        node(id)?;
        let query = format!("'{id}' in parents and trashed = false");
        let cursor = p["cursor"].as_str().filter(|s| *s != "0").unwrap_or("");
        let url = format!("{API}/files?q={}&fields=nextPageToken,files(id,name,mimeType,size,version,capabilities(canEdit))&pageSize=500&pageToken={}&supportsAllDrives=true&includeItemsFromAllDrives=true",
            urlencoding::encode(&query),urlencoding::encode(cursor));
        let page: Value = request(ctx, &token, "GET", &url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(error)?;
        let entries: Vec<_> = page["files"].as_array().ok_or_else(||error("missing files"))?.iter().map(|f| {
            let directory = f["mimeType"] == "application/vnd.google-apps.folder";
            // Drive permits duplicate names and slash characters. Include the
            // stable ID to make every logical mount path safe and unambiguous.
            let name = safe_name(f["name"].as_str().unwrap_or("file"), f["id"].as_str().unwrap_or(""));
            json!({"nodeId":f["id"],"name":name,"kind":if directory {"directory"} else {"file"},
                "size":f["size"].as_str().and_then(|s|s.parse::<u64>().ok()).unwrap_or(0),"modified":0,"revision":f["version"]})
        }).collect();
        return Ok(json!({"entries":entries,"nextCursor":page["nextPageToken"]}));
    }
    let id = node(text(p, "nodeId")?)?;
    let (file, _) = metadata(ctx, &token, id)?;
    if method == "fs.stat" {
        return Ok(json!({"revision":file["version"],"size":file["size"]}));
    }
    if file["mimeType"]
        .as_str()
        .is_some_and(|s| s.starts_with("application/vnd.google-apps."))
    {
        return Err(error(
            "Google 原生文档请在浏览器中编辑；离线文件同步支持普通文件",
        ));
    }
    let mut state = state.lock().map_err(error)?;
    if state.reads.len() >= 32 {
        return Err(error("too many streams"));
    }
    let stream = state.next_id("read");
    state.reads.insert(stream.clone(), (cfg.clone(), id.into()));
    Ok(json!({"streamId":stream,"size":file["size"].as_str().and_then(|s|s.parse::<u64>().ok())}))
}
fn safe_name(name: &str, id: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let safe = safe.trim_end_matches([' ', '.']);
    let (stem, extension) = safe.rsplit_once('.').unwrap_or((safe, ""));
    if extension.is_empty() {
        format!("{stem} [{id}]")
    } else {
        format!("{stem} [{id}].{extension}")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_unique_names_preserve_extensions() {
        assert_eq!(safe_name("a/b.txt", "abc"), "a_b [abc].txt");
        assert_ne!(safe_name("same", "a"), safe_name("same", "b"));
    }
}
