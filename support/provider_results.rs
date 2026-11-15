use fiex_plugin_sdk::InfoFieldResult;

pub fn result(subsection_id: &str, field_id: &str, value: impl Into<String>) -> InfoFieldResult {
    InfoFieldResult {
        subsection_id: subsection_id.into(),
        field_id: field_id.into(),
        value: Some(value.into()),
        unavailable: false,
    }
}
