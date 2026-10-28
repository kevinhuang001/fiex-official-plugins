mod sessions;
mod transport;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};
fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    match ctx.method() {
        "ssh.start" => transport::open(ctx),
        "ssh.exchange" => sessions::exchange(ctx.params()),
        "ssh.close" => sessions::close(ctx.params()),
        _ => Err(RpcError::method_not_found(ctx.method())),
    }
}

fn main() {
    if let Err(error) = sdk::Providers::new()
        .connection(SshConnection)
        .serve_with("org.fiex.ssh", run)
    {
        eprintln!("SSH provider failed: {error}");
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
