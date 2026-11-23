# FTW 外贸智能工作台

FTW 是面向 Windows 10/11 x64 的本地外贸工作台。工作台提供可保存的客户表格和看板、自定义字段、跟进、候选客户、SMTP、IMAP、Agent 接入和数据管理。用户与 Agent 通过同一个 Core 操作客户和邮件。可在「系统状态」查看 Core、迁移版本和数据目录。

## 开发运行

需要 Windows PowerShell 5.1、Node.js 24、npm、Rust x64 MSVC 工具链（含 rustfmt）、Visual Studio 2022 的 C++ x64 Build Tools、Windows SDK 和 WebView2 Evergreen Runtime。在项目根目录执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\dev.ps1
```

脚本构建 `ftw-core.exe` 并启动 Tauri。工作台优先连接开发目录的现有 Core，必要时按需启动。开发数据位于 `.ftw-dev\workbench\data`，Core 日志位于 `.ftw-dev\workbench\logs\core.log`。

可在单独的终端手动运行 Core，通过 Ctrl+C 退出：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\dev.ps1 -CoreOnly
```

Core 运行时，可以在另一个终端读取同一开发目录的健康状态（默认 Cargo 输出目录）：

```powershell
.\target\debug\ftw-core.exe health --dev-root (Join-Path $PWD '.ftw-dev\workbench')
```

## 检查与无安装器构建

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\test.ps1
```

脚本安装锁定的 npm 依赖，执行 ESLint、TypeScript、Vite、Rust 格式、测试和构建。已安装 Clippy 时，在命令末尾追加 `-Clippy`。

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\build-portable.ps1
```

脚本生成 Windows x64 ZIP 及 SHA-256，输出到 `.ftw-dev\packages`。整包解压到可写目录，运行 `workbench.exe`。保留三个 EXE 和 `portable.flag`，数据在同目录的 `data` 下。移除便携标志时，Core 使用 `%LOCALAPPDATA%\FTW`。迁移整目录前关闭工作台和 MCP 客户端，并等待按需 Core 退出或结束手动运行的 Core。

## 工作台入口

- 「客户」提供表格、看板、重点及样品客户视图。原有 AppFlowy 数据可先在 AppFlowy 导出 CSV，再通过「数据与审计 → 客户 CSV」预览并导入。导入支持姓名、公司、国家或地区、邮箱、电话、社交渠道、阶段等常用列。
- 「邮件中心」配置发件账户和收件账户。草稿进入待审核后，在 FTW 核对快照并人工批准，再由工作台或 Agent 提交发送。收件箱提供同步、会话和待关联列表。
- 「数据与审计」提供 SQLite 一致性备份、恢复、客户 CSV 导出和审计记录。升级旧数据前先结束旧版 Core。自动升级前备份保存在数据根目录的 `backups`。备份和 CSV 含业务数据，建议保存在仅授权用户可读的位置。

## 文档

- [Core 与存储设计](docs/ARCHITECTURE.md)
- [邮件代发与 Codex、WorkBuddy 接入](docs/MCP_MAIL.md)
- [CRM 接口与 JSON Schema](docs/CRM_API.md)
- [变更记录](docs/CHANGELOG.md)
- [V1 产品与技术基线](FTW_产品与技术规格书_V1.md)

### 开始使用

- 先在「客户」添加客户和收件邮箱，再从详情点击「写邮件」。在「邮件中心」配置 SMTP 发件账户，在「收件设置」配置 IMAP。
- 「邮件中心 → Agent 发信权限」可以登记 Codex、WorkBuddy 或其他 MCP stdio 客户端。持续授权由用户经 Windows 核验后开启，按发件邮箱、发送用途、数量和间隔生效。新数据目录默认逐封人工审批。
- 从早期版本升级时，关闭旧工作台与 MCP 客户端，确认旧 Core 已退出，再成套替换三个 EXE。迁移由 Core 完成。复制便携数据目录时，保留同目录发件安全账本和附件对象。
