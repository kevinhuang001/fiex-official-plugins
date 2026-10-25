# VFS：把远程文件树接进 Core Explorer

## 原理

VFS 是 Core 与插件之间的**节点协议**，不是真实挂载。

```
Explorer 切到 fiex://mount/<id>/
      │
      ▼
Core 调 fs.list / fs.stat / fs.readSmall ...
      │
      ▼
插件返回条目、节点 ID、revision、内容
```

Core 负责路径拼接、权限、缓存、离线副本、拖拽和本地文件操作；插件只处理自己协议里的节点。

## Manifest

```json
"extensions": {
  "vfs": [
    { "id": "example-files", "title": "示例文件", "readOnly": true }
  ]
}
```

## Rust 实现

```rust
"fs.list" => Ok(json!({
    "entries": [
        {
            "name": "README.md",
            "nodeId": "readme",
            "isDir": false,
            "size": 14,
            "modified": 0,
            "revision": "1"
        },
        {
            "name": "docs",
            "nodeId": "docs",
            "isDir": true,
            "size": 0,
            "modified": 0,
            "revision": "1"
        }
    ],
    "nextCursor": null
})),

"fs.stat" => Ok(json!({ "revision": "1", "size": 14 })),

"fs.readSmall" => Ok(json!({
    "base64": "aGVsbG8gZXhhbXBsZQo=", // "hello example\n"
    "eof": true
})),

"fs.openRead" => Ok(json!({ "streamId": "example-read" })),
"fs.readStream" => Ok(json!({
    "base64": "aGVsbG8gZXhhbXBsZQo=",
    "eof": true
})),
"fs.closeRead" => Ok(Value::Null),
```

## 返回字段

`fs.list.entries[]`：

| 字段                | 说明                         |
| ------------------- | ---------------------------- |
| `name`              | 当前目录内的名字，不能含 `/` |
| `nodeId`            | 插件自己的稳定节点 ID        |
| `isDir`             | 是否目录                     |
| `size` / `modified` | 可选元数据                   |
| `revision`          | 可选，用于缓存失效           |

分页用 `nextCursor`；Core 会带着它继续请求。

## 写操作

只有在 manifest 里批准对应权限，Core 才会调用：

- `fs.beginWrite` / `fs.writeChunk` / `fs.commit` / `fs.abort`；
- `fs.mkdir`；
- `fs.remove` / `fs.rename`。

**不要**用 `fs.readSmall` 读大文件；它只适合小内容。大文件走 `openRead`/`readStream`。

## 权限

- 列目录：`folders.read`；
- 读内容：`files.read`；
- 写/建/删/改名：分别申请对应权限；
- VFS 节点的真实身份由 Core 的 Scope 约束，插件不能接受任意绝对路径。
