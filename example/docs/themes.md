# Themes：声明式主题包

## 原理

主题不是 native 插件，不需要 Rust、可执行文件或 UI。

```
manifest.json (schemaVersion 2)
      │  extensions.themes[].path
      ▼
theme.json
      │  tokens
      ▼
Core 校验并注入 CSS 变量
```

## Manifest

```json
{
  "schemaVersion": 2,
  "id": "org.example.theme",
  "name": "Example Theme",
  "version": "1.0.0",
  "permissions": {},
  "extensions": {
    "themes": [{ "path": "theme.json" }]
  }
}
```

## theme.json

```json
{
  "id": "example",
  "name": "Example",
  "mode": "light",
  "tokens": {
    "accent": "#2f6fed",
    "surface": "#ffffff",
    "canvas": "#f6f7f9",
    "text": "#1d2430",
    "muted": "#6b7684",
    "radius-row": "8px"
  }
}
```

## 规则

- 只允许 token，不允许 CSS 注入、外链、脚本；
- Core 校验路径、token 名和值；
- 主题包没有 Rust 端代码，也没有 `preview.open`/`info.resolve`；
- 可以直接参考官方 `vista`、`win11` 主题包。
