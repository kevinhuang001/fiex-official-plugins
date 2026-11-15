use fiex_plugin_sdk::{self as sdk, ProviderInvocationExt, RpcError};
use serde_json::json;
use std::io::Read;

const MAX_INPUT: usize = 256 * 1024 * 1024;
const SUBMENU_ID: &str = "office.menu";
const CONVERT_ID: &str = "office.convertToPdf";

pub(crate) struct OfficeMenu;

fn extension(target: &sdk::ResourceContext) -> Option<String> {
    target
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata.extension.clone().or_else(|| {
                metadata
                    .name
                    .rsplit_once('.')
                    .map(|(_, extension)| extension.to_owned())
            })
        })
        .map(|extension| extension.to_ascii_lowercase())
}

fn supported(extension: &str) -> bool {
    matches!(extension, "docx" | "xlsx" | "pptx")
}

fn pdf_name(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{stem}.pdf")
}

impl sdk::ContextMenuProvider for OfficeMenu {
    fn resolve(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<Vec<sdk::MenuItem>, RpcError> {
        let request = ctx
            .input()?
            .context_menu_resolve()
            .ok_or_else(|| RpcError::invalid_params("missing context-menu request"))?;
        let Some(target) = (ctx.targets().len() == 1)
            .then(|| ctx.targets().first())
            .flatten()
        else {
            return Ok(Vec::new());
        };
        let Some(_extension) = extension(target).filter(|value| supported(value)) else {
            return Ok(Vec::new());
        };
        match request.parent_id.as_deref() {
            None => Ok(vec![sdk::MenuItem::submenu(SUBMENU_ID, "Office")]),
            Some(SUBMENU_ID) => {
                let mut convert = sdk::MenuItem::action(CONVERT_ID, crate::i18n::t("转换为 PDF"));
                convert.icon = Some("file-down".into());
                Ok(vec![convert])
            }
            Some(_) => Err(RpcError::invalid_params("unknown Office submenu")),
        }
    }

    fn execute(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<sdk::MenuExecuteResult, RpcError> {
        let request = ctx
            .input()?
            .context_menu_execute()
            .ok_or_else(|| RpcError::invalid_params("missing context-menu execute request"))?;
        if request.item_id.as_deref() != Some(CONVERT_ID) || ctx.targets().len() != 1 {
            return Err(RpcError::invalid_params("unknown Office menu action"));
        }
        let target = ctx
            .target()
            .ok_or_else(|| RpcError::invalid_params("missing Office file"))?;
        let extension = extension(target)
            .filter(|value| supported(value))
            .ok_or_else(|| RpcError::invalid_params("unsupported Office format"))?;
        let name = target
            .metadata
            .as_ref()
            .map(|metadata| metadata.name.as_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| RpcError::invalid_params("Office file name is unavailable"))?
            .to_owned();
        let output_name = pdf_name(&name);
        let source = target.reference.clone();
        let directory = ctx
            .directory()
            .ok_or_else(|| RpcError::invalid_params("missing output directory"))?
            .reference
            .clone();

        let task =
            ctx.spawn_task_with_label(crate::i18n::t("正在转换为 PDF"), move |task| {
                let files = task.files();
                let mut reader = files
                    .open_read(&source)
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                let mut bytes = Vec::new();
                let mut block = [0_u8; 64 * 1024];
                loop {
                    if task.is_cancelled() {
                        return Err(RpcError::cancelled());
                    }
                    let count = reader
                        .read(&mut block)
                        .map_err(|error| RpcError::internal(error.to_string()))?;
                    if count == 0 {
                        break;
                    }
                    if bytes.len().saturating_add(count) > MAX_INPUT {
                        return Err(RpcError::invalid_params("Office file is too large"));
                    }
                    bytes.extend_from_slice(&block[..count]);
                }
                let pdf = crate::pdf::document_pdf(&bytes, &extension)?;
                if task.is_cancelled() {
                    return Err(RpcError::cancelled());
                }
                let destination = files
                    .child(&directory, &output_name)
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                files
                    .create(&destination, &pdf)
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                Ok(json!({"fileName": output_name}))
            })?;

        Ok(sdk::MenuExecuteResult::Custom(json!({"taskId": task.id()})))
    }
}

#[cfg(test)]
mod tests {
    use super::{pdf_name, supported};

    #[test]
    fn office_menu_accepts_only_supported_documents() {
        for extension in ["docx", "xlsx", "pptx"] {
            assert!(supported(extension));
        }
        for extension in ["doc", "xls", "ppt", "pdf"] {
            assert!(!supported(extension));
        }
    }

    #[test]
    fn pdf_export_keeps_the_source_stem() {
        assert_eq!(pdf_name("Budget.xlsx"), "Budget.pdf");
        assert_eq!(pdf_name("資料.pptx"), "資料.pdf");
    }
}
