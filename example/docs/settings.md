# Settings：声明式设置与存储

## 原理

Settings 是**声明式**的：Core 根据 manifest 渲染设置项，把值存进插件的命名空间。没有 `settings.resolve` 事件。

```
settings 页
   │  渲染 manifest.settings
   ▼
用户修改
   │
   ▼
Core 存储（按 pluginId 隔离）
   │
   ▼
Rust: ctx.storage_get/set
```

## Manifest

```json
"extensions": {
  "settings": [
    {
      "id": "greeting",
      "label": "问候语",
      "type": "text",
      "defaultValue": "你好"
    },
    {
      "id": "maxItems",
      "label": "最大条目数",
      "type": "number",
      "defaultValue": 100
    },
    {
      "id": "enabled",
      "label": "启用",
      "type": "checkbox",
      "defaultValue": true
    },
    {
      "id": "ignores",
      "label": "忽略",
      "type": "list",
      "defaultValue": [".git"]
    }
  ]
}
```

支持类型：`text`、`number`、`checkbox`、`list`、`secret`。

## Rust 实现

```rust
"example.greet" => {
    let greeting = ctx
        .storage_get("global", "greeting")
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "你好".into());
    Ok(json!({ "message": format!("{greeting}，Fiex！") }))
}
```

写回：

```rust
ctx.storage_set("global", "greeting", json!("嗨"))?;
```

敏感值：

```rust
let token = ctx.secret_get("connection", "token")?;
ctx.secret_set("connection", "token", "…")?;
```

## 坑

- 设置值不通过 Context 自动下发，插件要自己 `storage_get`；
- 存储按插件隔离，`scope` 只是插件内部的命名空间；
- `secret` 存钥匙串，不要写普通 settings 或日志；
- 修改设置后通常需要用户重新触发操作，Core 不会自动重启插件。
