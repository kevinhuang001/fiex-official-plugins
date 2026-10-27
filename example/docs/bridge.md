# UI Bridge：面板页面怎么调 Rust

插件 UI 在沙箱 iframe 里，通过 Core 注入的 MessagePort 通信。标准事件由 Rust
侧的 Provider 处理；面板只需要调用 `plugin.invoke` 或 UI SDK 的类型化方法。

## 握手

页面加载后发 `fiex.bridge.listening`，Core 发 `fiex.bridge.init` 并转移
`MessagePort`；页面回 `fiex.bridge.ready`，之后发请求。

## UI SDK

```ts
import { FiexUiClient } from '@fiex/ui-sdk'
const fiex = new FiexUiClient()
await fiex.ready()
const context = await fiex.context.get()
await fiex.plugin.invoke('example.greet')
await fiex.clipboard.writeText('copied')
await fiex.navigation.open('fiex://mount/demo/')
const task = await fiex.plugin.task('task-id')
```

允许的方法由 Core 按面板类型限制：

- 普通 panel：`context.get`、`theme.get`、`plugin.invoke`、`plugin.task`、
  `plugin.error`、`clipboard.writeText`、`navigation.open`、`dialog.*`、
  `tasks.*`、`panel.close`、`panel.resize`
- preview：额外允许 `preview.ready`
- connection view：`context.get`、`theme.get`、`plugin.invoke`、
  `navigation.open`、`plugin.error`

UI SDK 不再提供 `fiex.vfs.*`：VFS 是 native Provider 的职责。

## 坑

- `plugin.invoke` 的 method 不能以 `host.`、`core.`、`fs.`、`connection.`、
  `vfs.` 开头；
- 预览页面不要画标题栏；
- 面板输入一律按不可信处理；
- 完整握手实现见 [`hello/ui/index.html`](../hello/ui/index.html) 和 SDK
  `ui/src/host.ts`。
