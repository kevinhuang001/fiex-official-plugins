#[path = "../../../support/i18n.rs"]
mod i18n;
mod sessions;
mod transport;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

/// Panel-only presentation settings. They are stored in the plugin's private
/// `global` scope by Core's settings sheet; no connection or credential is read.
fn settings(ctx: &sdk::RequestContext) -> Value {
    let value = |key: &str| ctx.storage().get::<Value>(key).ok().flatten();
    json!({
        "fontFamily": value("fontFamily").and_then(|value| value.as_str().map(str::to_owned)),
        "fontSize": value("fontSize").and_then(|value| value.as_u64()),
    })
}

fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    match ctx.method() {
        "ssh.start" => transport::open(ctx),
        "ssh.exchange" => sessions::exchange(ctx.params()),
        "ssh.close" => sessions::close(ctx.params()),
        "ssh.settings" => Ok(settings(&ctx)),
        _ => Err(RpcError::method_not_found(ctx.method())),
    }
}

fn main() {
    if let Err(error) = sdk::Providers::new()
        .diagnostics()
        .connection(SshConnection)
        .serve_with("org.fiex.ssh", run)
    {
        eprintln!("SSH provider failed: transport closed ({:?})", error.kind());
        std::process::exit(1);
    }
}

struct SshConnection;
impl sdk::ConnectionProvider for SshConnection {
    fn validate(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        let host = ctx
            .params()
            .pointer("/connection/host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        Ok(json!({"valid":!host.is_empty()}))
    }
    fn open(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        sessions::connection_view(ctx.params())
    }
}
