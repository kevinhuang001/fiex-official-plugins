# ContextMenus：右键菜单

## 原理

菜单是**动态**的：Core 不读 manifest 里的静态菜单项，只在右键时调用插件。

```
右键选择
   │  contextMenus.match（selection / extensions / mimeTypes / kinds）
   ▼
contextMenu.resolve → { items }  （仅顶层）
   │  用户打开 submenu
   ▼
contextMenu.resolve(parentId) → { items } （仅直接子项）
   │  用户点 action
   ▼
contextMenu.execute(itemId) → 结果
```

每个插件最多注册一个 `contextMenus` 配置（`true` 或 `{match}`）。

## Manifest

```json
"permissions": {
  "metadata": { "enabled": true }
},
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

- `selection`：`single` / `multi` / `any`，只在 contextMenus 合法；
- `extensions/mimeTypes/kinds` 对选中集合里的任意一项判断；
- 不写 `match` 或 `true` 表示所有选择都调用。

## Rust 实现

```rust
"contextMenu.resolve" => {
    if p.get("parentId").and_then(Value::as_str) == Some("example.more") {
        return Ok(json!({ "items": [
            {"id": "example.copy-path", "title": "示例：复制路径", "kind": "action"}
        ]}));
    }
    Ok(json!({ "items": [
        {"id": "example.inspect", "title": "示例：检查文件", "kind": "action"},
        {"id": "example.more", "title": "示例", "kind": "submenu"}
    ]}))
},

"contextMenu.execute" => {
    let item = p.get("itemId").and_then(Value::as_str).unwrap_or("");
    if !matches!(item, "example.inspect" | "example.copy-path") {
        return Err(RpcError::invalid_params("unknown menu item"));
    }
    Ok(json!({ "ok": true, "itemId": item }))
}
```

## 菜单项规则

- `kind` 只能是 `action`、`submenu`、`separator`；
- Root 只返回顶层项；子菜单展开时再次调用相同 `contextMenu.resolve`，通过 `params.parentId` 标识展开项；
- 子菜单响应只返回直接子项，`parentId` 由 Core 注入；不能返回静态 `children`；
- 不能写静态 `when` 条件；需要条件就在 `resolve` 里判断；
- `action` 的 id 必须唯一；`submenu` 不能被执行；
- 菜单请求可能只带 opaque refs，需要属性时按权限申请。

## 坑

- 菜单查询要快，不要做网络请求；
- 多选时注意 `selection` 和 Core 的选中集合；
- 内置的“快速查看/显示简介”只对单选显示，插件的菜单由自己的 match 决定。
