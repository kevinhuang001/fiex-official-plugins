use fiex_plugin_sdk::{InfoCompute, InfoFieldDescriptor, InfoFieldResult, InfoSectionDescriptor};

pub fn section(
    id: &str,
    title: impl Into<String>,
    fields: &[(&str, &str, InfoCompute)],
) -> InfoSectionDescriptor {
    InfoSectionDescriptor {
        id: id.into(),
        title: title.into(),
        fields: fields
            .iter()
            .map(|(id, label, compute)| InfoFieldDescriptor {
                id: (*id).into(),
                label: (*label).into(),
                compute: *compute,
            })
            .collect(),
    }
}

pub fn result(section_id: &str, field_id: &str, value: impl Into<String>) -> InfoFieldResult {
    InfoFieldResult {
        section_id: section_id.into(),
        field_id: field_id.into(),
        value: Some(value.into()),
        unavailable: false,
    }
}
