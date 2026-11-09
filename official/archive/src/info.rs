use super::*;
use fiex_plugin_sdk::{self as sdk, ProviderInvocationExt, RpcError};
use serde_json::Value;

pub(super) fn archive_info_fields(
    path: &sdk::ResourceRef,
    p: &Value,
    host: &dyn sdk::ProviderInvocation,
) -> Result<Vec<(String, String, String)>, RpcError> {
    let password = p.get("password").and_then(Value::as_str);
    let listing = listing::list(path, password, host.sdk_context()).map_err(RpcError::internal)?;
    let compressed = host
        .files()
        .metadata(path)
        .map(|entry| entry.size.unwrap_or(0))
        .unwrap_or(0);
    let entries = listing
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let count = entries.len();
    let uncompressed = entries
        .iter()
        .filter_map(|entry| entry.get("size").and_then(Value::as_u64))
        .sum::<u64>();
    let target = resources::from_params(p, "path", host)?;
    let format = resources::name(&target)?
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_uppercase())
        .unwrap_or_else(|| "UNKNOWN".into());
    let mut fields = vec![
        ("format".into(), crate::i18n::t("格式").into(), format),
        (
            "compressedSize".into(),
            crate::i18n::t("压缩后大小").into(),
            format_size(compressed),
        ),
        (
            "uncompressedSize".into(),
            crate::i18n::t("解压后大小").into(),
            format_size(uncompressed),
        ),
        (
            "entryCount".into(),
            crate::i18n::t("文件数").into(),
            count.to_string(),
        ),
    ];
    if uncompressed > 0 {
        let saved = 1.0 - compressed as f64 / uncompressed as f64;
        fields.push((
            "compressionRatio".into(),
            crate::i18n::t("压缩率").into(),
            format!("{:.1}%", saved * 100.0),
        ));
    }
    fields.push((
        "encrypted".into(),
        crate::i18n::t("加密").into(),
        if listing
            .get("encrypted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            crate::i18n::t("是").into()
        } else {
            crate::i18n::t("否").into()
        },
    ));
    if listing.get("supported").and_then(Value::as_bool) == Some(false)
        && let Some(message) = listing.get("message").and_then(Value::as_str)
    {
        fields.push((
            "message".into(),
            crate::i18n::t("说明").into(),
            message.into(),
        ));
    }
    Ok(fields)
}
