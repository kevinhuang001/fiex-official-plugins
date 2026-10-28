//! Typed standard-event results shared by official plugins.
use fiex_plugin_sdk::{InfoSection, RpcError};
use serde_json::Value;
pub fn section(id: &str, title: &str, rows: Value) -> Result<Vec<InfoSection>, RpcError> {
    Ok(vec![InfoSection {
        id: id.into(),
        title: title.into(),
        source: String::new(),
        provider: String::new(),
        plugin_id: None,
        rows: serde_json::from_value(rows)
            .map_err(|error| RpcError::internal(error.to_string()))?,
    }])
}
