use super::*;
use fiex_plugin_sdk::{self as sdk, InfoFieldKey, RpcError};
use serde_json::json;

pub(super) struct Archive;
impl sdk::InfoProvider for Archive {
    fn compute(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
        fields: &[InfoFieldKey],
    ) -> Result<Vec<sdk::InfoFieldResult>, RpcError> {
        let requested = fields
            .iter()
            .map(|field| {
                if field.subsection_id != "archive"
                    || !matches!(
                        field.field_id.as_str(),
                        "format"
                            | "compressedSize"
                            | "uncompressedSize"
                            | "entryCount"
                            | "compressionRatio"
                            | "encrypted"
                            | "message"
                    )
                {
                    return Err(RpcError::invalid_params("unknown archive field"));
                }
                Ok(field.field_id.as_str())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if requested.is_empty() {
            return Ok(Vec::new());
        }
        let target = ctx
            .target()
            .ok_or_else(|| RpcError::invalid_params("missing target"))?;
        let values = archive_info_fields(&target.reference, &json!({}), ctx)?
            .into_iter()
            .map(|(id, _, value)| (id, value))
            .collect::<std::collections::HashMap<_, _>>();
        Ok(fields
            .iter()
            .map(|field| match values.get(&field.field_id) {
                Some(value) => provider_results::result("archive", &field.field_id, value),
                None => sdk::InfoFieldResult {
                    subsection_id: "archive".into(),
                    field_id: field.field_id.clone(),
                    value: None,
                    unavailable: true,
                },
            })
            .collect())
    }
}
impl sdk::PreviewProvider for Archive {
    fn open(&self, _ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::PreviewView, RpcError> {
        Ok(sdk::PreviewView::html("ui/dist/preview.html"))
    }
}
impl sdk::DecorationProvider for Archive {
    fn resolve(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<Vec<sdk::DecorationContribution>, RpcError> {
        let mut decorations = Vec::new();
        for target in ctx.targets() {
            let Some(metadata) = target.metadata.as_ref() else {
                continue;
            };
            if metadata.kind == "directory" {
                continue;
            }
            let extension = metadata
                .extension
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase();
            let label = match extension.as_str() {
                "zip" => "ZIP",
                "rar" => "RAR",
                "7z" => "7Z",
                "tar" => "TAR",
                "gz" | "tgz" => "TGZ",
                "bz2" | "tbz2" => "TBZ",
                "xz" | "txz" => "TXZ",
                _ => continue,
            };
            let mut decoration = sdk::DecorationContribution::new(target.reference.clone());
            decoration.fields.insert(
                "name".into(),
                sdk::DecorationField {
                    badges: vec![sdk::DecorationBadge {
                        text: label.into(),
                        color: "neutral".into(),
                        tooltip: Some(crate::i18n::t("压缩包").to_string()),
                    }],
                    ..Default::default()
                },
            );
            decorations.push(decoration);
        }
        Ok(decorations)
    }
}
impl sdk::ContextMenuProvider for Archive {
    fn resolve(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<Vec<sdk::MenuItem>, RpcError> {
        let parent = ctx
            .input()?
            .context_menu_resolve()
            .ok_or_else(|| RpcError::invalid_params("missing context-menu request"))?
            .parent_id
            .clone();
        let params = archive_menu_input(ctx, parent.as_deref())?;
        serde_json::from_value(archive_menu(&params)["items"].clone())
            .map_err(|error| RpcError::internal(error.to_string()))
    }
    fn execute(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<sdk::MenuExecuteResult, RpcError> {
        let request = ctx
            .input()?
            .context_menu_execute()
            .ok_or_else(|| RpcError::invalid_params("missing context-menu execute request"))?;
        let mut params = json!({"itemId":request.item_id});
        if let Some(target) = ctx.targets().first() {
            params["targets"] = json!([archive_resource(target)]);
        }
        params["selectionCount"] = json!(ctx.selection_count());
        let operation = {
            let item = params["itemId"]
                .as_str()
                .ok_or_else(|| RpcError::invalid_params("missing menu itemId"))?;
            if item == "archive.advanced" {
                return Ok(sdk::MenuExecuteResult::Panel(sdk::PluginPanelAction {
                    kind: sdk::PluginPanelActionKind::Panel,
                    entry: Some("ui/dist/index.html".into()),
                    asset_path: None,
                    panel_id: Some(item.into()),
                    title: Some(crate::i18n::t("高级压缩").to_string()),
                    size: Some(sdk::PluginPanelSize {
                        width: 400,
                        height: 420,
                    }),
                }));
            }
            let format = match item {
                "archive.zip" => "zip",
                "archive.rar" => "rar",
                "archive.7z" => "7z",
                "archive.tar" => "tar",
                "archive.tar.gz" => "tar.gz",
                "archive.tar.bz2" => "tar.bz2",
                "archive.tar.xz" => "tar.xz",
                "archive.extract" => "",
                _ => return Err(RpcError::invalid_params("unknown menu action")),
            };
            if !format.is_empty() {
                params["format"] = json!(format);
                "archive.compress"
            } else {
                params["path"] = json!(
                    ctx.target()
                        .ok_or_else(|| RpcError::invalid_params("missing target"))?
                        .reference
                );
                "archive.extract"
            }
        };
        let value = run_operation(ctx, params, operation)?;
        serde_json::from_value(value).map_err(|error| RpcError::internal(error.to_string()))
    }
}
impl sdk::HookProvider for Archive {
    fn invoke(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<sdk::contracts::HookResult, RpcError> {
        let request = ctx
            .input()?
            .hook_request()
            .ok_or_else(|| RpcError::invalid_params("missing hook request"))?;
        if request.event != "beforeOpen" {
            return Ok(sdk::contracts::HookResult { handled: false });
        }
        let target = ctx
            .target()
            .ok_or_else(|| RpcError::invalid_params("missing hook target"))?;
        before_open(&target.reference, &json!({}), ctx)
    }
}
