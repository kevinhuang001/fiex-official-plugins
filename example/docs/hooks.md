# Hooks：生命周期钩子

## 原理

Hooks 允许插件在 Core 执行文件操作前介入，例如“先解压再打开”。

```
Core 准备打开文件
      │
      ▼
hooks.invoke(event, resourceRef, operationSession)
      │
      ├─ handled: false → Core 继续默认流程
      └─ handled: true  → 插件接管
```

当前分支状态：Core 已校验 `permissions.hooks` 和 `extensions.hooks` 注册，但**文件生命周期里的自动 `hooks.invoke` 接线还没完成**。下面的代码是目标合同，接线前不会被自动调用。

## Manifest

```json
"permissions": {
  "hooks": { "enabled": true }
},
"extensions": {
  "hooks": [
    {
      "id": "example.beforeOpen",
      "event": "beforeOpen",
      "method": "example.beforeOpen",
      "priority": 100,
      "match": { "extensions": ["zip"], "kinds": ["file"] }
    }
  ]
}
```

- `event`：`beforeOpen` 等生命周期事件；
- `method`：插件自己的 handler；
- `priority`：多个 hook 的执行顺序；
- `match`：可选，不满足就不调用。

## Rust 实现

```rust
"example.beforeOpen" => {
    let name = p
        .pointer("/resource/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    if name.ends_with(".zip") {
        // 目标合同：返回 handled=true 表示插件接管；
        // 真实实现应通过 ctx.files()/ctx.process_run() 完成操作。
        return Ok(json!({ "handled": false }));
    }
    Ok(json!({ "handled": false }))
}
```

## 坑

- 不要在没有 `handled: true` 的情况下偷偷执行副作用；
- 当前 Core 不会自动调用，不要依赖它做安全拦截；
- 需要读取/写入时严格按权限申请；
- 多个 hook 的先后顺序由 priority 决定，但不要假设一定串行执行重活。
