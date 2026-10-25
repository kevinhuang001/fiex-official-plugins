# 官方插件示例：Connections / Decorations / Toolbar 新手手册

这个目录是一份**可以直接编译、安装、改着玩**的插件示例，重点是三件事：

1. **Connection**：连接入口怎么声明，`connection.open` 怎么返回 VFS 或全宽 `pluginView`。
2. **Decoration**：怎么给文件名、时间、大小、种类加颜色和 badge；静态规则和动态事件各怎么写。
3. **Toolbar**：工具栏按钮怎么声明成面板或命令，面板里的 UI 怎么和 native 进程通信。

示例插件在 [`hello/`](hello/)，包 ID 是 `org.example.hello`。

> 所有路径都相对于 `fiex-official-plugins` 仓库根目录。示例代码故意不访问用户文件和网络，方便先跑通再扩展。

---

## 1. 先跑起来

```sh
# 编译 native 进程
cargo build --manifest-path example/hello/Cargo.toml

# 跑 SDK 自带的三项检查（进程握手、调用、退出）
python sdk/rust/examples/check_plugin.py \
  --binary example/hello/target/debug/fiex-plugin-example \
  --plugin-id org.example.hello \
  --method example.greet
```

手动安装时，把 `manifest.json`、编译出的可执行文件和 `ui/` 目录放在同一个插件目录里，例如：

```text
org.example.hello/
  manifest.json
  fiex-plugin-example
  ui/
    index.html
    account.html
```

然后放到 Fiex 的插件目录（开发时可用 `FIEX_PLUGIN_DIR` 指向它），在设置里启用并逐项批准权限。

发布时遵循仓库根目录 [`README.md`](../README.md) 的 `.fiexplugin` 打包约定；示例没有加入正式目录，不会被发布脚本自动打包。

---

## 2. Manifest 最小结构

```json
{
  "schemaVersion": 2,
  "id": "org.example.hello",
  "name": "示例插件",
  "version": "0.1.0",
  "runtime": {
    "protocolVersion": "3.0",
    "entrypoints": {
      "windows-x86_64": "fiex-plugin-example.exe",
      "macos-aarch64": "fiex-plugin-example",
      "macos-x86_64": "fiex-plugin-example",
      "linux-x86_64": "fiex-plugin-example"
    }
  },
  "permissions": {},
  "extensions": {}
}
```

规则：

- `extensions` 是**唯一的扩展注册容器**；不要再用 `contributions`。
- `permissions` 是**唯一的授权声明位置**；注册扩展不等于授权。
- 每个权限项都是 `{ "enabled": boolean }` 对象，不是裸 boolean。
- 插件被安装后默认未启用；用户必须批准 `permissions` 中声明的每一项。
- `runtime.entrypoints` 只写当前平台的相对可执行文件，不能是绝对路径。

示例申请的权限：

```json
"permissions": {
  "metadata": { "enabled": true },
  "folders": { "read": { "enabled": true } },
  "files": { "read": { "enabled": true } }
}
```

`metadata` 只给属性，不等于能读文件内容；`folders.read` 才能列目录；`files.read` 才能读字节。需要写文件时再分别申请 `files.write` / `files.create` / `files.delete` / `files.rename`。

---

## 3. Connections：两种导航目标，没有第三种

Manifest 示例：

```json
"extensions": {
  "connections": [
    {
      "id": "example-files",
      "title": "示例文件",
      "fields": [
        { "id": "server", "label": "服务器地址", "type": "text", "required": true }
      ]
    },
    {
      "id": "example-account",
      "title": "示例账号",
      "fields": [
        { "id": "account", "label": "账号", "type": "text", "required": true },
        { "id": "password", "label": "密码", "type": "secret", "required": true }
      ]
    }
  ],
  "vfs": [
    { "id": "example-files", "title": "示例文件", "readOnly": true }
  ]
}
```

要点：

- **想显示文件树，必须同时声明 `extensions.connections` 和 `extensions.vfs`。**
- 只做账号授权/配置页的连接，不需要 `vfs`。
- 字段类型只支持 `text | url | number | checkbox | secret`。`secret` 的值不会进普通 settings，而是进系统钥匙串。
- 连接插件有网络访问时，要声明 `permissions.network.enabled` 和精确的 `domains`；localhost/IP 也需要用户逐项批准。
- 插件不要自己拼 `fiex://mount/...`。`mountId` 由 Core 分配，插件只使用 Core 传进来的值。

### 3.1 `connection.open` / `connection.create`

Core 会带上 `factoryId`、`connectionId`、`mountId`、`interactionId`、`connection`（已保存的设置）调用：

```rust
"connection.open" | "connection.create" => {
    let factory = p.get("factoryId").and_then(Value::as_str).unwrap_or("");
    let mount = p.get("mountId").and_then(Value::as_str).unwrap_or("");
    match factory {
        "example-account" => Ok(json!({
            "kind": "pluginView",
            "view": { "source": "package", "entry": "ui/account.html" },
            "state": { "connectionId": p.get("connectionId") }
        })),
        _ => Ok(json!({
            "kind": "vfs",
            "mountId": mount,
            "nodeRef": { "mountId": mount, "nodeId": "root" }
        }))
    }
}
```

`kind` **只能是**：

| kind | 结果 |
| --- | --- |
| `vfs` | Core 用**原生 Explorer** 打开该连接的挂载点。必须属于本插件、且已声明 `extensions.vfs`。 |
| `pluginView` | Core 用插件包内 HTML **完整替换右侧主内容区**（保留左侧栏和全局标签页）。 |

`pluginView` 的 `view.entry` 必须是插件包内相对路径；不能是外链、`file://`、绝对路径或其他插件的 HTML。`state` 是业务状态，不是令牌，不能放密码或 OAuth refresh token。

Connection 导航**不能**返回弹窗、侧面板、局部 iframe 或外部 URL。账号配置、登录页放 `pluginView`；文件浏览放 `vfs`。

### 3.2 VFS 协议（最小可用）

声明了 `extensions.vfs` 后，Core 会把挂载点内的请求转给插件，常用方法：

```text
fs.list      { connection, directoryId, limit?, cursor? } -> { entries[], nextCursor? }
fs.stat      { connection, nodeId }                        -> { revision, size? }
fs.openRead  { connection, nodeId }                        -> { streamId }
fs.readStream{ streamId, offset, maxBytes }                -> { base64, eof }
fs.closeRead { streamId }                                  -> null
fs.writeFile { connection, nodeId, source, size, ifRevision } -> { revision }
```

示例只实现了只读的内存树。真实实现要注意：

- `nodeId` 是**插件命名空间内的稳定 ID**，不要用绝对路径冒充。
- 目录项返回稳定的 `nodeId` 和 `revision`；内容变化时 revision 必须变化。
- 读取用有界流，不要把整文件塞进一条 JSON；写入先比较 `ifRevision`，冲突要显式报错。
- 路径穿越（`../`、绝对路径、符号链接）必须在插件侧就拒绝。

---

## 4. Decorations：颜色、badge、整行/单列背景

Decoration 是**只读展示**，不能借它读文件内容，也不能改变文件本身。

Manifest 两种写法：

### 4.1 静态规则（declarative）

适合“按扩展名/目录名匹配”的固定角标，Core 本地计算，不启动插件进程：

```json
"decorations": {
  "mode": "declarative",
  "priority": 10,
  "rules": [
    {
      "match": { "isDir": false, "extensions": ["zip", "rar", "7z"] },
      "badge": { "text": "ZIP", "color": "neutral" }
    }
  ]
}
```

### 4.2 动态事件（dynamic）

适合需要读取 Git 状态、文件内容特征等运行时信息：

```json
"decorations": {
  "mode": "dynamic",
  "method": "decorations.resolve",
  "priority": 80,
  "scope": { "marker": ".git" }
}
```

- `method` 是插件进程的方法名；Core 会带 `{ directory, entries }` 调用。
- `scope.marker` 可选：只有目录下存在该文件/目录时才调用（例如 `.git`）。不写表示所有目录都调用。
- 动态 decorations 仍要求 `permissions.folders.read`；没有权限时 Core 直接跳过。

插件返回：

```json
{
  "decorations": [
    {
      "path": "/Users/me/project/src/main.ts",
      "badge": { "text": "M", "color": "orange", "tooltip": "Git：已修改" },
      "style": { "color": "#c2710c", "fontWeight": "600" }
    }
  ]
}
```

`badge` / `style` 是“文件名单元格”的简写。要精确控制每一列，用 `fields`：

```json
{
  "path": "/Users/me/project/notes.md",
  "fields": {
    "name": {
      "badges": [{ "text": "M", "color": "orange" }],
      "style": { "color": "#c2710c", "fontWeight": "600" }
    },
    "modified": { "style": { "color": "#c2710c" } },
    "size": { "badges": [{ "text": "新", "color": "green" }] },
    "kind": { "style": { "color": "#2f6f9f" } },
    "row": {
      "style": { "backgroundLight": "#fff9f0", "backgroundDark": "#2b2118" }
    }
  }
}
```

可用的字段：

| 字段 | 作用 |
| --- | --- |
| `name` | 文件名文字颜色、字重、斜体、badge |
| `modified` / `size` / `kind` | 对应列的单独颜色/badge/背景 |
| `row` | 整行背景；**选中时 Core 会用选中色覆盖整行背景**，不会被装饰色盖住 |

### 4.3 颜色和 badge 的硬限制

Core 会审核动态返回值，非法值会被丢弃：

- 颜色只能是 `orange`、`green`、`red`、`blue`、`gray`、`neutral`、`purple`，或 `#RGB` / `#RRGGBB`。
- badge 文本 1–4 个字符；每个字段最多 3 个 badge。
- `fontWeight` 只能是 `normal`、`500`、`600`、`bold`；`fontStyle` 只能是 `normal`、`italic`。
- `label` 最多 120 字符；`description` 最多 200 字符。
- 背景只在对应字段显式声明时绘制。**不要给 `row` 设置背景后又指望选中行保持装饰色**；选中态优先级更高。

> 想改文件名颜色：用 `style.color`（或 `fields.name.style.color`）。
> 想加角标：用 `badge`（一个）或 `badges`（数组）。
> 想只改时间颜色：用 `fields.modified.style.color`。
> 想整行浅色底：用 `fields.row.style.backgroundLight/backgroundDark`。

---

## 5. Toolbar：面板或命令

Manifest：

```json
"toolbar": [
  {
    "id": "example.home",
    "title": "示例插件",
    "icon": "puzzle",
    "panel": {
      "entry": "ui/index.html",
      "title": "示例插件",
      "defaultSize": { "width": 460, "height": 340 }
    }
  },
  {
    "id": "example.refresh",
    "title": "刷新示例",
    "icon": "refresh-cw",
    "command": "example.refresh"
  }
]
```

两种形态：

### 5.1 `panel`：打开插件自己的 HTML

- `entry` 是包内相对路径，单文件、自包含（CSS/JS 内联；Core 注入的 CSP 不允许外链脚本和网络请求）。
- HTML 最大 1 MiB。
- 面板内容由插件 iframe 渲染，Core 提供面板标题/关闭等外壳；**页面里不要再画一个重复的标题栏**。
- 面板通过 MessagePort 与 Core 通信；示例见 `hello/ui/index.html`。

面板里常用：

```js
const context = await invoke('context.get')
// context.directoryPath, context.selectionPaths, context.source = 'toolbar'
const value = await invoke('plugin.invoke', {
  method: 'example.greet',
  params: {}
})
```

`plugin.invoke` 只能调用插件自己的业务方法；`host.*`、`fs.*`、`vfs.*`、`connection.*` 等宿主方法会被 Core 拒绝。

### 5.2 `command`：只执行一个 native 方法

Core 用 `plugin_invoke` 调用：

```json
{
  "method": "example.refresh",
  "params": { "root": "/Users/me/Documents" },
  "directory": "/Users/me/Documents",
  "selectedPaths": ["/Users/me/Documents"]
}
```

适合“立即执行、不需要 UI”的动作。要展示结果，请改用 `panel`。

---

## 6. UI Bridge 最小协议

插件 HTML 不需要打包框架。最小流程：

1. 页面加载后向父窗口发 `{ type: 'fiex.bridge.listening' }`。
2. Core 发 `{ type: 'fiex.bridge.init', sessionId, nonce }`，并在 `event.ports[0]` 给一个 `MessagePort`。
3. 页面回 `{ type: 'fiex.bridge.ready', bridgeVersion: '1.0', sessionId, nonce }`。
4. 之后通过端口发请求：

```js
port.postMessage({
  type: 'request',
  bridgeVersion: '1.0',
  sessionId,
  requestId: 'req-1',
  method: 'plugin.invoke',
  params: { method: 'example.greet', params: {} }
})
```

5. Core 回：

```js
{ type: 'response', requestId: 'req-1', result: { message: '你好，Fiex！' } }
// 或 { type: 'response', requestId: 'req-1', error: { message: '...' } }
```

允许的方法由 Core 按场景限制：`context.get`、`theme.get`、`plugin.invoke`、`plugin.error`、`clipboard.writeText`、`navigation.open`、`panel.close`、`panel.resize` 等。不要尝试访问 `window.parent.document`、Tauri API 或本地绝对路径。

主题：调用 `theme.get`，把返回的 tokens 写成 CSS 变量，例如：

```js
for (const [key, value] of Object.entries(tokens)) {
  if (key !== 'mode' && value)
    document.documentElement.style.setProperty(`--${key}`, value)
}
```

这样浅色/深色主题切换时面板会自动跟随。

---

## 7. 调试清单

- 进程只能用 **stderr** 打日志；stdout 是 JSON-RPC 控制流，写日志会破坏协议。
- 先跑 `check_plugin.py`，再安装到 Core；安装后看设置里的插件错误。
- 权限没批准时 Core 不会启动进程；`plugin.ping` 失败先检查启用/授权。
- `pluginView`/`panel` 找不到资源时，检查 HTML 是否真的在 `.fiexplugin` 包内（`ui/` 目录），以及路径是否单层相对路径。
- 动态 decoration 不生效：确认 `mode:"dynamic"`、`method` 拼写、`folders.read` 已批准、`scope.marker` 条件满足。
- 颜色/角标被丢弃：检查是否用了未列出的颜色名、超过 4 字符的 badge、超过 3 个 badge。
- 连接点击报 `PermissionDenied`：检查是否声明了 `extensions.vfs`、`mountId` 是否等于 Core 传入值、连接是否绑定到本插件。

---

## 8. 文件

```text
example/
  README.md                ← 本手册
  hello/
    Cargo.toml
    manifest.json          ← connections / vfs / decorations / toolbar 全量示例
    src/main.rs            ← connection.open、fs.*、decorations.resolve、命令
    ui/index.html          ← toolbar 面板
    ui/account.html        ← connection 的 pluginView
```

建议顺序：先读 `manifest.json`，再对照 `src/main.rs` 的四个注释区块，最后打开两个 HTML 看 Bridge 写法。
