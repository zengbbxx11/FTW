# Core 进程与存储

`workbench.exe` 的本地 WebView 通过 Tauri 命令 `core_health`、`core_changes`、`crm_request`、`mail_request`、`inbox_request` 和 `data_request` 调用 Core。`ftw-mcp.exe` 通过同一 Core 的受限入口执行已授权的 CRM、文件、审计、任务及邮件操作。Core 负责输入校验、主体识别、权限、版本检查和事务。MCP 可在桌面关闭时按需启动 Core。[MCP 工具和权限](MCP_MAIL.md)

`ftw-core.exe` 按数据目录持有独占的 `core.lock` 文件句柄。命名管道按 Windows 账户 SID 和数据目录的卷与文件标识定位。Core 显式拒绝远程客户端，管道 ACL 授权当前 Windows 账户。人工批准由 Core 的 Windows 确认窗口记录，发件队列在入队及提交前重新检查版本、内容哈希、收件人和拒发规则。

手动启动的 Core 由终端前台管理，Ctrl+C 后关闭管道和 SQLite。客户端按需启动的 Core 在后台独立运行，连续五分钟没有连接后退出。「系统状态」显示最近一次健康检查结果，并提供手动复查入口。

## 健康检查协议

连接只承载一个请求和响应。每帧为四字节小端长度加 UTF-8 JSON，最大 1 MiB。健康检查请求为 `{"version":1,"operation":"health"}`。客户端读完响应后写入一个零字节，Core 收到确认后回收连接。CRM 操作和错误另见 [CRM 接口](CRM_API.md)。

成功返回 `{"status":"ready","health":{...}}`，其中 `health` 含 `coreVersion`、`schemaVersion`、`storageMode`、`dataRoot` 和 `processId`。错误返回 `{"status":"error","code":"..."}`。当前错误码有 `invalid_request`、`unsupported_protocol_version`、`database_unavailable`。CLI 的 `health` 命令成功时输出 `health` 对象，失败时退出非零并将错误写到 stderr。

## 故障诊断

桌面、Core 和 MCP 在数据根目录的 `logs` 记录结构化诊断事件。每个组件保留当前文件和一份历史文件，各约 256 KiB。便携目录无法写入时，尝试写入 Windows 用户目录下隔离的诊断目录。日志失败不阻断业务请求。

从「系统状态 → 导出诊断报告」另存 JSON。窗口无法打开时，双击便携目录的 `FTW-Diagnostics.cmd`，命令窗口会显示报告位置。也可运行 `ftw-core.exe diagnostics` 获取同口径 JSON。该命令不启动 Core 服务，也不打开 CRM 数据库。便携目录缺少 Core 或 Core 无法运行时，脚本仍会生成包含 Windows 版本与三个程序存在状态的最小报告。

完整报告包含程序版本、Windows 版本、WebView2 Runtime 安装版本、存储模式、三个程序文件的 SHA-256、最近一次观测到的 SQLite Schema、各组件最近的诊断事件和可用的发信任务编号。采集器只读取规定的结构化事件，排除历史自由文本日志、业务数据库、邮件、客户资料、附件、凭据、令牌以及机器目录路径。诊断与「数据与审计」里的业务历史分开保存。跨设备排障时，先查看导出的本地 JSON，再随出错时间、所用页面或 MCP 工具及复现步骤发给维护者。

## SQLite 路径和迁移

Core 打开 `data/workbench.db`，连接启用外键、WAL、FULL 同步和五秒锁等待。SQLx 使用内嵌迁移和自身的 `_sqlx_migrations` 表，健康检查读取最近成功迁移的版本。当前源码的最新迁移为 `0027_trusted_agents.sql`。迁移失败时 Core 记录错误并退出，升级前的一致性快照用于恢复。[按顺序查看 SQLx 迁移](../crates/ftw-core/migrations)

打开已有数据库且检测到待应用迁移时，Core 在新迁移前通过 SQLite Online Backup API 生成一致性快照。快照放在数据根目录的 `backups` 下，备份失败时停止升级。保留升级前的原始设置和迁移版本。历史发件附件和限长收件原文随数据库备份。新版附件和按需下载的大邮件保存在不可变对象文件，Core 校验并复制备份引用的对象。恢复前验证完整性和迁移校验和，创建当前库的安全备份，再替换内容。恢复期间暂停发件、审批和收件，旧审批失效，恢复的排队及发送中任务记录为结果未知。

| Schema | 主要数据变化 |
| --- | --- |
| 1–5 | 本地设置、联系人、候选、跟进和审计。SMTP 持久化队列、DPAPI 凭据、IMAP 同步位置、Agent 请求标识、发件限制和签名。 |
| 6–8 | 业务域版本、恢复代际、类型化自定义字段与保存视图。 |
| 9–14 | 持续授权、邮件开发状态、附件对象、多 Agent 发信授权、CRM 合并与变更核对、发件账户归档。 |
| 15–22 | 样品与批次、邮件模板、客户文档、来源和调查证据、公司档案及联系人关系。 |
| 23–25 | 每个 Agent 的协作权限、客户字段变更历史与分段上传、候选记录版本。 |
| 26 | 为回复草稿保存会话上下文，为已发邮件保存会话编号。已存在的邮件、审批和会话引用继续可读。 |

| 27 | Agent 暂停状态和最近连接记录。沿用原身份与发信授权，普通业务由有效登记直接获得共享访问。 |

### 跨进程同步和冲突

业务事务提交时更新对应业务域版本，备份恢复时更新数据纪元。桌面通过 `core_changes` 在前台每两秒检查，失败时退避到八秒，重新聚焦时立即检查。TanStack Query 根据变化重新获取客户、跟进、邮件、统计等受影响的查询。MCP 查询从同一 Core 回读持久化数据。

客户、候选、共享视图和草稿等按各自合约校验记录版本。客户、候选、公司等涉及数据纪元的写入也检查是否跨过备份恢复。待核对的客户字段建议记录前后值与来源，人工采纳时再校验当前值。可重试的 MCP 写入沿用同一身份、请求编号和参数。[CRM 接口](CRM_API.md) · [协同验证](ui-review/HUMAN_AGENT_PARITY.md)

### 邮件与附件

发送队列在入队和提交 SMTP 前复核适用的授权和快照，进入 SMTP 前先保存发送中状态。重启后把中断的发送标为结果未知。SMTP 接受后记录已发及客户时间线。IMAP 通过 UID 同步，按回复头和已知客户地址关联。线程回复保留原邮件引用，自动回复、退信和业务回复分别处理。[邮件工作流](MCP_MAIL.md#收件与后续跟进)

发送安全账本 `agent-send-safety.sqlite` 与业务 SQLite 分开，恢复旧库时继续保留。客户文档和新版邮件附件共用本地对象存储，按文件 ID 及授权访问。同一 Windows 用户的本地 MCP 客户端无需 Agent 令牌，共享“本机 Agent”身份；本地管道仅允许当前 Windows 用户访问并拒绝远程连接。Core 仍检查用户保存的发信授权及发送规则。移动便携数据时保留账本和对象，跨 Windows 用户或设备后重新输入 SMTP 与 IMAP 凭据。

## 与 V1 规格的实现选择

- 本机接口采用规格允许的 Windows IPC。Core 不监听 TCP，浏览器侧只有受限的 Tauri 命令。
- WebView2 使用系统 Evergreen Runtime。工作台启动前检测缺失的运行时，显示本机错误对话框。开发版使用独立的 WebView2 数据目录，避免与发布版并行时争用用户配置。
