//! Email plugin: IMAP/SMTP through the user's own mailbox via Core-brokered TCP.
fn main() {
    if let Err(error) = fiex_plugin_sdk::Providers::new()
        .diagnostics()
        .connection(fiex_plugin_email::commands::EmailConnection)
        .serve_with("org.fiex.email", fiex_plugin_email::commands::run)
    {
        eprintln!("Email plugin: transport closed ({:?})", error.kind());
        std::process::exit(1);
    }
}
