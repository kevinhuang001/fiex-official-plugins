//! Fiex official-plugin example: providers, connections and toolbar.
//!
//! This plugin is deliberately small. It does not touch the network or the
//! user's files; it shows the typed SDK provider API and the wire shapes Core
//! expects. Run it with:
//!
//!   cargo run --manifest-path example/hello/Cargo.toml
use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FsEntry, FsListing, FsRead, FsRequest, VfsProvider},
    RpcError,
};
use serde_json::{json, Value};

const HELLO: &[u8] = b"hello example\n";

fn greeting(ctx: &sdk::RequestContext) -> String {
    ctx.storage()
        .scoped("global")
        .get::<String>("greeting")
        .ok()
        .flatten()
        .unwrap_or_else(|| "你好".into())
}

struct HelloConnection;

fn connection_open(ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
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
            },
            "tabTitle": "示例账号"
        })),
        // The files factory navigates to Core's native Explorer. Core validates
        // that this plugin registered extensions.vfs and owns the mount.
        _ => Ok(json!({
            "kind": "vfs",
            "mountId": mount,
            "nodeRef": {"mountId": mount, "nodeId": "root"}
        })),
    }
}

impl sdk::ConnectionProvider for HelloConnection {
    fn validate(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({"valid": true}))
    }
    fn open(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        connection_open(ctx)
    }
    fn create(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        connection_open(ctx)
    }
}

fn readme() -> FsEntry {
    let mut entry = FsEntry::file("readme", "README.md")
        .with_size(HELLO.len() as u64)
        .with_revision("1")
        .readonly();
    entry.modified = Some(0);
    entry.extension = Some("md".into());
    entry.mime = Some("text/markdown".into());
    entry
}

fn docs() -> FsEntry {
    let mut entry = FsEntry::directory("docs", "docs").readonly();
    entry.modified = Some(0);
    entry.size = Some(0);
    entry
}

/// The typed file-system provider: `list`, `stat`, `read` and `search` are
/// declared by the macro exactly as written here.
struct HelloFiles;

impl VfsProvider for HelloFiles {
    fn operations(&self) -> &'static [sdk::filesystem::FsOperation] {
        &[
            sdk::filesystem::FsOperation::List,
            sdk::filesystem::FsOperation::Stat,
            sdk::filesystem::FsOperation::Read,
            sdk::filesystem::FsOperation::Search,
        ]
    }

    fn list(&self, _request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Ok(FsListing::new([readme(), docs()]))
    }

    fn stat(&self, request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Ok(if request.node_id() == Some("docs") {
            docs()
        } else {
            readme()
        })
    }

    fn read(&self, _request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        Ok(FsRead::from_bytes(HELLO.to_vec()))
    }

    fn search(&self, _request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Ok(FsListing::default())
    }
}

struct HelloInfo;

impl sdk::InfoProvider for HelloInfo {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<sdk::InfoSection>, RpcError> {
        let meta = ctx
            .targets()
            .first()
            .and_then(|target| target.metadata.as_ref());
        let name = meta.map(|value| value.name.as_str()).unwrap_or("unknown");
        let extension = meta
            .and_then(|value| value.extension.as_deref())
            .unwrap_or("");
        Ok(vec![sdk::InfoSection {
            source: String::new(),
            provider: String::new(),
            plugin_id: None,
            id: "example".into(),
            title: "示例信息".into(),
            rows: vec![
                sdk::InfoRow {
                    label: "文件".into(),
                    value: name.into(),
                    copyable: false,
                },
                sdk::InfoRow {
                    label: "扩展名".into(),
                    value: extension.into(),
                    copyable: false,
                },
                sdk::InfoRow {
                    label: "问候语".into(),
                    value: greeting(ctx),
                    copyable: false,
                },
                sdk::InfoRow {
                    label: "可复制".into(),
                    value: "这一行可以复制".into(),
                    copyable: true,
                },
            ],
        }])
    }
}

struct HelloPreview;

impl sdk::PreviewProvider for HelloPreview {
    fn open(&self, _ctx: &sdk::RequestContext) -> Result<sdk::PreviewView, RpcError> {
        Ok(sdk::PreviewView::html("ui/preview.html"))
    }
}

struct HelloDecorations;

impl sdk::DecorationProvider for HelloDecorations {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<Value>, RpcError> {
        let mut decorations = Vec::new();
        for target in ctx.targets() {
            let Some(metadata) = target.metadata.as_ref() else {
                continue;
            };
            let name = &metadata.name;
            if name.ends_with(".md") {
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
                decorations.push(json!({
                    "name": name,
                    "fields": {
                        "modified": {"style": {"color": "#b4443d"}},
                        "size": {"badges": [{"text": "TODO", "color": "orange"}]}
                    }
                }));
            }
        }
        Ok(decorations)
    }
}

struct HelloMenu;

impl sdk::ContextMenuProvider for HelloMenu {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<Value>, RpcError> {
        if ctx.params().get("parentId").and_then(Value::as_str) == Some("example.more") {
            return Ok(vec![json!({
                "id": "example.copy-path",
                "title": "示例：复制路径",
                "kind": "action"
            })]);
        }
        Ok(vec![
            json!({"id": "example.inspect", "title": "示例：检查文件", "kind": "action"}),
            json!({"id": "example.more", "title": "示例", "kind": "submenu"}),
        ])
    }
    fn execute(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        let item = ctx.params()["itemId"]
            .as_str()
            .ok_or_else(|| RpcError::invalid_params("missing menu itemId"))?;
        if !matches!(item, "example.inspect" | "example.copy-path") {
            return Err(RpcError::invalid_params("unknown example menu item"));
        }
        Ok(json!({"ok": true, "itemId": item}))
    }
}

struct HelloSearch;

impl sdk::SearchProvider for HelloSearch {
    fn query(&self, _ctx: &sdk::RequestContext) -> Result<sdk::SearchResults, RpcError> {
        Ok(sdk::SearchResults {
            entries: Vec::new(),
            truncated: false,
            next_cursor: None,
        })
    }
}

struct HelloHooks;

impl sdk::HookProvider for HelloHooks {
    fn invoke(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({"handled": false}))
    }
}

struct HelloToolbar;

impl sdk::ToolbarProvider for HelloToolbar {
    fn invoke(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({
            "refreshed": true,
            "directory": ctx.params().get("root").cloned().unwrap_or(Value::Null)
        }))
    }
}

/// Custom RPC methods stay outside the typed provider registry.
fn custom(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    match ctx.method() {
        "example.greet" => Ok(json!({"message": "你好，Fiex！"})),
        method => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    let providers = sdk::Providers::new().diagnostics()
        .connection(HelloConnection)
        .filesystem(HelloFiles)
        .info(HelloInfo)
        .preview(HelloPreview)
        .decorations(HelloDecorations)
        .context_menu(HelloMenu)
        .search(HelloSearch)
        .hooks(HelloHooks)
        .toolbar(HelloToolbar);
    if let Err(error) = providers.serve_with("org.example.hello", custom) {
        eprintln!("example plugin failed: {error}");
        std::process::exit(1);
    }
}
