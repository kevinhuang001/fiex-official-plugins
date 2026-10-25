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

Manifest 只有动态模式：

```json
"decorations": {
  "match": {
    "contains": [".git"],
    "extensions": ["ts", "js", "md"],
    "mimeTypes": ["text/*"]
  },
  "priority": 80
}
```

- `decorations` 只有动态模式；不存在 `mode`、`method`、`scope`、`rules`；
- Core 固定调用标准方法 `decorations.resolve`，**方法名不用写**；
- `match` 是包含谓词：不满足时 Core 不会调用插件；
  - `contains`：目录或祖先目录包含 `.git` 等名称（仅 decorations）；
  - `extensions` / `mimeTypes` / `kinds`：当前可见成员中至少有一项匹配；
  - 缺省/空 `match` 表示所有目录、所有成员都参与；
- 动态 decorations 仍要求 `permissions.folders.read`；没有权限时 Core 直接跳过。

插件返回：

```json
{
  "decorations": [
    {
      "name": "main.ts",
      "badge": { "text": "M", "color": "orange", "tooltip": "Git：已修改" },
      "style": { "color": "#c2710c", "fontWeight": "600" }
    }
  ]
}
```

响应只需要 `name`，Core 会在当前目录的可见成员里解析成真实 `path`；插件不要返回绝对路径。请求里的 entries 仍带 `path`，方便 Git 等插件做内部查询。

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

## 7. 其余扩展点：Info / Preview / ContextMenu / Search / Settings / Hooks / Themes

`extensions` 里每个扩展点都是独立的注册项；可以只实现其中一个。

| 扩展点 | 声明 | 触发 |
| --- | --- | --- |
| `contextMenus` | `true` 或 `{match}` | 右键 → `contextMenu.resolve`，点击 → `contextMenu.execute` |
| `decorations` | `{match?,priority?}` | 目录切换 → 标准 `decorations.resolve`（仅动态） |
| `info` | `{id,match?,priority?}` | 用户“显示简介” → `info.resolve` |
| `preview` | `{id,match?,priority?,maxBytes?}` | 用户预览 → `preview.open` |
| `connections` | `[{id,title,fields}]` | 添加连接 / 点击连接 → `connection.create/open` |
| `vfs` | `[{id,title,readOnly?}]` | Core Explorer 的文件树请求 |
| `search` | `[{id,title,icon?,command}]` | 搜索框 → `search.query` |
| `toolbar` | `[{id,title,icon?,panel?/command?}]` | 工具栏点击 |
| `settings` | `[{id,label,type,...}]` | 设置页渲染；值通过 `ctx.storage_*` |
| `hooks` | `[{id,event,method,priority?,match?}]` | 文件生命周期（见下方状态） |
| `themes` | `[{path}]` | 主题包；声明式 JSON，不含代码 |

`match` 对所有带条件的扩展点通用：

```json
"match": {
  "extensions": ["md", "txt"],
  "mimeTypes": ["text/*"],
  "kinds": ["file"],
  "contains": [".git"]
}
```

- 同一字段内 OR，不同字段 AND；
- `extensions` 支持 `md`、`.md`、`*.md`、`*`；
- `mimeTypes` 支持精确值或 `text/*`、`image/*`、`*/*`；
- `kinds` 为 `file` / `directory`；
- `contains` 仅用于 decorations：目录或其祖先包含这些名称；
- `selection` 仅用于 contextMenus：`single` / `multi` / `any`（默认 any）；
- 不满足就不会调用插件；空 `match` 表示不过滤；
- `info` / `preview` 只针对用户当前选中的**单个资源**评估，并且只在点击“显示简介 / 快速查看”时才调用；
- `contextMenus` 的 `selection` 先按单选/多选过滤，再对选中项做类型过滤。

### 7.1 Info：标准事件 `info.resolve`

Manifest：

```json
"info": {
  "id": "example.info",
  "match": {
    "extensions": ["txt", "md", "log"],
    "mimeTypes": ["text/*"],
    "kinds": ["file"]
  },
  "priority": 80
}
```

Core 会构造 `InfoContext`：

```json
{
  "version": 1,
  "sessionId": "opaque",
  "intent": "info",
  "resource": {
    "ref": { "mountId": "local", "nodeId": "fiex-resource:..." },
    "metadata": {
      "name": "notes.md",
      "kind": "file",
      "extension": "md",
      "mime": "text/markdown",
      "size": 2451,
      "modified": 1792920000
    },
    "read": { "resourceId": "fiex-read:..." }
  }
}
```

`metadata` 只在 `permissions.metadata.enabled` 时出现；`read` 只在 `permissions.files.read.enabled` 时出现。

插件返回：

```json
{
  "sections": [
    {
      "id": "example",
      "title": "示例信息",
      "rows": [
        { "label": "文件", "value": "notes.md" },
        { "label": "SHA-256", "value": "…", "copyable": true }
      ]
    }
  ]
}
```

- `id` 在插件内唯一，Core 会变成 `<pluginId>.<id>`；
- `copyable: true` 让 Core 在值旁边显示复制按钮；
- 没有 `files.read` 就不要调用 `ctx.files().open_read`；
- 内置的 `core.metadata` / 音频 / 视频 section 与插件 section 由同一个 Registry 合并。

示例实现见 `hello/src/main.rs` 的 `info.resolve`。

### 7.2 Preview：标准事件 `preview.open`

Manifest：

```json
"preview": {
  "id": "example.preview",
  "match": {
    "extensions": ["txt", "md", "log"],
    "mimeTypes": ["text/*"],
    "kinds": ["file"]
  },
  "priority": 60,
  "maxBytes": 1048576
}
```

Core 传入 `PreviewContext`（和 InfoContext 同形，`intent: "preview"`）。插件返回 `PreviewView`：

```json
{
  "kind": "html",
  "view": { "source": "package", "entry": "ui/preview.html" }
}
```

也可以返回：

```json
{ "kind": "text", "text": "..." }
{ "kind": "unsupported", "message": "不是我能处理的格式" }
```

规则：

- `kind: "html"` 的 `entry` 必须是包内单层相对路径；Core 会校验并注入 CSP；
- **Manifest 不再写 `panel`**；页面由 `preview.open` 的返回值决定；
- 预览页面不要再画标题栏，Core 已经提供；
- 需要读文件时用 `resource.metadata.name` + `ctx.files()`，或用 `read.resourceId` + `ctx.files().open_resource(...)`；
- 内置 text/image/media/pdf/fallback 与插件都走同一个 Registry，按 priority 和用户偏好排序。

示例实现见 `hello/src/main.rs` 的 `preview.open`，页面见 `hello/ui/preview.html`。

### 7.3 ContextMenu：扁平 items

Manifest：

```json
"contextMenus": {
  "match": {
    "selection": "single",
    "extensions": ["txt", "md", "log"],
    "kinds": ["file"]
  }
}
```

`contextMenu.resolve` 返回：

```json
{
  "items": [
    { "id": "example.inspect", "title": "示例：检查文件", "kind": "action" },
    { "id": "example.more", "title": "示例", "kind": "submenu" },
    {
      "id": "example.copy-path",
      "title": "示例：复制路径",
      "kind": "action",
      "parentId": "example.more"
    }
  ]
}
```

- 只能是 `action | submenu | separator`；
- 子菜单用 `parentId`，不能嵌套 `children`；
- 不能写 `when` 静态条件；需要条件就在 resolve 里自己判断；
- Core 会按 `menuSessionId`、item ID 和 owner 校验 `contextMenu.execute`；
- 菜单查询阶段只拿到 opaque refs，需要属性时再申请。

### 7.4 Search：`search.query`

Manifest：

```json
"search": [
  {
    "id": "example.search",
    "title": "示例搜索",
    "icon": "magnifying-glass",
    "command": "search.query"
  }
]
```

Core 传入：

```json
{
  "query": "report",
  "root": "/Users/kevin/Documents",
  "hidden": false,
  "ignores": [".git"],
  "limit": 200
}
```

返回 `entries`，每项必须是完整 Entry：

```json
{
  "entries": [
    {
      "path": "/Users/kevin/Documents/report.txt",
      "name": "report.txt",
      "isDir": false,
      "isSymlink": false,
      "size": 12,
      "modified": 0,
      "created": 0,
      "extension": "txt",
      "mime": "text/plain",
      "readonly": false,
      "linkTarget": null,
      "tags": [],
      "sync": null
    }
  ],
  "truncated": false
}
```

搜索必须限制在 `root` 内；越界路径会被 Core 拒绝。示例返回空列表，只展示合同外形。

### 7.5 Settings：声明式设置 + 存储

Manifest：

```json
"settings": [
  {
    "id": "greeting",
    "label": "问候语",
    "type": "text",
    "defaultValue": "你好"
  }
]
```

支持的类型与连接字段类似：`text`、`number`、`checkbox`、`list`、`secret` 等。设置值不经过 IPC 参数下发，插件用 SDK 自己读：

```rust
let greeting = ctx
    .storage_get("global", "greeting")
    .ok()
    .and_then(|value| value.as_str().map(str::to_owned))
    .unwrap_or_else(|| "你好".into());
```

写回：

```rust
ctx.storage_set("global", "greeting", &json!("嗨"))?;
```

敏感值用 `plugin_secret_*` / 钥匙串，不要写进普通 settings。

### 7.6 Hooks：生命周期钩子

Manifest：

```json
"permissions": {
  "hooks": { "enabled": true }
},
"extensions": {
  "hooks": [
    {
      "id": "example.beforeOpen",
      "event": "beforeOpen",
      "method": "example.beforeOpen",
      "priority": 100,
      "match": { "extensions": ["txt", "md"], "kinds": ["file"] }
    }
  ]
}
```

插件侧：

```rust
"example.beforeOpen" => Ok(json!({
    "handled": false
})),
```

`handled: true` 表示插件已经接管该操作（例如 Archive 的“先解压再打开”）；`false` 表示继续 Core 的默认流程。

> **当前分支状态**：Core 已经校验 `permissions.hooks` 和 `extensions.hooks` 注册，但文件生命周期里的 `hooks.invoke` 接线还没完成；示例 handler 展示的是目标合同，接线前不会自动执行。不要把它当成已经可用的拦截点。

### 7.7 Themes：声明式主题包

主题不是 native 插件，不需要 `runtime` 或 `ui`：

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

`theme.json` 只允许 token：

```json
{
  "id": "example",
  "name": "Example",
  "mode": "light",
  "tokens": {
    "accent": "#2f6fed",
    "surface": "#ffffff",
    "canvas": "#f6f7f9",
    "radius-row": "8px"
  }
}
```

Core 会校验路径、token 名和值，禁止 CSS 注入、外链和脚本。可直接参考 `official/vista`、`official/win11`。

### 7.8 扩展点组合示例

一个网盘插件通常：

```text
connections  +  vfs       → 连接入口 + 文件树
info                       → 显示简介
preview                    → 自定义预览
toolbar                    → 账号/设置面板
```

一个 Git 插件通常：

```text
connections + vfs          → 历史树
decorations (dynamic)      → 修改状态角标/颜色
contextMenus               → Git 操作
toolbar                    → 源代码管理面板
```

不要为了“注册更多”而声明用不到的扩展点；每个扩展点都会出现在用户可触发的位置，也增加审核面。

## 8. 调试清单

- 进程只能用 **stderr** 打日志；stdout 是 JSON-RPC 控制流，写日志会破坏协议。
- 先跑 `check_plugin.py`，再安装到 Core；安装后看设置里的插件错误。
- 权限没批准时 Core 不会启动进程；`plugin.ping` 失败先检查启用/授权。
- `pluginView`/`panel` 找不到资源时，检查 HTML 是否真的在 `.fiexplugin` 包内（`ui/` 目录），以及路径是否单层相对路径。
- 动态 decoration 不生效：确认 `extensions.decorations` 存在、`match` 条件满足、`folders.read` 已批准；Core 固定调用 `decorations.resolve`。
- 颜色/角标被丢弃：检查是否用了未列出的颜色名、超过 4 字符的 badge、超过 3 个 badge。
- 连接点击报 `PermissionDenied`：检查是否声明了 `extensions.vfs`、`mountId` 是否等于 Core 传入值、连接是否绑定到本插件。

---

## 9. 文件

```text
example/
  README.md                ← 本手册
  hello/
    Cargo.toml
    manifest.json          ← connections / vfs / decorations / toolbar 全量示例
    src/main.rs            ← connection.open、fs.*、decorations.resolve、命令
    ui/index.html          ← toolbar 面板
    ui/account.html        ← connection 的 pluginView
    ui/preview.html        ← preview.open 返回的 PreviewView html
```

建议顺序：先读 `manifest.json`，再对照 `src/main.rs` 的四个注释区块，最后打开两个 HTML 看 Bridge 写法。
