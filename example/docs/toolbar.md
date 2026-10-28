# Toolbar：按钮、命令和面板

Toolbar 有两种形态：`panel` 打开插件 HTML；没有 panel 时 Core 固定调用
`toolbar.invoke`，由 `ToolbarProvider` 处理。

## Manifest

```json
"extensions": {
  "toolbar": {
    "title": "示例插件",
    "icon": "puzzle",
    "panel": {
      "entry": "ui/index.html",
      "title": "示例插件",
      "defaultSize": { "width": 460, "height": 340 }
    }
  }
}
```

`toolbar` 是单个对象，不是数组。

## Rust 实现

有 panel 时，面板通过 UI Bridge 调 `plugin.invoke`；没有 panel 时实现
`ToolbarProvider`：

```rust
struct ExampleToolbar;

impl sdk::ToolbarProvider for ExampleToolbar {
    fn invoke(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        Ok(json!({
            "refreshed": true,
            "directory": ctx.params().get("root").cloned().unwrap_or(Value::Null)
        }))
    }
}
```

面板里的常用调用：

```js
await fiex.plugin.invoke('example.greet')
await fiex.clipboard.writeText('copied')
await fiex.navigation.open('fiex://mount/demo/')
```

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloToolbar`。
