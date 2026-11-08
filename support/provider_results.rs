use fiex_plugin_sdk::InfoFieldResult;

pub fn result(section_id: &str, field_id: &str, value: impl Into<String>) -> InfoFieldResult {
    InfoFieldResult {
        section_id: section_id.into(),
        field_id: field_id.into(),
        value: Some(value.into()),
        unavailable: false,
    }
}
