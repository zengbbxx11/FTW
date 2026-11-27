# FTW 外贸智能工作台——产品与技术规格书 V1.0

> 本文保留需求基线或阶段方案的历史内容。当前版本为 1.1.7；已批准的本机免令牌接入、使用体验和便携更新实现及验证范围，以[当前项目状态](docs/PROJECT_STATUS.md)为准。

> 项目代号：Foreign Trade Workbench（FTW）  
> 版本：V1.0（Windows 第一版）  
> 目标系统：Windows 10 / Windows 11，优先 x64  
> 开发方式：由 Codex 在用户本地项目中实施；按阶段开发、测试、验收  
> 本文档是需求基线。实现细节可以依据真实依赖和环境调整，但不能自行扩大第一版的业务范围。
> 当前发信规则在 V1.1 增加工作台持续授权，仍保留逐封人工审批。启动方式、现行接口和验证状态见 [README](README.md) 与 [MCP 说明](docs/MCP_MAIL.md#agent-发信权限)。

---

## 1. 产品目标和边界

FTW 是一个**本地优先、Windows 免安装便携优先、对 Agent 开放受控业务接口**的外贸业务工作台。

它不是一个内置聊天 AI 的系统，也不是完整 ERP。用户可以手动维护客户资料、查看/编辑邮件、记录跟进；Codex、WorkBuddy 等外部 Agent 可以经 MCP 检索客户、写入候选客户、建立正式客户记录、创建邮件草稿、读取历史邮件和跟进、在用户审核后触发邮件发送。

### 1.1 第一版必须实现

1. 客户联系人为核心的 CRM：新增、编辑、查询、筛选、排序、归档、去重提示、客户详情。
2. 线索池：Agent 导入待核实潜在客户，保留调查来源，支持审核后转为正式客户。
3. 跟进记录：手动创建和 Agent 创建；记录沟通方式、内容、时间、来源，显示时间线。
4. 邮件账户配置：至少支持符合标准的 SMTP + IMAP 邮箱；测试连接；安全保存凭据。
5. 开发信：编写草稿、修改、预览、人工审核、发送、发送状态记录、关联客户。
6. 回复同步：IMAP 拉取邮件、识别线程和联系人、记录客户回复、匹配不确定时等待确认。
7. MCP Server：Codex 等 Agent 不经 GUI 模拟点击即可使用业务工具；GUI 关闭后可按需启动核心服务。
8. 日志、备份、数据迁移、单实例/并发控制、真实 Windows 便携版 ZIP 发布。

### 1.2 第一版明确不做

- 产品数据中心、产品参数库、价格和报价中心。
- 订单、PI/CI、发货、ERP、财务和客户营销自动化平台。
- 内嵌 LLM、向量检索、自动搜索整个互联网的爬虫系统。
- 无人值守的大规模冷邮件群发、自动催发、自动回复、邮件已读追踪像素。
- Linux/macOS 安装包、多用户云协作、云同步、动态插件市场。

注：客户“感兴趣产品”字段采用**自由文本型号/品类**，不创建产品资料中心。界面可以为以后增加产品实体预留适配点，但不提前实现。

### 1.3 用户权限原则

- 人可以使用 GUI 进行全部授权范围内的业务操作。
- Agent 只能使用明确允许的 MCP 工具，不能直接访问数据库文件、原始 SMTP 密码或任意执行系统命令。
- **真实邮件发送需要本工作台可验证的人工批准**。Agent 可以起草、请求审核；首版通过工作台 UI 批准固定内容快照，然后 Agent/发送队列才可执行。不能把 Agent 自称“用户已同意”当作系统级审批凭据。
- 未来可以增加由宿主 Agent 提供、能够验证操作者身份的原生批准流程，但不属于第一版。

---

## 2. 技术栈与架构决策

### 2.1 固定首选技术栈

| 层次 | 技术 |
|---|---|
| Windows 桌面壳 | Tauri 2 |
| 前端 | React、TypeScript、Vite |
| UI | Tailwind CSS、shadcn/ui |
| 前端数据 | TanStack Query；复杂表格可用 TanStack Table |
| 核心后端 | Rust、Tokio |
| 本地 API | 优先 Axum HTTP，仅绑定 `127.0.0.1`，强鉴权；或同等安全的 IPC 方案 |
| 数据库 | SQLite，SQLx 和 migrations |
| 发信 | Lettre 或经验证的等价 Rust SMTP 实现 |
| 收信 | 经验证的 Rust IMAP 实现，支持 TLS 和稳定重连；优先标准协议实现 |
| Agent 接口 | MCP Rust SDK（rmcp），stdio transport |
| 日期时间 | SQLite 持久化 UTC；界面本地时区显示 |
| 凭据 | Windows Credential Manager / DPAPI（密钥不入 SQLite 明文） |
| 目标架构 | Windows x64，Windows 10/11；x86/ARM64 暂不承诺 |

不使用 Electron、Docker、远程数据库、强制在线服务。首版不启动 Python 运行时。如 Rust IMAP/认证在技术验证阶段存在明显阻碍，允许给出书面论证后再变更方案，禁止偷偷混用多个后端。

### 2.2 进程分层

```text
Workbench.exe                 Tauri 桌面 UI
     |
     v
ftw-core.exe                  本地核心业务进程（按需启动）
     |-- CRM 服务
     |-- 线索/跟进服务
     |-- 邮件草稿、审批和发送队列
     |-- IMAP 收信及邮件关联
     |-- SQLite 与日志/审计
     |
     +-- data/workbench.db

ftw-mcp.exe                   MCP stdio 适配器
     |
     +---- 连接已启动的 ftw-core 或安全地按需启动它

Codex / WorkBuddy  <--stdio--> ftw-mcp.exe
```

前端和 MCP 必须调用**同一套业务逻辑**。Core 进程与 UI 生命周期分离：UI 不运行时，Agent 仍可使用；无客户端时是否自动退出可作为配置，但不安装常驻 Windows 系统服务。

核心服务只能绑定回环接口，选取端口应防止冲突；具备本机认证令牌、Origin 校验/严格 CORS、令牌轮换以及非预期访问拒绝；禁止裸露无认证的 localhost 管理接口。安全地协调运行实例、读写和进程退出。认证令牌与核心进程元数据仅允许当前用户访问。若选命名管道亦须正确配置 Windows 用户访问控制列表。

Tauri 窗口内只暴露必要权限，严格使用 Tauri v2 capabilities；严禁为了省事启用全部 shell/filesystem 权限。

### 2.3 便携版数据模式

优先实现免安装 ZIP：用户解压即可运行（前提是系统具备受支持的 WebView2 Runtime）。

建议结构：

```text
FTW-Portable/
  Workbench.exe
  ftw-core.exe
  ftw-mcp.exe
  portable.flag
  data/
    workbench.db
    attachments/
  config/
    settings.json             # 不含邮箱密码/访问令牌
  backups/
  logs/
```

- `portable.flag` 存在时以应用根目录存储业务数据，所有路径采用规范化后的相对应用目录；无此标志时可使用 `%LOCALAPPDATA%/FTW`。
- 原始用户数据可整体迁移；**Windows 系统保护的邮箱凭据不保证跨电脑解密**，在另一台电脑首次使用时重新授权或输入凭据。
- 如果后续确实需要携带邮箱秘密，另设基于用户主密码的可移植加密凭据库，首版不应将密码写入配置文件。
- 实现目录不可写、路径移动、数据库锁、磁盘空间不足、WebView2 不存在时的清楚错误提示。
- 不能运行时简单复制 SQLite WAL 文件当作稳定备份；使用 SQLite backup API 或关闭数据库后的一致性备份。
- 发布 zip 必须包含真正可运行的构建产物，而不是仅仅把源码压缩打包。

### 2.4 开发和测试环境

Codex 首先探测 Windows 版本、Rust、Node.js、包管理器、MSVC Build Tools、Windows SDK、WebView2、Git，记录结果。可自动完成项目内安全操作；系统级依赖安装如需权限，明确列出指令。不得声称在未使用的 Win10 设备上完成真实兼容测试。

---

## 3. 核心数据模型

所有主表使用稳定 UUID/ULID 标识，包含 `created_at`、`updated_at`（UTC ISO 8601 或规范存储）；支持数据库 migrations 和必要的唯一索引、外键约束。执行删除时优先软删除/归档，禁止未经审查级联删除沟通历史。敏感凭据只保存 `secret_ref`，不保存密码明文。

### 3.1 `contacts`（正式客户/联系人主记录）

- `id`, `display_name`（客户名称，必填）
- `company_name`（公司/店铺名称，可选）
- `country_region`, `customer_type`, `customer_source`, `customer_level`
- `stage`（客户阶段）
- `first_contact_channel`（首次沟通渠道）
- `primary_language`（主要沟通语言）
- `main_need`（主要需求，多行）
- `sample_status`（样品进度）
- `bulk_order_status`（当前批量订单进度）
- `first_contact_at`, `last_contact_at`（自动记录为主，可人工修正）
- `website_url`, `alibaba_buyer_id`, `notes`
- `archived_at`, `created_by`, `updated_by`

客户主记录代表**联系人**，不是公司；同一公司未来可能有多个联系人。首版 `company_name` 为可选文字，数据库设计可逐步规范化但无须强制提前完成复杂公司关系模块。

用户截图中的“下次待跟进日期/下一步行动”不作为必填项，也**不在首版默认页面展示或引入依赖提醒逻辑**；用户自行掌控跟进节奏。

默认客户阶段可使用：待开发 / 已联系 / 已回复 / 需求沟通 / 报价或样品 / 商务谈判 / 已成交 / 暂停 / 不开发。界面选项需可扩展，不要在代码各处写死分散逻辑。

### 3.2 `contact_methods`（联系人渠道）

字段：`id`, `contact_id`, `type`（email/phone/whatsapp/wechat/other）, `value`, `label`, `is_primary`, `is_verified`, `created_at`。

设计多地址而不是只有一个邮箱，允许私人邮箱和公司邮箱并存。电话号码储存为文本，不当作数值。邮箱统一规范化以便搜索和重复校验，但必须保留原始显示形式。

### 3.3 `customer_interests`（产品兴趣与数量）

字段：`id`, `contact_id`, `product_text`（自由文本，如 BK05 或光变卡片机）, `intent_quantity`, `quantity_is_estimate`, `requirement`, `created_at`。

客户对多个型号/品类有兴趣时能分别记录采购意向数量。**不得引入完整产品中心**。

### 3.4 `lead_candidates`（候选客户池）

字段：`id`, `display_name`, `company_name`, `country_region`, `email`, `website_url`, `business_summary`, `source_urls_json`, `research_notes`, `confidence`, `review_status`, `created_by_agent_run_id`, `promoted_contact_id`, `created_at`, `updated_at`。

候选客户入库前后均应提示去重（规范化邮箱、域名、公司+国家组合）；允许人工检查后提升为正式联系人。不应把未经验证的推断自动写成客户档案中的确定事实。

### 3.5 `activities`（客户跟进与业务事件）

字段：`id`, `contact_id`, `event_type`（note/call/meeting/chat/email_sent/email_received/sample/other）, `occurred_at`, `summary`, `details`, `related_message_id`, `source`（manual/agent/system）, `actor_id`, `agent_run_id`, `created_at`。

邮件发送/回复事件只引用原始邮件 ID，不把邮件正文整份冗余复制到跟进表。时间线按业务发生时间排序，允许过滤渠道与类型。

### 3.6 `email_accounts`（邮箱账户设置）

字段：`id`, `display_name`, `email_address`, `sender_name`, `smtp_host`, `smtp_port`, `smtp_security`, `imap_host`, `imap_port`, `imap_security`, `auth_method`, `secret_ref`, `is_active`, `created_at`。

至少支持常见邮箱的 SMTP/IMAP TLS 和应用专用密码/授权码方案；**OAuth2 是否支持及所支持邮箱提供商必须真实验证并清楚披露**，不要暗示所有 Gmail/Microsoft 365 配置都能使用明文密码登录。

SMTP 和 IMAP 连接测试应分开显示，密钥不能进入普通日志、API 返回或 MCP 工具输出。

### 3.7 `email_threads`、`email_messages`、`mailbox_message_locations`

- `email_threads`: `id`, `contact_id`, `subject_normalized`, `last_message_at`, `created_at`。
- `email_messages`: `id`, `thread_id`, `account_id`, `direction`, `from_address`, `to_addresses_json`, `cc_addresses_json`, `subject`, `body_text`, `body_html`, `message_id_header`, `in_reply_to_header`, `references_json`, `received_at`, `submitted_at`, `status`, `created_at`。
- `mailbox_message_locations`: `id`, `message_id`, `account_id`, `folder`, `uidvalidity`, `uid`（用于 IMAP 增量同步及去重）。

邮件关联优先 `Message-ID`、`In-Reply-To`、`References`，其次匹配地址和已有会话；无法确认时放入待关联列表。不要仅靠 Subject 判定同一客户/同一会话。

收信具备 UIDVALIDITY 变化处理、增量抓取、重连、防重复插入。避免自动加载邮件 HTML 中的远程图片/脚本；预览必须净化 HTML。

### 3.8 `email_drafts`、`email_approvals`、`email_outbox_jobs`

- 草稿：`id`, `contact_id`, `account_id`, `to_address`, `subject`, `body_text`, `body_html`, `attachments`, `status`, `revision`, `created_by`, `created_at`。
- 审批：`id`, `draft_id`, `approved_revision`, `content_sha256`, `approved_by_user`, `approved_at`, `expires_at`, `status`。
- 发信任务：`id`, `draft_id`, `approval_id`, `idempotency_key`, `state`, `attempt_count`, `smtp_response`, `error`, `created_at`, `last_attempt_at`。

审批的快照必须涵盖发件账户、全部收件人、主题、正文、附件（及其哈希）。审批后任何相关更改都导致重新审核。

发送状态需要区分 `draft` / `pending_approval` / `approved` / `queued` / `sending` / `smtp_accepted` / `failed` / `delivery_unknown` / `bounced` / `replied`。SMTP 被服务器接受不等于送达收件箱，也不等于已读。

在 SMTP 网络超时导致结果不确定时，不可盲目自动重发，应标记 `delivery_unknown` 并让用户确认。发送任务使用幂等键，防止按钮重复点击或 Agent 重试造成重复发送。

### 3.9 其他表

- `mail_sync_state`：账户/文件夹同步游标及最近运行信息。
- `suppression_list`：退订、明确拒绝联系、不再开发的邮件或域名；发送前强制校验。
- `agent_runs`：外部 Agent 任务 ID、工具调用概要、来源、结果、时间。
- `audit_logs`：修改者、对象、操作类型、前后变化摘要、请求 ID、执行结果、时间。
- `app_settings`：非敏感设置、显示配置和 schema version。

日志中不得泄漏凭据；如需完整审计商业操作，应避免在普通 debug 日志内输出整封客户邮件。

---

## 4. 业务工作流与状态规则

### 4.1 Agent 发现客户并开发

1. 用户在 Codex/WorkBuddy 下达调查任务；**外部 Agent 自行使用其合法可用的搜索能力**，工作台首版不实现自动联网抓取。
2. Agent 调用 `create_lead_candidate`，把找到的企业/联系人、联系渠道、证据 URL、调查摘要放入候选池。
3. 工作台去重提示；用户检查并提升为客户，或 Agent 调用有权限的客户创建工具创建已验证联系人。
4. Agent `get_customer_context` 检查客户资料、过往邮件、跟进与拒收名单。
5. Agent 写开发信并调用 `create_email_draft` 存成草稿。
6. 用户在工作台检查具体地址、发件账号、标题、正文和附件后批准。
7. Agent 可调用 `send_approved_email`；核心服务重新校验内容哈希、审批、邮箱、收件地址、拒发名单、发送限制及幂等键。
8. SMTP 提交结果写入 `email_outbox_jobs` 和 `email_messages`；更新客户时间线及 `last_contact_at`，原则上**只以真实执行结果更新**，不可把草稿误记为已联系。
9. IMAP 收到回复后入库、去重并关联会话；Agent 可以读取回复并生成后续草稿。

### 4.2 自动更新与建议更新的区别

可自动更新：真实收/发邮件记录、时间线、最近联系时间、已知邮件线程、发送失败/退信状态。

建议更新且需要规则校验/必要审核：采购数量、产品兴趣、客户阶段、销售潜力、价格承诺、认证情况和其他商务事实。

若 Agent 从邮件提取“客户想了解 500 台 BK05 的价格”，只能记录客户询价需求，**不能自动填写真实报价或宣称认证已经通过**。

### 4.3 邮件发信基本合规约束

- 默认单联系人定向开发，人工批准；不做首版自动批量冷发。
- 支持拒绝联系/退订记录和发送前阻止。
- 保留真实发件人身份及邮件头，不伪造身份。
- 邮箱域的 SPF/DKIM/DMARC 由用户邮件服务配置，界面可给出配置提示，但不能冒充系统已替用户完成设置。
- 对邮件中的 HTML、附件和外部来源信息按不可信输入处理。

---

## 5. MCP 接口（第一版）

MCP 使用 `stdio`，在 Codex 的客户端配置中指向 `ftw-mcp.exe` 的绝对路径。MCP 适配器的 stdout 只能输出符合 MCP 协议的数据，调试日志写 stderr/日志文件。所有工具参数和返回值定义稳定 JSON Schema，错误需可理解、不得包含凭据。

### 5.1 必要的只读工具

| 工具 | 描述 |
|---|---|
| `list_customers` | 分页查询、关键字/国家/客户阶段/客户等级筛选 |
| `get_customer` | 查询单个联系人及联系渠道、兴趣等字段 |
| `get_customer_timeline` | 查询跟进事件与相关邮件摘要 |
| `list_lead_candidates` | 查询候选池 |
| `list_email_drafts` | 查询草稿和审核状态 |
| `get_email_thread` | 查询指定邮件会话内容 |
| `get_pending_approvals` | 查询待审核邮件及状态 |

### 5.2 必要的写入工具

| 工具 | 描述 |
|---|---|
| `create_lead_candidate` | 插入候选客户、来源信息和调查备注 |
| `create_customer` | 新增联系人，重复时返回候选重复项 |
| `update_customer` | 按允许字段更新联系人，并记录修改来源 |
| `add_customer_activity` | 新增跟进记录 |
| `create_email_draft` | 根据联系人和指定邮箱保存草稿 |
| `request_email_approval` | 将草稿置为待审核，并通知 UI |
| `send_approved_email` | **仅**对已由工作台用户批准的不可变草稿快照执行发送 |
| `sync_mailbox` | 明确触发邮箱增量同步，具备次数/时长控制 |

**不提供** `approve_email` 给普通 Agent 调用；不提供数据库 SQL 执行、读取密钥、执行命令、任意文件读取/写入工具。第一版可先实现最低可用工具子集，但每个工具都必须实际调用真实业务服务。

### 5.3 工具幂等与权限

写入工具尽量接收 `request_id`/`idempotency_key`；重复请求不产生重复客户、重复活动或重复邮件。工具按“只读 / 创建 / 变更 / 发信”权限分层，核心业务服务再二次校验，不能只依赖 MCP 层检查。

---

## 6. Windows 桌面界面

语言默认简体中文；预留国际化能力，不要求第一版实际提供英语界面。现代、简洁、重视信息密度；支持窗口缩放，避免弹窗过多。字段可以自定义隐藏/显示；优先清楚的数据布局而非图表。

### 6.1 导航与页面

1. **工作台首页**：近期客户回复、待审核草稿、邮件同步异常、最近客户活动；不做复杂 KPI 分析。
2. **客户管理**：联系人列表、关键词搜索、国家/阶段/等级/类型/来源筛选、快速新增、导入/导出 CSV（导入导出可 P1）、批量归档（需确认）。
3. **客户详情**：基本资料/联系方法、感兴趣产品与数量、客户阶段、样品状态、批量订单进度；标签页：沟通时间线、邮件记录、客户兴趣、备注。
4. **候选客户池**：待审核线索、来源链接和证据、疑似重复、转为正式客户。
5. **邮件中心**：草稿、待审核、已发送、收件箱、待关联、失败/退信；邮件撰写与预览、筛选邮箱和客户。
6. **跟进记录**：全部活动记录查询和编辑；邮件活动应跳转原始邮件。
7. **Agent 执行记录**：任务来源、工具调用、审批、错误摘要；不要模拟 Agent 的完整聊天界面。
8. **设置**：邮箱账户、邮件签名、发送限制、数据位置、备份恢复、MCP 连接指南、日志信息。

可以把“跟进记录”“Agent 执行记录”作为侧边导航独立项，也可以嵌入相应页面，但这些功能入口必须易发现。

### 6.2 客户字段映射（来自现有 CRM 习惯）

客户名称、客户阶段、样品进度、客户等级、国家/地区、客户来源、首次沟通渠道、客户类型、主要需求、WhatsApp/手机、邮箱、公司/店铺名称、感兴趣产品、意向采购数量、当前批量订单进度、最近联系日期、首次联系日期、主要沟通语言、微信/其他账号、客户网站/店铺链接、客户备注、阿里买家账号/ID、历史跟进。最后一个使用关联时间线而不是长文本。下一次待跟进日期默认不设计。

### 6.3 体验要求

- 初次启动展示空状态和引导，不强迫先添加邮箱才能维护客户。
- 邮件测试 SMTP 与 IMAP 时分开反馈。
- 草稿及已批准邮件有直观且不同的状态。
- Agent 创建的记录带 `Agent` 来源标识，允许人工复核。
- 客户资料编辑支持输入校验、保存反馈，危险操作需要确认。
- 列表数据达到较大规模仍可流畅分页/虚拟滚动；搜索应数据库端执行并加索引。
- 防止未经批准的邮件被自动发送。严禁用前端按钮状态代替后端强制校验。

---

## 7. 稳定性、性能与安全

### 7.1 稳定性

- SQLite 开启外键、合理 busy timeout、按需 WAL；有明确的写入并发方案和备份方案。
- 所有修改数据库 schema 均通过 migration；迁移失败可恢复旧数据库备份。
- 核心进程崩溃不能导致数据丢失；重新启动后恢复未完成任务，避免重复发送。
- IMAP 同步错误可重试，支持网络断开与恢复；错误要落入活动日志。
- 关闭窗口、任务栏程序、MCP 客户端断开时进程状态可预测，不能产生僵尸进程。
- 大附件不能无限制读入内存，限制单封邮件大小和上传大小，具体默认值在开发时文档化。

### 7.2 安全

- 禁止将 SMTP/IMAP 密码、token 写入明文 JSON、SQLite、提交到 Git、输出在日志或 MCP 响应。
- 使用 Windows 用户凭据保护；更换电脑时按需重新输入。
- 本地 API 严格限制 127.0.0.1、认证、Origin/CORS、CSRF 风险，不能对局域网暴露。
- Tauri 最小权限；前端邮件 HTML 必须净化，不运行脚本，不自动加载远程资源。
- 用户提供的网页/邮件、Agent 调查结果均是不可信数据，不能影响系统级权限策略。
- 操作审计涵盖客户修改、审批和发信；审计记录支持追溯。
- 数据库可能包含敏感商业和个人联系信息，导出/备份应提示数据安全风险；未来可增加加密备份。

### 7.3 可测性能指标（目标，不是宣称已达到）

- 1 万联系人、10 万封邮件的测试数据下，典型联系人查询与分页应保持可接受响应；在测试报告中记录机器配置和 P50/P95 延迟，不编造数字。
- Windows 11 常规办公设备上，启动和页面切换应无明显卡顿；用真实测量报告证明。
- 邮件同步不能阻塞 GUI 主线程。
- 客户搜索应使用索引/全文搜索（SQLite FTS5 可选），而非每次全表扫描。

---

## 8. 项目目录建议

```text
ftw/
  AGENTS.md
  README.md
  docs/
    PRODUCT_SPEC.md
    ARCHITECTURE.md
    DATABASE.md
    MCP_TOOLS.md
    MAIL_STATE_MACHINE.md
    SECURITY.md
    ACCEPTANCE.md
    WINDOWS_PORTABLE.md
    CHANGELOG.md
  apps/
    desktop/                  # React + TypeScript + Vite
      src/
      src-tauri/              # Tauri 外壳
  crates/
    ftw-domain/               # 业务实体、领域校验、应用服务接口
    ftw-storage/              # SQLite + SQLx migrations
    ftw-mail/                 # SMTP/IMAP、线程关联、同步
    ftw-core/                 # 本地核心业务守护进程
    ftw-mcp/                  # MCP stdio adapter
  scripts/
    dev.ps1
    build-portable.ps1
    test.ps1
  tests/
    integration/
    fixtures/
  .gitignore
```

目录仅为推荐，可根据 Cargo workspace / Tauri 实际需要微调，**不要为了目录整齐而制造没有业务价值的抽象层**。

---

## 9. 分阶段交付与验收

### M0：环境勘测与可运行骨架

- 检测 Windows 开发环境与依赖。
- 建立 Git 仓库、Cargo workspace、Tauri React UI、基础迁移、Core 健康检查、日志。
- 交付运行指令及现有能力说明。
- 验收：开发环境可以启动 UI 和独立 Core，错误可见；不要求已有邮件功能。

### M1：CRM 与跟进（第一个完整闭环）

- contacts/contact_methods/customer_interests/activities/lead_candidates 表结构与迁移。
- CRUD、分页筛选、关联时间线、去重提示、人工与 Agent 来源。
- UI 实际连数据库；基本单元/集成测试。
- 验收：手工建立联系人，写入跟进，重启后完整保留；相同邮箱重复提交有提示。

### M2：邮件发送与审批（第二个完整闭环）

- SMTP 账户配置、凭据保护、连接测试。
- 草稿、预览、审批、邮件快照哈希、单封发送、队列、日志、幂等。
- 验收：使用测试邮箱发送一封明确批准的邮件；未批准发送被后端拒绝；同一批准任务重复调用不重复发送。
- 测试时优先使用本地 fake SMTP/专用测试邮箱；**不擅自使用用户真实客户地址发信**。

### M3：IMAP 回复与客户联动

- IMAP 配置和增量同步、邮件会话关联、原文存储、安全预览、待关联收件。
- 自动更新真实沟通时间线和最近联系时间，保留手动纠错入口。
- 验收：测试邮箱收到回复后出现在正确客户页；重复同步不产生重复记录；无匹配进入待关联区。

### M4：MCP Agent 工作流

- 本地 MCP stdio Server、工具权限/Schema、可从 GUI 关闭的状态独立工作。
- 输出 Codex 配置示例、WorkBuddy 通用配置说明（具体字段以宿主当前文档为准）。
- 验收：MCP client 创建候选客户和正式客户、读回资料、写入跟进、生成草稿、请求审核；只有 UI 批准后才可发送。

### M5：便携版发布与回归

- PowerShell 构建脚本，真实生成 ZIP：包含 EXE、Core、MCP、运行时资源/说明。
- 检查 WebView2 运行依赖、数据路径、便携模式、备份/恢复、配置迁移。
- 在当前可用的 Win11 环境完成真实运行；如有 Win10 测试机，执行兼容测试。没有 Win10 实机只能列为待验证。
- 验收：解压到任意可写目录后启动、创建客户、关机重开仍保留；CLI MCP 在 UI 未开启时可正常访问；复制目录后业务数据可读、邮箱凭据需要重授权时有明确提示。

每个阶段必须提供：完成的文件/能力、运行命令、通过的测试、未完成项、风险和下一阶段任务。不要跳过验收直接声称“完成全部开发”。

---

## 10. 关键测试用例

1. 客户新增、编辑、归档、重启持久化。
2. 同公司不同联系人可分别建立档案。
3. 同邮箱重复创建、同网站重复导入得到合理提示，不误删数据。
4. 一个联系人可有多个邮箱、WhatsApp、微信等。
5. 一个客户可关联多个感兴趣型号及不同采购数量。
6. 手写和 Agent 写入的跟进都能追踪来源。
7. 未配置邮箱时 CRM 正常工作。
8. SMTP/IMAP 密码不会显示在日志和配置文件。
9. 未批准草稿通过 MCP 请求发送必须被拒绝。
10. 批准后修改收件人/主题/正文/附件，原批准立即失效。
11. 同一 `idempotency_key` 重复调用不得重复发送。
12. SMTP 回执不确定时标记未知，不自动重发。
13. 成功发信后客户时间线建立正确关联。
14. IMAP 回复自动匹配 `In-Reply-To`/`References`，主题相同但无关联时不能盲目匹配。
15. 重复 IMAP 同步不产生重复邮件。
16. 来自未知联系人邮件进入待关联区。
17. 禁止自动执行邮件正文中包含的可疑指令。
18. 数据库迁移失败可从备份恢复；数据目录不可写时提示明确。
19. 在 UI 关闭的情况下，Codex MCP 仍可读取并写入 CRM。
20. 完整 ZIP 在未安装开发工具的 Windows 机器上运行（具备 WebView2 运行环境）。
21. 数据库备份/恢复后邮件与客户关联仍完整。
22. 已拒绝联系邮箱再次发送时被禁止。
23. 程序从含空格和中文字符的目录启动正常。
24. 真实 Win10/Win11 兼容测试分别记录系统版本；未实测不得标记通过。

---

## 11. 交付约束与 Codex 工作规范

1. 请先阅读规格书，再检查现有项目和本地开发环境，先汇报架构与实施计划。
2. 在当前代码仓库实际创建文件和实现功能，不要只输出示例代码或伪代码。
3. 每完成一个里程碑应先运行相应构建/测试，再报告；单阶段完整可用优于一次铺开全部功能。
4. 发现规格不合理、依赖不兼容、Windows 目标受限时，简要记录原因及替代方案；不要擅自把 Rust 换成 Python/Electron 或增加云依赖。
5. 不要自动发送真实商业开发邮件；除非用户已经配置测试邮箱且对具体测试邮件进行了授权。
6. 不要使用假数据/占位按钮伪装成已实现功能；演示数据必须清楚标注。
7. 对数据库结构变更使用 migration；对公开 MCP 工具字段变更同步更新文档与测试。
8. 所有日志不要泄露密码、Token 和其他凭据；文件、数据库、邮件中读入的文本都视为不可信输入。
9. 尽可能自己完成当前阶段的必要开发、测试和修复；确实需要系统级安装或外部邮箱配置时再向用户说明。
10. 不需要为 Linux 写打包脚本，也不要为产品数据中心写空壳页面。

---

## 12. 官方技术资料（开发时以最新稳定文档复核）

- Tauri 2 Windows 环境：https://v2.tauri.app/start/prerequisites/
- Tauri 2 Sidecar：https://v2.tauri.app/develop/sidecar/
- Tauri 2 安全权限：https://v2.tauri.app/security/capabilities/
- Rust MCP SDK：https://github.com/modelcontextprotocol/rust-sdk
- Microsoft WebView2 分发：https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution

**实施优先级总结：M0 可运行骨架 → M1 CRM → M2 SMTP 与审批 → M3 IMAP 与关联 → M4 MCP → M5 便携版验证。**
