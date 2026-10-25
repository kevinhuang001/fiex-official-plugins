# Preview：预览

## 原理

Preview 也只在用户点击“快速查看”时调用。每个插件最多注册**一个** preview 对象。

```
用户点“快速查看”
      │
      ▼
Core 构造 PreviewContext（单个资源）
      │  match 命中？
      ▼
preview.open
      │
      ▼
PreviewView: html | text | unsupported
```

`html` 文档被 Core 放进沙箱 iframe；标题栏、关闭、左右切换由 Core 提供，插件页面只画内容。

## Manifest

```json
"permissions": {
  "metadata": { "enabled": true },
  "files": { "read": { "enabled": true } }
},
"extensions": {
  "preview": {
    "id": "example.preview",
    "match": {
      "extensions": ["txt", "md"],
      "mimeTypes": ["text/*"],
      "kinds": ["file"]
    },
    "priority": 60,
    "maxBytes": 1048576
  }
}
```

- 不写 `panel`，页面入口由 `preview.open` 返回值决定；
- 不写 `method`，标准方法固定为 `preview.open`；
- manifest 的 preview 对象没有 `title`；来源选择器显示插件名或“Fiex 内置”。

## Rust 实现

```rust
"preview.open" => Ok(json!({
    "kind": "html",
    "view": {
        "source": "package",
        "entry": "ui/preview.html"
    }
})),
```

文本型预览：

```rust
"preview.open" => Ok(json!({
    "kind": "text",
    "text": "第一行\n第二行"
})),
```

无法处理：

```rust
"preview.open" => Ok(json!({
    "kind": "unsupported",
    "message": "这个格式需要安装 XX 插件"
})),
```

## 页面

`ui/preview.html` 通过 UI Bridge 读取 `context.get`：

```js
const context = await invoke("context.get");
const name = context.resource?.metadata?.name;
```

- 不要自己画标题栏、关闭按钮；
- 可用 tabs/toolbar，但不要重复 Core 的文件头；
- 需要读文件时用 `context.resource.read.resourceId` + `ctx.files().open_resource(...)`，或 `metadata.name` 在 operation scope 内读；
- HTML 不能引用包外资源，Core 会校验入口并注入 CSP。

## 权限与坑

- `metadata` 决定 Context 里是否有属性；`files.read` 决定是否有 scoped read；
- 大文件不要塞进 `text`（上限 256 KiB）；
- 同一个插件只注册一个 preview，避免来源列表出现多个无法区分的选项。
