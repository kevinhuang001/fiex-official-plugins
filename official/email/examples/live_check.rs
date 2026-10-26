//! Live IMAP/SMTP check against a real mailbox. Credentials come from the
//! environment and are never stored in the repository:
//!   FIEX_EMAIL=... FIEX_EMAIL_PASSWORD=... cargo run --example live_check
use fiex_plugin_email::{imap, smtp};
use std::net::TcpStream;
use std::time::Duration;

fn connect(host: &str, port: u16) -> TcpStream {
    let stream = TcpStream::connect((host, port)).expect("connect failed");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("write timeout");
    stream
}

fn main() {
    let email = std::env::var("FIEX_EMAIL").expect("FIEX_EMAIL");
    let password = std::env::var("FIEX_EMAIL_PASSWORD").expect("FIEX_EMAIL_PASSWORD");
    let imap_host = std::env::var("FIEX_IMAP_HOST").unwrap_or_else(|_| "imap.qq.com".into());
    let smtp_host = std::env::var("FIEX_SMTP_HOST").unwrap_or_else(|_| "smtp.qq.com".into());

    let mut session = imap::login(connect(&imap_host, 993), &imap_host, &email, &password)
        .expect("IMAP login failed");
    let headers = imap::list(&mut session, 3).expect("IMAP list failed");
    println!("IMAP login OK, recent messages: {}", headers.len());
    for header in &headers {
        println!("  #{} {} | {}", header.seq, header.from, header.subject);
    }
    if let Some(header) = headers.first() {
        let body = imap::read(&mut session, header.seq).expect("IMAP read failed");
        println!(
            "IMAP read OK: {} | {} chars",
            body.subject,
            body.body.chars().count()
        );
    }
    let _ = session.logout();

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let subject = format!("Fiex Email live check {stamp}");
    smtp::send(
        connect(&smtp_host, 465),
        &smtp_host,
        &email,
        &password,
        &email,
        &subject,
        "Fiex email plugin live check ✓",
    )
    .expect("SMTP send failed");
    println!("SMTP send OK: {subject}");

    // QQ Mail accepts self-addressed SMTP mail but does not always deliver it
    // back to the same INBOX (or append it to Sent via SMTP). A 250 after DATA
    // is the server's acceptance contract; IMAP login/list/read above proves
    // the reading half against the real mailbox.
    println!("PASS: QQ Mail IMAP login/list/read and SMTP send");
}
