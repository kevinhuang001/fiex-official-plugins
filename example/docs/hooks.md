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
    "match": {"extensions": ["zip"], "kinds": ["file"]}
  }
}
```

`hooks` 是单个对象，不是数组；不再声明 `id`/`method`。

## Rust 实现

```rust
struct ExampleHooks;

impl sdk::HookProvider for ExampleHooks {
    fn invoke(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::HookResult, RpcError> {
        let request = ctx.input()?.hook_request()
            .ok_or_else(|| RpcError::invalid_params("missing hook request"))?;
        if request.event != "beforeOpen" {
            return Ok(sdk::HookResult { handled: false });
        }
        // 在这里消费打开操作，或返回 handled:false 交给 Core 默认流程。
        Ok(sdk::HookResult { handled: false })
    }
}
```

`handled: true` 表示插件接管；`false` 表示继续默认流程。Hook 响应只有
`handled` 字段；预览由 `PreviewProvider` 单独负责。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloHooks`。
