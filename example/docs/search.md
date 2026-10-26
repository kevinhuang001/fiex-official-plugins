# Search：搜索来源

## 原理

Core 把搜索分发到已注册的 Provider，每个 Provider 返回一个受 `root` 约束的 SearchResponse。

```
搜索框
  │
  ▼
search.query({ query, root, hidden, ignores, limit })
  │
  ▼
{ entries: Entry[], truncated }
```

Provider 之间互不影响；一个失败不会吞掉其他来源。

## Manifest

```json
"permissions": {
  "folders": { "read": { "enabled": true } }
},
"extensions": {
  "search": [
    {
      "id": "example.search",
      "title": "示例搜索",
      "icon": "magnifying-glass",
      "command": "search.query"
    }
  ]
}
```

`command` 可省略，默认 `search.query`；要自定义方法名就写在这里。

## Rust 实现

```rust
"search.query" => {
    let query = p.get("query").and_then(Value::as_str).unwrap_or("");
    let root = p.get("root").and_then(Value::as_str).unwrap_or("");
    let limit = p
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .min(500) as usize;

    // 这里替换成真正的索引查询；必须限制在 root 内。
    let entries = if query.is_empty() {
        Vec::new()
    } else {
        vec![json!({
            "path": format!("{root}/example-result.txt"),
            "name": "example-result.txt",
            "isDir": false,
            "isSymlink": false,
            "size": 12,
            "modified": 0,
            "created": 0,
            "extension": "txt",
            "mime": "text/plain",
            "readonly": false,
            "linkTarget": null,
            "tags": [],
            "sync": null
        })]
    };
    Ok(json!({
        "entries": entries.into_iter().take(limit).collect::<Vec<_>>(),
        "truncated": false
    }))
}
```

## 返回字段

每项必须是完整 Entry；最关键的字段：

- `path`、`name`；
- `isDir`、`isSymlink`；
- `size`、`modified`、`created`；
- `extension`、`mime`；
- `readonly`、`linkTarget`、`tags`、`sync`。

## 坑

- 结果必须位于 `root` 内，Core 会拒绝越界路径；
- 不要返回超过 `limit` 的条目；
- 索引/数据库查询放在插件进程内，别阻塞 Core；
- 需要文件内容搜索时先确认 `files.read` 权限。
