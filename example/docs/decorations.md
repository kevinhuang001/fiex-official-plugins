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
    fn resolve(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<Vec<sdk::DecorationContribution>, RpcError> {
        let mut values = Vec::new();
        for target in ctx.targets() {
            let Some(meta) = target.metadata.as_ref() else { continue };
            if meta.name.ends_with(".md") {
                let mut value = sdk::DecorationContribution::new(target.reference.clone());
                value.badges.push(sdk::DecorationBadge {
                    text: "MD".into(), color: "blue".into(), tooltip: Some("Markdown".into()),
                });
                value.style.color = Some("#2f6f9f".into());
                value.style.font_weight = Some("600".into());
                values.push(value);
            }
        }
        Ok(values)
    }
}
```

每项通过 Core 提供的 opaque `ResourceRef` 指向目标；插件不返回路径或名称作为目标。字段级样式用
`fields.modified/size/kind/row`，规则不变。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloDecorations`。
