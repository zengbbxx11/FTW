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

## 发送步骤

1. 打开 FTW，在「邮件中心 → 发件账户」配置账户名、发件邮箱、SMTP 主机、端口、登录名及授权码，选择 TLS 或强制 STARTTLS。点「测试 SMTP」验证连接和认证。凭据加密使用当前 Windows 用户的 DPAPI，移动到其他用户或设备时可通过「更新授权码」重新授权。
2. 在「客户」维护收件邮箱。可在客户详情点「写邮件」，也可让 Codex 或 WorkBuddy 通过 `list_customers` 和 `get_customer` 找到联系人，通过 `list_email_accounts` 选取发件账号。
3. 让 Agent 调用 `create_email_draft`，再调用 `request_email_approval`。草稿会显示在邮件中心。MCP 起草支持定向单收件人和纯文本，附件由工作台本地文件选择添加。工作台每封最多 4 个附件，合计 384 KiB。
4. 在 FTW 打开草稿，核对发件人、收件人、标题、正文和附件，点「人工批准」并在 Windows 对话框确认。审批对当前修订和内容哈希有效，有效期为 30 分钟。编辑正文、收发信息或附件后需重新审核。
5. 在工作台点击「发送已批准版本」，或让 Agent 用 `send_approved_email` 提交。Agent 显示队列返回后，用 `get_email_draft` 查询实际状态。SMTP 接受表示服务端接受提交。结果未知时先核对发件记录，Core 保留该作业状态供复核。

Core 在入队及提交前检查审批、最新快照、收件人关系和拒发名单。提交前还校验发件账户最近 24 小时的 SMTP 尝试次数，默认上限为 30，可在发件账户中调整。可在写邮件时将账户纯文本签名插入正文。每个审批修订最多创建一个发件任务。Windows 审批对话框只在 Core 内部记录用户决定，MCP 工具没有批准、读取凭据、任意文件、SQL 或命令入口。此权限边界适用于通过 MCP 使用 FTW 的 Agent；同账户可直接控制桌面或文件的进程属于 Windows 用户权限边界。

## 收件与后续跟进

在「邮件中心 → 收件设置」为对应发件账户配置 IMAP 主机、端口、登录名及凭据。支持使用密码或应用授权码的隐式 TLS IMAP。点击「测试 IMAP」，然后从收件箱同步。每次同步至多拉取 25 封新邮件，收件箱会显示同步结果及是否还有新邮件。

Core 按回复头和客户邮箱关联会话。待关联收件可以在工作台选择客户。Agent 可通过 `sync_mailbox` 触发同一同步，通过 `list_inbox` 查找来信，再用 `get_email_thread` 读取纯文本会话并生成新的跟进草稿。超过 2 MiB 的来信只保留邮件头。常规来信面板展示附件名称和正文预览。

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
| `update_customer` | `request_id`、`contact_id`，可传 `expected_revision` 和 `expected_epoch` 保护并发。可更新阶段、等级、语言、需求、样品状态及备注 |
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

重试写入时使用相同的 `request_id` 和参数。Core 会返回已记录的结果，参数改变时需要新的请求 ID。若 Core 返回 `agent_request_uncertain`，先查看客户资料或草稿确认执行结果。

审批列表和草稿详情以 Core 时间给出 `approval_expired`，详情同时返回最近一次原生审核结果。Agent 获得这些状态用于交接和查询，发送权限仍由 Core 按原审批流程验证。

工具输入 JSON Schema 由 rmcp 从 Rust 参数类型生成，可通过 `tools/list` 读取。返回内容是 JSON 文本，发件返回 `job_id` 与队列状态。业务失败使用工具错误，包含稳定的 Core 错误码。
