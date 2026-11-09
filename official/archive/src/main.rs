mod archives;
#[path = "../../../support/i18n.rs"]
mod i18n;
mod ignores;
mod info;
mod listing;
mod menu;
mod operations;
#[path = "../../../support/provider_results.rs"]
mod provider_results;
mod providers;
#[path = "../../../support/resources.rs"]
mod resources;
mod support;
#[cfg(test)]
mod tests;

use fiex_plugin_sdk as sdk;
use info::archive_info_fields;
use menu::{archive_menu, archive_menu_input, archive_resource};
use operations::run_operation;
use providers::Archive;
use support::*;

fn main() {
    if let Err(e) = sdk::Providers::new()
        .diagnostics()
        .info(Archive)
        .preview(Archive)
        .decorations(Archive)
        .context_menu(Archive)
        .hooks(Archive)
        .serve_with("org.fiex.archive", |ctx| {
            run_operation(&ctx, ctx.params().clone(), ctx.method())
        })
    {
        eprintln!("Archive plugin failed: transport closed ({:?})", e.kind());
        std::process::exit(1);
    }
}
