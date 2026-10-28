//! SMB writes stage into a sibling remote file and rename it on commit, so a
//! reader never sees a partially written target.
use super::*;
use fiex_plugin_sdk::filesystem::{FsEntry, FsRequest, FsWrite};
use std::sync::{Arc, Mutex};

struct Upload {
    connection: smb::Connection,
    session: smb::Session,
    tree: smb::Tree,
    original: smb::File,
    file: smb::File,
    target: String,
    before: String,
    offset: u64,
    renamed: bool,
}

impl Upload {
    fn write_at(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut done = 0;
        while done < bytes.len() {
            let sent = self
                .file
                .write_block(&bytes[done..], self.offset + done as u64, None)
                .map_err(|error| io::Error::other(error.to_string()))?;
            if sent == 0 {
                return Err(io::Error::other("SMB zero-length write"));
            }
            done += sent;
        }
        self.offset += bytes.len() as u64;
        Ok(bytes.len())
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        if !self.renamed {
            let _ = self.file.set_info(smb::FileDispositionInformation {
                delete_pending: true.into(),
            });
        }
        let _ = self.file.close();
        let _ = self.original.close();
        let _ = self.tree.disconnect();
        let _ = self.session.logoff();
        let _ = self.connection.close();
    }
}

struct StagedWriter {
    upload: Arc<Mutex<Upload>>,
}

fn locked(upload: &Arc<Mutex<Upload>>) -> io::Result<std::sync::MutexGuard<'_, Upload>> {
    upload
        .lock()
        .map_err(|_| io::Error::other("SMB upload state unavailable"))
}

impl io::Write for StagedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        locked(&self.upload)?.write_at(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        let upload = locked(&self.upload)?;
        upload
            .file
            .flush()
            .map_err(|error| io::Error::other(error.to_string()))
    }
}

pub(crate) fn write(request: &FsRequest<'_>) -> Result<FsWrite, RpcError> {
    let cfg = connection(request)?;
    let node = request
        .node_id()
        .ok_or_else(|| fail("missing nodeId"))?
        .to_owned();
    safe_relative(&node)?;
    safe_relative(&cfg.base)?;
    let (connection, session, tree) = connect(&request.ctx().tcp(), &cfg).map_err(fail)?;
    let target = smb_path(&cfg, &node);
    let original = tree
        .create(
            &target,
            &smb::FileCreateArgs::make_open_existing(
                smb::FileAccessMask::new().with_generic_read(true),
            ),
        )
        .map_err(fail)?
        .unwrap_file();
    let before = revision(&original)?;
    if request.if_revision() != Some(before.as_str()) {
        let _ = original.close();
        let _ = tree.disconnect();
        let _ = session.logoff();
        let _ = connection.close();
        return Err(fail(crate::i18n::t("同步冲突：远端已修改")));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(fail)?
        .as_nanos();
    let staging = format!("{target}.fiex-upload-{stamp:x}");
    let file = tree
        .create(
            &staging,
            &smb::FileCreateArgs::make_create_new(
                smb::FileAttributes::new(),
                smb::CreateOptions::new(),
            ),
        )
        .map_err(fail)?
        .unwrap_file();
    let upload = Arc::new(Mutex::new(Upload {
        connection,
        session,
        tree,
        original,
        file,
        target: target.clone(),
        before,
        offset: 0,
        renamed: false,
    }));
    let writer = StagedWriter {
        upload: Arc::clone(&upload),
    };
    let name = display_name(&node);
    Ok(FsWrite::with_commit(writer, move || {
        let mut upload = upload
            .lock()
            .map_err(|_| fail("SMB upload state unavailable"))?;
        upload.file.flush().map_err(fail)?;
        if revision(&upload.original)? != upload.before {
            return Err(fail(crate::i18n::t("同步冲突：上传期间远端已修改")));
        }
        // SMB rename with replace_if_exists needs exclusive delete access to the
        // target; close the revision-check handle before renaming.
        upload.original.close().map_err(fail)?;
        upload
            .file
            .set_info(smb::FileRenameInformation {
                replace_if_exists: true.into(),
                root_directory: 0,
                file_name: upload.target.as_str().into(),
            })
            .map_err(fail)?;
        upload.renamed = true;
        let after = revision(&upload.file)?;
        let size = upload.offset;
        Ok(FsEntry::file(node, name)
            .with_size(size)
            .with_revision(after))
    }))
}
