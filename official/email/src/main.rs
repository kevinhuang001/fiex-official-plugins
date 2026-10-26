//! Email plugin: IMAP/SMTP through the user's own mailbox via Core-brokered TCP.
fn main() {
    if let Err(error) = fiex_plugin_sdk::serve("org.fiex.email", fiex_plugin_email::commands::run) {
        eprintln!("Email plugin: {error}");
        std::process::exit(1);
    }
}
