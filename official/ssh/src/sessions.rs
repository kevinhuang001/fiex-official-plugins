//! Plugin-owned terminal sessions. Only opaque tokens cross the PluginView bridge.
use base64::{engine::general_purpose::STANDARD, Engine};
use fiex_plugin_sdk::RpcError;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_INPUT: usize = 64 * 1024;
const IDLE: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Registry {
    pending: HashMap<String, (Value, Instant)>,
    active: HashMap<String, Terminal>,
}
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}
struct State {
    input: Vec<u8>,
    output: Vec<u8>,
    cols: u16,
    rows: u16,
    ready: bool,
    ended: bool,
    closed: bool,
    error: Option<String>,
    touched: Instant,
}
#[derive(Clone)]
pub(crate) struct Terminal(Arc<Mutex<State>>);
pub(crate) struct Input {
    pub data: Vec<u8>,
    pub eof: bool,
}
pub(crate) struct Size {
    pub cols: u16,
    pub rows: u16,
}
fn unavailable() -> RpcError {
    RpcError::internal("terminal session unavailable")
}
fn token(params: &Value) -> Result<&str, RpcError> {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| RpcError::invalid_params("missing sessionId"))
}
fn sweep(registry: &mut Registry) {
    registry.pending.retain(|_, (_, at)| at.elapsed() < IDLE);
    registry.active.retain(|_, terminal| {
        if let Ok(mut state) = terminal.0.lock() {
            if state.touched.elapsed() >= IDLE {
                state.closed = true;
                return false;
            }
            true
        } else {
            false
        }
    });
}
pub(crate) fn connection_view(params: &Value) -> Result<Value, RpcError> {
    let connection = params
        .get("connection")
        .filter(|v| v.is_object())
        .ok_or_else(|| RpcError::invalid_params("missing SSH connection"))?;
    let mut registry = registry().lock().map_err(|_| unavailable())?;
    sweep(&mut registry);
    if registry.pending.len() + registry.active.len() >= 128 {
        return Err(RpcError::internal("too many SSH sessions"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    registry
        .pending
        .insert(id.clone(), (connection.clone(), Instant::now()));
    Ok(
        json!({"kind":"pluginView", "view":{"source":"package","entry":"ui/dist/index.html"},
        "tabTitle":format!("SSH · {}", connection.get("host").and_then(Value::as_str).unwrap_or("")),
        "state":{"sessionId":id}}),
    )
}
pub(crate) fn start(id: &str) -> Result<(Value, Terminal), RpcError> {
    let mut registry = registry().lock().map_err(|_| unavailable())?;
    sweep(&mut registry);
    let (config, _) = registry.pending.remove(id).ok_or_else(unavailable)?;
    let terminal = Terminal(Arc::new(Mutex::new(State {
        input: Vec::new(),
        output: Vec::new(),
        cols: 80,
        rows: 24,
        ready: false,
        ended: false,
        closed: false,
        error: None,
        touched: Instant::now(),
    })));
    registry.active.insert(id.to_string(), terminal.clone());
    Ok((config, terminal))
}
pub(crate) fn exchange(params: &Value) -> Result<Value, RpcError> {
    let id = token(params)?;
    let registry = registry().lock().map_err(|_| unavailable())?;
    let terminal = registry.active.get(id).ok_or_else(unavailable)?;
    let input = params
        .get("dataBase64")
        .and_then(Value::as_str)
        .unwrap_or("");
    if input.len() > MAX_INPUT * 2 {
        return Err(RpcError::invalid_params("terminal input too large"));
    }
    let input = STANDARD
        .decode(input)
        .map_err(|_| RpcError::invalid_params("invalid terminal input"))?;
    let mut state = terminal.0.lock().map_err(|_| unavailable())?;
    if state.input.len() + input.len() > MAX_INPUT {
        return Err(RpcError::invalid_params("terminal input queue full"));
    }
    state.touched = Instant::now();
    state.input.extend(input);
    if let (Some(cols), Some(rows)) = (
        params.get("cols").and_then(Value::as_u64),
        params.get("rows").and_then(Value::as_u64),
    ) {
        state.cols = cols.clamp(20, 65535) as u16;
        state.rows = rows.clamp(4, 65535) as u16;
    }
    let output = std::mem::take(&mut state.output);
    Ok(
        json!({"dataBase64":STANDARD.encode(output),"ready":state.ready,"ended":state.ended,"error":state.error}),
    )
}
pub(crate) fn close(params: &Value) -> Result<Value, RpcError> {
    let id = token(params)?;
    let mut registry = registry().lock().map_err(|_| unavailable())?;
    registry.pending.remove(id);
    if let Some(terminal) = registry.active.remove(id) {
        terminal.0.lock().map_err(|_| unavailable())?.closed = true;
    }
    Ok(Value::Null)
}
impl Terminal {
    pub fn ready(&self) -> Result<(), RpcError> {
        self.0.lock().map_err(|_| unavailable())?.ready = true;
        Ok(())
    }
    pub fn output(&self, bytes: &[u8]) -> Result<(), RpcError> {
        let mut state = self.0.lock().map_err(|_| unavailable())?;
        if state.closed || state.output.len() + bytes.len() > MAX_OUTPUT {
            state.closed = true;
            return Err(RpcError::internal(
                "terminal output buffer full or view closed",
            ));
        }
        state.output.extend_from_slice(bytes);
        Ok(())
    }
    pub fn closed(&self) -> Result<bool, RpcError> {
        let mut state = self.0.lock().map_err(|_| unavailable())?;
        if state.touched.elapsed() >= IDLE {
            state.closed = true;
        }
        Ok(state.closed)
    }
    pub fn input(&self, max: usize) -> Result<Input, RpcError> {
        let mut state = self.0.lock().map_err(|_| unavailable())?;
        let count = state.input.len().min(max);
        Ok(Input {
            data: state.input.drain(..count).collect(),
            eof: state.closed,
        })
    }
    pub fn size(&self) -> Result<Size, RpcError> {
        let state = self.0.lock().map_err(|_| unavailable())?;
        Ok(Size {
            cols: state.cols,
            rows: state.rows,
        })
    }
    pub fn end(&self, error: Option<&str>) -> Result<(), RpcError> {
        let mut state = self.0.lock().map_err(|_| unavailable())?;
        state.ended = true;
        state.error = error.map(str::to_string);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn view_tokens_keep_secrets_private_and_sessions_are_single_use() {
        let view =
            connection_view(&json!({"connection":{"host":"example.test","password":"private"}}))
                .unwrap();
        assert_eq!(view["kind"], "pluginView");
        assert_eq!(view["state"].as_object().unwrap().len(), 1);
        let id = view["state"]["sessionId"].as_str().unwrap();
        let (config, terminal) = start(id).unwrap();
        assert_eq!(config["password"], "private");
        assert!(start(id).is_err());
        terminal.ready().unwrap();
        terminal.output(b"stdout").unwrap();
        let output = exchange(
            &json!({"sessionId":id,"dataBase64":STANDARD.encode(b"input"),"cols":120,"rows":42}),
        )
        .unwrap();
        assert_eq!(output["ready"], true);
        assert_eq!(
            STANDARD
                .decode(output["dataBase64"].as_str().unwrap())
                .unwrap(),
            b"stdout"
        );
        assert_eq!(terminal.input(3).unwrap().data, b"inp");
        assert_eq!(terminal.input(32).unwrap().data, b"ut");
        assert_eq!(terminal.size().unwrap().cols, 120);
        terminal.end(Some("disconnected")).unwrap();
        assert_eq!(
            exchange(&json!({"sessionId":id})).unwrap()["error"],
            "disconnected"
        );
        close(&json!({"sessionId":id})).unwrap();
        assert!(terminal.closed().unwrap());
        assert!(exchange(&json!({"sessionId":id})).is_err());
    }
    #[test]
    fn abandoned_views_and_slow_consumers_are_bounded() {
        let view = connection_view(&json!({"connection":{"host":"example.test"}})).unwrap();
        let id = view["state"]["sessionId"].as_str().unwrap();
        let (_, terminal) = start(id).unwrap();
        assert!(exchange(
            &json!({"sessionId":id,"dataBase64":STANDARD.encode(vec![0;MAX_INPUT+1])})
        )
        .is_err());
        assert!(terminal.output(&vec![0; MAX_OUTPUT + 1]).is_err());
        assert!(terminal.closed().unwrap());
        terminal.0.lock().unwrap().touched = Instant::now() - IDLE;
        assert!(terminal.closed().unwrap());
        close(&json!({"sessionId":id})).unwrap();
    }
}
