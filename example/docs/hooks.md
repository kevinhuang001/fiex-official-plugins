# Hooks：生命周期钩子

Core 在文件生命周期事件上调用固定方法 `hooks.invoke`。用 `HookProvider`
注册，不要手写方法分发。

## Manifest

```json
"permissions": {
  "hooks": { "enabled": true }
},
"extensions": {
  "hooks": {
    "event": "beforeOpen",
    "priority": 100,
    "extensions": ["zip"]
  }
}
```

`hooks` 是单个对象，不是数组；不再声明 `id`/`method`。

## Rust 实现

```rust
struct ExampleHooks;

impl sdk::HookProvider for ExampleHooks {
    fn invoke(&self, ctx: &sdk::RequestContext) -> Result<Value, RpcError> {
        if ctx.params()["event"].as_str() != Some("beforeOpen") {
            return Ok(json!({"handled": false}));
        }
        // 在这里消费打开操作，或返回 handled:false 交给 Core 默认流程。
        Ok(json!({"handled": false}))
    }
}
```

`handled: true` 表示插件接管；`false` 表示继续默认流程。需要预览时返回
`{"handled": true, "preview": true}`。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloHooks`。
