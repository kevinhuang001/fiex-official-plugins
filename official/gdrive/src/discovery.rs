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
        // Native Docs/Sheets/Slides are written back by uploading the
        // exported Office format with a conversion target; Core always writes
        // the same node ID, so the editor is replaced rather than recreated.
        if caps["canModifyContent"] == true
            && (!native || super::reads::import_format(mime).is_some())
        {
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
    fn native_import_formats_cover_editors_but_not_drawings() {
        let docs = super::super::reads::import_format("application/vnd.google-apps.document")
            .expect("docs import");
        assert_eq!(docs.target, "application/vnd.google-apps.document");
        assert_eq!(
            docs.source,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        let sheets = super::super::reads::import_format("application/vnd.google-apps.spreadsheet")
            .expect("sheets import");
        assert_eq!(
            sheets.source,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        let slides = super::super::reads::import_format("application/vnd.google-apps.presentation")
            .expect("slides import");
        assert_eq!(
            slides.source,
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        );
        assert!(
            super::super::reads::import_format("application/vnd.google-apps.drawing").is_none()
        );
        assert!(super::super::reads::import_format("application/vnd.google-apps.script").is_none());
    }

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
    fn native_documents_export_as_readable_files_with_write() {
        let folder = json!({"mimeType":"application/vnd.google-apps.folder","capabilities":{"canListChildren":true,"canAddChildren":true,"canDelete":true}});
        let ops = operations(&folder, true);
        assert!(ops.contains(&FsOperation::List) && ops.contains(&FsOperation::Mkdir));
        assert!(!ops.contains(&FsOperation::Read) && !ops.contains(&FsOperation::Remove));
        let document = json!({"mimeType":"application/vnd.google-apps.document","capabilities":{"canDownload":true,"canModifyContent":true}});
        assert_eq!(
            operations(&document, false),
            vec![FsOperation::Stat, FsOperation::Read, FsOperation::Write]
        );
        let drawing = json!({"mimeType":"application/vnd.google-apps.drawing","capabilities":{"canDownload":true,"canModifyContent":true}});
        assert_eq!(
            operations(&drawing, false),
            vec![FsOperation::Stat, FsOperation::Read]
        );
        let shortcut = json!({"mimeType":"application/vnd.google-apps.shortcut","capabilities":{"canDownload":true}});
        assert_eq!(operations(&shortcut, false), vec![FsOperation::Stat]);
        let blocked = json!({"mimeType":"application/vnd.google-apps.document","capabilities":{"canDownload":false}});
        assert_eq!(operations(&blocked, false), vec![FsOperation::Stat]);
    }
}
