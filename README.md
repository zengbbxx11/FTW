# FTW 外贸智能工作台

FTW 是面向 Windows 10/11 x64 的本地外贸工作台。桌面用户和外部 Agent 通过同一个 Rust Core 使用客户、候选资料、邮件会话、跟进、样品和文件，变更会反映到已打开的工作台。当前源码的程序版本为 `1.1.5`，包含的最新数据库迁移版本为 `26`。正式便携 ZIP 和跨设备验收状态见[验收记录](docs/ACCEPTANCE_TESTS.md#已记录的验证)。

## 开始使用

1. 打开工作台，在「系统状态」确认数据目录。在「邮件中心 → 发件账户」配置 SMTP，在「收件设置」配置并测试 IMAP。仅使用 CRM 时可跳过邮箱配置。
2. 人工或 Agent 通过各自的数据来源搜集潜在客户后，先在「客户管理」和「候选客户」查重。待核实的信息可录入候选和调查证据，正式客户可记录多种联系方式、产品兴趣、自定义属性和跟进。
3. 从客户详情写邮件，或从已确认关联的业务来信回复原会话。通过 IMAP 同步后，在邮件中心和客户详情继续处理回复、样品和下一步沟通。
4. 如需让 Codex 或 WorkBuddy 协作，用同一工作台的 `ftw-mcp.exe` 配置本地 MCP stdio。在「邮件中心 → Agent 发信权限」分别登记客户端，按需保存协作权限和发送模式，并将该客户端的登记令牌安全地传入其 MCP 进程的 `FTW_AGENT_TOKEN` 环境变量。持续授权发送按已保存的账户、用途、数量和间隔执行，也可选择逐封人工审批。[接入说明](docs/FTW-Agent-Quickstart.txt) · [任务模板](docs/FTW-Agent-Workflows.txt)

## 开发运行

需要 Windows PowerShell 5.1、Node.js 24 或更高版本、npm、Rust x64 MSVC 工具链（含 rustfmt）、Visual Studio 2022 的 C++ x64 Build Tools、Windows SDK 和 WebView2 Evergreen Runtime。在项目根目录执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\dev.ps1
```

脚本构建 `ftw-core.exe` 和 `ftw-mcp.exe` 并启动 Tauri。工作台优先连接开发目录的现有 Core，必要时按需启动。开发数据位于 `.ftw-dev\workbench\data`，Core 日志位于 `.ftw-dev\workbench\logs\core.log`。

可在单独的终端手动运行 Core，通过 Ctrl+C 退出：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\dev.ps1 -CoreOnly
```

Core 运行时，可以在另一个终端读取同一开发目录的健康状态（默认 Cargo 输出目录）：

```powershell
.\target\debug\ftw-core.exe health --dev-root (Join-Path $PWD '.ftw-dev\workbench')
```

联调开发版 MCP 时，让客户端使用同一个开发工作区。以下示例使用 Codex CLI 和默认 Cargo 输出目录，构建的两个程序需保留在同一目录。

```powershell
cargo build --locked -p ftw-core -p ftw-mcp
codex mcp add ftw-dev -- (Join-Path $PWD 'target\debug\ftw-mcp.exe') --dev-root (Join-Path $PWD '.ftw-dev\workbench')
```

## 检查与无安装器构建

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\test.ps1
```

脚本安装锁定的 npm 依赖，执行 ESLint、TypeScript、Vite、Rust 格式、测试和构建。已安装 Clippy 时，在命令末尾追加 `-Clippy`。

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\build-portable.ps1
```

脚本在 `.ftw-dev\packages` 生成 Windows x64 便携目录和 ZIP，并在终端输出 ZIP 的 SHA-256。整包解压到可写目录，运行 `workbench.exe`。保留三个 EXE 和 `portable.flag`，数据在同目录的 `data` 下。移除便携标志时，Core 使用 `%LOCALAPPDATA%\FTW`。迁移整目录前关闭工作台和 MCP 客户端，并等待按需 Core 退出或结束手动运行的 Core。

ZIP 内的 `acceptance` 目录包含手工验收清单、四条虚构客户的 CSV 和 `Start-FTW-Acceptance.cmd`。双击验收入口会使用 `acceptance\workspace\data`，可从空工作区核查后再导入样本。

## 工作台入口

- 「客户管理」提供表格、看板、共享视图、字段管理、公司关联、样品进度和数据质量检查。查重后可在核对页选择要保留的客户档案。
- 「候选客户」保留来源、证据及核验状态。「跟进记录」汇总人工记录、Agent 记录和邮件活动。
- 「邮件中心」管理账户、签名、模板、草稿、审批、发送队列和收件会话。「待核对归属」用于处理无法可靠关联的来信。邮件工作流分别记录 SMTP 已接受、失败、提交结果未知、退信和业务回复。
- 「Agent 任务」查看已记录的业务操作和发信批次。「数据与审计」提供客户文件、客户 CSV 预览与导入导出、备份恢复及操作审计。「Agent 数据变更」显示客户字段差异和待核对建议。
- 「系统状态」显示 Core、数据库迁移、MCP 和邮箱配置情况，并提供「导出诊断报告」。窗口无法打开时，双击便携目录里的 `FTW-Diagnostics.cmd` 获取本地 JSON。反馈时附上报告、出错时间、所用页面或工具及复现步骤。[诊断报告范围](docs/ARCHITECTURE.md#故障诊断)

## 升级与换设备

关闭工作台和 MCP 客户端，确认旧 Core 退出，再成套替换三个 EXE。已有数据库需要迁移时，Core 会先生成一致性备份。移动便携工作台时，完整复制整个工作台目录，包括 `data`、`data\mail-objects`、`data\agent-send-safety.sqlite` 和已有的 `backups`，并更新 MCP 客户端中的 EXE 路径。更换 Windows 账户或设备后，在工作台重新输入 SMTP 和 IMAP 凭据并测试连接。通过工作台恢复业务备份后，按需重新登记 Agent 和保存发送权限。[存储与恢复](docs/ARCHITECTURE.md#sqlite-路径和迁移)

## 文档

- [Core 与存储设计](docs/ARCHITECTURE.md)
- [Codex 和 WorkBuddy 中文接入说明](docs/FTW-Agent-Quickstart.txt) · [可复制的外贸任务模板](docs/FTW-Agent-Workflows.txt)
- [邮件代发与 Codex、WorkBuddy 接入](docs/MCP_MAIL.md)
- [CRM 接口与 JSON Schema](docs/CRM_API.md)
- [Windows 验收用例与自动化测试入口](docs/ACCEPTANCE_TESTS.md)
- [功能与 UI 实施盘点](docs/ui-review/IMPLEMENTATION_STATUS.md) · [人工与 Agent 协同范围及验证](docs/ui-review/HUMAN_AGENT_PARITY.md)
- [变更记录](docs/CHANGELOG.md)
- [V1 产品与技术基线](FTW_产品与技术规格书_V1.md)
