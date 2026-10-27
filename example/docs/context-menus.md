# ContextMenus：右键菜单

Core 在右键时调用 `contextMenu.resolve`，用户点击 action 后调用
`contextMenu.execute`。两项事件都由同一个 `ContextMenuProvider` 处理。

## Manifest

```json
"extensions": {
  "contextMenus": {
    "match": {
      "selection": "single",
      "extensions": ["txt", "md"],
      "kinds": ["file"]
    }
  }
}
```

`contextMenus` 是单个对象（或 `true`），不是数组。

## Rust 实现

```rust
struct ExampleMenu;

impl sdk::ContextMenuProvider for ExampleMenu {
    fn resolve(&self, ctx: &sdk::RequestContext) -> Result<Vec<Value>, RpcError> {
        if ctx.params()["parentId"].as_str() == Some("example.more") {
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
        match ctx.params()["itemId"].as_str() {
            Some("example.inspect" | "example.copy-path") => {
                Ok(json!({"ok": true}))
            }
            _ => Err(RpcError::invalid_params("unknown menu item")),
        }
    }
}
```

- 只能是 `action | submenu | separator`；
- 子菜单用 `parentId`，不能嵌套 `children`；
- 菜单查询阶段只有 opaque refs，需要属性时再从 `ctx.targets()` 读。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloMenu`。
