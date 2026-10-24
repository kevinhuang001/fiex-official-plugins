use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct State {
    pub reads: HashMap<String, (Value, String)>,
    writes: HashMap<String, Upload>,
    next: u64,
}
impl State {
    pub fn next_id(&mut self, kind: &str) -> String {
        self.next += 1;
        format!("drive-{kind}-{}", self.next)
    }
}
struct Upload {
    config: Value,
    token: String,
    node: String,
    url: String,
    revision: String,
    etag: String,
    size: u64,
    received: u64,
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
pub(super) fn dispatch(ctx: &sdk::RequestContext, state: &Mutex<State>) -> Result<Value, RpcError> {
    let p = ctx.params();
    if ctx.method() == "fs.beginWrite" {
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
        let size = p["size"].as_u64().ok_or_else(|| error("missing size"))?;
        let url = if size == 0 {
            format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=media&supportsAllDrives=true")
        } else {
            let response = request(ctx,&token,"PATCH",&format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=resumable&supportsAllDrives=true"))
            .header("If-Match",&etag).header("X-Upload-Content-Length",size.to_string())
            .header("X-Upload-Content-Type","application/octet-stream").header("Content-Type","application/json").body("{}")
            .send().and_then(|r|r.error_for_status()).map_err(error)?;
            let session_url = response
                .headers
                .get("location")
                .ok_or_else(|| error("missing session URL"))?
                .clone();
            session_url
        };
        if !url.starts_with("https://www.googleapis.com/upload/drive/") {
            return Err(error("invalid upload session host"));
        }
        let mut state = state.lock().map_err(error)?;
        if state.writes.len() >= 8 {
            return Err(error("too many uploads"));
        }
        let stream = state.next_id("write");
        state.writes.insert(
            stream.clone(),
            Upload {
                config: p["connection"].clone(),
                token,
                node: id.into(),
                url,
                revision: revision.into(),
                etag,
                size,
                received: 0,
                sent: 0,
                pending: Vec::new(),
            },
        );
        return Ok(json!({"streamId":stream}));
    }
    let id = text(p, "streamId")?;
    let mut state = state.lock().map_err(error)?;
    if ctx.method() == "fs.abortWrite" {
        state.writes.remove(id);
        return Ok(Value::Null);
    }
    let upload = state
        .writes
        .get_mut(id)
        .ok_or_else(|| error("unknown upload"))?;
    upload.token = token(ctx, &upload.config)?;
    if ctx.method() == "fs.writeStream" {
        let encoded = text(p, "base64")?;
        if encoded.len() > 349528 {
            return Err(error("frame too large"));
        }
        let bytes = STANDARD.decode(encoded).map_err(error)?;
        if bytes.is_empty()
            || bytes.len() > 262144
            || p["offset"].as_u64() != Some(upload.received)
            || upload
                .received
                .checked_add(bytes.len() as u64)
                .is_none_or(|n| n > upload.size)
        {
            return Err(error("invalid upload frame"));
        }
        if !upload.pending.is_empty() {
            if upload.pending.len() != 262144 {
                return Err(error("nonfinal chunks must be 256 KiB"));
            }
            send(ctx, upload, false)?;
        }
        upload.received += bytes.len() as u64;
        upload.pending = bytes;
        return Ok(Value::Null);
    }
    if upload.received != upload.size {
        return Err(error("incomplete upload"));
    }
    check_version(ctx, upload)?;
    send(ctx, upload, true)?;
    let (file, _) = metadata(ctx, &upload.token, &upload.node)?;
    let result = json!({"revision":file["version"]});
    state.writes.remove(id);
    Ok(result)
}
