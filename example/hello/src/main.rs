//! Fiex official-plugin example: connections, decorations and toolbar.
//!
//! This is deliberately small. It does not touch the network or the user's
//! files; it shows the wire shapes Core expects. Run it with:
//!
//!   cargo run --manifest-path example/hello/Cargo.toml
//!
//! A real plugin can use the same shape with `ctx.files()`, `ctx.http()` and
//! `ctx.tcp()` inside the permissions it declares in `manifest.json`.
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

const HELLO_BASE64: &str = "aGVsbG8gZXhhbXBsZQo="; // "hello example\n"

fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let p = ctx.params();
    match method {
        // ------------------------------------------------------------------
        // Connections
        // ------------------------------------------------------------------
        "connection.validate" => Ok(json!({"valid": true})),
        "connection.open" | "connection.create" => {
            let factory = p.get("factoryId").and_then(Value::as_str).unwrap_or("");
            let mount = p.get("mountId").and_then(Value::as_str).unwrap_or("");
            match factory {
                // The account factory has no file tree: it returns a full-width
                // pluginView that replaces the right content area.
                "example-account" => Ok(json!({
                    "kind": "pluginView",
                    "view": {"source": "package", "entry": "ui/account.html"},
                    "state": {
                        "connectionId": p.get("connectionId").cloned().unwrap_or(Value::Null)
                    }
                })),
                // The files factory navigates to Core's native Explorer. Core
                // validates that this plugin registered extensions.vfs and owns
                // the mount before rendering it.
                _ => Ok(json!({
                    "kind": "vfs",
                    "mountId": mount,
                    "nodeRef": {"mountId": mount, "nodeId": "root"}
                })),
            }
        }
        // A read-only in-memory VFS. Core sends the stored connection settings
        // in `connection`; this example intentionally ignores them.
        "fs.list" => Ok(json!({
            "entries": [
                {
                    "name": "README.md",
                    "nodeId": "readme",
                    "isDir": false,
                    "size": 14,
                    "modified": 0,
                    "revision": "1"
                },
                {
                    "name": "docs",
                    "nodeId": "docs",
                    "isDir": true,
                    "size": 0,
                    "modified": 0,
                    "revision": "1"
                }
            ],
            "nextCursor": null
        })),
        "fs.stat" => Ok(json!({"revision": "1", "size": 14})),
        "fs.readSmall" => Ok(json!({"base64": HELLO_BASE64, "eof": true})),
        "fs.openRead" => Ok(json!({"streamId": "example-read"})),
        "fs.readStream" => Ok(json!({"base64": HELLO_BASE64, "eof": true})),
        "fs.closeRead" => Ok(Value::Null),
        // ------------------------------------------------------------------
        // Decorations
        // ------------------------------------------------------------------
        // Core calls this for every visible directory when the manifest
        // declares `extensions.decorations.mode = "dynamic"`. `entries` is the
        // visible set; return only paths from that set.
        "decorations.resolve" => {
            let mut decorations = Vec::new();
            if let Some(entries) = p.get("entries").and_then(Value::as_array) {
                for entry in entries {
                    let Some(path) = entry.get("path").and_then(Value::as_str) else {
                        continue;
                    };
                    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
                    if name.ends_with(".md") {
                        // Name-cell badge + color. The same `style` shape also
                        // works for modified/size/kind through `fields`.
                        decorations.push(json!({
                            "path": path,
                            "badge": {
                                "text": "MD",
                                "color": "blue",
                                "tooltip": "示例：Markdown 文件"
                            },
                            "style": {"color": "#2f6f9f", "fontWeight": "600"}
                        }));
                    }
                    if name.contains("todo") {
                        // Per-column fields: paint only the modified cell and
                        // add a badge to the size cell. `row` paints the whole
                        // row; leave it unselected-friendly.
                        decorations.push(json!({
                            "path": path,
                            "fields": {
                                "modified": {"style": {"color": "#b4443d"}},
                                "size": {"badges": [{"text": "TODO", "color": "orange"}]}
                            }
                        }));
                    }
                }
            }
            Ok(json!({"decorations": decorations}))
        }
        // ------------------------------------------------------------------
        // Toolbar / custom UI commands
        // ------------------------------------------------------------------
        "example.refresh" => Ok(json!({
            "refreshed": true,
            "directory": p.get("root").cloned().unwrap_or(Value::Null)
        })),
        "example.greet" => Ok(json!({"message": "你好，Fiex！"})),
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    if let Err(error) = sdk::serve("org.example.hello", run) {
        eprintln!("example plugin failed: {error}");
        std::process::exit(1);
    }
}
