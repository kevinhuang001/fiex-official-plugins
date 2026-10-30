//! Google reports each capability independently; canEdit is not a content
//! write or delete grant. Unknown fields fail closed.
//! https://developers.google.com/workspace/drive/api/reference/rest/v3/files
use super::*;
use sdk::filesystem::FsOperation;

pub(super) fn discover(ctx: &sdk::RequestContext) -> Result<Vec<FsOperation>, RpcError> {
    let connection = ctx
        .params()
        .get("connection")
        .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
    let token = token(&ctx.http(), connection)?;
    let id = ctx.params()["nodeId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .or_else(|| {
            ctx.params()["parentId"]
                .as_str()
                .filter(|id| !id.is_empty())
        })
        .unwrap_or("root");
    let (file, _) = metadata(&ctx.http(), &token, id)?;
    Ok(operations(&file, id == "root"))
}
pub(super) fn operations(file: &Value, root: bool) -> Vec<FsOperation> {
    let mut operations = vec![FsOperation::Stat];
    let caps = &file["capabilities"];
    let directory = file["mimeType"] == "application/vnd.google-apps.folder";
    if directory {
        if caps["canListChildren"] == true {
            operations.push(FsOperation::List);
        }
        if caps["canAddChildren"] == true {
            operations.extend([FsOperation::Write, FsOperation::Mkdir]);
        }
    } else {
        let mime = file["mimeType"].as_str().unwrap_or("");
        let native = mime.starts_with("application/vnd.google-apps.");
        if caps["canDownload"] == true && (!native || super::reads::export_format(mime).is_some()) {
            operations.push(FsOperation::Read);
        }
        // Native Google editors are exported as PDF; their content cannot be
        // written through the Drive file stream.
        if !native && caps["canModifyContent"] == true {
            operations.push(FsOperation::Write);
        }
    }
    if !root {
        if caps["canRename"] == true {
            operations.push(FsOperation::Rename);
        }
        if caps["canDelete"] == true {
            operations.push(FsOperation::Remove);
        }
    }
    operations
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editability_does_not_imply_download_write_rename_or_delete() {
        assert_eq!(
            operations(&json!({"capabilities":{"canEdit":true}}), false),
            vec![FsOperation::Stat]
        );
        let readable = json!({"capabilities":{"canDownload":true,"canRename":true}});
        assert_eq!(
            operations(&readable, false),
            vec![FsOperation::Stat, FsOperation::Read, FsOperation::Rename]
        );
    }
    #[test]
    fn native_documents_export_as_readable_files_without_write() {
        let folder = json!({"mimeType":"application/vnd.google-apps.folder","capabilities":{"canListChildren":true,"canAddChildren":true,"canDelete":true}});
        let ops = operations(&folder, true);
        assert!(ops.contains(&FsOperation::List) && ops.contains(&FsOperation::Mkdir));
        assert!(!ops.contains(&FsOperation::Read) && !ops.contains(&FsOperation::Remove));
        let document = json!({"mimeType":"application/vnd.google-apps.document","capabilities":{"canDownload":true,"canModifyContent":true}});
        assert_eq!(
            operations(&document, false),
            vec![FsOperation::Stat, FsOperation::Read]
        );
        let shortcut = json!({"mimeType":"application/vnd.google-apps.shortcut","capabilities":{"canDownload":true}});
        assert_eq!(operations(&shortcut, false), vec![FsOperation::Stat]);
        let blocked = json!({"mimeType":"application/vnd.google-apps.document","capabilities":{"canDownload":false}});
        assert_eq!(operations(&blocked, false), vec![FsOperation::Stat]);
    }
}
