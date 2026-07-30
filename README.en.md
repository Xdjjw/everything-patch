<p align="center">
  <a href="README.md"><img src="https://img.shields.io/badge/中文-Switch-64748b" alt="Chinese" /></a>
  <a href="README.en.md"><img src="https://img.shields.io/badge/English-Current-2563eb" alt="English" /></a>
</p>

<div align="center">
  <img src="apps/desktop/src-tauri/icons/icon.png" alt="Everything Patch Logo" width="132" />

  <h1>Everything Patch</h1>

  <p><strong>A local configuration and workflow console for Codex, Claude Code, Grok Build, and ZCode</strong></p>

  <p>
    <a href="https://github.com/Xdjjw/everything-patch/actions/workflows/main.yml"><img src="https://github.com/Xdjjw/everything-patch/actions/workflows/main.yml/badge.svg" alt="Build Package" /></a>
    <img src="https://img.shields.io/badge/desktop-Tauri%202-24C8DB" alt="Tauri 2" />
    <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-475569" alt="Windows and macOS" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-16a34a" alt="MIT License" /></a>
  </p>
</div>

## Overview

Everything Patch is a local desktop application for managing instructions, providers, configuration files, Skills, and MCP servers across several AI coding tools. It works with tools and configuration directories already present on your machine, making their current state visible and applying deliberate, traceable configuration changes after confirmation.

It is not a model service, account service, or third-party installer. In particular, its MCP catalog validates files that you select yourself and writes configuration only. It never downloads, installs, or runs third-party installers automatically.

The project was formerly named Codex-X. Remaining `codexx`, `codex-x`, and legacy data-directory references exist only for backward compatibility; the product name and future releases are **Everything Patch**.

## Preview

<p align="center">
  <img src="docs/screenshots/app/new-ui/prompts.png" alt="Everything Patch instruction and prompt management" width="900" />
</p>

<p align="center">
  <img src="docs/screenshots/app/new-ui/skills-mcp.png" alt="Everything Patch Skills and MCP management" width="900" />
</p>

## Supported Tools

| Tool | Configuration and instructions | Skills / MCP | Tool-specific capability |
| --- | --- | --- | --- |
| Codex | `config.toml`, authentication, providers, and instructions | Supported | Local session management and Skin Center |
| Claude Code | `CLAUDE.md`, settings, and providers | Supported | Burp MCP SSE connection |
| Grok Build | `AGENTS.md`, TOML configuration, and providers | Supported | Burp MCP stdio proxy configuration |
| ZCode | System role, JSON configuration, and providers | Supported | Burp MCP SSE connection |

Features are shown according to what is installed locally. Session synchronization, inspection, and deletion are Codex-only. Skin Center is also for Codex only.

## Main Capabilities

### Instructions and Prompts

- Manage bundled and custom Markdown instructions with categories, import, editing, enablement, and disablement.
- Choose whether to preserve existing instructions or replace them, without overwriting content that Everything Patch does not manage.
- Create a backup before a managed write so changes can be reviewed or restored.

### Providers and Configuration

- View, edit, test, and switch providers from one place.
- Manage Codex `config.toml` and `auth.json`, including importing existing providers from cc-switch.
- Read the TOML or JSON configuration used by each tool and handle sensitive values appropriately in previews.

### Codex Session Management

- Search local sessions by project path, title, and ID.
- Inspect session relationships to the active provider and model, then synchronize configuration where appropriate.
- Select individual sessions or entire projects for deletion. Deletion is irreversible and requires confirmation.

### Skills and MCP

- View and import existing Skills and MCP configuration, install ZIP Skills, and enable or disable entries individually.
- Review managed configuration and action results in one place. Failed configuration writes are shown in the dialog and the previous configuration is restored.
- Use the manual MCP catalog for common reverse-engineering and security tooling, described below.

### Codex Skin Center

- Import, export, and switch Codex theme packs; live application is supported on macOS and Windows.
- The skin runtime binds only to the local loopback interface and does not modify the official Codex app, `app.asar`, code signature, or configuration-directory permissions.
- Applying a theme may require restarting Codex. Save unsent input and in-progress work first.

## Manual MCP Catalog

From the **Skills & MCP** screen, select a target tool, choose a local project directory, script, or JAR yourself, then confirm before writing the MCP configuration. You must install and prepare third-party software and dependencies independently.

| Integration | Local prerequisites | Windows | macOS |
| --- | --- | --- | --- |
| [IDA Pro MCP](https://github.com/mrexodia/ida-pro-mcp) | IDA Pro 8.3+, Python 3.11+, `uv`, manually activated idalib, and a completed `uv sync` | Local | Local |
| [Cheat Engine MCP](https://github.com/miscusi-peek/cheatengine-mcp-bridge) | Cheat Engine, Python, and the MCP bridge project | Local Named Pipe mode | Remote Windows TCP bridge |
| [x64dbg MCP](https://github.com/Wasdubya/x64dbgMCP) | x64dbg/x32dbg plugin and Python bridge script | Local mode | Remote Windows HTTP bridge |
| [Burp Suite MCP](https://github.com/PortSwigger/mcp-server) | Burp MCP Server extension; Java is also required for stdio proxy mode | SSE or proxy by target tool | SSE or proxy by target tool |

Integration rules:

- **IDA Pro MCP** uses `uv run --offline --no-sync`; it will not synchronize or download dependencies while configuring the integration.
- **Cheat Engine and x64dbg** local modes are Windows-only. On macOS, connect to a Windows bridge that you operate.
- **Burp Suite MCP** supports direct SSE only for Claude Code and ZCode. Codex and Grok Build use the official `mcp-proxy-all.jar` emitted by the Burp extension as a stdio proxy to avoid transport incompatibilities.
- The application validates selected paths and expected file names. It does not download projects, install extensions, create Python environments, or run third-party installers for you.

Connect only to MCP servers and remote bridges that you trust. For HTTP bridges, prefer a trusted network or `127.0.0.1`.

## Downloads and Packages

Use [GitHub Releases](https://github.com/Xdjjw/everything-patch/releases) for published versions. The `Build Package` workflow runs on pushes to `main` and manual dispatch, producing these artifacts:

| Platform | Format | Actions artifact name |
| --- | --- | --- |
| Windows x64 | `.msi` | `Everything-Patch-package-windows-latest-x86_64-pc-windows-msvc` |
| macOS Apple Silicon | `.dmg` | `Everything-Patch-package-macos-latest-aarch64-apple-darwin` |
| macOS Intel | `.dmg` | `Everything-Patch-package-macos-latest-x86_64-apple-darwin` |

Verification packages are available from [Actions](https://github.com/Xdjjw/everything-patch/actions/workflows/main.yml). They are build artifacts, not necessarily signed, notarized, or published installers. Use them only when you trust the source commit.

## Run From Source

Prerequisites: Node.js 22, pnpm 9, and Rust stable. macOS also needs working Xcode Command Line Tools.

```bash
pnpm install --frozen-lockfile
pnpm dev
```

Common validation and packaging commands:

```bash
pnpm typecheck
pnpm build
```

Build for a specific target:

```bash
pnpm --dir apps/desktop build -- --target x86_64-pc-windows-msvc
pnpm --dir apps/desktop build -- --target aarch64-apple-darwin
pnpm --dir apps/desktop build -- --target x86_64-apple-darwin
```

When `TAURI_SIGNING_PRIVATE_KEY` is not set locally, the build wrapper disables updater artifacts that require the release key while still producing regular installers. Release signing and notarization belong to the controlled Release workflow.

## Configuration and Migration

Default Codex paths:

```text
~/.codex/config.toml
~/.codex/auth.json
```

Everything Patch local data defaults to:

```text
~/.everything-patch/everything-patch.db
```

Supported environment variables:

```text
CODEX_HOME=/path/to/.codex
EVERYTHING_PATCH_HOME=/path/to/everything-patch-data
CC_SWITCH_HOME=/path/to/.cc-switch
```

Legacy `CODEXX_HOME`, `~/.codexx/codexx.db`, and `codex-x-skins` locations are detected automatically, so existing users do not need to migrate manually.

## Boundaries

- Review the target tool, configuration directory, file path, and remote endpoint before writing.
- Third-party MCP servers, debuggers, proxies, and theme packs are maintained by their respective authors. Review their licenses, safety guidance, and compatibility requirements before using them.
- Use this project and connected tools only in legal, authorized environments.

## Contributing and License

Please use [Issues](https://github.com/Xdjjw/everything-patch/issues) for bugs, feature requests, and feedback. The project is available under the [MIT License](LICENSE).
