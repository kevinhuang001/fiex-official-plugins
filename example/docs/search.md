# Search：搜索来源

Core 固定调用 `search.query`，用 `SearchProvider` 注册即可。

## Manifest

```json
"extensions": {
  "search": { "icon": "magnifying-glass" }
}
```

`search` 是单个对象，不是数组；没有 `command`/`title` 字段。

## Rust 实现

```rust
struct ExampleSearch;

impl sdk::SearchProvider for ExampleSearch {
    fn query(&self, _ctx: &sdk::RequestContext)
        -> Result<sdk::SearchResults, RpcError>
    {
        Ok(sdk::SearchResults {
            entries: Vec::new(),
            truncated: false,
            next_cursor: None,
        })
    }
}
```

`extensions.search` 需声明 `scope: "local"` 和真实支持的 `modes`（`name`、
`content` 可独立声明），并可设置默认 `priority`。请求携带 `mode`；不支持时应
返回 Unsupported，不要返回空成功结果。搜索结果可逐页返回 `nextCursor`。

协议 4.0 的 `root` 是 opaque ref，不是宿主路径。本地搜索结果可以返回相对于
root 的路径，Core 会解析并拒绝越界；VFS 搜索返回 `relativePath`。不要凭
`params["root"]` 猜宿主路径。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloSearch`。
