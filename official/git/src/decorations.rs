use fiex_plugin_sdk as sdk;

fn badge(code: &str) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    if code == "??" {
        return Some(("U", "neutral", crate::i18n::t("Git：未跟踪"), "#6f7c88"));
    }
    let index = code.chars().next().unwrap_or(' ');
    let work = code.chars().nth(1).unwrap_or(' ');
    let letter = if work != ' ' { work } else { index };
    match letter {
        'M' => Some(("M", "orange", crate::i18n::t("Git：已修改"), "#c2710c")),
        'A' => Some(("A", "green", crate::i18n::t("Git：新增"), "#2f7d42")),
        'D' => Some(("D", "red", crate::i18n::t("Git：删除"), "#b4443d")),
        'R' => Some(("R", "blue", crate::i18n::t("Git：重命名"), "#2f6f9f")),
        'T' => Some(("T", "purple", crate::i18n::t("Git：类型变更"), "#7d4fa8")),
        'U' => Some(("U", "red", crate::i18n::t("Git：冲突"), "#b4443d")),
        _ => None,
    }
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/")
}

fn list(host: &dyn sdk::ProviderInvocation) -> Result<Vec<sdk::DecorationContribution>, String> {
    let directory = host
        .directory()
        .ok_or("missing directory ref")?
        .reference
        .clone();
    let Some(status) = crate::git_host::status(&directory, host) else {
        return Ok(Vec::new());
    };
    let statuses = status["entries"]
        .as_object()
        .ok_or("invalid status response")?
        .iter()
        .filter_map(|(path, code)| code.as_str().map(|code| (normalize(path), code.to_owned())))
        .collect::<std::collections::HashMap<_, _>>();
    let mut decorations = Vec::new();
    for target in host.targets() {
        let Some(metadata) = target.metadata.as_ref() else {
            continue;
        };
        let Some(code) = statuses.get(&normalize(&metadata.name)) else {
            continue;
        };
        let Some((text, color, tooltip, hex)) = badge(code) else {
            continue;
        };
        let mut contribution = sdk::DecorationContribution::new(target.reference.clone());
        contribution.badges.push(sdk::DecorationBadge {
            text: text.into(),
            color: color.into(),
            tooltip: Some(tooltip.into()),
        });
        contribution.style.color = Some(hex.into());
        contribution.style.font_weight = Some("600".into());
        decorations.push(contribution);
    }
    Ok(decorations)
}

pub struct GitDecorations;
impl sdk::DecorationProvider for GitDecorations {
    fn resolve(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<Vec<sdk::DecorationContribution>, sdk::RpcError> {
        list(ctx).map_err(sdk::RpcError::internal)
    }
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
