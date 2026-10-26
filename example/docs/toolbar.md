# Toolbar：按钮、命令和面板

## 原理

Toolbar 有两类：

```
toolbar[]
  ├─ panel  → 打开插件 HTML（全宽右侧或浮层）
  └─ command → 只调用 Rust 方法
```

面板通过 UI Bridge 调 Rust；命令直接由 Core 调 `plugin_invoke`。

## Manifest

```json
"extensions": {
  "toolbar": [
    {
      "id": "example.home",
      "title": "示例插件",
      "icon": "puzzle",
      "panel": {
        "entry": "ui/index.html",
        "title": "示例插件",
        "defaultSize": { "width": 420, "height": 520 }
      }
    },
    {
      "id": "example.refresh",
      "title": "示例刷新",
      "icon": "refresh-cw",
      "command": "example.refresh"
    }
  ]
}
```

`entry` 必须是包内单层相对路径；面板页面通过 Bridge 获取 `context.get`、`theme.get`、`plugin.invoke`。

## Rust 实现

```rust
"example.refresh" => Ok(json!({
    "refreshed": true,
    "directory": p.get("root").cloned().unwrap_or(Value::Null)
})),

"example.greet" => {
    let greeting = ctx
        .storage_get("global", "greeting")
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "你好".into());
    Ok(json!({ "message": format!("{greeting}，Fiex！") }))
}
```

面板里：

```js
const ctx = await invoke("context.get");
const result = await invoke("plugin.invoke", {
  method: "example.greet",
  params: {},
});
```

## 坑

- 面板不要自己画标题栏；标题和关闭由 Core 提供；
- `plugin.invoke` 只能调用插件自己的方法，不能调 `host.*`/`core.*`/`fs.*`；
- 面板入口和 `ui/` 资源必须一起打包；
- 命令方法要快，长时间任务应通过状态/事件报告。
