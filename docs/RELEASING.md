# Windows 便携版发行与更新

当前版本与验证范围见[项目状态](PROJECT_STATUS.md)。正式 Release 表示发布渠道，不等于全部环境与回归用例均已验证；发行说明应明确列出未执行项。

## 发行包

本项目将正式版本发布到 [GitHub Releases](https://github.com/zengbbxx11/FTW/releases)。统一更新 `Cargo.toml`、`apps/desktop/package.json`、`apps/desktop/package-lock.json` 和 `apps/desktop/src-tauri/tauri.conf.json` 的版本号，运行必要的构建检查，然后在项目根目录执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File .\scripts\build-portable.ps1 -ForRelease
```

普通开发构建保存在 `.ftw-dev\builds\v版本号\构建编号`；只有 `-ForRelease` 才会在 `.ftw-dev\packages\v版本号`（例如 `v1.1.7`）生成唯一一套正式发行文件：

- `FTW-Portable-win-x64.zip`：Windows x64 便携包，包含首次升级用的 `FTW-Update.exe`。
- `FTW-Update.json`：版本、目标平台和 ZIP 的 SHA-256。
- `FTW-Update.json.sig`：对上述清单原始内容的 Ed25519 签名。

ZIP 内的工作台文件夹固定为 `FTW-Portable`，不附带版本号和随机编号。版本由程序内的版本信息和发行标签表达；修复已发行版本时增加补丁号，例如 `1.1.6 → 1.1.7`，不覆盖同版本发行包。脚本拒绝覆盖已经存在的正式发行目录。旧用户原目录的历史名称可以保留，避免改变已有 MCP 路径，实际版本以「系统状态」显示为准。

创建与版本号对应的正式 Release（例如 `v1.1.7`），将这三个文件作为资产上传并发布。草稿和预发行版不会作为自动更新。只上传源代码 ZIP 或只推送 Git 提交不会生成可用更新。发布前核对包内没有业务数据、日志、凭据或签名私钥。

`-ForRelease` 要求签名密钥存在且与仓库公钥匹配。普通本地构建不要求密钥；若没有密钥，脚本会生成无法用于自动更新的未签名便携包并给出提示。可通过 `FTW_RELEASE_KEY_FILE` 指定受 Windows 当前用户保护的密钥文件路径。

## 跨设备开发与固定体验目录

开发者和 Agent 统一使用相对项目根目录的路径约定：

| 用途 | 路径 |
| --- | --- |
| 当前日常体验工作台 | `.ftw-dev/packages/FTW-LastTest` |
| 临时开发构建 | `.ftw-dev/builds/v版本号/构建编号/FTW-Portable` |
| 正式发行资产 | `.ftw-dev/packages/v版本号` |
| 本机路径覆盖配置 | `.ftw-dev/active-workbench.json`，由 Git 忽略 |

换到其他设备克隆仓库后，默认规则直接套用到该设备的项目目录，不依赖某个盘符。需要其他位置时，可在本机配置中写入绝对路径或相对项目根目录的 `directory`，例如：

```json
{
  "schema_version": 1,
  "directory": ".ftw-dev/packages/FTW-LastTest"
}
```

这份配置供维护工作台的开发者和 Agent 读取；当前 `build-portable.ps1` 生成新构建，不会自动替换正在运行的体验目录。构建完成后，维护者先备份并关闭正在使用的工作台与 MCP 客户端，再用本次构建的 `FTW-Update.exe` 更新固定目录。首次尚无体验目录时，可将干净构建复制到该位置并保留 `portable.flag`。已有数据的目录始终原位置升级，不复制空数据库覆盖它。

每次程序变更交付都要同步最新可运行包体：从当前源码重新构建四项 EXE 和说明文件，核对版本后更新本地体验目录。需要正式发布时，再将新版本 ZIP、清单及签名上传到对应源码标签。不能只变更版本文字或沿用旧二进制。仅修改开发约定和文档时，提交文档即可，已发布的相同二进制保持不可变。

数据库迁移文件通过 `.gitattributes` 固定为 LF 换行，确保其他 Windows 设备的 Git 配置不会改变 SQLx 迁移校验值。已发布的迁移文件不可随意修改；需要调整数据库时新增迁移。

若体验程序尚未退出或更新未成功，应明确报告，不能将“源码已提交”表述为“本机体验包已更新”。在新设备本地编译不需要发行私钥；签名发布按下一节恢复原有私钥。

## 签名密钥

当前项目的公钥位于 `crates/ftw-updater/src/update-public-key.txt`，构建时嵌入程序。对应的本机私钥默认保存在 Git 忽略的 `.ftw-dev\release\FTW-signing.dpapi`，使用当前 Windows 用户的 DPAPI 加密，不能直接迁移到其他 Windows 用户或电脑。**不要公开、提交、重建或替换已有私钥**；重建密钥后，已经发布的旧版无法信任新签名。

首次在另一台构建电脑上维护发行版之前，应在原电脑创建一次加密备份，并将备份和密码分开、安全保存。先构建 `ftw-release.exe`，再在项目根目录执行（目标文件应位于你选择的安全备份位置）：

```powershell
cargo build --release --locked -p ftw-updater --bin ftw-release
.\target\release\ftw-release.exe backup .\.ftw-dev\release\FTW-signing.dpapi D:\安全备份\FTW-release-key.json
```

程序会要求输入两次至少 12 个字符的备份密码，输入时不显示。更换电脑后，在含相同公钥的源码目录构建 `ftw-release.exe`，然后恢复到当前 Windows 用户：

```powershell
.\target\release\ftw-release.exe restore D:\安全备份\FTW-release-key.json .\.ftw-dev\release\FTW-signing.dpapi
```

恢复不会覆盖已有密钥。若加密备份和原 Windows 用户均不可用，现有用户将无法通过当前公钥验证后续更新，需要重新发布并人工安装新的可信版本。

只有全新分叉、尚未向任何用户发行过的项目才应初始化自己的密钥：将分叉中的公钥文件置为 `UNCONFIGURED` 后，运行 `ftw-release.exe init-key <受保护私钥路径> <公钥文件路径>`，再重新构建。不要在现有 FTW 项目中执行此操作。

## 更新过程

首次从没有更新入口的旧版过渡时，打开旧 FTW，运行新包的 `FTW-Update.exe`，选择工作台并按提示完成邮件任务、关闭 FTW 和 MCP 客户端。更新程序会等待旧版的发送队列清空，旧 Core 的空闲退出最长需要等待几分钟；如果发送队列长时间无法完成，请先在工作台处理队列后重试。

之后在「系统状态 → 软件更新」检查并确认更新。更新程序只替换指定的程序及说明文件，保留 `data`、`portable.flag`、`logs` 和原有备份。Core 忙于审批、发信或 IMAP 同步时会要求稍后重试；MCP 客户端占用文件时需要先关闭。旧程序文件保存在工作台的 `backups\program-update-*` 目录。数据库迁移沿用 Core 的现有备份机制，更新器不会自动用旧数据库覆盖当前数据。

更新失败时保留原工作台目录及备份，检查 GitHub 网络、版本资产和文件占用，然后重新尝试；若 FTW 能打开，可在「系统状态」导出诊断报告。检查更新不会上传客户资料、邮件内容或邮箱凭据。

新增桌面命令时，除 `invoke_handler` 外还必须同步更新 Tauri `build.rs` 的命令清单和 `capabilities/main.json` 的权限。1.1.6 的连接配置与更新命令遗漏了这两处声明，前端调用会被拦截；1.1.7 补齐了声明。若旧版出现“无法生成配置”和笼统的检查更新错误，使用发行包中的 `FTW-Update.exe` 完成过渡升级。
