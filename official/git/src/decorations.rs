use fiex_plugin_sdk as sdk;
use serde_json::{json, Value};

fn badge(code: &str) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    if code == "??" {
        return Some(("U", "neutral", "Git：未跟踪", "#6f7c88"));
    }
    let index = code.chars().next().unwrap_or(' ');
    let work = code.chars().nth(1).unwrap_or(' ');
    let letter = if work != ' ' { work } else { index };
    match letter {
        'M' => Some(("M", "orange", "Git：已修改", "#c2710c")),
        'A' => Some(("A", "green", "Git：新增", "#2f7d42")),
        'D' => Some(("D", "red", "Git：删除", "#b4443d")),
        'R' => Some(("R", "blue", "Git：重命名", "#2f6f9f")),
        'T' => Some(("T", "purple", "Git：类型变更", "#7d4fa8")),
        'U' => Some(("U", "red", "Git：冲突", "#b4443d")),
        _ => None,
    }
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/")
}

pub fn list(params: &Value, host: &sdk::RequestContext) -> Result<Value, String> {
    let directory = params
        .get("directory")
        .and_then(Value::as_str)
        .ok_or("missing directory")?;
    let Some(status) = crate::git_host::status(directory, host) else {
        return Ok(json!({ "decorations": [] }));
    };
    let statuses = status["entries"]
        .as_object()
        .ok_or("invalid status response")?
        .iter()
        .filter_map(|(path, code)| code.as_str().map(|code| (normalize(path), code.to_owned())))
        .collect::<std::collections::HashMap<_, _>>();
    let mut decorations = Vec::new();
    if let Some(entries) = params.get("entries").and_then(Value::as_array) {
        for entry in entries {
            let Some(path) = entry.get("path").and_then(Value::as_str) else {
                continue;
            };
            let Some(code) = statuses.get(&normalize(path)) else {
                continue;
            };
            let Some((text, color, tooltip, hex)) = badge(code) else {
                continue;
            };
            decorations.push(json!({
                "path": path,
                "badge": {"text": text, "color": color, "tooltip": tooltip},
                "style": {"color": hex, "fontWeight": "600"}
            }));
        }
    }
    Ok(json!({ "decorations": decorations }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_status_codes_to_badges() {
        assert_eq!(badge("??").unwrap().0, "U");
        assert_eq!(badge(" M").unwrap().0, "M");
        assert_eq!(badge(" M").unwrap().3, "#c2710c");
        assert_eq!(badge("A ").unwrap().0, "A");
        assert_eq!(badge("D ").unwrap().0, "D");
        assert_eq!(badge("R ").unwrap().0, "R");
        assert!(badge("  ").is_none());
    }
}
