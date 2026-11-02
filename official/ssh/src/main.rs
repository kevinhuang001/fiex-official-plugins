use fiex_plugin_sdk::ProviderInvocationExt;
#[path = "../../../support/i18n.rs"]
mod i18n;
mod sessions;
mod transport;
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

/// Panel-only presentation settings. They are stored in the plugin's private
/// `global` scope by Core's settings sheet; no connection or credential is read.
fn settings(ctx: &dyn sdk::ProviderInvocation) -> Value {
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
    fn validate(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<sdk::ConnectionValidation, RpcError> {
        let request = match ctx.input()? {
            sdk::ProviderInput::Connection(request) => request,
            _ => return Err(RpcError::invalid_params("expected connection request")),
        };
        let connection = request
            .connection_value()
            .map_err(RpcError::invalid_params)?
            .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
        let host = connection
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        Ok(if host.is_empty() {
            sdk::ConnectionValidation::invalid()
        } else {
            sdk::ConnectionValidation::valid()
        })
    }
    fn open(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<sdk::ConnectionOpenResult, RpcError> {
        let request = match ctx.input()? {
            sdk::ProviderInput::Connection(request) => request,
            _ => return Err(RpcError::invalid_params("expected connection request")),
        };
        let connection = request
            .connection_value()
            .map_err(RpcError::invalid_params)?
            .ok_or_else(|| RpcError::invalid_params("missing connection"))?;
        let result = sessions::connection_view(&json!({"connection": connection}))?;
        Ok(sdk::ConnectionOpenResult::PluginView {
            view: sdk::PackageView {
                source: "package".into(),
                entry: "ui/dist/index.html".into(),
            },
            state: result["state"].clone(),
            tab_title: result["tabTitle"].as_str().map(str::to_owned),
        })
    }
}
