//! Minimal outbound MIME encoding for SMTP. Inbound parsing uses mail-parser.
use base64::Engine;

pub fn encode_header(value: &str) -> String {
    if value.is_ascii() && !value.contains(['\r', '\n']) {
        return value.to_string();
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
    format!("=?UTF-8?B?{encoded}?=")
}

pub fn encode_body(value: &str, line_width: usize) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
    encoded
        .as_bytes()
        .chunks(line_width.clamp(4, 998))
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join("\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_encoder_only_encodes_non_ascii() {
        assert_eq!(encode_header("hello"), "hello");
        assert!(encode_header("你好").starts_with("=?UTF-8?B?"));
    }

    #[test]
    fn body_is_wrapped_base64() {
        let encoded = encode_body("hello world", 8);
        assert!(encoded.contains("\r\n"));
        assert!(!encoded.contains(' '));
    }
}
