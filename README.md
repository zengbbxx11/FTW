# FTW 外贸智能工作台

FTW 是面向 Windows 10/11 x64 的本地外贸工作台。当前版本支持 CRM 客户表格、阶段看板、跟进和候选客户，也支持 SMTP 邮件草稿、FTW 人工审批和通过 MCP 启用的代发任务。可在「系统状态」查看 Core、迁移版本和数据目录。

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
cargo build --release --locked -p ftw-core -p ftw-mcp
Push-Location .\apps\desktop
npm.cmd run tauri -- build --no-bundle -- --locked
Pop-Location
```

默认输出 `target\release\workbench.exe`、`ftw-core.exe` 和 `ftw-mcp.exe`。将三个 EXE 放在同一个目录，运行 `workbench.exe`。需要便携存储时，在同目录创建空的 `portable.flag`。没有标志时，Core 使用 `%LOCALAPPDATA%\FTW`。关闭手动启动的 Core，并等按需 Core 退出后再迁移目录。

## 阶段与文档

升级 M0 数据前先结束旧版 Core。新版迁移以前在数据根目录的 `backups` 中生成 SQLite 一致性快照。邮件代发 MCP 已随 SMTP 功能交付。IMAP 收件关联、其余 MCP 工具及可发布 ZIP 按后续里程碑交付。

- [Core 与存储设计](docs/ARCHITECTURE.md)
- [邮件代发与 Codex、WorkBuddy 接入](docs/MCP_MAIL.md)
- [CRM 接口与 JSON Schema](docs/CRM_API.md)
- [变更记录](docs/CHANGELOG.md)
- [V1 产品与技术基线](FTW_产品与技术规格书_V1.md)
