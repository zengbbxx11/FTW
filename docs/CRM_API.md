# FTW CRM 内部接口与共享数据合约

桌面 WebView 通过 Tauri 的 `crm_request` 调用 Core，参数对象包含 `command`。Rust 调用者可以使用 `ftw_ipc::crm::Command` 与 `ftw_ipc::crm::Result`。桌面操作由 Core 识别为 `manual`。MCP 经受限命令进入同一套 CRM 校验，Core 记录 Agent 来源和请求标识。下表是主要桌面接口，MCP 客户端用 [MCP 工具合约](MCP_MAIL.md)。

内部 IPC 请求示例

```json
{"version":1,"operation":{"crm":{"action":"list_contacts","query":{"keyword":"深圳","offset":0}}}}
```

IPC 响应返回 `{"status":"crm","data":{...}}`。桌面命令直接返回 `data`，错误通过 Promise rejection 返回 `code` 和 `detail`。表中的字段使用 snake_case。

| 操作 | 参数 | 成功返回 |
| --- | --- | --- |
| `list_contacts` | `query` 支持关键字与基础筛选，也支持 `filter`、`order`、`custom_columns`、`page_size`、`offset` | `contacts` |
| `get_contact` | `id` | `contact`，含资料、渠道、兴趣、计算的联系日期、`record_revision` 与 `data_epoch` |
| `state_catalog` | 无 | `state_catalog`，含稳定代码、标签和兼容别名 |
| `save_contact_checked` | `id`、`input`、`allow_duplicate`、`expected_revision`、可选 `expected_epoch` | `saved` 或 `duplicates`，旧版本时返回 `crm_revision_conflict` |
| `save_contact` | `id`（新建为空）、`input`、`allow_duplicate` | `saved` 或 `duplicates` |
| `set_archived` | `id`、`archived` | `saved` |
| `list_leads` | `offset`、`review_status` | `leads` |
| `get_lead` | `id` | `lead`，含证据网址、重复项、`record_revision` 和 `data_epoch` |
| `save_lead` | `id`（新建为空）、`input`、`allow_duplicate`，编辑传当前 `expected` | `saved` 或 `duplicates` |
| `set_lead_review` | `id`、`rejected`、当前 `expected` | `saved` |
| `promote_lead` | `id`、`allow_duplicate`、当前 `expected` | `saved`（正式客户 ID）或 `duplicates` |
| `list_activities` | `contact_id`（空值查询所有客户）、`event_type`、`offset` | `activities` |
| `save_activity` | `id`（新建为空）、`input` | `saved` |
| `update_activity` | `id`、`expected_revision`、`input` | `saved` |
| `list_lead_evidence` / `list_contact_evidence` | `lead_id` 或 `contact_id`，及 `offset` | `lead_evidence`，含证据、核验状态、统计及 `epoch` |
| `contact_history` / `custom_field_history` | `id`、`offset` | 客户字段变更记录及来源 |

常规列表分页结果含 `has_more`，客户视图可选择下文的分页大小。联系时间和发生时间使用 UTC Unix 秒，桌面按本地时区显示。归档后的联系人及跟进仍可读取，恢复后可继续编辑。候选写入的 `expected` 由当前 `record_revision` 和 `data_epoch` 组成，字段名为 `revision` 与 `epoch`。转正在同一事务中创建联系人、写入关联并记录审计，重复调用返回已有联系人。调查摘要、备注和置信度留在候选记录中，正式客户证据列表包含转正候选的证据并标记来源。

去重按规范化邮箱、网站域名、公司与国家组合提示。匹配的记录由调用者复核，确认时重新提交 `allow_duplicate: true`，源记录不会合并或删除。每种渠道最多设一个主要账号。产品兴趣为自由文本，数量用非负整数，可标记为估算。联系方式和产品兴趣随联系人事务提交并保留已有子项 ID。

业务错误为 `crm_invalid_field`（`detail` 是字段或校验名称）、`crm_not_found`、`crm_read_only` 和 `database_unavailable`。常规错误响应不包含 SQL 或客户正文。Core 事务审计保存操作者、来源、操作和对象 ID。

`quick_view` 支持 `priority` 与 `sample`。跟进可保存独立的 `channel`、`followup_category` 与 `customer_feedback`。这些字段缺省时保存为空文本，旧跟进在迁移后继续可读。

客户 CSV 的桌面入口为「数据与审计 → 客户 CSV」。预览在事务中校验并回滚，正式导入重新检验，跳过冲突或无效行。CSV 导出含 UTF-8 BOM，导出到数据根目录的 `exports`。

`level`、`sample_status`、`customer_type` 和 `source` 的筛选接收稳定代码和可识别别名。历史文本保留在记录中。重点和活跃样品预设使用统一规则。表格和详情的联系日期由 Core 组合真实沟通与人工日期，纯便签默认不参与。人工及 Agent 可用 `counts_as_contact` 标记业务事件。

## 自定义客户字段

桌面「客户 → 字段管理」可新增字段并管理选项、默认值、颜色、显示和顺序。归档会保留定义和值，可恢复。字段与选项使用稳定 ID，改名后现有值仍有效。现有字段的类型固定。新建客户时 Core 应用当前默认值，旧客户数据继续保留。

`list_fields` 与 `get_field_values` 分别读取定义和值。用 `patch_field_values` 提交 `contact_id`、最新 `expected_revision` 和 `expected_epoch`，及数组 `entries`。每项含 `field_id` 和带 `type`、`value` 的值，传 `null` 清除。新建可用 `create_contact_with_fields` 在同一事务提交客户和属性。

MCP 的 `create_customer_profile` 与桌面共用上述原子创建规则，`input` 使用 [`ContactInput`](../crates/ftw-ipc/src/crm.rs)，附带的 `entries` 限 Agent 可写字段。后续读取和写入仍遵守字段的当前访问等级。客户字段历史可以区分人工和 Agent，受限字段的历史仅在桌面查看。

MCP 的 `update_customer_profile` 只提交变动项和当前 `{id, revision, epoch}`。Core 读取当前档案并调用同一保存业务，沿用查重、字段校验和历史记录。`update_customer` 继续处理阶段、等级、备注等增量更新及待核对建议。[MCP 参数说明](MCP_MAIL.md#工具参数与结果)

## JSON Schema

从 Rust 合约生成命令和响应结构，在项目根目录执行：

```powershell
cargo run --locked -p ftw-ipc --example crm_schema -- command
cargo run --locked -p ftw-ipc --example crm_schema -- result
```

结构级校验由 Rust 反序列化执行，邮箱与网址规范化、长度限制、审核状态、归档和事务规则由 Core 执行。

## 保存视图

视图保存条件、排序、列宽、显示列、行密度、表格或看板布局。`list_views`、`save_view`、`archive_view` 与 `move_view` 是桌面的维护接口。视图修改需要当前版本。视图保存时清除分页位置和看板列值，恢复和归档保留客户原始记录。默认视图需要保持可见，切换默认视图后才可归档旧默认视图。

`query_customers` 和桌面 `list_contacts` 可接收组合条件。`filter` 支持 `group`（AND 或 OR）及 `rule`（稳定字段和选项 ID、运算符、类型化值）。分页支持 20、50 或 100 行。最多 3 个排序字段和 12 个投影自定义字段。表格列和看板分组需要保留原有的字段访问规则。行内和批量更新要求对应联系人的记录版本和数据纪元；批量最多更新 50 位客户，限阶段、等级和样品进度。

看板可按业务阶段或自定义单选字段分组。切换布局保留视图筛选和排序。看板各列单独分页，可拖动卡片或在卡片内用下拉菜单移动。
