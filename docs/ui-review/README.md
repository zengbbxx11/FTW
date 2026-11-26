# FTW 客户管理样板验收

本页记录客户管理样板及各阶段的隔离验证。当前功能、人工与 Agent 协同和待发布验收范围分别见[功能与 UI 实施盘点](IMPLEMENTATION_STATUS.md)、[双向协同记录](HUMAN_AGENT_PARITY.md)和[发布验收用例](../ACCEPTANCE_TESTS.md)。

2026-11-24 · Windows 10 x64（10.0.19042.870）· Tauri / WebView2

本次完成共享框架、客户管理表格及详情抽屉。已逐张检查参考目录的 26 张原图。首期主要对照 `10_已发未回复客户视图.png` 的表格、工具栏与详情布局，`05_自定义视图与字段.png` 的列设置，`14_高级筛选条件构建器.png` 的筛选入口，以及 `01_工作台首页.png` 的侧栏。

| 实际运行画面 | 截图 |
| --- | --- |
| 1600 × 900 客户管理 | [改造前](before/customers-1600.png) · [改造后](after/customers-1600.png) |
| 1600 × 900 并排详情 | [改造前](before/detail-1600.png) · [改造后](after/detail-1600.png) |
| 详情在其他尺寸下的表现 | [1280 × 720 覆盖式](after/detail-1280.png) · [1920 × 1080 并排](after/detail-1920.png) |
| 列设置和紧凑窗口 | [列设置](after/columns-1600.png) · [1000 × 710 折叠侧栏](after/compact-1000.png) |

新旧截图来自实际运行的 Windows Tauri 应用，使用同源的 8 条隔离样本。折叠侧栏截图包含为分页验收新增的样本。第一轮对照后修正了侧栏选中宽度、窄窗新建视图入口、详情标签截断和表格日期占宽，复验截图已归档。

## 验收结果

- **已实现并验证**。顶部视图和工具栏保持全宽。抽屉在宽窗并排，在窄窗覆盖。列设置可切换显示字段，行菜单支持键盘导航，抽屉字段编辑会写入 Core 并刷新同一客户表格。验证了搜索、高级筛选入口、保存视图、看板切换、当页选中及 22 条记录的两页查询。
- **MCP 联动**。MCP 更新客户阶段后，已打开的桌面表格自动显示更新。独立 stdio 回归通过 `2025-06-18`、`2025-11-25` 两个协议版本的工具发现、调用及审批隔离。
- **构建与核心回归**。`npm run build`、`npm run lint`、`cargo build --locked --offline --workspace --features tauri/custom-protocol` 通过。Rust 工作区测试 25 项、变更通知前端测试 2 项通过。邮件回归覆盖模拟 SMTP、结果未知处理和测试 IMAP；真实邮箱联调、Win11、Windows 125% / 150% 系统缩放留待后续验收。Vite 构建保留主包大小提示，约 616 kB，gzip 约 181 kB。
- **后续页面**。对应 `08_新增编辑客户表单.png` 的完整表单视觉、独立字段管理、邮件中心和其余业务页面进入后续阶段。当前样板使用已有业务接口与 SQLite 数据结构。

桌面样板可从项目内的 `.ftw-dev/ui-phase1-preview/workbench.exe` 启动，旁边的 Core、MCP 和样本目录需一同保留。组件复用、提交版本和许可见[开源组件来源](THIRD_PARTY.md)。

[第二阶段核心界面验收](phase2/README.md)

[第三阶段首批界面验收](phase3/README.md)

[第三阶段第二批界面验收](phase3-more/README.md)

[第三阶段收尾验收](phase3-close/README.md)

[第四阶段首批工作台与搜索验收](phase4-home/README.md)

[第四阶段第二批 Agent、邮件分析与系统状态验收](phase4-ops/README.md)

[第四阶段数据与审计工作区验收](phase4-data/README.md)

[第四阶段公司联系人及客户质量检查](phase4-crm/README.md)

[功能补齐与桌面复验](phase-gaps/README.md)

[CRM 剩余功能补齐与桌面复验](phase-final-features/README.md)
