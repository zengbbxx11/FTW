# M1 CRM 接口

桌面 WebView 通过 Tauri 的 `crm_request` 调用 Core，参数对象包含 `command`。Rust 调用者可以使用 `ftw_ipc::crm::Command` 与 `ftw_ipc::crm::Result`。当前桌面请求由 Core 识别为 `manual`，请求正文不接受操作者、来源或 Agent 执行 ID。MCP 适配器经受限 Agent 命令进入同一套 CRM 校验，并由 Core 记录 Agent 来源和请求标识。

内部 IPC 请求示例

```json
{"version":1,"operation":{"crm":{"action":"list_contacts","query":{"keyword":"深圳","offset":0}}}}
```

IPC 响应返回 `{"status":"crm","data":{...}}`。桌面命令直接返回 `data`，错误通过 Promise rejection 返回 `code` 和 `detail`。表中的字段使用 snake_case。

| 操作 | 参数 | 成功返回 |
| --- | --- | --- |
| `list_contacts` | `query` 支持 `keyword`、`quick_view`、`country`、`stage`、`level`、`customer_type`、`source`、`archive`、`sort`、`offset` | `contacts` |
| `get_contact` | `id` | `contact`，包含资料、渠道和产品兴趣 |
| `save_contact` | `id`（新建为空）、`input`、`allow_duplicate` | `saved` 或 `duplicates` |
| `set_archived` | `id`、`archived` | `saved` |
| `list_leads` | `offset`、`review_status` | `leads` |
| `get_lead` | `id` | `lead`，包含证据网址及重新计算的重复项 |
| `save_lead` | `id`（新建为空）、`input`、`allow_duplicate` | `saved` 或 `duplicates` |
| `set_lead_review` | `id`、`rejected` | `saved` |
| `promote_lead` | `id`、`allow_duplicate` | `saved`（正式客户 ID）或 `duplicates` |
| `list_activities` | `contact_id`（空值查询所有客户）、`event_type`、`offset` | `activities` |
| `save_activity` | `id`（新建为空）、`input` | `saved` |

分页每次最多 20 条，结果含 `has_more`。联系时间和发生时间使用 UTC Unix 秒，桌面按本地时区显示。归档后的联系人及跟进仍可读取，恢复后可继续编辑。候选在待审核状态可转正。转正操作在同一事务中创建联系人、写入关联并记录审计，重复调用返回已有联系人。调查摘要、备注和置信度留在候选记录中。

去重按规范化邮箱、网站域名、公司与国家组合提示。匹配的记录由调用者复核，确认时重新提交 `allow_duplicate: true`，源记录不会合并或删除。每种渠道最多设一个主要账号。产品兴趣为自由文本，数量用非负整数，可标记为估算。联系方式和产品兴趣随联系人事务提交并保留已有子项 ID。

业务错误为 `crm_invalid_field`（`detail` 是字段或校验名称）、`crm_not_found`、`crm_read_only` 和 `database_unavailable`。常规错误响应不包含 SQL 或客户正文。Core 事务审计保存操作者、来源、操作和对象 ID。

`quick_view` 支持 `priority` 与 `sample`。跟进可保存独立的 `channel`、`followup_category` 与 `customer_feedback`。这些字段缺省时保存为空文本，旧跟进在迁移后继续可读。

客户 CSV 的桌面入口为「数据与审计 → 客户 CSV」。预览在事务中校验并回滚，正式导入重新检验，跳过冲突或无效行。CSV 导出含 UTF-8 BOM，导出到数据根目录的 `exports`。

## JSON Schema

从 Rust 合约生成命令和响应结构，在项目根目录执行：

```powershell
cargo run --locked -p ftw-ipc --example crm_schema -- command
cargo run --locked -p ftw-ipc --example crm_schema -- result
```

结构级校验由 Rust 反序列化执行，邮箱与网址规范化、长度限制、审核状态、归档和事务规则由 Core 执行。
