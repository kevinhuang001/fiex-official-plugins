//! GnuPG process invocation and status-stream parsing.
use fiex_plugin_sdk::{self as sdk, ipc::ProcessResult, RpcError};

/// Runs the user's approved `gpg` executable. A missing installation is
/// reported as such rather than as a GnuPG command failure.
pub(crate) fn invoke(
    args: &[String],
    host: &sdk::RequestContext,
) -> Result<ProcessResult, RpcError> {
    invoke_with_stdin(args, host, None)
}

pub(crate) fn invoke_with_stdin(
    args: &[String],
    host: &sdk::RequestContext,
    stdin: Option<&str>,
) -> Result<ProcessResult, RpcError> {
    host.process_run("gpg", args, stdin).map_err(|error| {
        RpcError::internal(format!("无法运行 gpg，请先安装 GnuPG 并加入 PATH：{error}"))
    })
}

/// GnuPG reports machine-readable state on the status stream (`--status-fd=2`),
/// interleaved with human-readable text.
pub(crate) fn status_lines(stderr: &str) -> impl Iterator<Item = &str> {
    stderr
        .lines()
        .filter_map(|line| line.strip_prefix("[GNUPG:] "))
}

pub(crate) fn status_value<'a>(stderr: &'a str, keyword: &str) -> Option<&'a str> {
    status_lines(stderr).find_map(|line| {
        line.strip_prefix(keyword)
            .and_then(|rest| rest.trim().split(' ').next())
    })
}

/// The text after a status keyword, e.g. `KEY_CREATED B <fingerprint>`.
pub(crate) fn status_line<'a>(stderr: &'a str, keyword: &str) -> Option<&'a str> {
    status_lines(stderr).find_map(|line| line.strip_prefix(keyword).map(str::trim))
}

pub(crate) fn signed_by(stderr: &str) -> Option<String> {
    status_lines(stderr).find_map(|line| {
        let rest = line.strip_prefix("GOODSIG ")?;
        Some(
            rest.split_once(' ')
                .map_or(String::new(), |(_, name)| name.to_string()),
        )
    })
}

/// A failed command still carries the useful diagnostic; human text is kept
/// short because it is rendered in the panel, not a log.
pub(crate) fn failure(stderr: &str) -> String {
    let message: Vec<&str> = stderr
        .lines()
        .filter(|line| !line.starts_with("[GNUPG:]") && !line.trim().is_empty())
        .collect();
    let text = if message.is_empty() {
        stderr.trim()
    } else {
        message.last().copied().unwrap_or("")
    };
    text.chars().take(240).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failures_keep_the_last_human_line_and_stay_short() {
        let status = "[GNUPG:] ERROR scdaemon 1\n[GNUPG:] FAILURE sign 1234\ngpg: signing failed: No secret key\ngpg: processing message failed\n";
        assert_eq!(failure(status), "gpg: processing message failed");
        assert!(!failure(status).contains("[GNUPG:]"));
        let long = "x".repeat(400);
        assert_eq!(failure(&long).chars().count(), 240);
    }
    #[test]
    fn verify_result_requires_a_good_signature() {
        let status = "[GNUPG:] NEWSIG\n[GNUPG:] GOODSIG ABCDEF0123456789 Ada Lovelace <ada@example.org>\n[GNUPG:] VALIDSIG 0123456789ABCDEF0123456789ABCDEF01234567 2024-01-01 1700000000 0 4 0 1 10 00 0123456789ABCDEF0123456789ABCDEF01234567\n";
        assert_eq!(
            status_value(status, "VALIDSIG"),
            Some("0123456789ABCDEF0123456789ABCDEF01234567")
        );
        assert_eq!(
            signed_by(status).as_deref(),
            Some("Ada Lovelace <ada@example.org>")
        );
    }
}
