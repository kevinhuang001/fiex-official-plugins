# Connections：连接入口与两种导航结果

## 原理

`ConnectionProvider` 只负责验证和打开/创建连接；文件列表与读写由插件的
`VfsProvider` 提供。Core 通过同一插件注册的 VFS mount 处理文件访问。

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

- 返回 `vfs` 时，Core Explorer 接管文件树；
- 返回 `pluginView` 时，插件 HTML 占满右侧内容区；
- `mountId` 由 Core 分配，插件不能自己编。

## Manifest

```json
"permissions": {
  "metadata": { "enabled": true },
  "folders": { "read": { "enabled": true } },
  "files": { "read": { "enabled": true } }
},
"extensions": {
  "connections": {
    "title": "示例文件",
    "fields": [
      { "id": "server", "label": "地址", "type": "text", "required": true }
    ]
  },
  "vfs": { "title": "示例文件", "readOnly": true }
}
```

`vfs` **不是**数组；`connections` 是单个对象。连接工厂若要显示文件树，必须
同时声明 `extensions.vfs`。

## Rust 实现

只实现 `ConnectionProvider`，不要手写 `match ctx.method()`：

```rust
struct ExampleConnection;

impl sdk::ConnectionProvider for ExampleConnection {
    fn validate(&self, _ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::ConnectionValidation, RpcError> {
        Ok(sdk::ConnectionValidation::valid())
    }
    fn open(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::ConnectionOpenResult, RpcError> {
        let request = ctx.input()?.connection_request()
            .ok_or_else(|| RpcError::invalid_params("missing connection request"))?;
        let factory = request.factory_id.as_deref().unwrap_or("");
        if factory == "example-account" {
            return Ok(sdk::ConnectionOpenResult::PluginView {
                view: sdk::PackageView {
                    source: "package".into(),
                    entry: "ui/account.html".into(),
                },
                state: json!({"connectionId": request.connection_id}),
                tab_title: None,
            });
        }
        let mount_id = request.mount_id.as_deref()
            .ok_or_else(|| RpcError::invalid_params("missing mountId"))?;
        Ok(sdk::ConnectionOpenResult::Vfs {
            mount_id: mount_id.into(),
            node_ref: sdk::ResourceRef {
                mount_id: mount_id.into(),
                node_id: "root".into(),
            },
        })
    }
    fn create(&self, ctx: &dyn sdk::ProviderInvocation) -> Result<sdk::ConnectionOpenResult, RpcError> {
        self.open(ctx)
    }
}
```

连接返回 `vfs` 导航结果后，Core 通过 `VfsProvider` 的强类型 `list`、`stat`、
`read` 等方法访问挂载内容。SDK 负责将流映射到 `vfs.*` RPC。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloConnection`。
