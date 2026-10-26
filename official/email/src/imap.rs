//! IMAP through the `imap` crate. The network stream comes from the Fiex SDK;
//! the crate only speaks the protocol over the already-authorized TLS stream.
use crate::tls;
use imap::types::Seq;
use mail_parser::MessageParser;
use rustls::{ClientConnection, StreamOwned};
use serde::Serialize;
use std::io::{Read, Write};

pub type Session<S> = imap::Session<StreamOwned<ClientConnection, S>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub seq: u32,
    pub subject: String,
    pub from: String,
    pub date: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub seq: u32,
    pub subject: String,
    pub from: String,
    pub date: String,
    pub body: String,
}

pub fn login<S: Read + Write>(
    stream: S,
    host: &str,
    email: &str,
    password: &str,
) -> Result<Session<S>, String> {
    let stream = tls::wrap(stream, host).map_err(|error| error.to_string())?;
    imap::Client::new(stream)
        .login(email, password)
        .map_err(|(error, _)| error.to_string())
}

pub fn list<S: Read + Write>(
    session: &mut Session<S>,
    limit: usize,
) -> Result<Vec<Header>, String> {
    let mailbox = session.select("INBOX").map_err(|error| error.to_string())?;
    let exists = mailbox.exists;
    if exists == 0 || limit == 0 {
        return Ok(Vec::new());
    }
    let end = exists;
    let start = end.saturating_sub(limit.saturating_sub(1) as u32).max(1);
    let fetches = session
        .fetch(format!("{start}:{end}"), "(BODY.PEEK[HEADER])")
        .map_err(|error| error.to_string())?;
    let mut headers = Vec::new();
    for fetch in fetches.iter() {
        let raw = fetch.header().or_else(|| fetch.body());
        let Some(raw) = raw else {
            continue;
        };
        let Some(message) = MessageParser::default().parse(raw) else {
            continue;
        };
        headers.push(Header {
            seq: fetch.message,
            subject: message.subject().unwrap_or("").to_string(),
            from: sender(&message),
            date: message
                .date()
                .map(|date| date.to_rfc3339())
                .unwrap_or_default(),
        });
    }
    headers.sort_by_key(|header| std::cmp::Reverse(header.seq));
    Ok(headers)
}

pub fn read<S: Read + Write>(session: &mut Session<S>, seq: Seq) -> Result<Body, String> {
    session.select("INBOX").map_err(|error| error.to_string())?;
    let fetches = session
        .fetch(seq.to_string(), "(BODY.PEEK[])")
        .map_err(|error| error.to_string())?;
    let fetch = fetches.iter().next().ok_or("message not found")?;
    let raw = fetch.body().ok_or("message body unavailable")?;
    if raw.len() > 8 * 1024 * 1024 {
        return Err("message is too large".into());
    }
    let message = MessageParser::default()
        .parse(raw)
        .ok_or("message could not be parsed")?;
    let body = message
        .body_text(0)
        .map(|text| text.into_owned())
        .or_else(|| message.body_html(0).map(|html| html.into_owned()))
        .unwrap_or_else(|| String::from_utf8_lossy(raw).into_owned());
    Ok(Body {
        seq,
        subject: message.subject().unwrap_or("").to_string(),
        from: sender(&message),
        date: message
            .date()
            .map(|date| date.to_rfc3339())
            .unwrap_or_default(),
        body,
    })
}

fn sender(message: &mail_parser::Message<'_>) -> String {
    message
        .from()
        .and_then(|list| list.first())
        .map(|mailbox| match (mailbox.name(), mailbox.address()) {
            (Some(name), Some(address)) => format!("{name} <{address}>"),
            (None, Some(address)) => address.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}
