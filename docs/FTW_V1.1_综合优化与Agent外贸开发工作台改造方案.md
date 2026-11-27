# FTW V1.1 综合优化与 Agent 外贸开发工作台改造方案

> 本文保留需求基线或阶段方案的历史内容。当前版本为 1.1.7；已批准的本机免令牌接入、使用体验和便携更新实现及验证范围，以[当前项目状态](PROJECT_STATUS.md)为准。

评审稿 · 2026-11-23 · R0

## 1. 决策摘要

保留现有 Tauri 2、React/TypeScript/Vite/Tailwind、Rust/Tokio Core、SQLite/SQLx、MCP stdio、SMTP/IMAP 及 Windows x64 便携架构。V1.1 先修 CRM 与邮件的一致性，再增加字段、保存视图和高密度桌面工作区。自动开发使用**用户在 FTW 内主动启用的持续授权**，可切回逐封人工审核或禁发。开启前必须同时交付重复首次开发拦截、撤销权限、结果记录和审计。附件以可验证的本地文件对象升级，避免将 10 MiB 文件塞进现有 1 MiB IPC。

本次审查基线为 `main`、`0e55b1424cd701a37c38ebf1bf465264a862cf19`，开始时工作树干净，`main` 跟踪 `origin/main`。审查读取项目基线、开发守则、界面、Core、IPC、MCP、5 个迁移、依赖锁及测试。功能状态以下游实际代码为准。附图只用于视觉布局。V1 的逐封批准和禁止自动批量群发规则仍适用于已发布基线；本方案拟以可撤销的持续授权支持受限、单收件人、可审计的 Agent 发信。实时阻断机制、默认模式和开发上限获批后，先同步 V1.1 决议、`AGENTS.md` 和接口说明，再实施 R5。 [开发守则](../AGENTS.md) · [架构](ARCHITECTURE.md) · [改动基线](https://github.com/zengbbxx11/FTW/commit/0e55b1424cd701a37c38ebf1bf465264a862cf19)

## 2. 现状盘点

| 区域 | 实际能力与调用链 | 主要依据 |
| --- | --- | --- |
| 桌面和 Core | React Query 读写，经 Tauri 限定命令调用 Windows 本机命名管道。按数据目录互斥，Core 可独立运行，按需启动模式空闲退出。Tauri 主窗口权限、CSP 与管道账户 ACL 有不同作用。 | [桌面入口](../apps/desktop/src/main.tsx#L129) · [Tauri 命令](../apps/desktop/src-tauri/src/main.rs) · [ACL 和 IPC](../crates/ftw-ipc/src/windows.rs) · [Core 分发](../crates/ftw-core/src/server.rs#L114) |
| CRM | 客户列表、FTS5 搜索、阶段看板、等级/类型等筛选、联系方式、自由文本产品兴趣、归档恢复。候选支持待审核、否决、转正。跟进支持人工和 Agent 来源，邮件事件从 Core 写入。 | [客户列表和编辑](../apps/desktop/src/crm/Contacts.tsx) · [客户读写](../crates/ftw-core/src/crm/contacts.rs) · [候选转正](../crates/ftw-core/src/crm/leads.rs#L190) · [跟进](../crates/ftw-core/src/crm/timeline.rs) |
| 邮件 | SMTP TLS/STARTTLS、DPAPI 凭据、签名、拒发地址或域、发送次数限制。Core 原生窗口逐封确认，审批修订号与摘要入库，按草稿修订幂等入队，提交结果持久化。IMAP 使用隐式 TLS 和 INBOX 增量同步，保存原文或截取的邮件头、纯文本预览、线程引用与待关联队列。 | [发件服务](../crates/ftw-core/src/mail/mod.rs) · [批准与入队](../crates/ftw-core/src/mail/approval.rs) · [SMTP 状态](../crates/ftw-core/src/mail/worker.rs) · [IMAP 存储](../crates/ftw-core/src/mail/inbox/store.rs) |
| Agent | 现有 MCP 工具查写客户、候选、跟进，查询发件账户和草稿、申请审核、发送获批版本，查收件、会话和触发同步。Agent 命令与桌面在 Core 汇合。请求 ID 防重，普通工具集无批准和配置权限。 | [MCP 工具](../crates/ftw-mcp/src/main.rs) · [Agent 写入](../crates/ftw-core/src/agent.rs) · [IPC 合约](../crates/ftw-ipc/src/lib.rs) |
| 本地数据 | 迁移 1–5，SQLite WAL、外键及一致性备份。备份/恢复、客户 CSV 预览和导出、审计和数据路径可在 UI 使用。恢复旧审批会过期，旧发送任务不会自动继续。便携模式保留三 EXE 及 `portable.flag`，DPAPI 跨用户或设备需重授权。 | [迁移](../crates/ftw-core/migrations/) · [存储](../crates/ftw-core/src/storage.rs) · [恢复](../crates/ftw-core/src/storage/backup.rs#L85) · [CSV](../crates/ftw-core/src/data/transfer.rs) · [便携脚本](../scripts/build-portable.ps1) |

## 3. 审查结论与优先级

P1 会影响日常业务数据、发件判断或授权。P2 有明确影响和可恢复路径。注明“源码”表示条件和机制已确认，实际频率或 Windows 表现仍应复现。R0 未运行用真实资料的操作测试。

| 严重度 | 已由源码确认的事实及影响 | 处理 |
| --- | --- | --- |
| P1 | 客户表格、分组列、详情和时间线的 `useQuery` 没有 Core 跨进程失效来源。桌面写成功后主动 invalidate，MCP 写入直接到 Core。全局还关闭了聚焦自动重取。因此当前页面可持续显示旧快照。 | R1 版本通知。 [列表、看板、详情](../apps/desktop/src/crm/Contacts.tsx#L27) · [时间线](../apps/desktop/src/crm/Timeline.tsx#L12) · [Query 默认配置](../apps/desktop/src/main.tsx#L168) · [Agent 分发](../crates/ftw-core/src/agent.rs#L9) |
| P1 | `contacts::list` 以 `COALESCE(人工日期, MAX(活动))` 计算最近联系，详情以 `人工值 ?? 观察值` 计算。旧人工日期会覆盖新沟通。MIN/MAX 也包含所有活动，普通便签和沟通事件尚无明确语义区别。 | R1 统一首次、最近、最近回复及资料更新时间规则。 [查询计算](../crates/ftw-core/src/crm/contacts.rs#L9) · [详情显示](../apps/desktop/src/crm/Contacts.tsx#L253) |
| P1 | 阶段已有稳定枚举，客户等级、类型、来源、样品和批量订单进度由文本写入。重点视图识别少数别名，等级筛选按整值，样品视图基本依赖非空和两个文本例外。历史“未寄样”等含义与预设匹配可能不同。 | R1 兼容映射、分离业务维度。 [写入校验](../crates/ftw-core/src/crm/validation.rs#L100) · [快捷视图 SQL](../crates/ftw-core/src/crm/contacts.rs#L28) · [CSV 映射](../crates/ftw-core/src/data/transfer.rs#L120) |
| P1 | `open_approval` 即时返回“已打开”，后台 `record_approval` 的错误被丢弃，用户取消和对话框调用错误也无独立回执。审批过期在详情以本机时间换算，列表仍可读到持久化的 `approved`。Core 会拒绝过期发送，问题在状态回读、重审和反馈。 | R1 审批尝试记录、共享状态投影、清晰反馈。 [批准任务](../crates/ftw-core/src/mail/approval.rs#L11) · [详情状态](../apps/desktop/src/mail/MailCenter.tsx#L235) |
| P1 | 桌面长表单提交整份旧 `ContactInput`，Core 更新无记录版本条件，且同事务重写子项。Agent 在当前串行管道中按白名单更新，但用户打开表单后另一写入已经完成，旧表单仍可能覆盖对方修改。 | R1 引入记录版本与 PATCH，显示冲突并保留输入。未观察到实际用户数据丢失。 [表单快照](../apps/desktop/src/crm/Contacts.tsx#L131) · [保存 SQL](../crates/ftw-core/src/crm/contacts.rs#L166) · [Agent 更新](../crates/ftw-core/src/agent.rs#L113) · [Core 单请求循环](../crates/ftw-core/src/server.rs#L77) |
| P2 | 视图标签与看板布局放在同一状态，选择看板清除重点/样品快捷条件。看板列的本地 offset 可在切换搜索后保留，导致窄结果落在空页。表格空状态只检查搜索和阶段，忽略其他筛选，无 CSV 入口。 | R1 修分页和空状态判定，R3 拆分布局与条件。 [当前视图和分页](../apps/desktop/src/crm/Contacts.tsx#L12) |
| P2 | 邮件线程通过回复头查询并要求发件地址属于该联系人。无回复头命中时，唯一联系人邮箱可关联，但单独开会话。投递报告只有 MIME 类型标记，尚无可靠 DSN 与发件结果联动。 `large_message` 是邮件种类，`has_reply` 只承认 `kind='message'`，因此大型回复可能已有联系人/会话仍不亮回复状态。 | R6 分离内容完整性与邮件种类，避免把 DSN 或自动回复当成客户回复。 [线程匹配](../crates/ftw-core/src/mail/inbox/store.rs#L140) · [发件详情派生](../crates/ftw-core/src/mail/mod.rs#L378) |
| P2 | SMTP 只在明确 5xx 时映射为拒绝，其他异常保守为未知。历史连接失败与 DATA 后断线在用户界面缺少细分。已有未知不得盲重发的保护有效。 | R1 明确展示，R5 在能证明未提交时才细化失败分类。 [SMTP 分类](../crates/ftw-core/src/mail/smtp.rs#L63) · [恢复处理](../crates/ftw-core/src/mail/mod.rs#L86) |
| P2 | 发送总量 384 KiB、最多 4 件，纯文本附件数据以 Base64 入 JSON/SQLite。IMAP 超过 2 MiB 只请求最长 64 KiB 的头部，同步每批最多 25 封；无已截断内容的按需下载。 | R7 保留帧大小，分离二进制与业务元数据。 [UI 限额](../apps/desktop/src/mail/MailCenter.tsx#L181) · [Core 限额](../crates/ftw-core/src/mail/mod.rs#L589) · [IPC 帧](../crates/ftw-ipc/src/lib.rs#L19) · [IMAP 分支](../crates/ftw-core/src/mail/inbox/transport.rs#L107) |
| P2 | CRM 去重用邮箱、域名、公司和国家，允许人工复核后保留同地址的多个记录。邮件入队的唯一约束针对草稿修订及批准，两个客户或两封新草稿共享同一邮箱时没有首次开发级约束。V1 有逐封审批保护，持续授权前需要新的原子约束。 | R5/R6 在输送路径上实现用途分类、地址唯一认领和恢复保护。 [CRM 重复候选](../crates/ftw-core/src/crm/duplicates.rs#L15) · [队列约束](../crates/ftw-core/migrations/0003_mail.sql#L22) |

**产品与交互问题。** 当前导航是横向，客户页的筛选标签占高，名单首屏偏少。公司与邮箱挤在姓名下方，样品进度和等级缺稳定视觉区分。详情采用模态 `showModal()`，其独立滚动值得保留，宽屏并排工作方式需要改为非模态。客户编辑单屏集中展示表单，自定义属性与历史来源入口缺失。 [桌面壳](../apps/desktop/src/main.tsx#L139) · [客户 CSS](../apps/desktop/src/crm.css) · [全局 CSS](../apps/desktop/src/styles.css)

**需运行验证。** 用第二个 MCP 客户端与已打开的 CRM 同时操作，测刷新时延和争用。用隔离数据复现编辑冲突、审批取消/过期、回复时间归属和同名同域多联系人。对 Core 关闭/重启、附件和 IMAP provider 的边界、Win10/11 缩放焦点、100/1,000/10,000 客户列表性能采样。历史记录有 19 项 Rust 测试、两个 stdio MCP 协议版本及一次便携 Core 持久化验证，本阶段只读审查，未重跑。现有覆盖迁移失败与备份、CRM/候选/归档、模拟 SMTP 审核安全与掉线、模拟 IMAP 去重、CSV 和 IPC 帧；前端脚本只做 lint、类型检查和构建，未见自动化桌面 UI 用例。 [测试脚本](../scripts/test.ps1) · [进程回归](../crates/ftw-core/tests/core_process.rs) · [SMTP 测试](../crates/ftw-core/src/mail/approval.rs#L152) · [IMAP 测试](../crates/ftw-core/src/mail/inbox/tests.rs)

**可延后技术债。** 当前管道一次一请求、列表多用 offset、Core 审计以事件元数据为主、CSV 以当前覆盖字段导入导出。保留短请求和现有公平退让，先测量锁等待和看板请求数。若出现压力，再做游标分页、限并发读或独立事件订阅。保留归档/恢复作为业务删除语义，V1.1 无全库级物理删除。候选转正、查重、已验证邮件渠道校验继续复用现有 Core。 [Core 请求循环](../crates/ftw-core/src/server.rs#L77) · [归档与恢复](../crates/ftw-core/src/crm/contacts.rs#L338) · [候选转正](../crates/ftw-core/src/crm/leads.rs#L190)


## 4. 开源研究与复用策略

在 AppFlowy 0.15.1 标签实读了以下 Dart 实现。它提供清晰的组织边界，可用 FTW 自己的 React 和 Rust 实现相同操作能力。

| AppFlowy 源码 | 观察 | 应用于 FTW |
| --- | --- | --- |
| [DatabaseController](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/application/database_controller.dart#L50) 与 [GridBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/grid/application/grid_bloc.dart#L147) | 控制器把字段、筛选、排序、增改删行分别通知消费视图。 | Rust 提供数据修订，React Query 失效指定域，保留局部编辑状态。 |
| [FieldEditorBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/application/field/field_editor_bloc.dart)、[DatabasePropertyBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/application/setting/property_bloc.dart) 与 [EditSelectOptionBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/application/field/type_option/edit_select_option_bloc.dart) | 字段定义编辑、可见属性组织与单选选项编辑各有职责。 | 分离字段定义、选项与当前视图列序，重命名不破坏引用。 |
| [EditableCellBuilder](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/widgets/cell/editable_cell_builder.dart) 与 [RowDetailBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/grid/application/row/row_detail_bloc.dart#L123) | 按类型组装编辑器，行详情分别观察行和属性更新，并支持可见性和排列。 | 表格、抽屉和新建表单共享类型定义与编辑器，保存走同一 Core。 |
| [FilterEditorBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/grid/application/filter/filter_editor_bloc.dart) 与 [SortEditorBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/grid/application/sort/sort_editor_bloc.dart) | 筛选和排序编辑分别追踪字段与自己的配置。 | 用稳定字段 ID 构建带类型的条件和排序，字段归档后显示待处理规则。 |
| [DatabaseTabBarBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/application/tab_bar_bloc.dart#L180) 与 [BoardBloc](https://github.com/AppFlowy-IO/AppFlowy/blob/0.15.1/frontend/appflowy_flutter/lib/plugins/database/board/application/board_bloc.dart#L501) | 视图标签支持关联视图，改变布局仍观察同一数据库。看板订阅组和行变化。 | 保存视图是一组查询和显示配置，表格、看板共用联系人事实。 |

Twenty 的对象和字段做法用于明确内置字段与用户字段的边界，其活动与邮箱呈现适合分离业务时间线和邮件事件。Attio 提供表格行内编辑、筛选组、视图设置及记录预览的参考。Linear 的列表/看板切换、键盘命令和紧凑选项面板适合桌面。FTW 用自己的界面、语义和数据组织。开源复用、版本、许可证、兼容性、体积和候选替代详见独立评估表。 [Twenty 数据模型](https://github.com/twentyhq/twenty/blob/main/packages/twenty-docs/getting-started/core-concepts/data-model.mdx) · [Twenty 活动与邮件发布记录](https://twenty.com/releases) · [Attio 记录](https://attio.com/help/reference/attio-101/attios-data-model/understanding-records) · [Linear 显示选项](https://linear.app/docs/display-options) · [FTW 可复用开源方案评估表](FTW_可复用开源方案评估表.md)

## 5. R1 业务正确性设计

### 5.1 跨进程刷新

在 SQLite 中维护联系人、跟进、候选、邮件、字段/视图的单调域修订。与可见写入在**同一次数据库事务**中增加，优先用小而可测的表级触发器覆盖桌面、MCP、SMTP/IMAP、手动关联与 CSV。添加只读 `GetChanges` IPC 和受限 Tauri 命令，返回 Core 实例/数据库代际与域修订图，必要时再带有限的热实体 ID。备份恢复成功后切换代际，前端主动失效相关缓存，避免旧修订数字相撞。失败的事务和 CSV 预览回滚不发变化。

前台先用约 2 秒的单个轻量轮询并在恢复可见时立即检查。后台暂停，离线进行退避，保留当前缓存和连接提示。域变化只失效依赖的激活查询，例如新跟进影响时间线、最近联系排序和选中客户，IMAP 或 SMTP 变化影响邮件及 CRM 派生指标。查询仍由 Core 按分页服务，原有桌面成功后的立即失效照常。表格行和详情使用 ID，加载过程中保留选择和滚动。前端有未保存草稿时只更新远端状态并提示冲突。目标为已打开页面可见时在下一次成功检查后更新，健康本机 P95 在 3 秒内。 [TanStack Query 失效](https://tanstack.com/query/latest/docs/framework/react/guides/query-invalidation)

评估过常驻管道事件、Tauri 事件和 SQLite `PRAGMA data_version`。现有管道一次一请求，入门方案无需常驻订阅。Tauri 事件最多做 Rust 到 WebView 的桥接，仍需从独立 Core 接收变化。 `data_version` 的值只能在同一连接前后比较，难以在 SQLx 池或恢复场景给出稳定、带域的修订。若真实负载需要再用专用监听通道，保留同一版本合约。 [SQLite PRAGMA](https://sqlite.org/pragma.html#pragma_data_version) · [Tauri 前端消息](https://v2.tauri.app/develop/calling-frontend/)

### 5.2 状态、日期与并发编辑

- 阶段沿用现有枚举。等级 A/B/C、样品、客户类型、来源及批量订单进度使用稳定 code，标签、颜色和显示顺序可配置。客户来源、创建来源 `manual/agent`、邮件开发状态是不同字段。重点视图绑定等级 code，样品视图以确认的活跃样品选项为条件，所处业务阶段独立。
- 为状态增加候选 Code 元数据和保留旧文本的迁移路径。确定别名映射为 code，保留迁移前原值及来源。歧义或未知显示「历史值 · 原文字」，可按历史值筛选和手动映射。旧 V1 MCP 字符串输入通过兼容适配层，识别明确别名，未知值走兼容通道，V1.1 写入使用 code。旧 CSV 继续接收常用列和历史值。标签改名与列显示名均不影响已保存视图。迁移先以匿名样本覆盖“未寄样”“无样品”“已完成”等不同解释。
- Core 共用一个联系时间计算层。把有效人工首次、人工最近及确定的沟通事件作为时间证据，首次取最小非空值，最近取最大非空值。仅填写一项时，该日期也构成另一端的已知证据，界面可保留来源和精度。电话、会议、真实聊天、样品沟通、SMTP 已接受和已确认来信可记录为沟通。资料编辑、纯便签、审批和退信只更新各自事实，旧 `other/note` 可由用户显式标注为真实沟通。最近回复时间来自确认的人类回复，保留邮箱同步口径。列表、排序、详情、视图及 Agent 使用同一计算结果；旧手填时间仍可看和修改。
- 增加单调 `contact_revision` 与 `expected_revision`。新版桌面、行内编辑和 MCP 使用白名单字段 PATCH，在事务内比较版本并保存，校验失败保留本地输入。旧命令保留兼容入口，服务器给新客户端启用强校验。新接口冲突返回当前修订、可比较字段及稳定错误码。子项按 ID 局部变更，删除需来自显式操作。Agent 调查建议独立记录来源 URL、日期、事实/推断/待确认及值，不覆盖人工确认的字段。候选转正、查重提示和归档继续复用 Core。

### 5.3 异步审批与队列反馈

新增 `approval_attempts` 或等价的短查询对象。启动返回尝试 ID 和对应修订，Core 持久化「等待确认、已批准、用户取消、快照已变化、写入失败」等结果。原生审批交互移出请求超时，但验证与审批写入在 Core 完成。UI 用版本信号刷新，不以弹窗已打开当批准成功。Core 在读和入队返回审批有效状态与截止；列表、详情和 MCP 一致显示过期及重新申请动作。发送后分别显示排队、提交中、SMTP 已接受、明确失败、结果未知，展示作业 ID、时间、原因和安全下一步。结果未知只提供发件箱核对及人工记录结果。靠模拟 SMTP 和可注入的审批对话结果验证，保留 Windows 10/11 原生窗口实测。


## 6. R2/R3 自定义客户数据库

### 6.1 数据与类型

保留 `contacts`、`contact_methods`、`customer_interests`、`lead_candidates`、`activities` 等核心表。系统字段使用固定 ID，例如 `sys:stage`、`sys:email_development`。标签、列及布局可配置，必要字段和业务含义由 Core 管理。自定义字段 ID 使用 UUID，分享同一套类型、校验、编辑和查询合约。

| 建议结构 | 字段与约束 |
| --- | --- |
| `crm_fields` | ID、类型、名称、类型配置、默认值、顺序、归档时间、是否允许 MCP 写入、修订。用户字段默认可读，写入需用户标记允许。敏感字段可设仅桌面可见。 |
| `crm_field_options` | 字段 ID、选项 ID、名称、语义颜色、顺位、归档时间。选项 ID 稳定，颜色从有限 token 中选择。归档后历史选择仍可读，禁作新值。 |
| `crm_field_values` | `(contact_id, field_id)` 唯一。显式、可索引的 TEXT / NUMBER / DATE / BOOL / 单选列，CHECK 约束放置类型；Core 校验与字段定义一致。索引用 `(field_id, normalized_value, contact_id)` 或日期数字等专用列。 |
| `crm_field_multi_values` | `(contact_id, field_id, option_id)` 唯一，复合外键保证选项属于字段。支持 contains-any、contains-all 和 is-empty。 |
| `crm_views` / `crm_layout_preferences` | 视图 ID、名称、顺序、隐藏、默认、修订、查询配置和布局 JSON。详情属性顺序、折叠和隐藏空项单独存布局偏好。配置有 schema version，Core 验证全部引用。 |

文本和 URL 长度有界，URL 限 http/https，而且用户点击才打开。数字拒绝 NaN/Infinity，有界，日期型用日历日期而非 JS 本地午夜时间戳。复选框允许未设置与 false 的区别。默认值只在新建时按 Core 策略应用，更新默认值不偷偷重写历史。空值由明确清空动作形成。字段、选项归档可恢复，保留历史值和筛选引用。已有值的类型转换先显示可转换、冲突、不可转换数量，推荐新建字段并保存逐条结果，原字段留存；只对明确安全的空字段做原位变更。 [SQLite JSON 适合有界配置](https://sqlite.org/json1.html) · [SQLite 索引参考](https://sqlite.org/partialindex.html)

### 6.2 保存视图与查询

- 内置「全部客户、重点客户、样品客户」作为可复制、可隐藏的预设。保存视图支持新建、重命名、复制、归档恢复、拖排、默认。初次升级保留熟悉的入口，阶段看板作为布局切换。视图保存筛选、排序、可见列、列序、列宽、行密度、看板分组和布局。用户能把操作中的临时条件保存为当前或新视图，临时查询无需影响默认视图。
- 查询合约采用有界的类型树。组节点 `and/or`，叶节点 `field_id + operator + typed_value`。比如「国家为德国或法国」和「等级为 A」组合，服务端一次编译。建议初始限制为深度 3、条件 30、排序 3 键、分页 20/50/100。新增筛选涵盖日期相对区间、空值、产品型号和邮件状态，FTS 搜索保留。Core 白名单映射列和算子，值通过 SQLx 绑定。未知/归档字段在视图管理中明确标记，启用的视图若含失效规则应报错并引导修复，避免意外扩大结果集。
- SQLite 输出稳定排序，末尾以联系人 ID 打破并列。表格和看板走同一筛选与权限范围。看板按阶段或受支持的单选字段分组，含未分类列。每列独立分页，筛选/排序变化时重置游标。多选字段初期不用于看板分组，避免同一客户多张卡片的修改歧义。拖卡跨组只触发一次带修订的 Core 更新，失败回原组并定位提示。
- TanStack Table 管列配置、勾选和分页状态，Radix 与类型化单元格编辑器处理交互。双击或 Enter 编辑，文本用 Tab/Shift+Tab 保存并移到可编辑格，Escape 放弃并归焦原单元格，选项/日期弹层优先处理自身按键。支持列宽、列序、隐藏、复制及整行选择。批量修改初期只支持有明确含义的可编辑状态与自定义值，显示范围和预检冲突，按明确用户选择的 ID 保存，建议单批最多 100。异步错误保留选中行和未提交的值。客户、联系方式及邮件安全字段不进入无预览的批改动作。

## 7. R4 界面与桌面交互

**以附图为视觉基准。** 保留深蓝左栏、浅色工作面、浓青色主操作、顶部视图标签、低噪声工具栏和右侧详情。FTW 使用真实客户数据、原始渠道和产品兴趣。对没有真实图片的兴趣只显示型号、品类、采购量和需求。统计可折叠为一条小型摘要，值来自 Core 对当前范围的计数，筛选、归档状态及部分加载时有清楚口径。

| 区域 | 规格与交互 |
| --- | --- |
| 应用壳 | 宽屏导航 172–192 CSS px，有图标、文字和当前页标识。中等窗口缩成 56–64 px 图标轨道，仍有可访问名称。系统标题栏沿用 Tauri 原生行为。主区域可收缩，有统一主滚动。 |
| CRM 首屏 | 标题与「新增客户」同一行，下面是保存视图标签，再下一行合并搜索、阶段、排序、字段、更多筛选及表格/看板切换。普通筛选用 Popover，已生效条件显示计数和可清除条目。目标 1080p/100% 缩放时导航内 CRM 首行距离内容顶边不超过约 190 CSS px。 |
| 客户表格 | 以强对比联系人姓名为首列，悬停和选中态明确。公司、国家、阶段、等级、样品状态、兴趣、邮件开发状态、最近联系为可配置列。默认压缩行高 36–40 px，标准 44–48 px。时间等宽数字，状态文字加标记，颜色仅补足语义。表头固定，独立横向滚动且列操作可用键盘。 |
| 空/错/加载 | 尚无客户时显示「新增客户」「导入 CSV」。筛选无结果时保留条件并给「清除筛选」。连接错误显示稳定错误，允许重试，并保留上次内容的可识别状态。分页按钮按真实 `has_more` 或计数启用。 |
| 新建与编辑 | 姓名为唯一必填业务字段。公司、地区、主要邮箱和客户阶段优先。联系方式、其他业务状态、产品兴趣、自定义属性、调查来源及备注分组折叠。错误将焦点带到首个出错字段，未保存时关闭有提示，失败保留输入。 |
| 客户详情 | ≥1280 CSS px 时约 400–460 px 非模态侧区，保留可交互的列表。窄屏使用真实模态，最大宽度适配窗口。头部固定，可切换「概览 / 沟通渠道 / 邮件 / 时间线 / 调查与审计」。可快速复制联系方式、从匹配已保存邮箱起草、增加跟进和就地改属性。属性顺序、隐藏空值和折叠可持久化。 |
| 时间线与邮件 | 每项显示业务发生日期、渠道、来源和摘要，邮件行按 ID 打开对应会话。使用同一时间线表达手工、Agent、邮件、样品和沟通事件。展开才读详情，正文为纯文本，邮件状态和下一步在对应条目明确显示。 |

**样式 token。** 初始：导航 `#102439`、选中/主操作 `#00756F`、画布 `#F3F7FB`、表面 `#FFFFFF`、主文本 `#14253B`、次文本 `#52647B`、边框 `#DDE6EF`。用 CSS 自定义属性管理品牌、表面、文字、状态、阴影、间距和密度，基准间距 4/8/12/16/24，圆角 6/8/12。主 UI 优先 Segoe UI 与系统中文字体，正文约 13–14 px，标题约 22–26 px。青蓝与红黑采用独立主题映射，状态颜色有相同语义，核心文字、焦点、悬停和高对比须实测。

**滚动与焦点。** 以附图为默认宽屏，而以 900–1600 CSS px 的实际可用宽度、100–175% Windows 缩放验收。宽屏抽屉独立滚动，关闭按钮与保存入口固定，列表同屏时用分区管理滚动，不强制焦点陷阱；窄屏模态陷住 Tab，Esc 先关最上层弹层，关闭恢复触发焦点，原触发项已因筛选移除则回到列表区。 `Ctrl+K` 聚焦搜索，与输入法编辑态、菜单和模态避让。用语义标题、清晰键盘焦点、文本状态、简化动效和高对比兼容 `forced-colors`。 [Radix Dialog 焦点](https://www.radix-ui.com/primitives/docs/components/dialog) · [Linear 键盘交互参考](https://linear.app/docs/issues)


## 8. R5/R6 Agent 自动发送、邮件开发与 CRM 联动

### 8.1 用户授权与信任边界

| 模式 | Agent 可做什么 | Core 决定 |
| --- | --- | --- |
| A · 持续授权自动发送 | 查客户和邮件、记录调查、写草稿、在获授权的用途和额度内逐个排队、查结果。 | 经 FTW 用户主动启用，持续有效，撤销或修改立即生效。每封须通过同一校验和审计，无逐封弹窗。 |
| B · 人工审批 | 查、写草稿、申请审批、提交已审批的修订。 | 升级保留 V1 默认行为。审核与撤销规则继续有效。 |
| C · 禁止 Agent 发件 | 查、写草稿。 | Agent 不能提交自动或已批准发送。用户从桌面主动处理邮件仍走桌面的人工流程。 |

授权记录绑定 profile、已登记的本机 Agent 接入身份、发件账户和允许用途，保存策略修订、批准者、时间、单任务/日总量、最小间隔和可撤销 ID。 `task_id` 是追溯字段，不能作为身份。一个 Agent 的额度受 Core 发出的任务 ID、同账户/同身份滚动额度、有效任务数和间隔共同限制，避免重新命名任务重置日限额。初始建议为每任务 20 封、近 24 小时每发件账户 30 次实际尝试、最短 60 秒。由用户配置，Core 永远取账户、策略和服务商设置允许的更严格值。所有出站一封一收件人，无任意收件人数组、暗送群发或自动回复。邮箱、拒发地址/域、用途，及是否允许已批准的目录附件由用户在授权时看见。

**必须保护设置请求。** 现有管道 ACL 只证明客户端属于同一 Windows 账户。 `Operation::Mail` 与 `Operation::Agent` 共用管道，Rust 枚举、前端路由和 MCP 工具名都不能证明一条请求来自人工。R5 将发件账户与政策、拒发名单的放宽、恢复后重启用、附件库白名单等管理操作放进受限的桌面路径。Core 生成一次性挑战，绑定 profile、策略内容和修订，在 Core 控制的本机交互中核验用户，再由服务内部事务记录授权，业务参数不接受「我已批准」或发件身份自报。消息进入 Core 后仍检查调用权限。按接入注册的会话或安装标识由 Core 绑定，普通 Agent 仅得到自己的受限授权路径，不能自行提升。Windows Hello HWND 接入实测 Win11，官方接口最低要求 Build 22000。Win10 验证经正确认证并匹配 SID 的原生凭据路径，兼容性或核验能力不足时保持模式 B/C。普通 `MessageBoxW` 点击只能作人机交互，无权替代此保护。此边界覆盖 FTW API、MCP、无权限 Agent 和非当前 Windows 用户，无法隔离拥有同一 Windows 用户任意本机执行能力且可控制文件/进程的恶意程序。 [命名管道 ACL](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights) · [Win11 用户核验 API](https://learn.microsoft.com/en-us/windows/win32/api/userconsentverifierinterop/nf-userconsentverifierinterop-iuserconsentverifierinterop-requestverificationforwindowasync) · [Windows 凭据 UI](https://learn.microsoft.com/windows/win32/api/wincred/nf-wincred-creduipromptforwindowscredentialsw)

### 8.2 自动发件路径与原子去重

1. Agent 用 Core 查询已存在的联系人、同域/同邮箱、拒发情况、首次开发历史和未决作业。外部搜索内容作为待验证数据保存。客户、阶段和发件权限变更都经 Core，HTTP 和网页内容不提供权限。
2. 以客户或候选转正的正式 ID 建立单收件人草稿，标注邮件用途、Agent 任务、来源及可选父会话。建议用途枚举为 `first_outreach`、`outreach_followup`、`quote_communication`、`sample_communication`、`other_business`。来信统一从 IMAP 认领 `customer_reply` 或相应事件类别。报价或样品用途用于沟通分类，无报价单和订单模块。
3. 在短事务内比较草稿修订和权限版本、重新确认收件人属于活动客户、检查拒发、额度与用途，并持久化规范化快照摘要、作业和认领键。幂等键以「Agent 身份、operation、request_id」作用域绑定参数摘要。同值重复返回作业 ID，变更参数返回冲突。对 `first_outreach` 使用独立认领表及工作区级唯一地址键，与队列一起提交。换草稿、联系人或发件账户无法绕过。地址键沿用经测试的 Core 规范化逻辑，域名统一处理，保留本地部分的原始值，不把不同邮箱服务商的点号或加号规则做成全局等价。同名和同域不同地址给提示，允许核对后形成不同联系人。
4. Worker 取队列后做第二次权限、版本、哈希、拒发、额度和发件设置校验。权限关闭或缩紧，尚未进入提交步骤的任务阻断且保留记录。频率额度用实际尝试占额，所有身份和人工路径仍受账户上限。保持一次 SMTP worker 顺序准入，若以后并发则用事务认领和账户节流。已进入 SMTP 的网络操作可能已经提交，界面如实说明。Core 为队列操作记录稳定 Message-ID，保存服务器接受、明确拒绝或结果未知。
5. 首次开发的排队、发送中、已接受、结果未知均占用认领键。只有可证明未提交的失败才可解除，采用受控的重新排队动作并保留审计。当前 Lettre 无法可靠提供阶段的错误继续标记未知。未知状态只在有证据核对后由用户处理，Mail/Agent 不提供自动复投。恢复、换机和移植进入受限状态。老记录用途是 `legacy_unclassified`，历史已提交/未知的同邮箱默认阻断新首次开发，允许人工核对并修复归类。
6. 正常业务跟进可以向已建立会话的客户继续发送，使用独立用途和策略。针对完全无联系历史的邮箱，即使被 Agent 标成「样品沟通」或「报价沟通」，Core 仍按首次联系执行限制或要求明确人工证据。邮件发送成功后写入 `email_messages` 和 `activities` 引用，保存当时的联系人/公司快照、账户、目标邮箱、用途、主题/正文、附件引用、来源、任务 ID 和会话 ID。首次认领状态与失败审计可回读。

为防止恢复较旧备份丢掉已尝试记录，建议保留一个恢复范围之外、受同 profile 锁保护的最小发送安全账本，存不可重放的作业键、收件规范化键、配额事实和结果。业务事务先建队列，网络前必须持久写入安全账本的发送意图，失败就阻断。系统不依赖两个 WAL 库原子同时提交。写意图后崩溃取更保守状态，和现有发件记录、备份后的账本做对账。恢复时常规备份只恢复业务库状态，保留较新的发送约束，自动授权失效。新机器或外部恢复缺少安全账本时，暂停自动发送，先导入相匹配的安全历史或人工核对。便携目录迁移与备份清单都包含这项恢复规则。

一个验收示例是「调查 20 家德国数码相机经销商并开发」。Agent 保存候选、调查来源与置信状态，先搜索 FTW 的同名、同邮箱、同域和历史动作，有效联系方式关联正式联系人。可发对象分别建草稿并按政策排队。MCP 回报新增、已存在、需人工处理、排队、已接受、失败和未知各有对应事实，不因任务目标为 20 而绕过重复、拒发或配额。后续同步回复，Agent 查询会话并记录下一次业务动作。

### 8.3 邮件开发状态、同步和退信

将 CRM 阶段与邮件开发分开。Core 提供每客户和每会话两个查询投影，V1.1 优先就地从现有事实和索引查询。新投影视图字段包括 `latest_sent_at`、`latest_human_reply_at`、`outreach_count`、`recent_subject`、`thread_id`、`mailbox_sync_status` 和「距上次发信的天数」。事实保留原始队列作业与邮件引用。若基准测试证明需要缓存，再增加同事务失效的可重建投影。

| 汇总状态 | 证据与显示 |
| --- | --- |
| 不再联系 | 当前联系地址/域受拒发约束。多邮箱时显示受影响地址，标明是否所有渠道均禁止。退订或用户确认后加规则并让未提交作业阻断。 |
| 已回复 / 已退信 | 同一相关会话有确定的人工回复，或已验证、与特定提交匹配的永久 DSN。对旧邮件和多会话保留独立事实并按所选业务会话展示。自动回复、延迟通知单独标记。 |
| 结果未知 / 失败 | 对最新相关作业显示提交未知或明确失败，附原因、时间和核对入口。未知标志始终能从客户详情和发送历史看到。 |
| 待发送 | 有尚未提交的待审批、获批或已排队作业，展示子状态。单纯草稿与研究活动不会算已联系。 |
| 已发送未回复 | SMTP 已接受并且相关邮箱已有覆盖发信之后的成功完整同步，界面写「截至某次同步未见回复」。运行中、部分、失败、待同步或邮箱不可用时显示「SMTP 已接受 · 回复待同步」。 |
| 未开发 | 无相关发送尝试和已确认业务会话。人工或 Agent 写入的文字不改变邮件投影。 |

回复关联按账户和真实发送/收件事实核对 Message-ID、In-Reply-To、References，校验对方地址属于联系人或用户明确确认的别名。无头或转发命中只有唯一地址可连到联系人，存为独立会话或待确认。主题相同和名字相似只作提示。新出站跟进保存父消息并在快照中绑定受校验的回复头。对 `Auto-Submitted`、退信 `multipart/report; report-type=delivery-status`、别名和转发单独处理。DSN 至少需收件、原始消息或回执 ID、账户、Action/Status 代码交叉吻合，无法确认进入待关联。自动回复和退信不计最近人工回复。用户确认退订后记录渠道和时间，邮件中的链接或命令只作为不可信内容呈现。联动保留每邮箱的 `last_attempt`、`last_success`、完整游标、批次 `has_more` 和同步错误。 [消息头 RFC 5322](https://www.rfc-editor.org/rfc/rfc5322.html#section-3.6.4) · [DSN RFC 3464](https://www.rfc-editor.org/rfc/rfc3464.html) · [自动回复 RFC 3834](https://www.rfc-editor.org/info/rfc3834) · [SMTP RFC 5321](https://www.rfc-editor.org/info/rfc5321)


## 9. R7 附件与大邮件

### 9.1 发件附件

现状是 `FileReader.readAsDataURL` → Base64 JSON → 1 MiB 命名管道 → `attachments_json`。现在每封最多 4 件、合计 384 KiB。本轮建议将产品目录 PDF、JPEG/PNG 实物图片和常用的 WebP 作为首批大附件白名单，同时支持读取及发送原已存的合法旧附件。按账户先以原始合计 **10 MiB** 作为评测目标，保持 4 件上限用于首批验证。Base64 及换行会将 10 MiB 加到约 13.7 MiB，再加邮件正文和 MIME 头。以完整 MIME 大小和服务商允许值中较小的限额调度。服务端 SIZE 能力需用实际 transport 版本验证是否可读取，不能保证所有服务器提供。不可读取时使用用户配置的服务商安全上限，并把确定的拒绝显示给用户。 [Base64 RFC 2045](https://www.rfc-editor.org/rfc/rfc2045.html#section-6.8) · [SMTP SIZE RFC 1870](https://www.rfc-editor.org/info/rfc1870) · [Lettre MIME API](https://docs.rs/lettre/0.11.24/lettre/message/struct.Attachment.html)

- 用桌面受限命令在原生文件选择后由 Rust 读取候选文件并流式写到 profile 的临时区，既有权限范围中 Agent 只传经过用户入库和授权的附件 ID。保留最多 1 MiB 的控制帧。实施前比较 Core 所有的原生选择与专用有界二进制导入命令，避免让通用路径参数直接成为读文件入口。MCP 工具不接收路径、URL、Base64 或可任意读取的文件句柄。
- `mail_attachment_blobs` 只保存内容 hash、长度、类型、状态和存储版本，`mail_attachment_refs` 关联草稿修订、历史提交和入站 MIME 部分，保存文件名及显示顺序。对象按 hash 写入 `data/attachments/`，文件名只用于显示/MIME 元数据。Core 校验扩展名与已识别内容、文件上限、Windows 保留名、路径跳转、重解析点、硬链接和路径重开竞态。通过受验证文件句柄读取，归属 SQLite 前先持久化原子发布对象。改动本地原文件不会改变已入库内容。
- 授权快照 V2 绑定附件 hash、字节数、名字、MIME 类型和顺序，发送前从同一已验证句柄复核内容，核验失败在建立 SMTP 提交前停止。优先复用 `lettre`，对单挂起发送限制内存。先测近限额真实字节、MIME 编码和重复分配，再决定是否采用流式 MIME 构建/存储。失败保留已被草稿、历史或备份引用的对象；未引用的临时件在定期核对及宽限后回收。
- 新备份使用 SQLite 一致性快照加 manifest 和所引用对象，快照引用集合在复制期间固定。写入临时包，核对散列、文件数和 Schema 后原子公布备份。恢复先校验格式、尺寸、hash、目录边界，暂存所有对象及 DB，保留当前安全备份，再由 Core 单写入者切换新的 bundle。旧纯 SQLite 备份仍可恢复，保留旧附件读取或采用可回滚转换。先双读后迁移，不重算历史审批凭据，旧格式的待发送或旧摘要有变化则需要新批准。

### 9.2 大收件与附件按需读取

- 默认同步取得 UID、UIDVALIDITY、RFC822.SIZE、INTERNALDATE 及限长头部，必要时取 `BODYSTRUCTURE` 和安全的文本部件预览。先写有界元数据、会话关联、解析完整程度，再提交游标。UI 显示「仅邮件头」「正文局部」「可下载附件」「远端已移动或失效」「已下载」等实际状态。
- 用户在邮件详情选择「下载正文」或单个附件时，由 Core 按邮箱/文件夹/UIDVALIDITY/UID/部件 ID 取得 `BODY.PEEK` 分段或受限整封，设置连接总时长、每段上限、总量、并发、MIME 深度和解码膨胀限制。试验值可用 256 KiB 分段、单件下载 15 MiB，按基准调整。超过上限只留安全预览。必要时在解码器输入层限制 IMAP literal，不能只靠请求范围或事后截断。UIDVALIDITY 变化后重新查找已验证的远端位置，不能定向读取上个会话 UID。
- 入站身份避免依赖「不完整头部 hash 与完整原文 hash 相同」。使用账户/文件夹/UIDVALIDITY/UID 识别位置，采用消息头和已知原文指纹核对升级。不能证实是同一条就保留待确认。落库设计把「内容完整性」和「消息用途」分开，下载完成后不能重复生成业务跟进。对 HTML 继续纯文本预览，附件可见元数据与本地保存由用户触发，且不会自动打开或执行。Agent 初期只读取限长正文和元数据，是否允许读取授权附件文本需要另行评审。 [IMAP RFC 9051](https://www.rfc-editor.org/rfc/rfc9051.html) · [async-imap Session](https://docs.rs/async-imap/0.12.0/async_imap/struct.Session.html) · [mail-parser MIME 模型](https://docs.rs/mail-parser/1.0.1/mail_parser/)

## 10. IPC、MCP 与迁移边界

| 接口族 | 拟新增或扩展 | 兼容行为 |
| --- | --- | --- |
| 刷新与编辑 | `GetChanges`、`PatchContact`、`RevisionConflict`、`GetApprovalAttempt`、`ListContactMailHistory` | 保留旧请求、读字段和旧 JSON Schema。新字段默认可缺省，并保持读取现有值。使用协议能力字段允许新版 UI 显示旧 Core 需要重启升级。 |
| CRM | `ListFieldDefinitions`、`PatchFieldDefinition`、`PatchFieldValues`、`ListSavedViews`、`QueryCustomers`、视图及布局 CRUD | 桌面可管理定义和视图，MCP 可查询允许范围及写允许的值。 `query_customers` 使用有界结构与范围，保留 `list_customers`。UI 与 Agent 共用筛选编译。 |
| Agent 邮件 | `check_outreach`、`list_outreach_history`、`get_send_status`、`enqueue_authorized_email`，扩展 `create_email_draft` 和会话查询 | 旧 `request_email_approval`、`send_approved_email` 保持逐封语义。新工具可返回结构化、带版本的任务/状态对象和稳定错误。按会话绑定功能权限，新增工具不开放批准、权限管理、任意文件、系统命令、SQL 或密钥。 |
| 管理、附件 | Core 内部策略设置及撤销，桌面受控导入/导出和附件关联操作 | 新管理命令需可信人工交互并受限使用。新草稿快照按版本读附件 ID，已有 Base64 附件解析兼容。 |

迁移 **只追加**。建议 R1 增加 `0006_crm_consistency`，包含域修订、联系事件标识、状态码兼容和审批回执。R2/R3 增加 `0007_custom_crm` 和需要的后续视图迁移。R5/R6 在授权、快照和首次认领设计通过后增加兼容 migration，保留 `email_approvals` 的所有旧记录。当前 `email_outbox_jobs.approval_id` 为非空唯一外键，自动作业不能填假批准。新模型须用两类授权引用的 CHECK，批准 ID 保留唯一。实施前设计 SQLx 事务内保留外键的重建顺序，包括引用队列的 `email_messages`、旧 ID/时间、索引与触发器，跑 `foreign_key_check`。若无法安全重建，额外增加统一的发件授权关联表，再在共同 worker 中迁移读取两种旧新来源。 `first_outreach` 认领表单独唯一，复用队列表和事实表，不建立两套互相竞争的队列。

上线前分别验证 V1 备份和迁移版本 1/3/5 的夹具，以及新版本备份恢复。升级前先关闭同 profile 旧 Core 并制作一致性备份。新程序失败时按旧程序 + 升级前备份回退，先保全升级后新增数据。老数据库和格式可向前导入，旧 EXE 不直接读取新业务库。MCP Schema、错误说明、示例和连接文档随阶段更新。 [现有队列外键](../crates/ftw-core/migrations/0003_mail.sql#L28) · [现有迁移入口](../crates/ftw-core/src/storage.rs#L13) · [SQLite 备份 API](https://sqlite.org/backup.html) · [MCP 工具合约](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)


## 11. 分阶段实施、主要文件与阶段验收

每阶段在同一仓库、独立小提交实现，先跑本阶段回归。以下新文件名为建议落点，最终以最小改动为准。

| 阶段 | 交付和关口 | 预计主要文件 |
| --- | --- | --- |
| R0 · 评审 | 源码审查、开源选型和本方案。用户确认后进入 R1。 | 本方案和 [开源评估表](FTW_可复用开源方案评估表.md) |
| R1 · 正确性 | 跨 MCP 更新、日期、历史状态兼容、审批可见回执、表单冲突与分页。以隔离 Core 和已打开页面复现再回归。 | `crates/ftw-core/src/crm/{contacts,validation,timeline}.rs`、`agent.rs`、`server.rs`、`mail/{approval,mod}.rs`、`migrations/0006_*.sql`、`ftw-ipc`、`src-tauri/src/main.rs`、`apps/desktop/src/{main.tsx,crm,mail}` |
| R2 · 字段 | 七种类型、增改/归档、选项与排序、个别敏感字段、Core/MCP 值写入、旧值保存、动态表单。字段重命名、归档和重启后值保持。 | `crates/ftw-core/src/crm/fields.rs`、迁移、`ftw-ipc/src/crm.rs` 与 `agent.rs`、`ftw-mcp/src/main.rs`、`apps/desktop/src/crm/{fields,editors}/` |
| R3 · 保存视图 | 查询 AST、列控制、表格/看板、键盘编辑与拖排、有限批量。核对同一视图的表格/看板范畴一致且保存持久化。 | `crates/ftw-core/src/crm/{query,views}.rs`、视图迁移、MCP Schema、桌面 `crm/{views,table,board}/`、锁文件 |
| R4 · UI | 导航与主题、紧凑工具栏、空状态及 CSV 入口、新建表单和可配置详情。按附图审查视觉，同时验收小窗口和原生焦点。 | `apps/desktop/src/main.tsx`、`styles.css`、`crm.css`、`crm/Contacts.tsx`、`Timeline.tsx`、`components.tsx`、`data/DataTools.tsx` |
| R5 · 发件安全底座 | 政策管理、稳定 Agent 作用域、统一队列、第一次触达认领及安全账本、撤销与限额、测试模式。默认继续模式 B，模式 A 对真实 SMTP 保持关闭至 R6 验收。 | `crates/ftw-core/src/mail/{mod,approval,worker,smtp}.rs`、`policy.rs`、`outreach.rs`、发送迁移、Tauri 能力、`ftw-ipc/src/mail.rs`、`ftw-mcp/src/main.rs` |
| R6 · 邮件联动 | CRM 投影、发件历史、真实回复/DSN 分类、查询、恢复保护与审计。并发与对账用例共同通过，才为具备核验能力的安装开放用户主动选择模式 A。 | `crates/ftw-core/src/mail/inbox/{store,transport}.rs`、`crm/contacts.rs` 与查询层、投影索引、`apps/desktop/src/{mail,crm}/` |
| R7 · 附件 | 本地内容存储、目录/图片、正文和附件按需下载、老数据兼容与完整备份。做近上限及损坏对象回归。 | `crates/ftw-core/src/mail/attachments.rs`、`smtp.rs`、`inbox/transport.rs`、`storage/backup.rs`、附件迁移、Tauri 文件选择及 `apps/desktop/src/mail/` |
| R8 · 验收 | Win10/11 x64 无开发工具便携验证、Codex/WorkBuddy 接入、经授权测试邮件、经过验收的 ZIP 和 SHA-256。 | `scripts/{test,build-portable}.ps1`、`scripts/test-mcp.mjs`、`crates/ftw-core/tests/`、桌面 UI 测试、分发说明与变更记录 |

### 11.1 回归与验收矩阵

- **R1 可见一致性**：已打开表格、详情、时间线，MCP 创建、改等级、改阶段、写活动、SMTP/IMAP 改状态后按域刷新。验证不重复加载所有页面、失效期间保留滚动、恢复及 Core 重启代际正确，离线与失败保留上次可识别缓存。人工日期早晚边界、空值、时间区、普通便签、失效快照、审批取消、过期、写入失败、SMTP 未知覆盖。双客户端编辑保留各自输入，数据冲突可解释。
- **R2/R3 数据能力**：七类字段创建和重启读回、字段及选项重命名后值和筛选仍有效、历史文本迁移和用旧 MCP 写入、类型转换拒绝目标、敏感字段不可从 MCP 读取、字段归档与恢复。保存视图的条件、列序、列宽、筛选 AND/OR 和默认行为，布局切换同 ID 集合，行内及有限批量更新正确落 SQLite，异常回滚。校验带注入字符的值、无效字段、巨大过滤树和分页拼接。
- **R4 桌面交互**：鼠标、键盘、Tab/Enter/Esc、中文输入法、焦点复位、右栏滚动、窄屏回退、表头列宽拖动、无数据/无匹配/错误状态。用 Vite 隔离页面做 UI 自动化，Windows 真应用优先评估 Tauri WebDriver。记录 Win10/11 WebView2、显示缩放和逻辑尺寸。 [Tauri WebDriver](https://v2.tauri.app/develop/tests/webdriver/)
- **R5/R6 发送与联动**：本地假 SMTP/IMAP 验证模式切换、撤销、越权提高、伪装身份、无授权/过期/改稿、收件地址/域拒发、节流、首封并发认领只一单、换 ID/账户重复也阻断、正常跟进可发。测试 SMTP accepted/拒绝/DATA 断线、Core 掉电或重启、未知无自动重发、安全账本与备份回退。检查 DSN 永久与延迟、自动回复、别名、不完整同步、退订、重复 UID 和会话人工更正。
- **R7/R8 存储与性能**：模拟 384 KiB 历史件、产品 PDF、JPEG/PNG/WebP、阈值两侧、整信大小和 SMTP 拒绝。覆盖出入站失效、临时中断、篡改 hash、重解析点、磁盘满、对象缺失、备份清单不符、SQL 回滚与可读恢复。分别造 100/1,000/10,000 条合成客户，记录首屏、快捷筛选和看板请求 P50/P95、包体和内存。健康本机评审目标为 R1 可见刷新 P95 ≤3 秒，10,000 条的典型筛选本地 Core P95 ≤300 ms。以固定样本和预算复测，未测不得宣称达标。
- **命令**：每阶段先跑对应前端和 Rust 的特定用例，再跑 `scripts/test.ps1 -Clippy`，含 npm 锁依赖、lint、TypeScript、Vite、Rust fmt、测试、build 和两个 MCP 协议回归。R8 再运行 `scripts/build-portable.ps1` 带发布构建回归。真实 SMTP/IMAP 只使用得到明确许可的测试账号和测试收件人。

### 11.2 Windows 便携版验收

按 Windows 10 22H2 x64 和 Windows 11 x64 分别记录系统版本、WebView2 版本及缩放设置。对没有 Rust/Node/VS 的环境验收原生 EXE 与系统 WebView2 依赖，缺失时显示明确安装入口。整包解压到普通、中文和空格路径，验证 `portable.flag`、数据目录、Core 单实例、只读目录的提示。重新启动后验证客户、筛选、详情和备份。验证同 Windows 用户迁移应用目录与恢复备份，换用户测试 DPAPI 提示、重新授权与邮件自动模式回到受限状态。工作台关闭时 MCP 可查写受授权的 CRM，并在支持的已授权条件下由 Core 驱动队列。分别实测 Codex 与 WorkBuddy，签名与公开分发条件由用户确认。 [Microsoft WebView2 分发](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution) · [FTW 便携构建](../scripts/build-portable.ps1)

## 12. 风险、暂缓与评审决定

| 风险或未决条件 | 决策或门槛 |
| --- | --- |
| 保留旧数据又统一字段 | 历史别名需真实但可匿名的举例确认。迁移保留未知及真实原值，不自动重分类歧义样品状态。 |
| 同账户应用边界和 Win10 核验 | R5 在 Win10/11 实测授权设置、同 SID 直接 IPC、工作台与 Agent 角色。设备无法可靠核验时自动模式不可启用，继续人工流程。 |
| SMTP 提交、DSN 和不完整同步 | 每个提供商要验证连接、大小和退信可归因程度。Core 区分已接受、确认失败、未知、延迟与用户确认。真实收件校验单独授权。 |
| 恢复导致去重历史回滚 | 发件授权停止，安全历史独立保留，旧业务库恢复后对账。跨机器移动时确认携带完整数据和安全账本，否则持续授权暂停。 |
| 组件维护与许可证 | 公共 GitHub 仓库不等同于有开源授权。复用已核验的 MIT/Apache-2.0 组件，记录第三方清单。AppFlowy 与 Twenty 的业务代码仅作研究。检验每个依赖的实际 tree shake、键盘与 WebView2。 |
| 通用表格复杂度 | 初版控制字段类型和操作组合。函数计算、关系字段、整块粘贴平台和列表虚拟化留到需要时。优先按客户数和明确业务场景验收。 |

暂缓产品数据中心、报价和 PI/CI、订单管理、ERP、Linux、内置 AI、通用知识库、多人实时协作和完整 AppFlowy 替代。R7 暂缓任意扩展名上传、在线文件传输服务、HTML 渲染、附件自动打开和 OCR/内容提取；OAuth2、多 IMAP 文件夹自动订阅及完整邮件规则可根据实际邮箱需求另行扩展。

### 提请确认

1. **批准实施顺序**。建议先批准 R1，再执行 R2–R4；R5/R6 安全关口与 R7/R8 仍按独立验收。当前方案完成后进入评审等待。
2. **持续授权初始策略**。建议升级后默认模式 B，用户在 FTW 主动启用 A 后持续有效。建议每任务 20 封、账户近 24 小时 30 次尝试、间隔 60 秒，缺少可信核验时使用 B/C。请确认是否采用这些初始值。
3. **外贸字段与首次开发口径**。建议历史歧义值保留原文待人工映射，首次开发对同一工作区规范化邮箱全局去重，同域不同人仅提示，旧邮件用途不明时自动首发需人工核对。请确认有无需保留的惯用等级或样品状态。
4. **附件试验边界与视觉基准**。建议附图青蓝为默认，红黑为独立可切换主题。大附件先评估 PDF、JPEG/PNG、WebP 与 10 MiB 合计目标，逐服务商验收后确定发布限额。
