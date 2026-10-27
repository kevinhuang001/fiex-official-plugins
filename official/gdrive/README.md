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

连接只声明 `extensions.connections`。Core 根据实际文件版本与 I/O 操作结果协调缓存，不再读取 `capabilities` 或同步开关。官方插件没有特权，调用前检查插件启用状态和全部授权。

`fs.list` 的每个条目是 `FsEntry`（`nodeId`、`name`、`kind`、`size`、`modified`、`revision` 等）；Drive 文件 ID 就是稳定节点 ID。`fs.list` 返回 `{entries, nextCursor?, warnings?}`，`fs.stat` 返回 `FsEntry`。Core 保存逻辑路径到稳定节点 ID 的映射。目录分页成功后保存离线元数据，失败时读取上次完整快照。

| 能力    | 线方法                                           | 返回                    |
| ------- | ------------------------------------------------ | ----------------------- |
| `list`  | `fs.list`                                        | `FsListing`             |
| `stat`  | `fs.stat`                                        | `FsEntry`               |
| `read`  | `fs.openRead` / `fs.readStream` / `fs.closeRead` | SDK 管理流              |
| `write` | `fs.beginWrite` / `fs.writeStream` / `fs.commit` / `fs.abort` | `fs.commit` 返回 `FsEntry` |
| `mkdir` | `fs.mkdir` | 新建的文件夹 `FsEntry` |
| `rename` | `fs.rename` | 新名称/父目录下的 `FsEntry` |
| `remove` | `fs.remove` | 空文件夹或文件删除成功；非空文件夹明确拒绝 |

上传使用 `fs.beginWrite({nodeId, size, ifRevision})`：Core 通过标准 `Write` 分块写入，插件以 256 KiB 为界边收边执行 resumable PATCH；`fs.commit` 校验版本与 ETag 后发送最后一块、读取提交后的元数据并返回 `FsEntry`。没有 `writeMode`、`sync` 或 `conflictControl` 配置字段。

WebDAV 使用 HEAD/PROPFIND ETag 和 If-Match 条件 PUT；没有强 ETag 的服务器仍可离线缓存，但拒绝不安全的自动覆盖。SMB 使用修改/变更时间和大小校验，先写远端临时文件，提交前再检查后原子替换。OpenList 使用 `/api/fs/get` 的元数据基线和 `/api/fs/put` 的流式上传；其 API 无标准 CAS，属于尽力检测并发冲突，不能承诺服务端原子版本条件。SMB 提交前检查与替换之间同样存在协议客户端不配合时的窄竞争窗口。这些差异由 Provider 的版本检查和实际操作结果体现，比较后写入不能描述为 CAS。

写入必须检查远程基线，已检测出的冲突返回错误。Core 使用磁盘快照，按 256 KiB 顺序分块，不向插件暴露宿主路径，不在内存装载整个文件。写入失败或插件重启后从本地文件重试，不宣称跨进程恢复上传会话。下载使用同目录临时文件和原子替换。JSON 元数据也先同步临时文件再替换；每连接异步锁序列化下载、释放和协调。

Google Drive 使用 OAuth 自动刷新访问令牌、范围读取、resumable 上传、版本号与 ETag 条件保护。Core 的 HTTP 流传输会保留无 Location 的 308 上传确认，而不是将它误判为重定向。

## 验证

Core CI 覆盖离线目录元数据持久化、基线版本不被目录刷新覆盖、固定继承与释放例外、路径边界和内容指纹。官方插件 CI 在三个平台运行真实插件进程，对接确定性 Core HTTP 替身，覆盖范围读取、断网失败/恢复、分块上传、过期基线、提交前远端变化和资源释放。真实 Google 账号 OAuth 和 Drive 服务联调仍需使用测试账号完成。

统一上传：`fs.beginWrite` 以 `nodeId, size, ifRevision` 打开写流，`fs.commit` 返回最终 `FsEntry`。普通虚拟树不能调用 Connection Provider 接口。
