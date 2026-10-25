# Info：显示简介

## 原理

Info 只在用户点击“显示简介”时调用，不会在目录列表阶段执行。

```
用户点“显示简介”
      │
      ▼
Core 构造 EventContext（targets 数组）
      │  match 命中？
      ▼
info.resolve
      │
      ▼
InfoResponse.sections → Core 合并 core.metadata
```

每个插件最多注册**一个** info 对象，不是数组。

## Manifest

```json
"permissions": {
  "metadata": { "enabled": true },
  "files": { "read": { "enabled": true } }
},
"extensions": {
  "info": {
    "match": {
      "extensions": ["txt", "md"],
      "mimeTypes": ["text/*"],
      "kinds": ["file"]
    },
    "priority": 80
  }
}
```

`match` 是包含谓词；不匹配时 Core 不会调用插件。`contains`/`selection` 不允许出现在 info。

## Rust 实现

```rust
"info.resolve" => {
    let meta = ctx.targets().first().and_then(|target| target.metadata.as_ref());
    let name = meta.map(|value| value.name.as_str()).unwrap_or("unknown");
    let extension = meta.and_then(|value| value.extension.as_deref()).unwrap_or("");
    let greeting = ctx
        .storage_get("global", "greeting")
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "你好".into());

    Ok(json!({
        "sections": [{
            "id": "example",
            "title": "示例信息",
            "rows": [
                { "label": "文件", "value": name },
                { "label": "扩展名", "value": extension },
                { "label": "问候语", "value": greeting },
                { "label": "可复制", "value": "这一行可以复制", "copyable": true }
            ]
        }]
    }))
}
```

## 返回格式

```json
{
  "sections": [
    {
      "id": "example",
      "title": "示例信息",
      "rows": [
        { "label": "文件", "value": "notes.md" },
        { "label": "SHA-256", "value": "…", "copyable": true }
      ]
    }
  ]
}
```

- `section.id` 在插件内唯一，Core 显示为 `<pluginId>.<id>`；
- `copyable: true` 让 Core 在值右边显示复制按钮；
- `title` 用于 section 标题，不是插件/贡献标题；manifest 的 info 对象没有 `title`；
- 没有 `files.read` 时不要调用读文件 API。

## 权限与坑

- `metadata` 只提供属性；要读内容再加 `files.read`；
- 只针对用户选中的单个资源评估，多选不会逐个调用；
- 内置 `core.metadata` 和音频/视频 section 由 Core 自己生成，插件不要重复。
