# Connections：连接入口与两种导航结果

## 原理

`connections` 只负责“用户怎么添加/打开一个连接”。文件列表由同一插件的
`ConnectionProvider`（`vfs.*`）或 `VfsProvider`（`vfs.*`）提供。

```
用户点 + / 连接项
      │
      ▼
Core 读 extensions.connections 生成表单
      │  connection.validate
      ▼
用户保存
      │  connection.open 或 connection.create
      ▼
NavigationTarget: { kind: "vfs" } | { kind: "pluginView" }
```

- 返回 `vfs` 时，Core Explorer 接管文件树；
- 返回 `pluginView` 时，插件 HTML 占满右侧内容区；
- `mountId` 由 Core 分配，插件不能自己编。

## Manifest

```json
"permissions": {
  "metadata": { "enabled": true },
  "folders": { "read": { "enabled": true } },
  "files": { "read": { "enabled": true } }
},
"extensions": {
  "connections": {
    "title": "示例文件",
    "fields": [
      { "id": "server", "label": "地址", "type": "text", "required": true }
    ]
  },
  "vfs": { "title": "示例文件", "readOnly": true }
}
```

`vfs` **不是**数组；`connections` 是单个对象。连接工厂若要显示文件树，必须
同时声明 `extensions.vfs`。

## Rust 实现

只实现 `ConnectionProvider`，不要手写 `match ctx.method()`：

```rust
struct ExampleConnection;

impl sdk::ConnectionProvider for ExampleConnection {
    fn validate(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({"valid": true}))
    }
    fn open(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        let factory = ctx.params()["factoryId"].as_str().unwrap_or("");
        if factory == "example-account" {
            return Ok(json!({
                "kind": "pluginView",
                "view": {"source": "package", "entry": "ui/account.html"},
                "state": {"connectionId": ctx.params()["connectionId"]}
            }));
        }
        Ok(json!({
            "kind": "vfs",
            "mountId": ctx.params()["mountId"],
            "nodeRef": {"mountId": ctx.params()["mountId"], "nodeId": "root"}
        }))
    }
    fn create(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        self.open(ctx)
    }
    fn list(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({"entries": [{
            "name": "README.md", "nodeId": "readme", "kind": "file",
            "size": 14, "modified": 0, "revision": "1"
        }], "nextCursor": null}))
    }
    fn stat(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({"revision": "1", "size": 14}))
    }
    fn open_read(&self, _ctx: &sdk::RequestContext)
        -> Result<sdk::ConnectionReader, RpcError>
    {
        Ok(sdk::ConnectionReader::new(
            std::io::Cursor::new(b"hello example\n".to_vec()),
            Some(14),
        ))
    }
}
```

`vfs.openRead` / `vfs.readStream` / `vfs.closeRead` 不需要自己实现：返回
`ConnectionReader` 后，SDK 负责 stream ID、offset、分块和关闭。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloConnection`。
