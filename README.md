<p align="center">
  <a href="README.md"><img src="https://img.shields.io/badge/中文-当前-2563eb" alt="中文" /></a>
  <a href="README.en.md"><img src="https://img.shields.io/badge/English-Switch-64748b" alt="English" /></a>
</p>

<div align="center">
  <img src="apps/desktop/src-tauri/icons/icon.png" alt="DevConduit Logo" width="132" />

  <h1>DevConduit</h1>

  <p><strong>Codex、Claude Code、Grok Build、ZCode 与 Kilo Code 的本地配置与工作流控制台</strong></p>

  <p>
    <a href="https://github.com/Xdjjw/everything-patch/actions/workflows/main.yml"><img src="https://github.com/Xdjjw/everything-patch/actions/workflows/main.yml/badge.svg" alt="Build Package" /></a>
    <img src="https://img.shields.io/badge/desktop-Tauri%202-24C8DB" alt="Tauri 2" />
    <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-475569" alt="Windows and macOS" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-16a34a" alt="MIT License" /></a>
  </p>
</div>

## 概览

DevConduit 是一个本地桌面工具，用于集中管理多种 AI 编程工具的指令、Provider、配置文件、Skills 与 MCP。它直接对接你已经安装在本机的工具和配置目录，帮助你看清当前状态，并在确认后完成可追溯的配置写入。

它不是模型服务或账号服务。MCP 目录可在你明确确认后下载固定版本的上游文件、校验 SHA-256、准备隔离依赖环境并写入配置；Windows 本地模式还会检测 Cheat Engine、x64dbg/x32dbg 的安装目录，备份后安装桥接文件。也可随时切换为手动选择文件。DevConduit 不会在应用启动时自动改写任何工具配置。

项目此前使用过 Codex-X 和 Everything Patch。仓库中仍可见的 `codexx`、`codex-x`、`everything-patch` 和旧数据目录仅用于兼容已有配置；产品名称与后续发布均以 **DevConduit** 为准。

## 支持范围

| 工具 | 配置与指令 | Skills / MCP | 专属能力 |
| --- | --- | --- | --- |
| Codex | `config.toml`、登录信息、Provider、指令 | 支持 | 本地会话管理、皮肤中心 |
| Claude Code | `CLAUDE.md`、设置与 Provider | 支持 | Burp MCP 的 SSE 直连 |
| Grok Build | `AGENTS.md`、TOML 配置与 Provider | 支持 | Burp MCP 的 stdio 代理配置 |
| ZCode | system role、JSON 配置与 Provider | 支持 | Burp MCP 的 SSE 直连 |
| [Kilo Code](https://kilo.ai/) | 全局 `AGENTS.md`、JSONC 配置 | 支持 | 原生 JSONC MCP、全局 Skills 与 `/reload` 工作流 |

功能会根据本机实际安装状态显示。会话同步、检查和删除只适用于 Codex；皮肤中心也只面向 Codex。

## 主要功能

### 指令与提示词

- 管理内置与自定义 Markdown 指令，支持分类、导入、编辑、启用和禁用。
- Codex、Claude Code 与 ZCode 的默认模板分别同步自对应 Keysmith 项目并离线内置；Kilo 也提供适配其全局 `AGENTS.md` 的离线默认模板。启用仍需用户预览确认，应用启动不会自动写入工具配置。
- 在“保留原提示词”和“替换原提示词”之间切换，避免覆盖不属于 DevConduit 管理的内容。
- 写入前创建备份，便于检查和恢复；Kilo 首次安装会保留原始 `AGENTS.md` 的固定快照，卸载时精确恢复。

### Provider 与配置

- 在一个界面中查看、编辑、测试和切换 Provider。
- 管理 Codex 的 `config.toml` 与 `auth.json`，并支持从 cc-switch 导入已有 Provider。
- 按工具读取对应的 TOML、JSON 或 JSONC 配置，预览时对敏感信息进行处理并保留 Kilo JSONC 注释。

### Codex 会话管理

- 按项目路径、标题和 ID 搜索本地会话。
- 检查会话与当前 Provider / 模型的关系，并在需要时同步配置。
- 支持精确选择会话或项目范围删除。删除是不可恢复操作，界面会要求再次确认。

### Skills 与 MCP

- 查看和导入已有 Skills / MCP 配置，安装 ZIP Skill，并按条目启用或禁用。
- 在同一处查看受管配置与操作结果，配置写入失败会在弹窗中显示并恢复原配置。
- 提供针对常见逆向与安全工具的自动 MCP 接入目录，并保留手动文件模式，详见下一节。

### Codex 皮肤中心

- 导入、导出和切换 Codex 主题包；实机应用支持 macOS 和 Windows。
- 皮肤运行时只绑定本机回环地址，不修改官方 Codex 应用、`app.asar`、代码签名或配置目录权限。
- 首次应用可能需要重启 Codex。请先保存未发送的输入和进行中的工作。

## MCP 自动接入目录

在“Skills 与 MCP”页面选择目标工具和集成后，默认使用“自动获取”：DevConduit 下载经过固定提交或版本及 SHA-256 校验的文件，在需要时创建独立 Python 环境，并在确认后写入 MCP 配置。下载内容保存在应用数据目录下的 `mcp-integrations` 中；离线文件或自定义版本仍可使用“手动选择”。

| 集成 | 本地条件 | Windows | macOS |
| --- | --- | --- | --- |
| [IDA Pro MCP](https://github.com/mrexodia/ida-pro-mcp) | IDA Pro 8.3+、Python 3.11+、`uv`，且已激活 idalib | 自动准备并本地运行 | 自动准备并本地运行 |
| [Cheat Engine MCP](https://github.com/miscusi-peek/cheatengine-mcp-bridge) | Cheat Engine 与 Python | 自动安装 Lua 桥的 Named Pipe 本地模式 | 远程 Windows TCP 桥接 |
| [x64dbg MCP](https://github.com/Wasdubya/x64dbgMCP) | x64dbg/x32dbg 与 Python | 自动安装对应 32/64 位插件 | 远程 Windows HTTP 桥接 |
| [Burp Suite MCP](https://github.com/PortSwigger/mcp-server) | Burp Suite；stdio 代理模式还需要 Java | 依目标工具选择 SSE 或代理 | 依目标工具选择 SSE 或代理 |

接入规则：

- 每次自动获取都先校验固定 SHA-256；缓存不匹配时会重新下载，校验失败不会写入工具配置。固定来源与许可证见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
- **IDA Pro MCP** 首次自动获取会执行 `uv sync --locked`；之后使用 `uv run --offline --no-sync`，避免运行时自行同步依赖。
- **Cheat Engine 与 x64dbg** 的本地模式仅适用于 Windows。DevConduit 会自动搜索常见安装位置，也可由你指定便携版目录；安装前创建宿主文件备份，恢复入口可还原最近一次安装，且不会覆盖安装后被你另行修改的插件。在 macOS 上使用时，连接到你自己维护的远程 Windows 桥接端，本机无法越过网络替远程主机写文件。
- **Burp Suite MCP** 的传统 SSE 直连用于 Claude Code、ZCode 和 Kilo Code。Codex 与 Grok Build 使用 Burp 扩展提供的官方 `mcp-proxy-all.jar` 作为 stdio 代理，以避免传输协议不兼容。
- 第三方软件自身仍有两项无法由外部应用可靠代办：IDA 首次 idalib 激活，以及 Burp 首次在 Extensions 中启用官方扩展。其余下载、校验、依赖准备、目标配置和 Windows CE/x64dbg 文件安装均在确认后自动完成。任何写入失败都会显示在弹窗内，并恢复原工具配置与本次宿主安装。

请只连接自己信任的 MCP 服务和远程桥接地址。对于 HTTP 桥接，建议始终限制在受信任网络或 `127.0.0.1`。

## 下载与安装包

正式版本请从 [GitHub Releases](https://github.com/Xdjjw/everything-patch/releases) 获取。推送形如 `v0.4.0` 的版本标签后，发布工作流会构建并把以下安装包直接上传到对应的 Release：

| 平台 | 格式 | Release 安装包 |
| --- | --- | --- |
| Windows x64 | `.msi` | `DevConduit_<version>_x64.msi` |
| macOS Apple Silicon | `.dmg` | `DevConduit_<version>_aarch64.dmg` |
| macOS Intel | `.dmg` | `DevConduit_<version>_x64.dmg` |

`Build Package` 工作流仍会在推送到 `main` 时生成 Actions 验证产物。当前发布包会校验 macOS 应用包的完整 ad-hoc 签名，但没有 Apple Developer 签名或公证；首次打开 macOS 安装包时，请在 Finder 中右键 `DevConduit.app` 并选择“打开”。Windows 也可能显示 SmartScreen 提示。

## 从源码运行

前置条件：Node.js 22、pnpm 9 和 Rust stable。macOS 还需要可用的 Xcode Command Line Tools。

```bash
pnpm install --frozen-lockfile
pnpm dev
```

常用校验与打包命令：

```bash
pnpm typecheck
pnpm build
```

按目标平台构建：

```bash
pnpm --dir apps/desktop build -- --target x86_64-pc-windows-msvc
pnpm --dir apps/desktop build -- --target aarch64-apple-darwin
pnpm --dir apps/desktop build -- --target x86_64-apple-darwin
```

本地未设置 `TAURI_SIGNING_PRIVATE_KEY` 时，构建脚本会关闭需要发布私钥的 updater 产物，但仍会生成常规安装包。发布、签名和公证应由受控的 Release 流程处理。

## 配置与迁移

Codex 默认配置路径：

```text
~/.codex/config.toml
~/.codex/auth.json
```

DevConduit 会继续使用原有本地数据目录，保证覆盖升级后保留已有配置：

```text
~/.everything-patch/everything-patch.db
```

可用环境变量：

```text
CODEX_HOME=/path/to/.codex
EVERYTHING_PATCH_HOME=/path/to/everything-patch-data  # legacy compatibility key
CC_SWITCH_HOME=/path/to/.cc-switch
```

旧版 `CODEXX_HOME`、`~/.codexx/codexx.db` 与 `codex-x-skins` 目录会被自动识别，已有用户无需手动迁移。

## 使用边界

- 在写入前核对目标工具、配置目录、文件路径和远程端点。
- 第三方 MCP、调试器、代理和主题包由其各自作者维护；使用前请阅读其许可证、安全说明和兼容性要求。
- 请仅在合法、合规且获得授权的环境中使用本项目及其连接的工具。

## 贡献与许可证

欢迎通过 [Issues](https://github.com/Xdjjw/everything-patch/issues) 报告问题、提出功能建议或提交改进。项目使用 [MIT License](LICENSE)。
