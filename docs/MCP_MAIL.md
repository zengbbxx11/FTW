# 通过 MCP 使用 FTW 代发邮件

工作台、Core 和 MCP 三个 EXE 放在同一目录。便携模式用同一个 `portable.flag` 和数据目录。MCP 适配器按需启动 Core，工作台可以在 Agent 查客户和写草稿时关闭。进行人工审核时需要打开工作台。

## Codex 与 WorkBuddy

Codex 在有 Codex CLI 的环境中配置标准 stdio 服务：

```powershell
codex mcp add ftw -- "D:\FTW\.ftw-dev\FTW 邮件试运行\ftw-mcp.exe"
```

对 WorkBuddy，在 MCP 管理中添加本地 stdio 服务，命令填相同的 EXE 绝对路径，参数留空。支持导入 MCP 配置的版本可以使用以下 JSON。两个客户端可各自启动一个适配器，适配器共享同一数据目录的 Core。

```json
{
  "mcpServers": {
    "ftw": {
      "type": "stdio",
      "command": "D:\\FTW\\.ftw-dev\\FTW 邮件试运行\\ftw-mcp.exe",
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

Core 在入队及提交前检查审批、最新快照、收件人关系、拒发名单。每个审批修订最多创建一个发件任务。Windows 审批对话框只在 Core 内部记录用户决定，MCP 工具没有批准、读取凭据、任意文件、SQL 或命令入口。此权限边界适用于通过 MCP 使用 FTW 的 Agent；同账户可直接控制桌面或文件的进程属于 Windows 用户权限边界。

## 工具参数与结果

| 工具 | 输入 |
| --- | --- |
| `list_customers` | 可选 `keyword`、`country`、`stage`、`level`、`offset` |
| `get_customer` | `contact_id` |
| `list_email_accounts` | 空对象 |
| `list_email_drafts` | 可选 `contact_id`、`offset` |
| `get_email_draft` | `draft_id` |
| `create_email_draft` | `contact_id`、`account_id`、`recipient`、`subject`、`body` |
| `request_email_approval` | `draft_id` |
| `send_approved_email` | `draft_id` |

工具输入 JSON Schema 由 rmcp 从 Rust 参数类型生成，可通过 `tools/list` 读取。返回内容是 JSON 文本，发件返回 `job_id` 与队列状态。业务失败使用工具错误，包含稳定的 Core 错误码。
