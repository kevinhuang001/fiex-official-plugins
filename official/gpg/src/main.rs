//! OpenPGP operations through the user's explicitly approved GnuPG executable.
mod commands;
mod gpg;
#[path = "../../../support/i18n.rs"]
mod i18n;
mod keys;

use fiex_plugin_sdk as sdk;

fn main() {
    if let Err(error) = sdk::Providers::new()
        .context_menu(commands::GpgMenu)
        .serve_with("org.fiex.gpg", commands::run)
    {
        eprintln!("GPG plugin: {error}");
        std::process::exit(1);
    }
}
