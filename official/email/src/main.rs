//! Email plugin: IMAP/SMTP through the user's own mailbox via Core-brokered TCP.
fn main() {
    if let Err(error) = fiex_plugin_sdk::Providers::new()
        .connection(fiex_plugin_email::commands::EmailConnection)
        .serve_with("org.fiex.email", fiex_plugin_email::commands::run)
    {
        eprintln!("Email plugin: {error}");
        std::process::exit(1);
    }
}
