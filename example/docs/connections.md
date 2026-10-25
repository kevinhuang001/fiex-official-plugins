# Connections：连接入口与两种导航结果

## 原理

`connections` 只负责“用户怎么添加/打开一个连接”。它本身不返回文件列表。

```
用户点 + / 连接项
      │
      ▼
Core 读 extensions.connections 生成表单
      │  connection.validate
      ▼
用户保存
      │  connection.open 或 connection.create
      ▼
NavigationTarget: { kind: "vfs" } | { kind: "pluginView" }
```

- `connection.open`：打开已保存的连接；
- `connection.create`：新建后直接打开；
- 返回 `vfs` 时，Core Explorer 接管文件树；
- 返回 `pluginView` 时，插件 HTML 占满右侧内容区（仍保留标签栏）；
- `mountId` 由 Core 分配，插件不能自己编一个。

## Manifest

```json
"permissions": {
  "metadata": { "enabled": true },
  "folders": { "read": { "enabled": true } },
  "files": { "read": { "enabled": true } }
},
"extensions": {
  "connections": [
    {
      "id": "example-files",
      "title": "示例文件",
      "fields": [
        { "id": "endpoint", "label": "地址", "type": "text", "required": true }
      ]
    },
    {
      "id": "example-account",
      "title": "示例账号",
      "fields": []
    }
  ],
  "vfs": [
    { "id": "example-files", "title": "示例文件", "readOnly": true }
  ]
}
```

字段类型：`text`、`url`、`number`、`checkbox`、`secret`。`secret` 的值进入系统钥匙串，不写进 `connections.json`。

## Rust 实现

```rust
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn run(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let p = ctx.params();
    match ctx.method() {
        "connection.validate" => Ok(json!({ "valid": true })),

        "connection.open" | "connection.create" => {
            let factory = p.get("factoryId").and_then(Value::as_str).unwrap_or("");
            let mount = p.get("mountId").and_then(Value::as_str).unwrap_or("");
            match factory {
                "example-account" => Ok(json!({
                    "kind": "pluginView",
                    "view": { "source": "package", "entry": "ui/account.html" },
                    "state": {
                        "connectionId": p.get("connectionId").cloned().unwrap_or(Value::Null)
                    }
                })),
                _ => Ok(json!({
                    "kind": "vfs",
                    "mountId": mount,
                    "nodeRef": { "mountId": mount, "nodeId": "root" }
                })),
            }
        }
        _ => Err(RpcError::method_not_found(ctx.method())),
    }
}
```

## 校验与坑

- 返回 `pluginView` 的 `entry` 必须是包内单层相对路径，Core 会校验并注入 CSP；
- 返回 `vfs` 必须同时注册 `extensions.vfs`，否则 Core 拒绝；
- 不要在 `connection.open` 里返回文件列表；文件列表走 `fs.*`；
- 连接设置里的密码请声明为 `secret`，不要从 UI 明文传给 `connection.open`；
- 远程域名需要在 `permissions.network.domains` 中声明，Core 在保存连接时校验。
