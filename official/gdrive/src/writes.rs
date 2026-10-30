use super::*;
mod session;
use sdk::filesystem::{FsEntry, FsRequest, FsWrite};
use std::{
    io,
    sync::{Arc, Mutex},
};

/// One resumable upload chunk. The writer only holds one chunk at a time and
/// PATCHes it as soon as it fills, so a large file is never buffered whole.
const CHUNK: usize = 256 * 1024;

/// Shared live upload state. The writer streams non-final chunks; `commit`
/// sends the held chunk (or the zero-length final range) and reads back the
/// committed metadata.
struct Upload {
    http: sdk::HttpClient,
    token: String,
    node: String,
    url: String,
    revision: String,
    etag: String,
    size: u64,
    sent: u64,
    pending: Vec<u8>,
    failure: Option<String>,
    creating: bool,
    content_type: String,
    completed: Option<Value>,
}

fn check_version(
    http: &sdk::HttpClient,
    token: &str,
    node: &str,
    revision: &str,
    etag: &str,
) -> Result<(), RpcError> {
    let (file, current_etag) = metadata(http, token, node)?;
    if file["version"].as_str() != Some(revision) || current_etag != etag {
        return Err(error(crate::i18n::t("同步冲突：云端版本已改变")));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn send(
    http: &sdk::HttpClient,
    token: &str,
    url: &str,
    etag: &str,
    size: u64,
    sent: u64,
    chunk: &[u8],
    final_chunk: bool,
    content_type: &str,
) -> Result<Option<Value>, RpcError> {
    let len = chunk.len() as u64;
    let range = if size == 0 {
        "bytes */0".to_string()
    } else {
        format!("bytes {}-{}/{}", sent, sent + len - 1, size)
    };
    let mut request = http_request(http, token, "PUT", url)
        .header("Content-Type", content_type)
        .header("Content-Range", range);
    if !etag.is_empty() {
        request = request.header("If-Match", etag);
    }
    let response = request.body(chunk).send().map_err(error)?;
    if final_chunk {
        if !(200..300).contains(&response.status) {
            return Err(error(format!("Drive commit HTTP {}", response.status)));
        }
    } else if response.status != 308 {
        return Err(error(format!("Drive upload HTTP {}", response.status)));
    }
    if final_chunk {
        return response.json::<Value>().map(Some).map_err(error);
    }
    Ok(None)
}

fn io_failure(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

impl Upload {
    /// Stream one incoming buffer through the resumable session, holding back
    /// the chunk that can still turn out to be the final one.
    fn stream(&mut self, mut bytes: &[u8]) -> io::Result<usize> {
        if let Some(message) = &self.failure {
            return Err(io_failure(message.clone()));
        }
        let total = bytes.len();
        while !bytes.is_empty() {
            if self.pending.len() == CHUNK {
                if self.sent + CHUNK as u64 >= self.size {
                    let message = "upload source size changed";
                    self.failure = Some(message.into());
                    return Err(io_failure(message));
                }
                if let Err(error) = self.flush_chunk(false) {
                    self.failure = Some(error.message.clone());
                    return Err(io_failure(error.message));
                }
            }
            let room = CHUNK - self.pending.len();
            let take = room.min(bytes.len());
            self.pending.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if self.pending.len() == CHUNK {
                let end = self.sent + CHUNK as u64;
                if end > self.size {
                    let message = "upload source size changed";
                    self.failure = Some(message.into());
                    return Err(io_failure(message));
                }
                // Exactly `size` means this is the final chunk: keep it for
                // commit so `ifRevision` is checked immediately before publish.
                if end < self.size {
                    if let Err(error) = self.flush_chunk(false) {
                        self.failure = Some(error.message.clone());
                        return Err(io_failure(error.message));
                    }
                }
            } else if self.sent + self.pending.len() as u64 > self.size {
                let message = "upload source size changed";
                self.failure = Some(message.into());
                return Err(io_failure(message));
            }
        }
        Ok(total)
    }

    fn flush_chunk(&mut self, final_chunk: bool) -> Result<(), RpcError> {
        let chunk = std::mem::take(&mut self.pending);
        let completed = send(
            &self.http,
            &self.token,
            &self.url,
            &self.etag,
            self.size,
            self.sent,
            &chunk,
            final_chunk,
            &self.content_type,
        )?;
        if final_chunk {
            self.completed = completed;
        }
        self.sent += chunk.len() as u64;
        Ok(())
    }

    fn commit(mut self) -> Result<FsEntry, RpcError> {
        if let Some(message) = self.failure.take() {
            return Err(error(message));
        }
        if self.sent + self.pending.len() as u64 != self.size {
            return Err(error("upload source size changed"));
        }
        if !self.creating {
            check_version(
                &self.http,
                &self.token,
                &self.node,
                &self.revision,
                &self.etag,
            )?;
        }
        self.flush_chunk(true)?;
        if let Some(file) = self.completed.filter(|file| file["id"].is_string()) {
            return Ok(entry(&file));
        }
        if self.creating {
            return Err(error("Drive create response omitted file identity"));
        }
        let (file, _) = metadata(&self.http, &self.token, &self.node)?;
        Ok(entry(&file))
    }
}

struct UploadWriter {
    upload: Arc<Mutex<Option<Upload>>>,
}

impl io::Write for UploadWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .upload
            .lock()
            .map_err(|_| io_failure("upload state unavailable"))?;
        let upload = guard
            .as_mut()
            .ok_or_else(|| io_failure("upload already committed"))?;
        upload.stream(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Core supplies one exact-file snapshot. Resumable upload mechanics stay
/// inside this provider; the manifest has no sync/write-mode switches.
pub(super) fn write(request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
    let ctx = request.ctx();
    let cfg = request
        .connection()
        .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
    let size = request.size().ok_or_else(|| error("missing size"))?;
    let http = ctx.http();
    let token = token(&http, cfg)?;
    let started = session::start(request, &http, &token, size)?;
    let upload = Arc::new(Mutex::new(Some(Upload {
        http,
        token,
        node: started.id,
        url: started.url,
        revision: started.revision,
        etag: started.etag,
        creating: started.creating,
        content_type: started.content_type,
        completed: None,
        size,
        sent: 0,
        pending: Vec::with_capacity(CHUNK),
        failure: None,
    })));
    let writer = UploadWriter {
        upload: Arc::clone(&upload),
    };
    Ok(FsWrite::with_commit(writer, move || {
        let upload = {
            let mut guard = upload
                .lock()
                .map_err(|_| error("upload state unavailable"))?;
            guard
                .take()
                .ok_or_else(|| error("upload already committed"))?
        };
        upload.commit()
    }))
}
