# FTW 第二阶段核心界面验收

> 本页为历史阶段记录，截图和测试结果对应当时构建；当前 1.1.7 的实现和未验证范围见[项目状态](../../PROJECT_STATUS.md)。

2026-11-24 · Windows 10 x64 · Tauri / WebView2

截图来自实际桌面应用。Rust Core 读取隔离 SQLite 样本，包含客户、已提交的发件记录、来信和跟进。合成邮箱使用 `example.test`，本次桌面测试未向外部发信，也未连接真实邮箱。

| 参考图 | 本轮实现与桌面复验 | 运行截图 |
| --- | --- | --- |
| `04_邮件中心.png` | 分类、列表、阅读三栏。验证了关联来信、会话、草稿分页以及较早的失败和结果未知记录。第 22 条草稿可直接通过搜索找到。 | [旧收件箱](baseline/inbox-1600.png) · [旧草稿列表](baseline/mail-1600.png) · [新收件箱 1600 × 900](final/inbox-1600.png) · [1280 × 720](final/inbox-1280.png) |
| `09_Agent自动发信权限.png` | 三种模式、账户、用途、限额及拒发名单集中呈现。验证了当前模式显示、显式选择自动发送配置以及拒发规则保存。截图中的自动模式为编辑状态。 | [发信设置](final/agent-1600.png) |
| `03_跟进记录.png` | 类型图标、来源筛选和右侧概要。验证了查找客户、新增跟进和重新搜索记录。 | [跟进时间线](final/followup-1600.png) |
| `08_新增编辑客户表单.png` | 分区表单、填写进度、保存前检查提示和快捷添加联系渠道。验证了新客户写入 Core 后打开详情。 | [客户表单](final/customer-form-1600.png) |
| `05_自定义视图与字段.png` | 保存视图在客户页内管理，字段编辑采用独立双栏。验证了新建视图和字段持久化。列显示、列宽、顺序和行密度由当前视图的列设置控制。 | [视图管理](final/views-1600.png) · [字段编辑](final/fields-1600.png) |
| `10_已发未回复客户视图.png` | 邮件中心直达 CRM 保存视图。验证了同步后尚未见回复的客户入列，已回复客户被排除。邮件历史沿用客户详情抽屉，统计卡片仍为客户概览。 | [已发未见回复](final/unreplied-1600.png) |

截图对照后，修正了 1280 × 720 邮件导航的密度、跟进操作图标的对齐，以及权限页初始选项与已保存模式的对应关系。使用相同桌面链路重新运行和截图。源码复用延续第一阶段的 Radix 弹层、Lucide 图标与 TanStack Table，未新增外部依赖。 [开源组件来源与许可](../THIRD_PARTY.md)

桌面 IPC 为 `list_drafts` 增加了可选的 `folder` 和 `keyword`，为收件列表增加 `keyword`，为 `list_activities` 增加可选的 `source` 和 `keyword`。Core 先过滤，再分页。拒发名单增加 `list_suppression` 分页读取，修改继续使用已有操作。MCP 工具参数和 SQLite 架构沿用现有版本。 [MCP 邮件说明](../../MCP_MAIL.md)

## 核心回归

- `npm run lint`、`npm run build`、`cargo build --locked --offline --workspace --features tauri/custom-protocol` 通过。Vite 提示主包约 634 kB，gzip 约 186 kB。
- Rust 工作区测试 25 项通过，覆盖 CRM 持久化、Agent 更新与冲突保护，以及模拟邮件传输和回复关联。变更通知前端测试 2 项通过。
- `scripts/test-mcp.mjs` 对 MCP `2025-06-18` 与 `2025-11-25` 的工具发现、调用及审批隔离检查均通过。
- 后续验收包括真实 SMTP / IMAP、Win11、系统 125% / 150% 缩放及最终便携发布。回复匹配与队列异常的专门增强进入第三阶段。

预览程序在 `D:\FTW\.ftw-dev\ui-phase2-preview\workbench.exe`，同目录 Core、MCP 和样本数据需一并保留。
