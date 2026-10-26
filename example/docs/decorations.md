# Decorations：动态颜色、badge 与字段样式

## 原理

Decorations 是**只读展示层**，不改变文件本身。

```
切换目录
   │
   ▼
Core 收集可见 entries
   │  decorations.match 满足？
   ▼
decorations.resolve({ directory, entries })
   │
   ▼
Core 校验 name → 合并 style/badge
```

只有一个 Provider 能返回某个字段时就用它；多个 Provider 按 `priority` 合并，高优先级覆盖。

## Manifest

```json
"permissions": {
  "folders": { "read": { "enabled": true } }
},
"extensions": {
  "decorations": {
    "match": {
      "extensions": ["md", "txt"],
      "kinds": ["file"]
    },
    "priority": 80
  }
}
```

- **只有动态模式**：没有 `mode`、`method`、`scope`、`rules`；
- 标准方法固定为 `decorations.resolve`，manifest 不写方法名；
- `match.contains` 只属于 decorations，用来判断目录或祖先目录是否包含 `.git` 等；
- `match.extensions/mimeTypes/kinds` 判断“当前目录至少有一个可见成员满足”。

## Rust 实现

```rust
"decorations.resolve" => {
    let mut decorations = Vec::new();
    if let Some(entries) = p.get("entries").and_then(Value::as_array) {
        for entry in entries {
            let Some(name) = entry.get("name").and_then(Value::as_str) else {
                continue;
            };
            if name.ends_with(".md") {
                decorations.push(json!({
                    "name": name,
                    // 文件名单元格的简写
                    "badge": {
                        "text": "MD",
                        "color": "blue",
                        "tooltip": "Markdown"
                    },
                    "style": { "color": "#2f6f9f", "fontWeight": "600" }
                }));
            }
            if name.contains("todo") {
                // 精确到列
                decorations.push(json!({
                    "name": name,
                    "fields": {
                        "modified": { "style": { "color": "#b4443d" } },
                        "size": { "badges": [{ "text": "TODO", "color": "orange" }] }
                    }
                }));
            }
        }
    }
    Ok(json!({ "decorations": decorations }))
}
```

## 响应格式

```json
{
  "decorations": [
    {
      "name": "main.ts",
      "badge": { "text": "M", "color": "orange", "tooltip": "已修改" },
      "style": { "color": "#c2710c", "fontWeight": "600" }
    }
  ]
}
```

- **只返回 `name`**，不要返回绝对路径；Core 在当前目录的 visible entries 里解析；
- 旧 `path` 只是迁移兼容，且必须命中 visible entries；
- `badge.text` 最多 4 个字符；颜色只接受预设名或 `#RGB`/`#RRGGBB`；
- `fields` 键只允许 `name`、`modified`、`size`、`kind`、`row`；`row` 只接受 style；
- 选择状态下的背景由 Core 控制，不要给每一列都刷背景。

## 权限与坑

- 需要 `permissions.folders.read`，否则 Core 直接跳过；
- 请求里带完整 `path` 方便插件查 Git 等内部状态，但**响应只用 name**；
- 不要在 decorations 里做耗时网络请求；它发生在目录切换路径上。
