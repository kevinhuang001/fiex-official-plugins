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
        // Core calls this when the manifest declares `extensions.decorations`
        // and its `match` is satisfied. `entries` is the visible set; return
        // direct child names, never absolute paths.
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
                            "name": name,
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
                            "name": name,
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
        // Info / Preview
        // ------------------------------------------------------------------
        "info.resolve" => {
            let name = p
                .pointer("/resource/metadata/name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let extension = p
                .pointer("/resource/metadata/extension")
                .and_then(Value::as_str)
                .unwrap_or("");
            let greeting = ctx
                .storage_get("global", "greeting")
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "你好".into());
            Ok(json!({
                "sections": [{
                    "id": "example",
                    "title": "示例信息",
                    "rows": [
                        {"label": "文件", "value": name},
                        {"label": "扩展名", "value": extension},
                        {"label": "问候语", "value": greeting},
                        {"label": "可复制", "value": "这一行可以复制", "copyable": true}
                    ]
                }]
            }))
        }
        "preview.open" => Ok(json!({
            "kind": "html",
            "view": {"source": "package", "entry": "ui/preview.html"}
        })),
        // ------------------------------------------------------------------
        // Context menu
        // ------------------------------------------------------------------
        "contextMenu.resolve" => Ok(json!({
            "items": [
                {"id": "example.inspect", "title": "示例：检查文件", "kind": "action"},
                {"id": "example.more", "title": "示例", "kind": "submenu"},
                {"id": "example.copy-path", "title": "示例：复制路径", "kind": "action", "parentId": "example.more"}
            ]
        })),
        "contextMenu.execute" => {
            let item = p.get("itemId").and_then(Value::as_str).unwrap_or("");
            if !matches!(item, "example.inspect" | "example.copy-path") {
                return Err(RpcError::invalid_params("unknown example menu item"));
            }
            Ok(json!({"ok": true, "itemId": item}))
        }
        // ------------------------------------------------------------------
        // Search
        // ------------------------------------------------------------------
        "search.query" => Ok(json!({
            "entries": [],
            "truncated": false,
            "warning": "示例搜索不返回真实文件；这里展示 SearchResponse 的外形"
        })),
        // ------------------------------------------------------------------
        // Hooks (contract only in this example)
        // ------------------------------------------------------------------
        "example.beforeOpen" => Ok(json!({"handled": false})),
        // ------------------------------------------------------------------
        // Toolbar / custom UI commands
        // ------------------------------------------------------------------
        "example.refresh" => Ok(json!({
            "refreshed": true,
            "directory": p.get("root").cloned().unwrap_or(Value::Null)
        })),
        "example.greet" => {
            let greeting = ctx
                .storage_get("global", "greeting")
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "你好".into());
            Ok(json!({"message": format!("{greeting}，Fiex！")}))
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    if let Err(error) = sdk::serve("org.example.hello", run) {
        eprintln!("example plugin failed: {error}");
        std::process::exit(1);
    }
}
