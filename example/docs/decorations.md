# Decorations：动态颜色、badge 与字段样式

Decorations 是只读展示层。Core 在目录变化后调用固定事件
`decorations.resolve`；manifest 只声明统一 `match` 规则和 `priority`。

## Manifest

```json
"permissions": {
  "folders": { "read": { "enabled": true } }
},
"extensions": {
  "decorations": {
    "match": {
      "view": {"types": ["fs"]},
      "extensions": ["md", "txt"],
      "kinds": ["file"]
    },
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
                value.fields.insert("name".into(), sdk::DecorationField {
                    badges: vec![sdk::DecorationBadge {
                        text: "MD".into(),
                        color: "blue".into(),
                        tooltip: Some("Markdown".into()),
                    }],
                    style: sdk::DecorationStyle {
                        color: Some("#2f6f9f".into()),
                        font_weight: Some("600".into()),
                        ..Default::default()
                    },
                });
                values.push(value);
            }
        }
        Ok(values)
    }
}
```

返回项必须用 `ref` 关联本次请求中的 `ResourceRef`；Core 只接受当前可见目标，
不按文件名或插件提供的路径猜测目标。列样式和徽标都放在
`fields.name/modified/size/kind/row`。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloDecorations`。
