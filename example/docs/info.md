# Info：声明字段并按需计算

Info 使用两个标准事件：`info.describe` 在打开窗口时声明当前资源适用的字段；`info.compute` 只计算 Core 请求的字段。打开窗口不会自动读取文件内容或计算哈希。

## Manifest

```json
"extensions": {
  "info": {
    "match": { "extensions": ["txt", "md"], "kinds": ["file"] },
    "priority": 80
  }
}
```

描述阶段无需文件内容读取权限。需要读取内容的字段在用户主动请求计算后，仍通过 `ctx.files()` 执行，并继续受到 Core 权限检查。

## Rust 实现

```rust
impl sdk::InfoProvider for ExampleInfo {
    fn describe(
        &self,
        _ctx: &dyn sdk::ProviderInvocation,
    ) -> Result<Vec<sdk::InfoSectionDescriptor>, RpcError> {
        Ok(vec![sdk::InfoSectionDescriptor {
            id: "example".into(),
            title: "示例信息".into(),
            fields: vec![sdk::InfoFieldDescriptor {
                id: "name".into(),
                label: "文件名".into(),
                compute: sdk::InfoCompute::OnOpen,
            }],
        }])
    }

    fn compute(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
        fields: &[sdk::InfoFieldKey],
    ) -> Result<Vec<sdk::InfoFieldResult>, RpcError> {
        let name = ctx.targets().first()
            .and_then(|target| target.metadata.as_ref())
            .map(|metadata| metadata.name.as_str())
            .unwrap_or("unknown");
        fields.iter().map(|field| {
            if field.section_id != "example" || field.field_id != "name" {
                return Err(RpcError::invalid_params("unknown Info field"));
            }
            Ok(sdk::InfoFieldResult {
                section_id: field.section_id.clone(),
                field_id: field.field_id.clone(),
                value: Some(name.to_string()),
                unavailable: false,
            })
        }).collect()
    }
}
```

使用 `onOpen` 的字段会在窗口打开后自动请求；`onDemand` 字段显示计算入口，只有用户点击时才会提交对应字段 ID。计算结果统一是展示文本，复制由 Core UI 提供。

完整实现见 [`hello/src/main.rs`](../hello/src/main.rs) 的 `HelloInfo`。
