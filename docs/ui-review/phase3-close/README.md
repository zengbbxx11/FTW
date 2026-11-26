# FTW 第三阶段收尾验收

2026-11-24 · Windows 10 x64 · Tauri / WebView2 · 隔离客户和邮件样本

| 界面 | 原始参考图 | 改造前 | 改造后 |
| --- | --- | --- | --- |
| 开发邮件模板 | `17_开发邮件模板管理.png` | 原版仅能直接撰写草稿，[桌面截图](before/compose-1600.png) | [1600 × 900](after/templates-1600.png) · [1280 × 720](after/templates-1280.png) · [应用到草稿](after/compose-1600.png) |
| 客户文件与附件 | `25_客户文件与附件.png` | [邮件附件索引](before/files-1600.png) | [客户文件与来信附件](after/files-1600.png) |

模板页面对照原图采用模板库、编辑和规则三栏，保留邮件中心的分类导航。首轮运行时预览位于编辑器下方，需要垂直滚动才能读完。复验版将预览移至右栏，在相同视口可同时查看编辑内容和预览。客户文件页按参考图采用文件来源切换、列表和独立详情，保留关联邮件入口。

## 业务与回归

- 邮件模板支持用途筛选、编辑、归档和恢复。用 CRM 客户资料渲染主题和正文后，可把结果填入草稿。Rust Core 校验支持的四个变量，并在提交审批、入队和 SMTP 提交前检查草稿占位符。已在桌面验证保存、客户检索、预览、填入草稿、归档恢复、未知变量拒绝和未替换变量拦截。
- 客户资料与邮件附件共用索引。通过 Windows 原生对话框导入并另存测试 PDF，导出文件与原件逐字节一致。Core 为客户资料维护客户归属，复用内容哈希存储和备份。格式为 PDF、JPEG、PNG 和 WebP，每批最多 4 个、合计 10 MiB。Core 回归覆盖去重、备份恢复和目标文件防覆盖。
- 复用既有 FTW 的 Radix Popover 客户选择器、确认弹窗、分页、邮件框架和 Lucide 图标。复用版本与许可见[开源组件来源](../THIRD_PARTY.md)。SQLite 迁移为 `0016`，MCP 新增 `list_email_templates` 和 `render_email_template` 查询工具。
- `npm run lint`、`npm run build`、`cargo test --locked --offline --workspace`（27 项）、Tauri 构建、前端数据刷新测试（2 项）及两个 MCP stdio 协议版本回归通过。SMTP 和 IMAP 使用现有隔离回归，新流程未向真实收件人发信。Vite 仍提示主包约 678 kB。Win11 与 Windows 125% / 150% 系统缩放未复验。

从 `.ftw-dev/ui-phase3-close-preview/workbench.exe` 启动隔离样本版。邮件模板在「邮件中心 → 邮件模板」，客户文件在「数据与审计 → 客户附件」或客户档案内。预览版旁需保留 Core、MCP 和 `data` 目录。
