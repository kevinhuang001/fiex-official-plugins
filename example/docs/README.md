# 扩展点手册

每个扩展点一篇，按“原理 → manifest → Rust 实现 → 返回/校验 → 权限/坑”组织。

| 文档                                 | 扩展点                       | 注册位置                  |
| ------------------------------------ | ---------------------------- | ------------------------- |
| [connections.md](connections.md)     | 连接入口与 `connection.open` | `extensions.connections`  |
| [vfs.md](vfs.md)                     | 虚拟文件树                   | `extensions.vfs`          |
| [decorations.md](decorations.md)     | 装饰、颜色、badge            | `extensions.decorations`  |
| [info.md](info.md)                   | 显示简介                     | `extensions.info`         |
| [preview.md](preview.md)             | 预览                         | `extensions.preview`      |
| [context-menus.md](context-menus.md) | 右键菜单                     | `extensions.contextMenus` |
| [search.md](search.md)               | 搜索来源                     | `extensions.search`       |
| [settings.md](settings.md)           | 插件设置                     | `extensions.settings`     |
| [hooks.md](hooks.md)                 | 生命周期钩子                 | `extensions.hooks`        |
| [toolbar.md](toolbar.md)             | 工具栏按钮/面板              | `extensions.toolbar`      |
| [themes.md](themes.md)               | 声明式主题                   | `extensions.themes`       |
| [bridge.md](bridge.md)               | 面板 UI ↔ Rust               | 不是扩展点，是 UI Bridge  |

所有示例都来自 [`hello/src/main.rs`](../hello/src/main.rs) 和 [`hello/manifest.json`](../hello/manifest.json)。先读 [`../README.md`](../README.md) 的“Rust 端：从 main 到 dispatch”。
