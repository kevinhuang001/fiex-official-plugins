//! OpenPGP operations through the user's explicitly approved GnuPG executable.
mod commands;
mod gpg;
mod keys;

use fiex_plugin_sdk as sdk;

fn main() {
    if let Err(error) = sdk::serve("org.fiex.gpg", commands::run) {
        eprintln!("GPG plugin: {error}");
        std::process::exit(1);
    }
}
