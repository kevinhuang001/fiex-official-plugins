# 按需缓存与双向文件同步

按需缓存、离线目录、固定副本、变更检测、后台重试及冲突处理由 Core 统一实现，WebDAV、SMB、OpenList、Google Drive 共用。协议插件只负责远端读写；新连接插件也可按同一扩展接口接入。Google Drive 插件的连接需要填写自己的 OAuth 客户端 ID、客户端密钥和刷新令牌。刷新令牌需要 Google Drive 读写权限（`https://www.googleapis.com/auth/drive`，或只访问由应用创建/选择的文件的 `drive.file`）。客户端密钥和刷新令牌保存在系统密钥环。插件只能通过 Core 访问用户批准的 Google 域名，无本地文件或进程权限。

- 浏览目录只请求元数据，不生成空文件、不下载整盘。
- 首次打开或快速查看才下载该文件。缓存目录和已浏览目录元数据持久保存，重启断网后仍可访问。
- “始终保留在本地”保存固定意图；文件夹递归下载，之后发现的新云端子项也会下载。中断后后台重试。
- 用本地应用编辑缓存中的普通文件，Core 检测内容变化，联网时上传。后台每轮完成后等待 30 秒；失败保留本地文件和下载基线，在列表警告中显示原因。
- 云端版本变化时更新未修改的缓存；两端都修改则标记冲突，停止覆盖，等待右键选择本地覆盖或保留本地备份后采用云端。
- “释放本地空间”只移除已登记的干净缓存，不删除云端文件；含未同步内容或冲突的子树拒绝释放。
- Google 原生 Docs、Sheets、Slides 不属于普通文件，应在浏览器编辑；不把导出文件当作可回写原生文档。Drive 允许重名，为避免本地路径歧义，名称附稳定文件 ID，保留扩展名。

本功能同步已存在的普通文件内容，不将本地缓存目录中手工新增、删除或重命名的文件解释为云端新增、删除或重命名命令。缓存文件缺失时保留云端原件并报告错误。系统 Finder/Explorer 挂载不在此功能中。

## 提供者扩展协议

连接工厂可声明 `capabilities: { "sync": true }`，启用 Core 后台文件内容协调。未声明的连接仍可按需下载和固定离线副本。官方插件没有特权，调用前检查插件启用状态和全部授权。

`fs.list` 的每个条目返回 `nodeId`、`name`、`kind`、`size`、`modified` 和不透明字符串 `revision`。Core 保存逻辑路径到稳定节点 ID 的映射。目录分页成功后保存离线元数据，失败时读取上次完整快照。

| 方法             | 参数（另含 connection）      | 返回                                    |
| ---------------- | ---------------------------- | --------------------------------------- |
| `fs.stat`        | `nodeId`                     | `{revision, size}`；删除/无权限返回错误 |
| `fs.openRead`    | `nodeId`                     | `{streamId, size?}`                     |
| `fs.readStream`  | `streamId, offset, maxBytes` | `{base64, eof}`                         |
| `fs.closeRead`   | `streamId`                   | null                                    |
| `fs.beginWrite`  | `nodeId, size, ifRevision`   | `{streamId}`                            |
| `fs.writeStream` | `streamId, offset, base64`   | null                                    |
| `fs.commitWrite` | `streamId`                   | `{revision}`                            |
| `fs.abortWrite`  | `streamId`                   | null                                    |

文件上传支持两种通用模式：`writeMode: "stream"` 使用分块 RPC；`writeMode: "file"` 使用 `fs.writeFile({nodeId, source, size, ifRevision}) -> {revision}`。Core 只向该次操作授权上传快照这个文件，插件用 SDK 的 `files().open_read(source)` 读取。HTTP 插件通过 `Request::body_reader(reader)` 让 Core 直接流式上传，不经 JSON 传递整文件，也不授权快照所在目录。

WebDAV 使用 HEAD/PROPFIND ETag 和 If-Match 条件 PUT；没有强 ETag 的服务器仍可离线缓存，但拒绝不安全的自动覆盖。SMB 使用修改/变更时间和大小校验，先写远端临时文件，提交前再检查后原子替换。OpenList 使用 `/api/fs/get` 的元数据基线和 `/api/fs/put` 的流式上传；其 API 无标准 CAS，属于尽力检测并发冲突，不能承诺服务端原子版本条件。SMB 提交前检查与替换之间同样存在协议客户端不配合时的窄竞争窗口。连接工厂通过 `conflictControl` 明示能力差异，不能把比较后写入描述为 CAS。

写入必须检查远程基线，已检测出的冲突返回错误。Core 使用磁盘快照，按 256 KiB 顺序分块，不向插件暴露宿主路径，不在内存装载整个文件。写入失败或插件重启后从本地文件重试，不宣称跨进程恢复上传会话。下载使用同目录临时文件和原子替换。JSON 元数据也先同步临时文件再替换；每连接异步锁序列化下载、释放和协调。

Google Drive 使用 OAuth 自动刷新访问令牌、范围读取、resumable 上传、版本号与 ETag 条件保护。Core 的 HTTP 流传输会保留无 Location 的 308 上传确认，而不是将它误判为重定向。

## 验证

Core CI 覆盖离线目录元数据持久化、基线版本不被目录刷新覆盖、固定继承与释放例外、路径边界和内容指纹。官方插件 CI 在三个平台运行真实插件进程，对接确定性 Core HTTP 替身，覆盖范围读取、断网失败/恢复、分块上传、过期基线、提交前远端变化和资源释放。真实 Google 账号 OAuth 和 Drive 服务联调仍需使用测试账号完成。
