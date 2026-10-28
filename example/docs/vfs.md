# VFS：把插件文件树接进 Core Explorer

## 原理

VFS 是 Core 与插件之间的**节点协议**，不是真实挂载。Core 负责 URI、权限、
缓存、离线副本和拖拽；插件只返回节点和内容。

```
Explorer 切到 fiex://mount/<id>/
      │
      ▼
Core 调 fs.list / fs.stat / fs.search / fs.read
      │
      ▼
插件返回节点、revision、内容
```

协议 5 只有一套 `FSProvider`：连接挂载和独立视图使用相同的方法、
DTO 和流语义，差别只在请求携带的身份块（`connection` 或
`contributionId`/`viewContext`）。

## Manifest

```json
"extensions": {
  "vfs": { "title": "示例文件", "readOnly": true }
}
```

`vfs` 是单个对象。连接工厂还要声明 `extensions.connections`。

## Rust 实现

```rust
use fiex_plugin_sdk::{
    self as sdk,
    filesystem::{FSProvider, FsEntry, FsListing, FsRead, FsRequest},
    RpcError,
};

struct ExampleFs;

impl FSProvider for ExampleFs {
    fn operations(&self) -> &'static [sdk::filesystem::FsOp] { &[sdk::filesystem::FsOp::List, sdk::filesystem::FsOp::Stat, sdk::filesystem::FsOp::Read, sdk::filesystem::FsOp::Search] }

    fn list(&self, _request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Ok(FsListing::new([
            FsEntry::file("readme", "README.md")
                .with_size(14)
                .with_revision("1")
                .readonly(),
        ]))
    }

    fn stat(&self, _request: &FsRequest<'_>) -> Result<FsEntry, RpcError> {
        Ok(FsEntry::file("readme", "README.md")
            .with_size(14)
            .with_revision("1")
            .readonly())
    }

    fn read(&self, _request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        Ok(FsRead::from_bytes(b"hello example\n".to_vec()))
    }

    fn search(&self, _request: &FsRequest<'_>) -> Result<FsListing, RpcError> {
        Ok(FsListing::default())
    }
}
```

能力必须由 `operations()` 显式报告；`discover(ctx)` 可以按服务器权限进一步收窄。
只读插件仅报告读取相关操作。

普通 SDK 分发器按 `FSProvider::operations()` 与 `discover(ctx)` 的交集调用方法，SDK 自动管理
`fs.openRead/readStream/closeRead` 的 offset、分块和 `fs.closeRead`，以及
`fs.beginWrite/writeStream/commit/abort` 的顺序写入。不要手写 stream ID 表。

注册时把 provider 交给 `Providers`，自定义 RPC 仍走 fallback：

```rust
let providers = sdk::Providers::new()
    .connection(ExampleConnection) // connection.validate/open/create
    .filesystem(ExampleFs);
providers.serve_with("org.example.plugin", custom)?;
```

## 规则

- `nodeId` 是插件命名空间内稳定 ID，不是宿主路径；
- 内容变化时 `revision` 必须变化；
- 读取返回标准 `Read`，写入返回标准 `Write`，大文件不要整文件塞进一条 JSON；
- `search` 返回的条目带 `relativePath`，Core 会绑定到自己的 mount；
- 路径穿越（`../`、绝对路径、symlink）必须在插件侧拒绝。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs)。
