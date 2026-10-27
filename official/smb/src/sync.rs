use super::*;
use std::io::Read;
fn fail(e: impl std::fmt::Display) -> RpcError {
    RpcError::internal(e.to_string())
}
fn revision(file: &smb::File) -> Result<String, RpcError> {
    let basic = file
        .query_info::<smb::FileBasicInformation>()
        .map_err(fail)?;
    let size = file
        .query_info::<smb::FileStandardInformation>()
        .map_err(fail)?
        .end_of_file;
    Ok(format!(
        "{:?}:{:?}:{size}",
        basic.last_write_time, basic.change_time
    ))
}
pub(crate) fn dispatch(ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
    let cfg: StoredConnection = serde_json::from_value(p["connection"].clone()).map_err(fail)?;
    let node = p["nodeId"].as_str().ok_or_else(|| fail("missing nodeId"))?;
    safe_relative(node)?;
    safe_relative(&cfg.base)?;
    let (connection, session, tree) = connect(&ctx.tcp(), &cfg).map_err(fail)?;
    let result = (|| {
        let target = smb_path(&cfg, node);
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
        if ctx.method() == "fs.stat" {
            let _ = original.close();
            return Ok(json!({"revision":before}));
        }
        if p["ifRevision"].as_str() != Some(before.as_str()) {
            return Err(fail("同步冲突：远端已修改"));
        }
        let mut reader = ctx
            .files()
            .open_read(
                &serde_json::from_value::<fiex_plugin_sdk::ResourceRef>(p["source"].clone())
                    .map_err(|error| fail(error.to_string()))?,
            )
            .map_err(fail)?;
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
        let written = (|| {
            let mut buffer = vec![0; 262144];
            let mut offset = 0u64;
            loop {
                let n = reader.read(&mut buffer).map_err(fail)?;
                if n == 0 {
                    break;
                }
                let mut done = 0;
                while done < n {
                    let sent = file
                        .write_block(&buffer[done..n], offset, None)
                        .map_err(fail)?;
                    if sent == 0 {
                        return Err(fail("SMB zero-length write"));
                    }
                    done += sent;
                    offset += sent as u64;
                }
            }
            file.flush().map_err(fail)?;
            if revision(&original)? != before {
                return Err(fail("同步冲突：上传期间远端已修改"));
            }
            // SMB rename with replace_if_exists needs exclusive delete access
            // to the target. The revision-check handle otherwise blocks the
            // rename on Samba/Windows, even when it was opened read-only.
            original.close().map_err(fail)?;
            file.set_info(smb::FileRenameInformation {
                replace_if_exists: true.into(),
                root_directory: 0,
                file_name: target.as_str().into(),
            })
            .map_err(fail)?;
            let after = revision(&file)?;
            Ok(json!({"revision":after,"size":offset}))
        })();
        if written.is_err() {
            let _ = file.set_info(smb::FileDispositionInformation {
                delete_pending: true.into(),
            });
        }
        let _ = file.close();
        let _ = original.close();
        written
    })();
    let _ = tree.disconnect();
    let _ = session.logoff();
    let _ = connection.close();
    result
}
