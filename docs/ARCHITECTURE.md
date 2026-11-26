# Core 进程与存储

`workbench.exe` 的本地 WebView 通过 Tauri 命令 `core_health`、`crm_request`、`mail_request`、`inbox_request` 和 `data_request` 调用 Core。主窗口具备对应的命令权限。`ftw-mcp.exe` 通过同一 Core 的受限 Agent 请求执行 CRM、收件、审批申请及代发流程。

`ftw-core.exe` 按数据目录持有独占的 `core.lock` 文件句柄。命名管道按 Windows 账户 SID 和数据目录的卷与文件标识定位。Core 显式拒绝远程客户端，管道 ACL 授权当前 Windows 账户。人工批准由 Core 的 Windows 确认窗口记录，发件队列在入队及提交前重新检查版本、内容哈希、收件人和拒发规则。

手动启动的 Core 由终端前台管理，Ctrl+C 后关闭管道和 SQLite。桌面启动的 Core 在后台独立运行，连续五分钟没有连接后退出。健康检查只在打开窗口和手动重新检查时发起，界面显示最近一次成功检查的时间。

## 健康检查协议

连接只承载一个请求和响应。每帧为四字节小端长度加 UTF-8 JSON，最大 1 MiB。健康检查请求为 `{"version":1,"operation":"health"}`。客户端读完响应后写入一个零字节，Core 收到确认后回收连接。CRM 操作和错误另见 [CRM 接口](CRM_API.md)。

成功返回 `{"status":"ready","health":{...}}`，其中 `health` 含 `coreVersion`、`schemaVersion`、`storageMode`、`dataRoot` 和 `processId`。错误返回 `{"status":"error","code":"..."}`。当前错误码有 `invalid_request`、`unsupported_protocol_version`、`database_unavailable`。CLI 的 `health` 命令成功时输出 `health` 对象，失败时退出非零并将错误写到 stderr。

### 故障诊断

桌面、Core 和 MCP 在数据根目录的 `logs` 记录结构化诊断事件。每个组件保留当前文件和一份历史文件，各约 256 KiB。便携目录无法写入时，尝试写入 Windows 用户目录下隔离的诊断目录。日志失败不阻断业务请求。

从「系统状态 → 导出诊断报告」另存 JSON。窗口无法打开时，双击便携目录的 `FTW-Diagnostics.cmd`，命令窗口会显示报告位置。也可运行 `ftw-core.exe diagnostics` 获取同口径 JSON。该命令不启动 Core 服务，也不打开 CRM 数据库。便携目录缺少 Core 或 Core 无法运行时，脚本仍会生成包含 Windows 版本与三个程序存在状态的最小报告。

完整报告包含程序版本、Windows 版本、WebView2 Runtime 安装版本、存储模式、三个程序文件的 SHA-256、最近一次观测到的 SQLite Schema、各组件最近的诊断事件和可用的发信任务编号。采集器只读取规定的结构化事件，排除历史自由文本日志、业务数据库、邮件、客户资料、附件、凭据、令牌以及机器目录路径。文件仅保存在本地。排障时可附上出错时间、所用页面或 MCP 工具及复现步骤。

## SQLite 路径和迁移

Core 打开 `data/workbench.db`，连接启用外键、WAL、FULL 同步和五秒锁等待。SQLx 使用内嵌迁移和自身的 `_sqlx_migrations` 表，健康检查读取最近成功迁移的版本。版本 1 创建可保存非敏感键值设置的 `app_settings`。迁移失败时 Core 保留原库、记录具体错误并退出。

打开已有数据库且检测到待应用迁移时，Core 在新迁移前通过 SQLite Online Backup API 生成一致性快照。快照放在数据根目录的 `backups` 下，备份失败时停止升级。保留升级前的原始设置和迁移版本。历史发件附件和限长收件原文随数据库备份。新版附件和按需下载的大邮件保存在不可变对象文件，Core 校验并复制备份引用的对象。恢复前验证完整性和迁移校验和，创建当前库的安全备份，再替换内容。恢复期间暂停发件、审批和收件，旧审批失效，恢复的排队及发送中任务记录为结果未知。

版本 2 建立 contacts、contact_methods、customer_interests、lead_candidates、activities 和 audit_logs。联系渠道、产品兴趣、候选转正及跟进通过外键保留联系人关系。包含三个字符及以上的搜索词走 SQLite FTS5 trigram 索引，短词按字面匹配，结果同样分页。业务时间以 UTC Unix 秒存储。候选转正和活动写入与审计在同一 SQLite 事务完成。

版本 3 增加 SMTP 账户、DPAPI 加密凭据、邮件草稿、审批、持久化发件队列、已接受邮件和拒发名单。发送工作线程在进入 SMTP 前写入发送中状态，重启后将中断的发送标记为结果未知。只有确认 SMTP 接受才写入已发邮件和客户时间线。 [邮件和 MCP 接口](MCP_MAIL.md)

版本 4 增加 IMAP 账户、UIDVALIDITY 游标、收件原文及 IMAP 位置。邮件关联先比较回复头，再比较发件地址，待关联邮件可由工作台人工更正。版本 5 持久化 Agent 请求 ID 及账号的发件限制和签名。

版本 6 增加数据域修订和备份恢复代际。桌面通过 `core_changes` 轮询一个轻量 Core 回执，按变化使客户、跟进、邮件等查询失效。Core 事务回滚时修订一起回滚。桌面失去焦点不持续轮询，重新聚焦时马上检查。邮件时限以 Core 时间计算。

## 与 V1 规格的实现选择

- 本机接口采用规格允许的 Windows IPC。Core 不监听 TCP，浏览器侧只有受限的 Tauri 命令。
- WebView2 使用系统 Evergreen Runtime。工作台启动前检测缺失的运行时，显示本机错误对话框。开发版使用独立的 WebView2 数据目录，避免与发布版并行时争用用户配置。

版本 7 和 8 增加类型化扩展字段与保存视图。桌面表格、看板和 Agent 使用同一套参数化查询、稳定字段 ID 和写入规则。版本 9 增加 Windows 验证的发件策略、已登记的 Agent 和同一个发件队列里的持续授权作业。发件账本独立于业务 SQLite，恢复旧备份时保留。版本 10 以邮件事实派生 CRM 状态。版本 11 增加受限本地附件对象及大邮件正文下载元数据。
