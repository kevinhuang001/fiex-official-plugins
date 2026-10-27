# VFS：把插件文件树接进 Core Explorer

## 原理

VFS 是 Core 与插件之间的**节点协议**，不是真实挂载。Core 负责 URI、权限、
缓存、离线副本和拖拽；插件只返回节点和内容。

```
Explorer 切到 fiex://mount/<id>/
      │
      ▼
Core 调 vfs.list / vfs.stat / vfs.readSmall / vfs.search
      │
      ▼
插件返回节点、revision、内容
```

独立 VFS 用 `VfsProvider`；连接内的文件树用 `ConnectionProvider` 的 `fs.*`。

## Manifest

```json
"extensions": {
  "vfs": { "title": "示例文件", "readOnly": true }
}
```

`vfs` 是单个对象。连接工厂还要声明 `extensions.connections`。

## Rust 实现：独立 VFS

```rust
struct ExampleVfs;

impl sdk::VfsProvider for ExampleVfs {
    fn list(&self, _ctx: &sdk::RequestContext) -> Result<Vec<Value>, RpcError> {
        Ok(vec![json!({
            "name": "README.md", "nodeId": "readme", "kind": "file",
            "isDir": false, "size": 14, "modified": 0,
            "extension": "md", "mime": "text/markdown", "readonly": true
        })])
    }
    fn stat(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({
            "name": "README.md", "nodeId": "readme", "kind": "file",
            "isDir": false, "size": 14, "modified": 0,
            "extension": "md", "mime": "text/markdown", "readonly": true
        }))
    }
    fn read_small(&self, _ctx: &sdk::RequestContext) -> Result<Vec<u8>, RpcError> {
        Ok(b"hello example\n".to_vec())
    }
    fn search(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({"entries": [], "truncated": false}))
    }
}
```

## 连接内的文件树

连接文件树由 `ConnectionProvider` 提供：

```rust
impl sdk::ConnectionProvider for ExampleConnection {
    fn list(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> { /* fs.list */ }
    fn stat(&self, _ctx: &sdk::RequestContext) -> Result<Value, RpcError> { /* fs.stat */ }
    fn open_read(&self, _ctx: &sdk::RequestContext)
        -> Result<sdk::ConnectionReader, RpcError> { /* fs.openRead + stream */ }
}
```

`open_read` 返回 `ConnectionReader`，SDK 自动管理 `fs.readStream` 的 offset、
分块和 `fs.closeRead`。不要手写 stream ID 表。

## 规则

- `nodeId` 是插件命名空间内稳定 ID，不是宿主路径；
- 内容变化时 `revision` 必须变化；
- 大文件用流式读取，不要整文件塞进一条 JSON；
- `search` 返回 `relativePath`，Core 会绑定到自己的 mount；
- 路径穿越（`../`、绝对路径、symlink）必须在插件侧拒绝。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs)。
