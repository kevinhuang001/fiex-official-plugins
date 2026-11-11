# Info：静态字段声明与按需计算

一个 Info Provider 在窗口里对应一个可见 Section。Provider 可以声明多个 `subsections`，但它们只用于匹配资源、组织字段和定位计算请求；用户看不到 Subsection 标题或容器。符合条件的字段按 Manifest 中的声明顺序平铺显示。

Core 在打开 Info 时读取 Manifest 中的静态描述，不会为描述字段调用插件。插件只实现 `info.compute`。`onOpen` 字段由 Core 在打开后请求；`onDemand` 字段只有用户点击“计算”后才会请求。

## Manifest

```json
{
  "extensions": {
    "info": {
      "title": "Example information",
      "priority": 80,
      "match": { "extensions": ["txt", "md"], "kinds": ["file"] },
      "subsections": [
        {
          "id": "details",
          "fields": [
            { "id": "name", "label": "File name", "compute": "onOpen" },
            { "id": "sha256", "label": "SHA-256", "compute": "onDemand" }
          ]
        }
      ]
    }
  }
}
```

`title` is the visible Provider heading. Subsections have an `id`, optional static `match` rules, and fields; they do not have a title. Extension and MIME conditions are case insensitive. Values within a match dimension use OR, while different dimensions use AND. An omitted or empty dimension places no restriction on that dimension.

For example, an audio Provider can declare separate `audio` and `video` subsections. An MP3 matches the audio subsection only, and the Info window still shows one Provider heading. If no subsection matches the trusted resource snapshot, Core omits that Provider instead of displaying an empty heading.

## Rust implementation

The compute request identifies a field by `(subsectionId, fieldId)`. Keep the subsection ID and field ID paired when validating requests and returning results.

```rust
impl sdk::InfoProvider for ExampleInfo {
    fn compute(
        &self,
        ctx: &dyn sdk::ProviderInvocation,
        fields: &[sdk::InfoFieldKey],
    ) -> Result<Vec<sdk::InfoFieldResult>, RpcError> {
        let name = ctx
            .targets()
            .first()
            .and_then(|target| target.metadata.as_ref())
            .map(|metadata| metadata.name.as_str())
            .unwrap_or("unknown");

        fields
            .iter()
            .map(|field| {
                if field.subsection_id != "details" || field.field_id != "name" {
                    return Err(RpcError::invalid_params("unknown Info field"));
                }
                Ok(sdk::InfoFieldResult {
                    subsection_id: field.subsection_id.clone(),
                    field_id: field.field_id.clone(),
                    value: Some(name.to_owned()),
                    unavailable: false,
                })
            })
            .collect()
    }
}
```

Return exactly the requested fields. Return `unavailable: true` with no value when the data cannot be obtained. `InfoFieldKey` and `InfoFieldResult` reject the old `sectionId` wire key. Reading file contents still goes through the authorized `ctx.files()` API after the user requests an `onDemand` field.

Complete working code is in [`hello/src/main.rs`](../hello/src/main.rs).
