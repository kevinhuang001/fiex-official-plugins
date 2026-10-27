# Info：显示简介

Info 只在用户点击“显示简介”时调用，固定事件为 `info.resolve`。

## Manifest

```json
"permissions": {
  "metadata": { "enabled": true },
  "files": { "read": { "enabled": true } }
},
"extensions": {
  "info": {
    "match": { "extensions": ["txt", "md"], "kinds": ["file"] },
    "priority": 80
  }
}
```

`info` 是单个对象，不是数组。

## Rust 实现

```rust
struct ExampleInfo;

impl sdk::InfoProvider for ExampleInfo {
    fn resolve(&self, ctx: &sdk::RequestContext)
        -> Result<Vec<sdk::InfoSection>, RpcError>
    {
        let name = ctx.targets().first()
            .and_then(|target| target.metadata.as_ref())
            .map(|meta| meta.name.as_str())
            .unwrap_or("unknown");
        Ok(vec![sdk::InfoSection {
            source: String::new(),
            provider: String::new(),
            plugin_id: None,
            id: "example".into(),
            title: "示例信息".into(),
            rows: vec![sdk::InfoRow {
                label: "文件".into(),
                value: name.into(),
                copyable: true,
            }],
        }])
    }
}
```

不要手写 `match ctx.method()`。需要读内容时使用
`ctx.files().read_to_string(&target.reference)`，不要传宿主路径。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloInfo`。
