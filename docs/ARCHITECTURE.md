# M0 进程与存储

`workbench.exe` 的本地 WebView 通过单一的 Tauri 命令 `core_health` 获取状态。Tauri 的主窗口具备该命令权限，网页资源与其他窗口没有 Core 通用访问权限。桌面 Rust 代码和命令行健康检查复用 `ftw-ipc`。M4 将 MCP 作为同一 Core 的客户端。

`ftw-core.exe` 按数据目录持有独占的 `core.lock` 文件句柄。命名管道按 Windows 账户 SID 和数据目录的卷与文件标识定位。Core 显式拒绝远程客户端，管道 ACL 授权当前 Windows 账户。该身份范围允许同账户的本机进程接入，因此 M2 的人工批准还需独立的业务权限和快照校验。

手动启动的 Core 由终端前台管理，Ctrl+C 后关闭管道和 SQLite。桌面启动的 Core 在后台独立运行，连续五分钟没有连接后退出。健康检查只在打开窗口和手动重新检查时发起，界面显示最近一次成功检查的时间。

## 健康检查协议

连接只承载一个请求和响应。每帧为四字节小端长度加 UTF-8 JSON，最大 8 KiB。M0 唯一请求为 `{"version":1,"operation":"health"}`。客户端读完响应后写入一个零字节，Core 收到确认后回收连接。

成功返回 `{"status":"ready","health":{...}}`，其中 `health` 含 `coreVersion`、`schemaVersion`、`storageMode`、`dataRoot` 和 `processId`。错误返回 `{"status":"error","code":"..."}`。当前错误码有 `invalid_request`、`unsupported_protocol_version`、`database_unavailable`。CLI 的 `health` 命令成功时输出 `health` 对象，失败时退出非零并将错误写到 stderr。

## SQLite 路径和迁移

Core 打开 `data/workbench.db`，连接启用外键、WAL、FULL 同步和五秒锁等待。SQLx 使用内嵌迁移和自身的 `_sqlx_migrations` 表，健康检查读取最近成功迁移的版本。版本 1 创建可保存非敏感键值设置的 `app_settings`。迁移失败时 Core 保留原库、记录具体错误并退出。

在 M1 首次升级可能已含业务数据的库之前，实现迁移前的一致性备份。M5 的备份恢复需覆盖数据库与附件，运行中的 WAL 数据库由一致性快照捕获。

## 与 V1 规格的实现选择

- 本机接口采用规格允许的 Windows IPC。Core 不监听 TCP，浏览器侧只有受限的 Tauri 命令。
- 第一条迁移随 Core 交付，后续按业务里程碑增加迁移。M0 不提前建立 CRM、邮件或 MCP 的空模块。
- WebView2 使用系统 Evergreen Runtime。工作台启动前检测缺失的运行时，显示本机错误对话框。开发版使用独立的 WebView2 数据目录，避免与发布版并行时争用用户配置。
