# FTW 第四阶段第二批验收

> 本页为历史阶段记录，截图和测试结果对应当时构建；当前 1.1.7 的实现和未验证范围见[项目状态](../../PROJECT_STATUS.md)。

2026-11-24 · Windows 10 x64 · Tauri / WebView2 · 隔离样本

| 界面 | 对照原图 | 改造前 | 改造后 |
| --- | --- | --- | --- |
| Agent 开发任务中心 | `11_Agent开发任务中心.png` | [工作台 Agent 操作摘要](before/workbench-1600.png) | [1600 × 900](after/agent-1600.png) · [1280 × 720](after/agent-1280.png) |
| 邮件开发效果分析 | `21_邮件开发效果分析.png` | [工作台 SMTP 趋势](before/workbench-1600.png) | [1600 × 900](after/analytics-1600.png) · [1280 × 720](after/analytics-1280.png) |
| 系统状态 | `06_系统状态.png` | [Core 健康卡片](before/system-1600.png) | [1600 × 900](after/system-1600.png) · [1280 × 720](after/system-1280.png) |

- **Agent 执行记录。** 按登记 Agent 和任务 ID 汇总发信队列，另列有幂等记录的 MCP 写入调用。队列有实际待处理项时显示处理中，缺失调用结果、发送异常和明确退信进入核对。详情可打开客户、跟进、候选客户或原稿。FTW 记录业务结果，外部 Agent 的工作进度由其宿主负责。
- **邮件效果。** 默认统计明确归类的开发和跟进邮件，可以包含历史未分类记录。报告以 SMTP 已接受邮件为分母，每封邮件只计算一次同线程、同收件账户确认的人工回复。退信、同步已完成但未见回复、同步待确认分别显示。提交失败和结果未知单独统计。导出的 CSV 与所选范围采用同一 Core 统计逻辑。
- **系统状态。** 展示本机 Core IPC 和 SQLite 查询、MCP 工具发现及只读 CRM 查询结果、SMTP 配置、IMAP 最近同步和 WebView2 版本。主动的邮箱连接测试调用账户设置已有的 Core 检测命令。
- **接口与组件。** 复用共享面板、分页、状态样式、Lucide 和 TanStack Query。新增桌面接口 `data.agent_runs`、`data.agent_run_mail`、`data.mail_analytics`、`data.export_mail_analytics` 与 `mcp_status`。复用目前的业务表与 SQLite Schema 16。MCP 检测使用已有 stdio 工具。[开源组件来源](../THIRD_PARTY.md) · [MCP 邮件说明](../../MCP_MAIL.md)

## 桌面与核心回归

- 首轮截图后，把任务详情的联系人图标与名称调整为同行，并去掉三页简单表格继承的固定最小宽度。用 1600 × 900 与 1280 × 720 重新截图。
- 在同一个隔离工作区，用真正的 MCP 客户端新建客户，已打开的 Agent 任务中心显示写入结果并能打开新档案。还验证了原稿跳转、需核对筛选、退信明细和缺失结果的状态。
- 所选样本中有第二封人工回复、自动回信、退信、失败和结果未知。页面统计、逐日表、客户来源和 CSV 已核对。系统状态实测了本地 MCP 查询与 SMTP 无效凭据的失败反馈，并核对了已存的 IMAP 同步状态。
- `npm run lint`、`npm run build`、Tauri 构建、29 项 Rust 测试、2 项前端刷新测试和两版 MCP stdio 回归通过。Vite 保留约 722 kB 主包提示。真实邮箱联调、Win11 和 Windows 125%／150% 系统缩放留待最终验收。

打开 `.ftw-dev/ui-phase4-ops-preview/workbench.exe` 后，使用侧栏「Agent 任务」「系统状态」，或在邮件中心点击「效果分析」。预览版需要同目录的 Core、MCP 和隔离 `data` 目录。
