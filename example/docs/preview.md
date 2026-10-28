# Preview：预览

Preview 在用户点击“快速查看”时调用，固定事件为 `preview.open`。

## Manifest

```json
"permissions": {
  "metadata": { "enabled": true },
  "files": { "read": { "enabled": true } }
},
"extensions": {
  "preview": {
    "match": { "extensions": ["txt", "md"], "kinds": ["file"] },
    "priority": 60,
    "maxBytes": 1048576
  }
}
```

`preview` 是单个对象。页面入口由返回值决定，manifest 不写 `panel`/`method`。

## Rust 实现

```rust
struct ExamplePreview;

impl sdk::PreviewProvider for ExamplePreview {
    fn open(&self, _ctx: &sdk::RequestContext)
        -> Result<sdk::PreviewView, RpcError>
    {
        Ok(sdk::PreviewView::html("ui/preview.html"))
    }
}
```

文本预览：

```rust
Ok(sdk::PreviewView {
    kind: "text".into(),
    view: None,
    text: Some("预览内容".into()),
    message: None,
    state: Value::Null,
})
```

需要读取文件时用 `ctx.targets()` 的 ref 和 `ctx.files()`；不要把路径塞进
PreviewView。预览页面调用 `fiex.preview.ready()` 通知 Core 首屏完成。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloPreview`。
