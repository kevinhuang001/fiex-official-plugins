# Settings：声明式设置与存储

Settings 是声明式的：Core 渲染 manifest 设置项，值存进插件命名空间。没有
`settings.resolve` 事件；Rust 侧用类型化 `ctx.storage()` 读写。

## Manifest

```json
"extensions": {
  "settings": [
    { "id": "greeting", "label": "问候语", "type": "text", "defaultValue": "你好" },
    { "id": "maxItems", "label": "最大条目数", "type": "number", "defaultValue": 100 },
    { "id": "enabled", "label": "启用", "type": "checkbox", "defaultValue": true },
    { "id": "ignores", "label": "忽略", "type": "list", "defaultValue": [".git"] }
  ]
}
```

## Rust 实现

```rust
let storage = ctx.storage().scoped("global");
let greeting: Option<String> = storage.get("greeting")?;
storage.set("greeting", &"嗨".to_string())?;
```

需要原子更新时用 `Storage::update`；需要显式比较时用
`Storage::compare_and_set`。权限按 manifest 的 `storage` 声明。

敏感值用 `ctx.secrets().scoped(scope).get/set`，声明 `secrets` 权限。它区分
`Denied`、`Unavailable`、缺失和数据错误，不要用普通 settings 保存密码。
