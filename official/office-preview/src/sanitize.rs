//! Defense in depth: the renderer escapes text, but an iframe DOM is built from
//! this string, so drop anything executable before it reaches the UI.
pub(crate) fn sanitize(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let mut index = 0;
    while index < html.len() {
        if lower[index..].starts_with("<script")
            || lower[index..].starts_with("<iframe")
            || lower[index..].starts_with("<object")
            || lower[index..].starts_with("<embed")
        {
            let closing = if lower[index..].starts_with("<script") {
                "</script>"
            } else if lower[index..].starts_with("<iframe") {
                "</iframe>"
            } else if lower[index..].starts_with("<object") {
                "</object>"
            } else {
                "</embed>"
            };
            index = lower[index..]
                .find(closing)
                .map(|offset| index + offset + closing.len())
                .unwrap_or(html.len());
            continue;
        }
        let rest = &html[index..];
        let consumed = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        if rest.starts_with("on")
            && rest
                .chars()
                .nth(2)
                .is_some_and(|next| next.is_ascii_alphabetic())
        {
            // Attribute handlers only appear inside tags; dropping the name is
            // enough because the tag itself is kept.
            if let Some(end) = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '=') {
                index += end;
                continue;
            }
        }
        if rest.to_ascii_lowercase().starts_with("javascript:") {
            index += "javascript:".len();
            continue;
        }
        output.push_str(&html[index..index + consumed]);
        index += consumed;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_executable_markup() {
        let html = sanitize(
            "<p onclick=\"x()\">a<script>alert(1)</script><img src=x onerror=alert(2)></p>",
        );
        assert!(!html.contains("<script"));
        assert!(!html.contains("onclick"));
        assert!(!html.contains("onerror"));
        assert!(html.contains("<img src=x"));
    }
}
