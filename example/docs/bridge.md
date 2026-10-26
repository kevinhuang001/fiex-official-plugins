# UI Bridge：面板页面怎么调 Rust

## 原理

插件 UI 在沙箱 iframe 里，不能直接访问文件系统或网络。它通过 `postMessage` 与 Core 注入的 Bridge 通信。

```
ui/index.html
   │  fiex.bridge.ready
   ▼
Core Bridge（attachFiexPanel）
   │  context.get / theme.get / plugin.invoke / ...
   ▼
Rust dispatch
```

面板只能拿到 Core 投影后的 Context；真实路径和凭据不会下发。

## 握手

面板先发 `fiex.bridge.listening`，Core 注入 `MessagePort` 后发送 `fiex.bridge.init`；页面用收到的 `sessionId`/`nonce` 回 `fiex.bridge.ready`，之后通过 port 发请求。

完整实现见：

- [`hello/ui/index.html`](../hello/ui/index.html)
- [`hello/ui/preview.html`](../hello/ui/preview.html)
- SDK 的 `ui/src/host.ts`

## 允许的方法

- `context.get`
- `theme.get`
- `plugin.invoke`
- `plugin.error`
- `clipboard.writeText`
- `navigation.open`
- `panel.close`
- `panel.resize`

## 坑

- `plugin.invoke` 的 method 不能以 `host.`、`core.`、`fs.`、`connection.` 开头；
- 预览面板不要画自己的标题栏；
- 面板里的任何输入都按不可信处理；
- 完整 Bridge 握手代码见 [`hello/ui/index.html`](../hello/ui/index.html) 和 SDK `ui/src/host.ts`。
