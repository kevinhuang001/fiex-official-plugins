# Google Drive

1. 在 [Google Cloud Console](https://console.cloud.google.com/apis/credentials) 创建自己的项目并启用 **Google Drive API**。
2. 配置 Google Auth Platform / OAuth 同意屏幕；测试模式下把自己的 Google 账号加入测试用户。
3. 创建 **桌面应用（Desktop app）** OAuth 客户端，复制该应用的 Client ID 和 Client Secret。
4. 在 Fiex 新建 Google Drive 连接并填写这两项。保存连接将打开系统浏览器，使用 Google 账号登录并同意 Drive 授权后返回 Fiex。

插件没有内置或共享 OAuth 应用凭据，不需要用户手工获取刷新令牌。Core 生成 PKCE S256 和随机 state，监听随机 IPv4 loopback 端口并验证回调；插件通过 SDK 取得一次性 code 后执行 token exchange。Core 保存返回的刷新令牌与 Client Secret（默认本地私有文件，可在设置中选择系统钥匙串），它们不写入连接注册表，也不会返回编辑连接界面。Client ID 不是秘密，随连接配置保存。

已有连接的刷新令牌继续使用；授权过期或应用凭据改变后，重新保存连接会进入浏览器授权。后台同步不会自行弹出浏览器。Google 外部应用在 Testing 状态下的离线授权可能过期，应按 Google 控制台要求配置应用状态。

浏览整个已有 Drive 使用 `https://www.googleapis.com/auth/drive` 范围。仅能访问应用创建/选择的文件的 `drive.file` 范围不能替代完整云盘浏览。

参考：[Google Desktop OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)。

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
