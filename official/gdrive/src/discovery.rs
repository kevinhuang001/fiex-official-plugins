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
    let native = file["mimeType"]
        .as_str()
        .is_some_and(|mime| mime.starts_with("application/vnd.google-apps."));
    if directory {
        if caps["canListChildren"] == true {
            operations.push(FsOperation::List);
        }
        if caps["canAddChildren"] == true {
            operations.extend([FsOperation::Write, FsOperation::Mkdir]);
        }
    } else if !native {
        if caps["canDownload"] == true {
            operations.push(FsOperation::Read);
        }
        if caps["canModifyContent"] == true {
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
    fn folders_and_native_documents_do_not_claim_file_streams() {
        let folder = json!({"mimeType":"application/vnd.google-apps.folder","capabilities":{"canListChildren":true,"canAddChildren":true,"canDelete":true}});
        let ops = operations(&folder, true);
        assert!(ops.contains(&FsOperation::List) && ops.contains(&FsOperation::Mkdir));
        assert!(!ops.contains(&FsOperation::Read) && !ops.contains(&FsOperation::Remove));
        let document = json!({"mimeType":"application/vnd.google-apps.document","capabilities":{"canDownload":true,"canModifyContent":true}});
        assert_eq!(operations(&document, false), vec![FsOperation::Stat]);
    }
}
