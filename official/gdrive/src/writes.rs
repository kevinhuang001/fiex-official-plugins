use super::*;
use std::collections::HashMap;
use std::io::Read;

#[derive(Default)]
pub(super) struct State {
    pub reads: HashMap<String, (Value, String)>,
    next: u64,
}
impl State {
    pub fn next_id(&mut self, kind: &str) -> String {
        self.next += 1;
        format!("drive-{kind}-{}", self.next)
    }
}
struct Upload {
    token: String,
    node: String,
    url: String,
    revision: String,
    etag: String,
    size: u64,
    sent: u64,
    pending: Vec<u8>,
}
fn check_version(ctx: &sdk::RequestContext, upload: &Upload) -> Result<(), RpcError> {
    let (file, etag) = metadata(ctx, &upload.token, &upload.node)?;
    if file["version"].as_str() != Some(upload.revision.as_str()) || etag != upload.etag {
        return Err(error("同步冲突：云端版本已改变"));
    }
    Ok(())
}
fn send(ctx: &sdk::RequestContext, upload: &mut Upload, final_chunk: bool) -> Result<(), RpcError> {
    let len = upload.pending.len() as u64;
    let range = if len == 0 {
        format!("bytes */{}", upload.size)
    } else {
        format!(
            "bytes {}-{}/{}",
            upload.sent,
            upload.sent + len - 1,
            upload.size
        )
    };
    let mut req = request(
        ctx,
        &upload.token,
        if upload.size == 0 { "PATCH" } else { "PUT" },
        &upload.url,
    )
    .header("Content-Type", "application/octet-stream")
    .header("If-Match", &upload.etag)
    .body(&upload.pending);
    if upload.size != 0 {
        req = req.header("Content-Range", range);
    }
    let response = req.send().map_err(error)?;
    if final_chunk {
        if !(200..300).contains(&response.status) {
            return Err(error(format!("Drive commit HTTP {}", response.status)));
        }
    } else if response.status != 308 {
        return Err(error(format!("Drive upload HTTP {}", response.status)));
    }
    upload.sent += len;
    upload.pending.clear();
    Ok(())
}
/// Core supplies one exact-file snapshot grant. Resumable upload mechanics
/// stay inside this provider; the manifest has no sync/write-mode switches.
pub(super) fn write_file(ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
    let size = p["size"].as_u64().ok_or_else(|| error("missing size"))?;
    let mut reader = ctx
        .files()
        .open_read(
            &serde_json::from_value::<fiex_plugin_sdk::ResourceRef>(p["source"].clone())
                .map_err(error)?,
        )
        .map_err(error)?;
    if reader.len() != size {
        return Err(error("upload source size changed"));
    }
    let token = token(ctx, &p["connection"])?;
    let id = node(text(p, "nodeId")?)?;
    let revision = text(p, "ifRevision")?;
    let (file, etag) = metadata(ctx, &token, id)?;
    if file["version"].as_str() != Some(revision) {
        return Err(error("同步冲突：云端版本已改变"));
    }
    if etag.is_empty() {
        return Err(error(
            "Drive did not return an ETag; refusing unsafe overwrite",
        ));
    }
    if file["capabilities"]["canEdit"] != true {
        return Err(error("Drive file is read-only"));
    }
    let url = if size == 0 {
        format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=media&supportsAllDrives=true")
    } else {
        let response = request(ctx,&token,"PATCH",&format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=resumable&supportsAllDrives=true"))
            .header("If-Match",&etag).header("X-Upload-Content-Length",size.to_string())
            .header("X-Upload-Content-Type","application/octet-stream").header("Content-Type","application/json").body("{}")
            .send().and_then(|r|r.error_for_status()).map_err(error)?;
        response
            .headers
            .get("location")
            .ok_or_else(|| error("missing session URL"))?
            .clone()
    };
    if !url.starts_with("https://www.googleapis.com/upload/drive/") {
        return Err(error("invalid upload session host"));
    }
    let mut upload = Upload {
        token,
        node: id.into(),
        url,
        revision: revision.into(),
        etag,
        size,
        sent: 0,
        pending: Vec::new(),
    };
    loop {
        let remaining = size - upload.sent;
        upload.pending.resize(remaining.min(262144) as usize, 0);
        reader.read_exact(&mut upload.pending).map_err(error)?;
        let final_chunk = remaining <= 262144;
        if final_chunk {
            check_version(ctx, &upload)?;
        }
        send(ctx, &mut upload, final_chunk)?;
        if final_chunk {
            break;
        }
    }
    let (file, _) = metadata(ctx, &upload.token, &upload.node)?;
    Ok(json!({"revision":file["version"]}))
}
