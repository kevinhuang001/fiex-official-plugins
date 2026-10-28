//! Official-plugin resources; the SDK owns request locale and fallback behavior.
use fiex_plugin_sdk::i18n::Catalog;
use std::sync::LazyLock;

static MESSAGES: LazyLock<Catalog> = LazyLock::new(|| {
    Catalog::from_json(
        include_str!("i18n/en.json"),
        include_str!("i18n/zh-TW.json"),
    )
    .expect("official plugin catalogs")
});

pub fn t(source: &'static str) -> &'static str {
    MESSAGES.t(source)
}
