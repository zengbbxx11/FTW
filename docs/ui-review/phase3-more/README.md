# FTW 第三阶段第二批界面验收

> 本页为历史阶段记录，截图和测试结果对应当时构建；当前 1.1.7 的实现和未验证范围见[项目状态](../../PROJECT_STATUS.md)。

2026-11-24 · Windows 10 x64 · Tauri / WebView2

运行截图来自 Windows 桌面与隔离 Rust Core。样本邮箱为 `example.test`，本次桌面验收使用已有邮件记录，未连接真实邮箱或向外发信。

| 原始参考图 | 本轮改造 | 运行截图 |
| --- | --- | --- |
| `19_多邮箱与签名管理.png` | 账户列表、SMTP 与 IMAP 设置、签名和默认发件账户集中在同一工作区。连接测试结果和同步状态来自各自协议。 | [改造前](before/accounts-1600.png) · [1600 × 900](final/accounts-1600.png) · [1280 × 720](final/accounts-1280.png) |
| `18_样品进度跟踪.png` | 通过客户管理的更多操作进入样品工作区。使用 CRM 客户样品状态、最近的样品沟通、型号、数量和反馈。 | [原客户保存视图](before/samples-1600.png) · [样品工作区](final/samples-1600.png) |
| `26_Agent数据变更与冲突处理.png` | 使用 Core 的统计与筛选，分别展示工作台当前值、Agent 建议值及记录时原值，提供查看审计和人工处理入口。 | [改造前](before/changes-1600.png) · [改造后](final/changes-1600.png) |
| `25_客户文件与附件.png` | 按客户、文件名、主题和收发来源检索邮件附件。从数据与审计或客户邮件面板进入，可打开对应来信、发件或草稿记录。 | [原邮件内附件](before/files-1600.png) · [附件索引](final/files-1600.png) |

样品字段通过 `0015_sample_activity.sql` 增量迁移，历史记录保留。`add_customer_activity` 接受可选 `sample` 对象，型号和数量随客户时间线返回。样品状态使用 CRM 既有状态，数量来自具体样品沟通记录。 [MCP 接口说明](../../MCP_MAIL.md)

新增桌面 IPC `sample_workspace`、`file_workspace` 和 `set_default_sender`。`list_agent_changes` 支持按冲突和处理状态筛选。默认发件账户按工作区持久化，用于新建桌面草稿。账户编辑、限额、Agent 发信权限和 SMTP 发信仍由 Rust Core 负责。

截图对照后，将邮箱设置改为行内标签布局，并调整账户列表中的默认标记和地址显示。在 1280 × 720 下复核签名栏和 SMTP 保存按钮的位置。

## 核心回归

- 前端 lint、TypeScript 检查、Vite 构建、Rust 格式检查和 Tauri 工作区构建通过。Rust 测试 25 项，前端变更通知测试 2 项通过。Vite 仍提示主包约 666 kB，gzip 约 193 kB。
- Rust 集成测试覆盖迁移后的旧记录、Agent 样品写入及旧调用者编辑时的样品字段保留。MCP `2025-06-18` 与 `2025-11-25` 已验证样品写入、时间线读取和审批隔离。
- 隔离桌面实测保存样品反馈和数量、切换并保存签名与默认发件账户、新草稿采用默认账户、核对并处理 Agent 建议，以及从附件索引打开来信与发件记录。
- `D:\FTW\.ftw-dev\ui-phase3-more-preview\workbench.exe` 已验证能启动隔离 Core。请保留同目录 Core、MCP 和样本数据。

样品工作区按客户显示最近一条样品沟通。附件索引覆盖已有邮件附件，独立客户文件导入与邮件模板留在后续批次。邮箱同步继续使用现有同步入口和连接方式。组件沿用 Lucide、Radix 弹层和 FTW 表格样式，无新增外部依赖。 [开源组件来源](../THIRD_PARTY.md)
