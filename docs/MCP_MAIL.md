# 通过 MCP 使用 FTW 代发邮件

工作台、Core 和 MCP 三个 EXE 放在同一目录。便携模式用同一个 `portable.flag` 和数据目录。MCP 适配器按需启动 Core，工作台可以在 Agent 查客户和写草稿时关闭。进行人工审核时需要打开工作台。

## Codex 与 WorkBuddy

把下面的示例路径换成实际解压目录。Codex CLI 可用时，通过命令配置标准 stdio 服务。

```powershell
codex mcp add ftw -- "D:\FTW-Portable\ftw-mcp.exe"
```

对 WorkBuddy，在 MCP 管理中添加本地 stdio 服务，命令填相同的 EXE 绝对路径，参数留空。支持导入 MCP 配置的版本可以使用以下 JSON。两个客户端可各自启动一个适配器，适配器共享同一数据目录的 Core。

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

Core 按回复头和客户邮箱关联会话。待关联收件可以在工作台选择客户。Agent 可通过 `sync_mailbox` 触发同一同步，通过 `list_inbox` 查找来信，再用 `get_email_thread` 读取纯文本会话并生成新的跟进草稿。超过 2 MiB 的来信先保留邮件头，支持按需下载完整邮件。常规来信面板展示附件名称和正文预览。

## 工具参数与结果

| 工具 | 输入 |
| --- | --- |
| `list_customers` | 可选 `keyword`、`country`、`stage`、`level`、`sample_status`、`quick_view`、`offset` |
| `get_customer` | `contact_id` |
| `list_customer_fields` | 空对象。列出 Agent 可读字段及稳定选项 ID |
| `get_customer_field_values` | `contact_id`。带当前修订及数据库代际 |
| `set_customer_field_values` | `request_id`、`patch`。字段需在工作台设为 Agent 可读写 |
| `get_customer_state_catalog` | 空对象。返回稳定状态代码、标签与历史别名 |
| `get_customer_timeline` | `contact_id`，可选 `event_type`、`offset` |
| `list_lead_candidates` | 可选 `status`、`offset` |
| `create_lead_candidate` | `request_id`、`display_name`，可附公司、国家、邮箱、网站、调查和来源信息 |
| `create_customer` | `request_id`、`display_name`，可附公司、国家、邮箱、网站、来源和备注 |
| `update_customer` | `request_id`、`contact_id`，可传 `expected_revision` 和 `expected_epoch` 保护并发。可更新阶段、等级、语言、需求、样品状态及备注。推断或未核实内容使用 `review_only: true` 提交人工核对 |
| `add_customer_activity` | `request_id`、`contact_id`、`event_type`、`occurred_at`（UTC Unix 秒）、`summary`，可附正文、渠道及 `counts_as_contact` |
| `list_email_accounts` | 空对象 |
| `get_pending_approvals` | 可选 `offset` |
| `list_inbox` | 可选 `contact_id`、`offset` |
| `get_email_thread` | `thread_id`，可选 `offset` |
| `sync_mailbox` | `account_id` |
| `list_email_drafts` | 可选 `contact_id`、`offset` |
| `get_email_draft` | `draft_id` |
| `create_email_draft` | `contact_id`、`account_id`、`recipient`、`subject`、`body`，可选 `request_id` |
| `request_email_approval` | `draft_id` |
| `send_approved_email` | `draft_id` |
| `list_customer_views` | 空对象 |
| `query_customers` | 与 Core 客户视图相同的 `filter`、`order`、`page_size` 等查询参数 |
| `check_customer_outreach` | `recipient` |
| `send_authorized_email` | `draft_id`、`revision`、`purpose`、`task_id`、`request_id` |
| `list_email_assets` | 空对象，需要 MCP 启动环境中的已登记令牌 |
| `attach_email_assets` | `draft_id`、`revision`、`asset_ids` |
| `fetch_inbox_message` | `message_id` |
| `read_inbox_body` | `message_id`、可选 `offset` |

建议在 `update_customer` 提供从客户详情读到的修订号和数据纪元。同一数据纪元中的修订冲突继续返回 `crm_revision_conflict`，不同于当前值的字段会进入「数据与审计 → Agent 数据变更」。`review_only: true` 返回 `agent_review_pending` 及待核对字段数，不修改客户字段。应用方应把该响应显示为待核对，用户可在工作台选择保留或采纳。客户合并及来信归属人工核对使用桌面入口。

重试写入时使用相同的 `request_id` 和参数。Core 会返回已记录的结果，参数改变时需要新的请求 ID。若 Core 返回 `agent_request_uncertain`，先查看客户资料或草稿确认执行结果。

审批列表和草稿详情以 Core 时间给出 `approval_expired`，详情同时返回最近一次原生审核结果。Agent 获得这些状态用于交接和查询，发送权限仍由 Core 按原审批流程验证。

工具输入 JSON Schema 由 rmcp 从 Rust 参数类型生成，可通过 `tools/list` 读取。返回内容是 JSON 文本，发件返回 `job_id` 与队列状态。业务失败使用工具错误，包含稳定的 Core 错误码。

## 保存的客户视图

`list_customer_views` 返回 Agent 可见的已保存视图以及 Core 查询条件。把一个视图的 `config.query` 传入 `query_customers` 即可应用同一套业务条件。可按客户字段和启用了 Agent 读取的自定义字段筛选或排序。引用受限制字段的视图仅桌面可见。已有 `list_customers` 调用格式保持兼容。

## Agent 发信权限

从「邮件中心 → Agent 发信权限」可以查看和切换人工审批、禁止 Agent 发送与持续授权模式。登记和撤销身份即时生效。开启持续授权或扩大自动发送权限时，Core 在工作台的 Windows 原生窗口核验当前账户。收紧权限或关闭自动发送即时生效。采用能够经 Windows 密码验证的本机账户，核验失败时保留原设置。完成 R6 后可以主动选择持续授权，默认模式为人工审批。安全账本不可用时，工作台保留人工审批和禁止两种模式。

分别登记 Codex、WorkBuddy 或其他 Agent，每次登记领取一个独立令牌。可以先完成所有登记，再在发送模式中勾选允许自动发信的 Agent，一次核验保存。新登记身份仅持有令牌，未获授权时不能自动发信。用现有 stdio 配置运行 `ftw-mcp.exe`，将 FTW 一次性展示的令牌提供给该 MCP 进程的 `FTW_AGENT_TOKEN` 环境变量。Codex、WorkBuddy 或其他支持标准 MCP stdio 的客户端调用同一工具。令牌属于本机工作台，不要粘贴到 Agent 对话、项目配置、提交或日志中。Core 保存令牌摘要，撤销立即阻止其新的自动发送。

`send_authorized_email` 接收 `draft_id`、当前 `revision`、`purpose`、`task_id` 和客户端稳定的 `request_id`。请求返回真实队列状态。轮询 `get_email_draft` 查看 SMTP 已接受、失败和结果未知。首次开发前用 `check_customer_outreach` 检查邮箱、同域联系人和历史，发送时 Core 再次执行规则。逐封审批继续使用 `request_email_approval` 与 `send_approved_email`，禁止模式下 Agent 仍可创建草稿和读邮件。

便携复制整个停用后的数据目录时保留 `agent-send-safety.sqlite`。业务备份恢复会撤销已登记 Agent、关闭自动授权并冻结旧的待发送任务。安全账本留在同一数据目录，防止业务数据回退后重复提交。

## 目录、图片和完整邮件

桌面写邮件支持 PDF、JPEG、PNG、WebP，最多四件，合计 10 MiB 原文件，实际收发上限仍受邮箱服务器限制。保存位置是数据目录下的 `mail-objects`。同名文件内容变化会产生新的校验值，批准和发送以草稿保存的文件为准。

在「Agent 发信权限 → Agent 可用附件」导入并核验授权后，已登记 Agent 用 `list_email_assets` 查文件 ID，用 `attach_email_assets` 更新自己的草稿附件，再读回最新修订送审或授权入队。

收件同步对超过 2 MiB 的邮件保存邮件头，桌面可在会话中按需下载最多 16 MiB 的完整邮件。Agent 可用 `fetch_inbox_message` 开始下载，并用 `read_inbox_body` 的分页偏移读取纯文本。附件可在桌面通过 Windows 另存窗口写入指定新文件。迁移前或手工备份复制 `backups/mail-objects` 与 `workbench-*.db`，恢复时 Core 校验引用的对象完整性。
