# CRM 剩余功能补齐与桌面复验

2026-11-25 · Windows 10 x64 · Tauri / WebView2 · 隔离 SQLite 数据

| 功能 | 桌面交互与记录 | 运行截图 |
| --- | --- | --- |
| Agent 更正客户资料 | 使用稳定的联系渠道和产品兴趣 ID 提出建议，展示修改前后与来源。人工确认后更新原记录。实际邮箱更正保留首选设置，原地址的核实标记清除。 | [联系渠道更正](final/corrections-1600.png) |
| 正式客户证据 | 详情新增调查证据分区。候选转正证据可以追溯来源，正式客户证据可由桌面与 MCP 追加，由桌面核验。 | [客户详情抽屉](final/customer-evidence-1600.png) |
| CRM 跨页批量操作 | 先从两页各选一人更新，再从第 2 页选择全部 23 位匹配客户。SQLite 实测范围外联系人未受影响。Core 在执行前核对选区指纹，遇范围或记录版本变化时拒绝本批修改。 | [全部匹配选择](final/bulk-1600.png) |
| 独立公司档案 | 独立保存公司名称、别名、网站和备注。按别名找到公司后可绑定不同地区的联系人，并填写其分支。联系人的原公司文本和地区各自保留。 | [1600 × 900](final/companies-1600.png) · [1280 × 720](final/companies-1280.png) |

## 视觉和兼容

- 参考图编号沿用 `10`、`13`、`20`、`26`。针对首轮实拍调整了证据抽屉的窄列、批量工具栏图标对齐、公司表格的日期及邮箱显示。原窗口布局与入口见[前阶段证据](../phase-gaps/README.md)和[原公司工作区](../phase4-crm/README.md)。
- 使用 Schema 19 的隔离数据副本启动 Schema 22，原已核实的候选证据、Agent 记录和联系资料可正常读取。公司关联记录独立持久化，创建公司联系人与保存公司归属使用同一事务。
- 补验了历史人工联系时间颠倒。质量检查可筛出异常客户，并打开业务状态分区供用户修正。[联系时间质量检查](quality/contact-date-quality-1280.png)
- `npm run lint`、`npm run build`、32 项 Rust 工作区测试、2 项前端刷新测试、Tauri 构建和两版 MCP stdio 回归通过。Vite 仍提示主包约 770 kB。沿用现有确认弹窗、TanStack Table、Radix 弹层和 Lucide。[MCP 工具说明](../../MCP_MAIL.md)

正式便携 ZIP、真实 SMTP／IMAP 联调和 Win11、高倍系统缩放整体验收继续暂停。

项目内的 `.ftw-dev/ui-feature-complete-preview/workbench.exe` 已验证能启动隔离 Core。预览程序使用同目录的 Core、MCP 和样本数据库。
