# FTW MCP 客户开发、邮件和协作

桌面邮件中心将收件、草稿与审批、SMTP 已接受、发送队列、发送失败、提交结果未知和退信分开查询。收发件搜索按主题与地址在 Core 中筛选，再返回当前页。已发未见回复入口打开原有 CRM 视图，记录仍以最后一次有效的 IMAP 同步为准。

工作台、Core 和 MCP 三个 EXE 放在同一目录。便携模式用同一个 `portable.flag` 和数据目录。MCP 适配器按需启动 Core，工作台可以在 Agent 查客户和写草稿时关闭。进行人工审核时需要打开工作台。

第一次使用可先看便携包根目录的中文接入说明，再复制任务模板到 Codex 或 WorkBuddy。 [接入说明](FTW-Agent-Quickstart.txt) · [任务模板](FTW-Agent-Workflows.txt)

## 从发现客户到持续跟进

Agent 用宿主中已获准的搜索或资料来源发现潜在客户。FTW 负责现有资料查重、调查证据、客户身份、联系资格和后续业务记录。

| 阶段 | MCP 工具和业务结果 |
| --- | --- |
| 查重和沉淀线索 | 用 `search_workbench`、`list_customers` 和 `list_lead_candidates` 查已有资料。有地址时用 `check_customer_outreach` 查看精确邮箱、同域线索、开发历史和禁止联系状态。用 `create_lead_candidate` 与 `add_lead_evidence` 保留待核实资料、来源和日期。 |
| 建档和管理 | 授权后用 `create_customer_profile` 在同一事务写入联系人、联系渠道、产品兴趣和共享可写属性。已有客户先读 `get_customer`，再用 `update_customer` 或 `set_customer_field_values` 提交增量变更。用 `review_lead_candidate` 核对并转正候选。 |
| 起草和发信 | 用 `create_email_draft` 或模板渲染创建草稿。用户选择持续授权时调用 `send_authorized_email`，选择逐封模式时调用 `request_email_approval` 并在工作台批准后用 `send_approved_email`。入队后用 `get_email_draft` 或获准的 `get_mail_task` 读回实际结果。 |
| 收信和原会话回复 | 用 `sync_mailbox` 同步，`get_mailbox_sync_status` 核对进度与时间，`list_inbox` 和 `get_email_thread` 查会话。确认来信归属后用 `create_reply_draft`，再走相应发送模式。 |
| 跟进和交接 | 用 `add_customer_activity` 记录电话、样品、客户反馈等独立沟通，用 `get_customer_timeline` 查看同一时间线。FTW 自动归档收发邮件事件。用共享视图继续处理客户，用 `list_agent_runs` 或获准的审计工具查询自己的操作和结果。 |

工具返回版本、数据纪元或请求回执时，将这些值用于后续写入和状态核对。交接时保留客户或候选 ID、草稿或作业 ID，以及待核对事项。工作台可直接接续同一资料。[桌面与 Agent 能力范围](ui-review/HUMAN_AGENT_PARITY.md#当前协同范围)

## Codex 与 WorkBuddy

把下面的示例路径换成实际解压目录。Codex CLI 可用时，通过命令配置标准 stdio 服务。

```powershell
codex mcp add ftw -- "D:\FTW-Portable\ftw-mcp.exe"
```

对 WorkBuddy，在 MCP 管理中添加本地 stdio 服务，命令填相同的 EXE 绝对路径，参数留空。先在工作台登记 Agent 并开启本机直接连接，按[接入说明](FTW-Agent-Quickstart.txt)操作；无需将令牌写入配置。两个客户端可各自启动适配器，共享同一数据目录的 Core。

```json
{
  "mcpServers": {
    "ftw": {
      "type": "stdio",
      "command": "D:\\FTW-Portable\\ftw-mcp.exe",
      "args": []
    }
  }
}
```

[WorkBuddy 官方 MCP 指引](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/MCP-Guide) · [WorkBuddy 本地 stdio 连接器技术说明](https://open.workbuddy.cn/docs/connector)

## 逐封人工审批

1. 打开 FTW，在「邮件中心 → 发件账户」配置账户名、发件邮箱、SMTP 主机、端口、登录名及授权码，选择 TLS 或强制 STARTTLS。点「测试 SMTP」验证连接和认证。凭据加密使用当前 Windows 用户的 DPAPI，移动到其他用户或设备时可通过「更新授权码」重新授权。
2. 在「客户」维护收件邮箱。可在客户详情点「写邮件」，也可让 Codex 或 WorkBuddy 通过 `list_customers` 和 `get_customer` 找到联系人，通过 `list_email_accounts` 选取发件账号。
3. 让 Agent 调用 `create_email_draft`，再调用 `request_email_approval`。草稿会显示在邮件中心。MCP 起草支持定向单收件人和纯文本，附件由工作台本地文件选择添加。工作台每封最多 4 个 PDF 或图片附件，合计 10 MiB。
4. 在 FTW 打开草稿，核对发件人、收件人、标题、正文和附件，点「人工批准」并在 Windows 对话框确认。审批对当前修订和内容哈希有效，有效期为 30 分钟。编辑正文、收发信息或附件后需重新审核。
5. 在工作台点击「发送已批准版本」，或让 Agent 用 `send_approved_email` 提交。Agent 显示队列返回后，用 `get_email_draft` 查询实际状态。SMTP 接受表示服务端接受提交。结果未知时先核对发件记录，Core 保留该作业状态供复核。

Core 在入队及提交前检查审批、最新快照、收件人关系和拒发名单。提交前还校验发件账户最近 24 小时的 SMTP 尝试次数，默认上限为 30，可在发件账户中调整。可在写邮件时将账户纯文本签名插入正文。每个审批修订最多创建一个发件任务。Windows 审批对话框只在 Core 内部记录用户决定，MCP 工具没有批准、读取凭据、任意文件、SQL 或命令入口。此权限边界适用于通过 MCP 使用 FTW 的 Agent；同账户可直接控制桌面或文件的进程属于 Windows 用户权限边界。

## 收件与后续跟进

在「邮件中心 → 收件设置」为对应发件账户配置 IMAP 主机、端口、登录名及凭据。支持使用密码或应用授权码的隐式 TLS IMAP。点击「测试 IMAP」，然后从收件箱同步。每次同步至多拉取 25 封新邮件，收件箱会显示同步结果及是否还有新邮件。

Core 按回复头和客户邮箱关联会话。Agent 可用 `sync_mailbox` 启动同步，再用 `get_mailbox_sync_status` 核对该邮箱最近一次结果和时间。`complete` 为 `false` 或同步早于有关发信时，只能报告「截至已同步范围，未看到回复」。用 `list_inbox` 查找来信，`get_email_thread` 读取纯文本会话，`create_reply_draft` 从已确认关联的业务来信起草原会话回复。超过 2 MiB 的来信先保留邮件头，支持按需下载完整邮件。

回复草稿由 Core 从来信确定关联客户、发件账户、收件地址及原邮件标识。发送时写入 `In-Reply-To` 与 `References`，发送结果和后续来信留在原会话。可信客户端可创建回复草稿。发送仍需已保存的发信授权或有效的逐封批准。持续授权时使用真实的跟进、样品或其他已授权业务用途。已有首次开发历史的开发跟进用 `outreach_followup`，回复客户主动来信时选实际适用的业务用途。会话回复不能以 `first_outreach` 发送。

## 工具参数与结果

`get_workbench_context` 返回版本、当前 Agent 身份、定义版本、发件账户摘要与生效的发信规则。会话开始时读一次，账户、权限或工作区变化后再查。

`get_workbench_definitions` 接收 `kind`（`fields`、`states` 或 `views`）和可选的 `known_version`。版本相同时返回 `version` 和 `unchanged: true`，变化时附完整定义 `data`。工作区恢复会更换版本所用的数据纪元。旧的字段、状态和视图查询可继续使用。

`list_customers` 和 `query_customers` 返回联系人摘要、记录版本和发信状态，使用 `next_offset` 翻页。`query_customers` 支持结构化筛选、排序及显式的 `custom_columns`。通过 `get_customer` 读取详情。附件列表仅传元数据，内容用文件 ID 分段读取。邮件列表按主题和状态查询，正文按草稿或会话读取。

| 批量工具 | `items` 每项 |
| --- | --- |
| `get_customers_batch` | `{ "contact_id": "…" }` |
| `create_customers_batch` | 与 `create_customer_profile` 相同，包括独立的 `request_id`。 |
| `update_customers_batch` | 与 `update_customer` 相同，包括当前版本和独立的 `request_id`。 |
| `add_customer_activities_batch` | 与 `add_customer_activity` 相同，包括独立的 `request_id`。 |
| `get_email_statuses` | 草稿 ID 字符串。返回草稿版本、状态、作业 ID、结果和已知退信。 |

每批 1 至 10 项，`rows` 按输入顺序返回，失败项有自己的错误代码，成功项独立保留。写入中断后用同一登记身份和原 `request_id` 重读回执。跨 Agent 接续时先用 `list_agent_runs`、`get_mail_task`、客户 ID 或草稿 ID 核对已有进度。确认 SMTP 结果未知的作业前暂停重发。

| 工具 | 输入 |
| --- | --- |
| `list_customers` | 可选 `keyword`、`country`、`stage`、`level`、`sample_status`、`quick_view`、`sort`、`page_size`（20、50 或 100）、`offset`。默认返回摘要及后续 `next_offset`。 |
| `get_customer` | `contact_id` |
| `list_customer_fields` | 空对象。列出 Agent 可读字段及稳定选项 ID |
| `get_customer_field_values` | `contact_id`。带当前修订及数据库代际 |
| `set_customer_field_values` | `request_id`、`patch`。字段需在工作台设为 Agent 可读写 |
| `get_customer_state_catalog` | 空对象。返回稳定状态代码、标签与历史别名 |
| `get_customer_timeline` | `contact_id`，可选 `event_type`、`offset` |
| `list_lead_candidates` | 可选 `status`、`offset` |
| `create_lead_candidate` | `request_id`、`display_name`，可附公司、国家、邮箱、网站、调查和来源信息 |
| `get_lead_candidate` | `lead_id`。返回完整候选资料、查重结果、`record_revision` 和 `data_epoch`。 |
| `update_lead_candidate` | `request_id`、`lead_id`、`expected`（`revision`、`epoch`）、`patch`。需要工作台单独授予 `edit_leads`，只允许补充待审核候选。 |
| `list_lead_evidence` | `lead_id`，可附 `offset`。返回该候选的逐条证据、来源、调查时间、核验状态和分页信息 |
| `add_lead_evidence` | `request_id`、`lead_id`、`evidence`。证据含 `subject`、`url` 或 `note`，可附 `observed_at`（UTC Unix 秒）。工作台将其标为待核实 |
| `list_contact_evidence` | `contact_id`，可附 `offset`。返回正式客户证据和转正候选的证据，后者带 `candidate` 来源标记 |
| `add_contact_evidence` | `request_id`、`contact_id`、`evidence`。向正式客户补记一项待核实证据 |
| `get_customer_company` | `contact_id`，可附 `offset`。返回正式公司档案、联系人地区、分支、独立 CRM 状态及分页信息 |
| `find_company_profiles` | 可选 `keyword`。按公司名称或别名查询，返回档案编号、版本和数据纪元。 |
| `save_company_profile` | `request_id`、`anchor`、`input`，可附 `profile` 和 `include_group`。 |
| `associate_company_contact` | `request_id`、`anchor`、`contact`。补齐未绑定独立档案的联系人公司或地区。 |
| `set_company_membership` | `request_id`、`contact`、`change`。加入公司、修改分支或解除关联。 |
| `create_company_contact` | `request_id`、`profile`、`input`，可附 `entries` 及 `allow_duplicate`。 |
| `create_customer` | `request_id`、`display_name`，可附公司、国家、邮箱、网站、来源和备注 |
| `create_customer_profile` | `request_id`、`input`（完整客户表单，可包含多种 `methods`、`interests` 和采购信息）、可选 `entries`（共享可写自定义字段）。Core 在同一事务创建档案和属性，返回重复线索供核对。[字段合约](CRM_API.md#json-schema) |
| `update_customer` | `request_id`、`contact_id`，须传 `expected_revision` 和 `expected_epoch` 保护并发。可更新阶段、等级、语言、需求、样品状态及备注。可附 `evidence` 或显式传 `review_only: true` |
| `update_customer_profile` | `request_id`、`target`（`id`、`revision`、`epoch`）、`patch`。按需写姓名、公司、地区、类型、来源、首次联系渠道、大货状态、网站、买家编号、日期、联系方式或产品兴趣。传入 `dates`、`methods` 或 `interests` 时替换对应分区，保留已有子项 ID 以持续追踪。 |
| `suggest_customer_detail` | `request_id`、`contact_id`、`expected_revision`、`expected_epoch`、`detail`、`evidence`。提出联系方式或产品兴趣的补充或更正，等待桌面用户采纳 |
| `add_customer_activity` | `request_id`、`contact_id`、`event_type`、`occurred_at`（UTC Unix 秒）、`summary`，可附正文、渠道及 `counts_as_contact`。样品记录可附 `sample: { "model": "DC226", "quantity": 2, "batch_ref": "PILOT-1", "progress": "evaluating" }`。数量为 1 至 1,000,000 的整数。相同客户的同一批次沿用同一编号 |
| `list_email_accounts` | 空对象 |
| `get_pending_approvals` | 可选 `offset` |
| `list_inbox` | 可选 `contact_id`、`account_id`、`keyword`、`offset`。 |
| `get_email_thread` | `thread_id`，可选 `offset` |
| `sync_mailbox` | `account_id` |
| `get_mailbox_sync_status` | 可选 `account_id`。返回各邮箱的收件配置状态、`last_sync`、`outcome` 和 `complete`。 |
| `create_reply_draft` | `request_id`、`inbox_message_id`、`body`，可选 `subject`。省略主题时沿用原邮件的回复主题。遇到来信归属不明时先确认关联。 |
| `list_email_drafts` | 可选 `contact_id`、`offset`、`keyword`、`folder`。`folder` 支持 `drafts`、`submissions`、`accepted`、`queue`、`failed`、`unknown`、`bounced`。 |
| `list_email_templates` | 可选 `purpose`、`offset`、`archived`。默认读取在用模板及主题、正文。`archived: true` 查询归档模板。 |
| `save_email_template` | `request_id`、`input`（`name`、`purpose`、`subject`、`body`）。创建时省略 `template_id` 和 `expected_revision`，编辑时同时传入两项。 |
| `set_email_template_archived` | `request_id`、`template_id`、`expected_revision`、`archived`。`archived: false` 恢复模板。 |
| `render_email_template` | `template_id`、`contact_id`。返回主题、正文及 `missing` 缺失变量列表。 |
| `get_email_draft` | `draft_id` |
| `create_email_draft` | `contact_id`、`subject`、`body`，可选 `request_id`。省略 `account_id` 时使用默认或唯一发件账户，省略 `recipient` 时使用客户主邮箱或唯一邮箱。无明确默认值时提供对应参数。 |
| `request_email_approval` | `draft_id` |
| `send_approved_email` | `draft_id` |
| `list_customer_views` | 空对象 |
| `search_workbench` | `keyword`，可附 `category`（`all`、`customer`、`activity`、`mail`、`file`）与 `offset`。 |
| `query_customers` | 与 Core 客户视图相同的 `filter`、`order`、`page_size` 等查询参数 |
| `check_customer_outreach` | `recipient` |
| `send_authorized_email` | `draft_id`、`revision`、`purpose`、`task_id`、`request_id` |
| `list_email_assets` | 空对象，需要有效的已登记 Agent 身份 |
| `attach_email_assets` | `draft_id`、`revision`、`asset_ids` |
| `fetch_inbox_message` | `message_id` |
| `read_inbox_body` | `message_id`、可选 `offset` |

建议在 `update_customer` 提供从客户详情读到的修订号和数据纪元。同一数据纪元中的修订冲突继续返回 `crm_revision_conflict`，不同于当前值的字段会进入「数据与审计 → Agent 数据变更」。`review_only: true` 返回 `agent_review_pending` 及待核对字段数，不修改客户字段。应用方应把该响应显示为待核对，用户可在工作台选择保留或采纳。有歧义的来信归属在桌面核对。

`evidence` 的 `kind` 可为 `research`（调查所得）、`inferred`（推断）、`unverified`（未核实）。可附 HTTP(S) `url` 和简短 `note`。带来源的现有字段更新中，推断、未核实及替换已有值的建议进入人工核对。每次提供单个字段及其来源可独立追溯。

`suggest_customer_detail` 需要使用 `get_customer` 返回的当前修订号和数据纪元。补充资料使用以下 `detail` 结构。

```json
{ "type": "contact_method", "kind": "email", "value": "buyer@example.test", "label": "备用采购邮箱" }
```

```json
{ "type": "product_interest", "product_text": "DC226", "intent_quantity": 300, "quantity_is_estimate": true, "requirement": "需先核对 USB 兼容性" }
```

更正资料时，使用 `get_customer` 返回的联系方式或产品兴趣 ID。产品更正可传 `intent_quantity`、`quantity_is_estimate`、`requirement` 中的一项或多项，缺省项沿用原值。空字符串可清除采购需求。

```json
{ "type": "correct_contact_method", "method_id": "<method id>", "value": "updated-buyer@example.test" }
```

```json
{ "type": "correct_product_interest", "interest_id": "<interest id>", "intent_quantity": 520, "quantity_is_estimate": false }
```

Core 在人工采纳时复核资料和目标行。更正邮箱实际地址时保留首选设置并清除旧地址的验证标记。在等待采纳期间，目标行发生其他修改会触发冲突。已有产品的补充结构只填空白字段，修改现有值应使用更正结构。`agent_review_pending` 中 `count: 0` 表示无新增建议。相同请求重试请复用 `request_id`。

重试写入时使用相同的 `request_id` 和参数。Core 会返回已记录的结果，参数改变时需要新的请求 ID。若 Core 返回 `agent_request_uncertain`，先查看客户资料或草稿确认执行结果。

审批列表和草稿详情以 Core 时间给出 `approval_expired`，详情同时返回最近一次原生审核结果。Agent 获得这些状态用于交接和查询，发送权限仍由 Core 按原审批流程验证。

工具输入 JSON Schema 由 rmcp 从 Rust 参数类型生成，可通过 `tools/list` 读取。返回内容是 JSON 文本，发件返回 `job_id` 与队列状态。业务失败使用工具错误，包含稳定的 Core 错误码。

## 保存的客户视图

`list_customer_views` 返回 Agent 可见的已保存视图、`id`、`revision` 和 Core 查询条件。用 `query_customer_view` 传入 `view_id` 和可选的 `offset`，即可执行当前保存的筛选与排序。也可以把 `config.query` 传入 `query_customers`。引用受限制字段的视图仅桌面可见。现有 `list_customers` 调用格式保持兼容。

`list_archived_customer_views` 返回可见的已归档共享视图。`restore_customer_view` 接收 `view_id` 和 `expected_revision`，使用与工作台相同的名称冲突和版本校验。两个工具均隐藏视图及引用受限制字段的视图受原访问规则保护。

`move_customer_view` 调整可编辑的在用共享视图顺序，`move_customer_field` 调整在用且共享可写的字段顺序，两者均接收 `request_id`、`id`、`expected_revision` 和 `direction`（`up` 或 `down`）。一次移动在同一事务中交换相邻可编辑项目的位置并审计。其他项目保留原位置，默认视图保留桌面管理。到达边界时返回成功，顺序维持原样。

`list_archived_customer_fields` 返回已归档、仍对 Agent 共享写入的自定义字段。`set_customer_field_archived` 使用 `request_id`、`field_id`、`expected_revision`、`archived`。设为 `false` 恢复字段，两项工具均归档保留原有字段值。使用字段进行筛选、排序或分组的在用视图须先调整，依赖字段的归档视图可在字段恢复后重新启用。

## 全局搜索

`search_workbench` 可筛选客户、跟进、邮件或文件元数据。发件文件库中仅向桌面开放的素材仍由桌面用户管理。

## 公司与联系人

请从 `get_customer`、`get_customer_company` 或 `find_company_profiles` 取得最新 `{id, revision, epoch}`。联系人版本分别对应 `record_revision` 与 `data_epoch`，公司档案直接返回 `revision` 和 `epoch`。

`save_company_profile` 的 `input` 使用 `name`、`website`、`notes`、`aliases`。省略 `profile` 表示从当前联系人建立档案。`include_group: true` 会一并关联同名同地区且尚未绑定正式公司的联系人，默认仅关联当前联系人。编辑时传入已有 `profile` 的版本。

`set_company_membership` 要显式提供 `change: {"action":"link","profile":{...},"branch":"分支"}` 或 `change: {"action":"unlink"}`。已属于另一家正式公司的联系人会返回 `company_membership_conflict`，可读取最新关联后明确解除，再提交新的关联。`create_company_contact` 按普通客户和字段规则校验，并在同一事务创建客户和关联。`allow_duplicate` 默认为 `false`。在未创建独立公司档案时，可按公司名称与地区调用 `create_customer` 创建同组联系人。

每项写入使用稳定的 `request_id`。重试同一请求会返回原回执，版本冲突后应重新读取相关记录再提交。公司操作会记录登记的操作者和请求编号。

## 人工与 Agent 协作权限

在「邮件中心 → Agent 发信权限」登记客户端并开启本机直接连接。当前 Windows 用户的本地 MCP 客户端共享所选 Agent 身份；若需区分客户端，也可为不同进程使用各自的 `FTW_AGENT_TOKEN`。页面显示最近连接时间，并支持暂停、恢复和撤销。持续发信独立使用已保存的账户、用途和限额。凭据、Agent 接入管理、解除拒发和恢复旧备份由桌面处理。

### 批量客户更新与质量检查

| 工具 | 调用与权限 |
| --- | --- |
| `preview_customer_batch` | `query` 使用 `query_customers` 的筛选定义，返回全部匹配的在用客户数量及 `fingerprint`。 |
| `update_customer_batch` | `request_id` 和 `change`。`change.selection` 为 `matching` 时传原 `query` 和 `fingerprint`，为 `selected` 时传 `targets`，每项包括 `id`、`revision`、`epoch`。两种模式都传 `field`（`stage`、`level` 或 `sample`）及 `value`。 |
| `list_customer_quality_issues` | 可选 `category`、`keyword`、`offset`。类别为 `all`、`duplicate`、`missing_email`、`invalid_email`、`mail_problem`、`missing_market`、`invalid_website`、`date_order`、`sample_mismatch`、`excluded_mail`。 |

批量提交在同一事务中校验选区、写入记录并审计。选区或客户版本变化时整批拒绝。重新查询或预览并确认后，用新 `request_id` 提交修订后的操作。相同 `request_id` 仅用于重试完全相同的请求。

### 邮件分析与业务审计

| 工具 | 调用与权限 |
| --- | --- |
| `get_mail_analytics` | `days` 支持 7、30、90，可附 `include_legacy`。返回邮件开发汇总、逐日趋势和客户来源分组。 |
| `export_mail_analytics_csv` | 使用相同参数和统计口径，返回 UTF-8 CSV 文本。 |
| `list_business_audit` | 可选 `source`（`manual`、`agent`、`system`）、`entity`、`keyword`、`days`（0、7、30、90）、`offset`。授权、凭据和私有配置的审计由桌面查看。 |

### 候选核验与回复关联

| 工具 | 调用与权限 |
| --- | --- |
| `review_lead_candidate` | `request_id`、`lead_id`、`expected`（`revision`、`epoch`）、`decision`（`reject`、`reopen` 或 `promote`）。转正校验待审核状态、记录版本和重复客户。重复客户提示交桌面处理。人工拒绝的候选由人工重新打开。 |
| `review_customer_evidence` | `request_id`、`evidence_id`、`expected`（证据的 `revision` 及列表的 `epoch`）、`decision`（`verified`、`rejected` 或 `pending`）。保留既有人工核验。桌面显示核验来源，可人工确认 Agent 的决定。 |
| `confirm_reply_link` | `request_id`、`message_id`、`contact_id`、`expected_contact`（未关联为 `null`）、`expected_match`（`pending` 或 `contact`）。仅确认发件地址唯一对应一个在用客户、且该客户邮箱已核实的待确认业务来信。匹配有歧义时使用 `suggest_reply_link` 交人工核对。 |

`update_customer_batches`、`export_mail_reports`、`review_leads` 和 `confirm_reply_links` 由用户在工作台按客户端开启，现有客户端不会自动获得。

联系方式的核实标记由桌面用户维护。Agent 可以保留原地址已有的核实标记，新地址或变更后的地址须由用户核实。

| 新工具 | 使用方式和权限 |
|---|---|
| `query_customer_view` | `view_id` 和可选 `offset`。与基础 CRM 查询共用权限。 |
| `list_sample_records` | 可选 `keyword`、`status`、`offset`、`by_batch`。使用样品工作台的查询和分页。 |
| `save_customer_field_definition` | 使用 `fields::Definition`。桌面和 Agent 新建字段默认为共享可写，工作台可调整字段访问方式。Agent 只能编辑标记为 `write` 的已有字段，且须保持该标记。 |
| `save_customer_view` | 使用 `views::ViewInput`。新建传空 `id`，编辑传当前 `expected_revision`。工作台负责默认和隐藏视图。 |
| `archive_customer_view` | `view_id`、`expected_revision`。在桌面可恢复归档视图。 |
| `edit_customer_activity` | `activity_id`、当前 `expected_revision` 和跟进 `input`。 |
| `edit_email_draft` | `draft_id`、`expected_revision`、客户、发件账户、收件人、主题和正文。适用于 `draft` 状态，保留附件。 |
| `list_customer_documents` | `contact_id`、可选 `offset`。 |
| `upload_customer_document` | `contact_id`、`filename` 和 `content_base64`。每项最多 576 KiB，支持 PDF、PNG、JPEG 和 WebP。重复导入相同客户、文件名及哈希会返回已有 ID。 |
| `read_customer_document` | `document_id`、可选字节 `offset`。逐段解码并拼接，传回 `next_offset` 直到为 null。结果包含文件总大小和 SHA-256。 |
| `get_customer_change_history` | `contact_id`、可选 `offset`。查询迁移后跟踪的客户字段差异。 |
| `get_mail_task` | `task_id`、可选 `offset`。可传 `agent_id` 缩小查找范围，省略时查共享任务。 |
| `list_customer_duplicates` | 可选 `keyword`、`offset`。查询与桌面一致的潜在重复客户。 |
| `set_customer_archived` | 客户 `target`（`id`、`revision`、`epoch`）和 `archived`。 |
| `merge_customers` | 已核实为同一联系人的 `primary`、`secondary`，两项均须含编号、版本和数据版本戳。 |
| `import_customer_csv` | UTF-8 `csv`、`default_stage`、可选 `apply`。默认预检，应用须附 `request_id`。 |
| `export_customer_csv` | `scope`、`fields`（如 `display_name`、`email`）和可选 `offset`。按 `next_offset` 翻页，每页含一行列名。 |
| `get_customer_field_history` | `contact_id` 和可选 `offset`。返回类型化的前后值、字段名、版本、来源主体及有记录时的请求编号。当前隐藏或归档的字段仅桌面可查看。 |
| `upload_customer_document_chunk` | `upload_id`（调用方生成的 UUID）、`contact_id`、`filename`、`total_size`、字节 `offset` 和 `content_base64`。每段 192 KiB，末段为剩余字节，整份文件上限 10 MiB。 |
| `read_email_attachment` | `source`（`inbox` 或 `outbound`）、`message_id`、从 0 开始的 `index`、可选字节 `offset`。发件文件库素材还须已有 Agent 访问许可。对于尚未下载全文的收件，先用现有工具获取全文。 |
| `list_reply_review` | 可选 `offset`。查询桌面的回复待确认列表。 |
| `get_reply_link_candidates` | `message_id`。返回已有候选客户和建议来源。 |
| `suggest_reply_link` | `message_id` 和 `contact_id`。把候选提交到桌面待确认界面。 |
| `list_agent_runs` | 可选 `filter`、`keyword` 和 `offset`。返回所有已登记客户端的业务请求和邮件批次，包括 `success`、`retryable` 与 `requires_human`。 |
| `update_mail_job` | `job_id` 和 `action`（`cancel` 或 `reopen`）。取消尚未提交的邮件，或为明确失败的邮件准备新草稿版本。 |
| `list_contact_suppressions` | 可选 `offset`。读取拒发的地址和域名。 |

分段上传从偏移 0 开始。每次沿用同一上传编号及元数据，按 `next_offset` 继续，收到 `document_id` 即完成。重复提交内容相同的分段会返回当前进度。分段内容冲突时返回 `upload_chunk_conflict`，暂存记录可用 24 小时。每个客户端最多同时保留 4 项未完成上传。

`update_mail_job` 返回草稿编号和新版本。取消仅适用于未开始 SMTP 提交的队列项。准备重试适用于未尝试提交的失败或 SMTP 明确拒绝的失败。新草稿按正常流程再次核对，原审批随旧版本失效。SMTP 结果未知时由人工核对。请求结果中的 `retryable` 表示读取最新资料后可以重新发起业务操作。

`update_customer` 使用 `get_customer` 或查询摘要中的 `expected_revision` 和 `expected_epoch` 对应值。版本冲突后先读取并比对当前记录。SMTP 结果以队列记录为准。

可信 Agent 可以用 `update_lead_candidate` 提交增量更新。`patch` 支持姓名、公司、地区、邮箱、网站、业务摘要、调查备注及置信度。省略字段保留原值，`add_source_urls` 追加网址并记录其来源。把 `get_lead_candidate` 的 `record_revision` 和 `data_epoch` 分别传入 `expected.revision` 和 `expected.epoch`。身份信息触发查重时，返回待核对的 `duplicates`。用 `review_lead_candidate` 核验候选状态，桌面也可处理重复提示和已有人工拒绝。

## Agent 发信权限

从「邮件中心 → Agent 发信权限」可以查看和切换人工审批、禁止 Agent 发送与持续授权模式。登记和撤销身份即时生效。首次设置时界面预选持续授权、现有 Agent、邮箱、用途及最高限额，用户在工作台保存后生效，Core 记录操作并执行所选范围。已有权限设置不会因升级改变。安全账本不可用时，工作台保留人工审批和禁止两种模式。

「发件账户」支持编辑 SMTP 配置和删除账户。删除后停止该账户的收发并清除当前凭据，保留历史邮件和草稿。修改发件邮箱地址后需重新配置 IMAP 和选择该邮箱的 Agent 授权。

在工作台登记 Agent 并开启本机直接连接，可让当前 Windows 用户的本地 MCP 客户端共享该身份。需要自动发信时，在发送模式中勾选允许的 Agent、账户和用途并保存限额。若需区分客户端，可使用各自的 `FTW_AGENT_TOKEN` 启动环境；明确提供的令牌优先于本机默认身份。令牌不要粘贴到 Agent 对话、明文配置、提交或日志中。Core 保存令牌摘要并验证每次操作，暂停或撤销立即阻止后续访问。

`send_authorized_email` 接收 `draft_id`、当前 `revision`、`purpose`、`task_id` 和客户端稳定的 `request_id`。用途为 `first_outreach`、`outreach_followup`、`quote_communication`、`sample_communication` 或 `other_business`。请求返回真实队列状态。轮询 `get_email_draft` 查看 SMTP 已接受、失败和结果未知。首次开发前用 `check_customer_outreach` 检查邮箱、同域联系人和历史，发送时 Core 再次执行规则。逐封审批继续使用 `request_email_approval` 与 `send_approved_email`。禁止发送模式下，可信 Agent 仍可起草和查询邮件。

便携复制整个停用后的数据目录时保留 `agent-send-safety.sqlite`。业务备份恢复会撤销已登记 Agent、关闭自动授权并冻结旧的待发送任务。安全账本留在同一数据目录，防止业务数据回退后重复提交。

## 开发邮件模板

可信客户端可以新建、编辑、归档和恢复共享邮件模板。保存和归档回执带 `id`、`revision`。重试同一写入时使用相同的 `request_id` 和参数。遇到 `template_revision_conflict` 时先查询最新版本，再用新请求编号提交确定要保留的内容。模板的创建、编辑、归档和恢复均在工作台审计中记录操作主体，有 Agent 请求编号时一并记录。

用 `render_email_template` 获取的内容可交给 `create_email_draft`。支持 `{{contact_name}}`、`{{company_name}}`、`{{country_region}}` 和 `{{product_interest}}`。如果 `missing` 有内容，先向用户确认缺失资料或调整正文。Core 会在审批、入队和正式提交 SMTP 前拦截未展开的模板变量。模板用途用于分类，实际发送权限仍以发信工具传入的用途与已保存授权为准。

## 目录、图片和完整邮件

独立客户资料通过工作台的文件选择器或 MCP 文档上传工具导入，存放在同一便携数据目录并随备份保存。客户文档按客户和文件 ID 管理。发件素材通过工作台的「Agent 可用附件」单独授权。

客户文件、收件和发件附件中的 PDF 可以通过 Windows 文件选择器另存，也可以作为附件发送。Core 按记录编号读取原始文件，已有目标文件保留原内容。

桌面写邮件支持 PDF、JPEG、PNG、WebP，最多四件，合计 10 MiB 原文件，实际收发上限仍受邮箱服务器限制。保存位置是数据目录下的 `mail-objects`。同名文件内容变化会产生新的校验值，批准和发送以草稿保存的文件为准。

在「Agent 发信权限 → Agent 可用附件」导入并授权后，已登记 Agent 用 `list_email_assets` 查文件 ID，用 `attach_email_assets` 更新共享草稿的附件，再读回最新修订送审或授权入队。

收件同步对超过 2 MiB 的邮件保存邮件头，桌面可在会话中按需下载最多 16 MiB 的完整邮件。Agent 可用 `fetch_inbox_message` 开始下载，并用 `read_inbox_body` 的分页偏移读取纯文本。附件可在桌面通过 Windows 另存窗口写入指定新文件。迁移前或手工备份复制 `backups/mail-objects` 与 `workbench-*.db`，恢复时 Core 校验引用的对象完整性。

## 操作回执

MCP 工具的 `content[0].text` 继续提供原有业务结果或错误说明。`structuredContent` 补充第 1 版回执。可读取 `operation`、`status`、`success`、`retryable` 与 `requires_human`。Core 返回实体编号、任务编号或版本时，回执也提供相应字段。失败回执包含稳定的 `code`，并在 Core 提供时包含 `detail`。

`success: null` 表示业务仍在处理或结果未知。邮件入队对应 `queued`，SMTP 接受对应 `smtp_accepted`，提交失败对应 `failed`，提交结果未知对应 `delivery_unknown`。失败或未知时，若 Core 已保存具体原因，`code` 会带回该原因。未知发送需要人工核对。版本冲突类错误的 `retryable` 表示读取最新记录后可重新发起。桌面错误和任务中心使用相同的错误处理标记。工具名错误或参数结构错误继续按 MCP 协议返回。
