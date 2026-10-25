//! SSH connection manifest provider. Core owns the PTY/session and xterm
//! transport; this plugin only declares the connection surface and validates
//! that the host is configured.
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
    match ctx.method() {
        "connection.validate" => {
            let host = p
                .pointer("/connection/host")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            Ok(json!({ "valid": !host.is_empty() }))
        }
        "connection.open" | "connection.create" => Ok(json!({
            "kind": "vfs",
            "mountId": p.get("mountId").cloned().unwrap_or(Value::Null)
        })),
        _ => Err(RpcError::method_not_found(ctx.method())),
    }
}

fn main() {
    if let Err(error) = sdk::serve("org.fiex.ssh", run) {
        eprintln!("SSH provider failed: {error}");
        std::process::exit(1);
    }
}
