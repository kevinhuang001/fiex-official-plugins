# ContextMenus：右键菜单

Core 在右键时调用 `contextMenu.resolve`，用户点击 action 后调用
`contextMenu.execute`。两项事件都由同一个 `ContextMenuProvider` 处理。

## Manifest

```json
"extensions": {
  "contextMenus": {
    "match": {
      "view": {"types": ["fs"]},
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
    fn resolve(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<Vec<sdk::MenuItem>, RpcError> {
        let request = ctx.input()?.context_menu_resolve()
            .ok_or_else(|| RpcError::invalid_params("missing menu request"))?;
        if request.parent_id.as_deref() == Some("example.more") {
            return Ok(vec![sdk::MenuItem::action(
                "example.copy-path",
                "示例：复制路径",
            )]);
        }
        Ok(vec![
            sdk::MenuItem::action("example.inspect", "示例：检查文件"),
            sdk::MenuItem::submenu("example.more", "示例"),
        ])
    }
    fn execute(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::MenuExecuteResult, RpcError> {
        let request = ctx.input()?.context_menu_execute()
            .ok_or_else(|| RpcError::invalid_params("missing menu request"))?;
        match request.item_id.as_deref() {
            Some("example.inspect" | "example.copy-path") => {
                Ok(sdk::MenuExecuteResult::Custom(json!({"ok": true})))
            }
            _ => Err(RpcError::invalid_params("unknown menu item")),
        }
    }
}
```

- 只能是 `action | submenu | separator`；
- 子菜单用 `parentId`，不能嵌套 `children`；
- 菜单查询阶段只有 opaque refs，需要属性时再从 `ctx.targets()` 读。
- 空白处右键应在规则中写 `"selection": "zero"`；默认 `any` 不会加入空选择菜单。
- 多选时每个目标都要满足声明的资源条件。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloMenu`。
