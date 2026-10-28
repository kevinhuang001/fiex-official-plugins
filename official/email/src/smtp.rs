//! SMTP through the user's mailbox. The socket is a Core-brokered stream; this
//! module only wraps it in TLS and speaks SMTP.
use crate::mime;
use crate::tls;
use std::io::{BufRead, BufReader, Read, Write};

pub fn send<S: Read + Write>(
    stream: S,
    host: &str,
    email: &str,
    password: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let stream = tls::wrap(stream, host).map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(stream);
    expect(&mut reader, 220)?;
    command(&mut reader, "EHLO fiex.local", &[250])?;
    command(&mut reader, "AUTH LOGIN", &[334])?;
    command(
        &mut reader,
        &base64::Engine::encode(&base64::engine::general_purpose::STANDARD, email.as_bytes()),
        &[334],
    )?;
    command(
        &mut reader,
        &base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            password.as_bytes(),
        ),
        &[235],
    )?;
    command(&mut reader, &format!("MAIL FROM:<{email}>"), &[250])?;
    command(&mut reader, &format!("RCPT TO:<{to}>"), &[250])?;
    command(&mut reader, "DATA", &[354])?;
    let message = format!(
        "From: {}\r\nTo: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n.\r\n",
        mime::encode_header(email),
        mime::encode_header(to),
        mime::encode_header(subject),
        mime::encode_body(body, 76)
    );
    reader
        .get_mut()
        .write_all(message.as_bytes())
        .and_then(|_| reader.get_mut().flush())
        .map_err(|error| error.to_string())?;
    expect(&mut reader, 250)?;
    let _ = command(&mut reader, "QUIT", &[221]);
    Ok(())
}

fn command<S: Read + Write>(
    reader: &mut BufReader<S>,
    line: &str,
    codes: &[u16],
) -> Result<(), String> {
    reader
        .get_mut()
        .write_all(format!("{line}\r\n").as_bytes())
        .and_then(|_| reader.get_mut().flush())
        .map_err(|error| error.to_string())?;
    expect(reader, codes[0])
}

fn expect<S: Read + Write>(reader: &mut BufReader<S>, code: u16) -> Result<(), String> {
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if line.is_empty() {
            return Err(crate::i18n::t("SMTP 连接已关闭").into());
        }
        let status = line
            .get(..3)
            .and_then(|value| value.parse::<u16>().ok())
            .ok_or_else(|| {
                fiex_plugin_sdk::i18n::format(
                    crate::i18n::t("SMTP 响应无效：{0}"),
                    &[("0", line.trim().to_string())],
                )
            })?;
        if status != code {
            return Err(fiex_plugin_sdk::i18n::format(
                crate::i18n::t("SMTP 错误 {0}：{1}"),
                &[("0", format!("{}", status)), ("1", line.trim().to_string())],
            ));
        }
        // 250-... continues; 250 ... is the last line.
        if line.as_bytes().get(3) == Some(&b'-') {
            continue;
        }
        return Ok(());
    }
}

pub fn check<S: Read + Write>(
    stream: S,
    host: &str,
    email: &str,
    password: &str,
) -> Result<(), String> {
    let stream = tls::wrap(stream, host).map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(stream);
    expect(&mut reader, 220)?;
    command(&mut reader, "EHLO fiex.local", &[250])?;
    command(&mut reader, "AUTH LOGIN", &[334])?;
    command(
        &mut reader,
        &base64::Engine::encode(&base64::engine::general_purpose::STANDARD, email.as_bytes()),
        &[334],
    )?;
    command(
        &mut reader,
        &base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            password.as_bytes(),
        ),
        &[235],
    )?;
    let _ = command(&mut reader, "QUIT", &[221]);
    Ok(())
}
