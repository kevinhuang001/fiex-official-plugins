use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{Value, json};

pub(super) fn archive_menu(params: &Value) -> Value {
    // Source semantics and capabilities alone decide applicability. A
    // launch-only object or unreadable file cannot be archived.
    let readable = params["targets"].as_array().is_some_and(|targets| {
        !targets.is_empty()
            && targets.iter().all(|target| {
                let metadata = &target["metadata"];
                let operation = if metadata["kind"] == "directory" {
                    "list"
                } else {
                    "read"
                };
                metadata["kind"] != "symlink" && metadata["operations"][operation] == true
            })
    });
    if !readable {
        return json!({"items":[]});
    }

    let parent = params.get("parentId").and_then(Value::as_str);
    if let Some(parent) = parent {
        if parent != "archive.compress" {
            return json!({"items":[]});
        }
        let formats = [
            ("archive.zip", "ZIP"),
            ("archive.rar", "RAR"),
            ("archive.7z", "7z"),
            ("archive.tar", "TAR"),
            ("archive.tar.gz", "TAR.GZ"),
            ("archive.tar.bz2", "TAR.BZ2"),
            ("archive.tar.xz", "TAR.XZ"),
            ("archive.advanced", crate::i18n::t("高级压缩…")),
        ];
        return json!({"items":formats.into_iter().map(|(id, title)| {
            json!({"id":id,"title":title,"kind":"action"})
        }).collect::<Vec<_>>()});
    }

    let targets = params.get("targets").and_then(Value::as_array);
    let selection = targets.map_or(0, Vec::len);
    if selection == 0 {
        return json!({"items":[]});
    }
    let archive = |name: &str| {
        [
            "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz",
        ]
        .iter()
        .any(|ext| name.to_ascii_lowercase().ends_with(&format!(".{ext}")))
    };
    let only_archive = selection == 1
        && targets
            .and_then(|targets| targets.first())
            .and_then(|target| target.pointer("/metadata/name"))
            .and_then(Value::as_str)
            .is_some_and(archive);
    if only_archive {
        return json!({"items":[
            {"id":"archive.extract","title":crate::i18n::t("解压缩"),"kind":"action"}
        ]});
    }
    json!({"items":[{"id":"archive.compress","title":crate::i18n::t("压缩"),"kind":"submenu"}]})
}

pub(super) fn archive_resource(target: &sdk::ResourceContext) -> Value {
    let Some(metadata) = target.metadata.as_ref() else {
        return json!({"metadata":{}});
    };
    let operations = target
        .capabilities
        .operations
        .iter()
        .map(|operation| (operation.as_str().to_string(), Value::Bool(true)))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "metadata": {
            "name": metadata.name,
            "kind": metadata.kind,
            "operations": operations,
        }
    })
}

pub(super) fn archive_menu_input(
    ctx: &dyn sdk::ProviderInvocation,
    parent_id: Option<&str>,
) -> Result<Value, RpcError> {
    let targets = ctx
        .targets()
        .iter()
        .map(archive_resource)
        .collect::<Vec<_>>();
    Ok(json!({"targets":targets,"parentId":parent_id}))
}
