# Decorations：动态颜色、badge 与字段样式

Decorations 是只读展示层。Core 在目录变化后调用固定事件
`decorations.resolve`；manifest 只声明 `match`/`priority`。

## Manifest

```json
"permissions": {
  "folders": { "read": { "enabled": true } }
},
"extensions": {
  "decorations": {
    "match": { "extensions": ["md", "txt"], "kinds": ["file"] },
    "priority": 80
  }
}
```

`decorations` 是单个对象，不是数组。

## Rust 实现

```rust
struct ExampleDecorations;

impl sdk::DecorationProvider for ExampleDecorations {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<Value>, RpcError> {
        let mut values = Vec::new();
        for target in ctx.targets() {
            let Some(meta) = target.metadata.as_ref() else { continue };
            if meta.name.ends_with(".md") {
                values.push(json!({
                    "name": meta.name,
                    "badge": {"text": "MD", "color": "blue", "tooltip": "Markdown"},
                    "style": {"color": "#2f6f9f", "fontWeight": "600"}
                }));
            }
        }
        Ok(values)
    }
}
```

返回项只需要 `name`；Core 在当前目录可见成员里解析。字段级样式用
`fields.modified/size/kind/row`，规则不变。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloDecorations`。
