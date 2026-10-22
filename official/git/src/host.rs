//! Git commands are executed by the Fiex host, never by the plugin process.
use fiex_plugin_sdk::HostServices;
use serde_json::json;
use std::path::Path;

pub struct ExecResult {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

pub fn git(host: &mut dyn HostServices, root: &Path, args: &[&str]) -> Result<ExecResult, String> {
    let value = host
        .invoke(
            "host.exec",
            json!({
                "command": "git",
                "args": args,
                "cwd": root.to_string_lossy(),
            }),
        )
        .map_err(|error| error.to_string())?;
    let stdout = value
        .get("stdoutBase64")
        .and_then(serde_json::Value::as_str)
        .map(|encoded| {
            use base64::{engine::general_purpose::STANDARD, Engine};
            STANDARD.decode(encoded).unwrap_or_default()
        })
        .or_else(|| {
            value
                .get("stdout")
                .and_then(serde_json::Value::as_str)
                .map(|text| text.as_bytes().to_vec())
        })
        .unwrap_or_default();
    Ok(ExecResult {
        code: value
            .get("code")
            .and_then(serde_json::Value::as_i64)
            .map(|code| code as i32),
        stdout,
        stderr: value
            .get("stderr")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

pub fn text(host: &mut dyn HostServices, root: &Path, args: &[&str]) -> Result<String, String> {
    let result = git(host, root, args)?;
    if result.code == Some(0) {
        Ok(String::from_utf8_lossy(&result.stdout).trim().to_string())
    } else {
        Err(if result.stderr.trim().is_empty() {
            "Git 操作未完成".into()
        } else {
            result.stderr.trim().into()
        })
    }
}
