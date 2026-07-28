# 跨平台兼容性审计（v0.3.3）

审计范围：桌面端平台分支、外部进程调用、路径与文件锁、ZCode 注入、皮肤运行时、
Tauri 资源打包，以及 GitHub Actions 正式发布流程。

## 结论

当前正式发布流程（`release.yml`）支持以下目标：

| 平台 | 正式发布 runner | 架构 | 状态 |
|---|---|---|---|
| Windows | `windows-latest` | x86_64 | 本机测试与 MSI/NSIS 打包已验证 |
| macOS | `macos-latest` | Apple Silicon | 原生 CI 测试、签名、公证与打包 |
| macOS | `macos-15-intel` | Intel x86_64 | 原生 CI 测试、签名、公证与打包 |
| Linux | `ubuntu-22.04` | x86_64 | 原生 CI 测试与 DEB/RPM/AppImage 打包 |

手动验证流程（`build.yml`）的 runner 口径不同：

| 目标 | 验证 runner | 构建方式 |
|---|---|---|
| Windows x86_64 | `windows-latest` | 原生构建 |
| macOS Apple Silicon | `macos-latest` | 原生构建 |
| macOS Intel x86_64 | `macos-latest` | 在 Apple Silicon runner 上交叉编译 |

因此，“macOS Intel 原生 runner”只适用于正式发布流程中的 `macos-15-intel`，
不能用于描述 `build.yml`。

平台分层整体合理：`zcode/` 和 `skin_runtime/` 由统一入口按 `#[cfg]` 分派，
Windows 子进程通过 `platform::program_command` 或同等的 `CREATE_NO_WINDOW` 配置运行，
用户目录通过 `dirs::home_dir()` 解析。

本轮修复了构建入口不一致、Windows 文件锁、macOS ZCode 持久化和跨工具概览串值。
目前没有已知的 Windows x64 / macOS ARM / macOS Intel / Linux x64 编译阻塞。

---

## 已修复问题

### H1. 两套工作流同时发布同一个版本

修复前，`.github/workflows/build.yml` 和 `.github/workflows/release.yml` 都监听 `v*` 标签。
前者直接发布未经过正式校验的安装包，后者创建草稿、签名、公证、验证 updater 后再发布。
两个工作流可能争抢同一个 GitHub Release，并产生内容不同的 Windows/macOS 产物。

修复后：

- `release.yml` 是唯一监听版本标签并发布 Release 的工作流。
- `build.yml` 改为仅可手动触发的构建验证，不再创建 Release。
- 手动构建产物使用 `validation` 名称，避免被误认为正式发布包。

### H2. Windows Node.js 只在部分构建入口准备

Windows 皮肤运行时依次使用：

1. `EVERYTHING_PATCH_SKIN_NODE` / `CODEX_X_SKIN_NODE`
2. `<应用目录>/skin-runtime/node/node.exe`
3. PATH 中的 Node.js

仓库原本已经有 `tauri.windows.conf.json` 资源映射，正式 `release.yml` 也会准备 Node，
但本地 `pnpm build` 和旧 `build.yml` 不会准备，因此不同入口生成的安装包内容不同。

修复后：

- 新增 `apps/desktop/scripts/stage-windows-node.ps1`。
- 固定下载 `node-v22.23.1-win-x64.zip`。
- 下载后校验 SHA-256，再复制 `node.exe` 与 LICENSE。
- 本地 `build.mjs`、手动 `build.yml`、正式 `release.yml` 共用同一个脚本。
- 已有正确版本时直接复用，不重复下载。
- `tauri.windows.conf.json` 仅在 Windows 包中映射到 `skin-runtime/node/`。

本机重新打包后：

- MSI：约 40.16 MB
- NSIS：约 26.59 MB
- WiX 与 NSIS 生成清单均包含 `skin-runtime/node/node.exe`、LICENSE 和 README。

### H3. macOS ZCode 环境变量无法可靠跨重启持久化

修复前，LaunchAgent 直接执行 `zcode-keysmith-env.sh`，但脚本通过普通原子写入创建，
没有可执行权限。当前会话的 `launchctl setenv` 可以成功，注销或重启后则可能出现
`Permission denied`。

修复后：

- env 脚本每次安装或备份还原后都设置为 `0755`。
- `launchctl`、`mdfind`、`pgrep` 使用绝对系统路径，避免 GUI 进程 PATH 差异。
- LaunchAgent plist 中的动态路径进行 XML 转义。
- 增加 macOS-only 权限、plist 和 ZCode 安装目录测试。

### M4. Windows rollout 文件占用检测缺失

修复前，`rollout_file_is_open()` 在 Windows 永远返回 `false`。Codex 正在写会话时，
同步可能以共享冲突错误中断，而不是进入“跳过文件”的正常流程。

修复后：

- Windows 使用 `OpenOptionsExt::share_mode(0)` 尝试独占打开。
- 读取或原子替换阶段遇到 `ERROR_SHARING_VIOLATION` / `ERROR_LOCK_VIOLATION` 时按跳过处理。
- 原子重命名失败时清理临时文件，避免残留 `*.tmp.*`。
- Windows 独占句柄与临时文件清理测试已通过。

### M5. ZCode app.asar 检测两端行为不一致

macOS 与 Windows 现在共同使用：

- `file_contains_needle()`：1 MB 分块流式扫描，保留块边界重叠。
- `dir_contains_needle()`：扫描被解包成目录的 app.asar，不跟随符号链接。

已增加跨分块边界和 `out/host/index.js` 目录布局测试。

### M6. macOS ZCode 安装路径缺少校验

`ZCODE_APP_PATH`、Spotlight 结果和默认 `/Applications/ZCode.app` 现在都必须同时包含：

- `Contents/MacOS/ZCode`
- `Contents/Resources/glm/zcode.cjs`

无效目录不会再延迟到注入阶段才报错。

### M7. macOS ZCode 进程名精确匹配过窄

保留 `pgrep -x ZCode` 快速路径；如果进程名发生变化，则发现已校验的 ZCode.app，
再从 `ps` 命令行中匹配其主可执行文件完整路径。Helper/Renderer 进程不会被误判为主进程。

### B8. 概览页仍可能显示 Codex 供应商

修复前虽然模型回退已经限制到 Codex，但 `currentProvider` 仍来自 Codex 状态并传给全部工具。
ZCode、Grok 或 Claude 没有解析出供应商时，仍可能显示 Codex 的供应商。

修复后：

- `main.tsx` 只在 Codex Tab 传入 Codex 模型、供应商和目录回退值。
- `OverviewPage` 自身再次按 `tool === "codex"` 限制回退。
- 其他工具没有供应商时显示“未配置供应商”，不会显示 Codex 数据。

### H9. ZCode 原生供应商读取与切换

ZCode 供应商不再写入 Everything Patch 自身数据库，而是直接读取 ZCode 3.5.x 的原生配置：

- `~/.zcode/v2/config.json`：供应商注册表、认证选项和模型表。
- `~/.zcode/v2/setting.json`：Z.ai / BigModel 的 API Key、Coding Plan、Team Plan 通道。
- `~/.zcode/cli/config.json`：CLI 默认 `provider/model`，并保留原有 MCP、Hook、Plugin 与未知字段。

Windows x64、macOS ARM 和 macOS Intel 共用同一套 Rust JSON 读写代码；只有“ZCode 是否正在运行”
继续使用现有平台分支。切换前会备份三份文件，备份目录名会清理 Windows 不允许的 `:` 等字符；
写入采用原子替换，任一文件失败会恢复全部原文件。ZCode 正在运行时会拒绝切换，避免退出时被其内存状态覆盖。

原生条目只允许刷新与切换，增删改仍在 ZCode 中完成，API Key 不进入 Everything Patch 数据库或前端 IPC。
fixture 已覆盖对象式模型表、内置通道、自定义供应商、Windows 安全文件名和部分写入失败回滚。

---

## 保留的平台差异

### 环境变量持久化

| | macOS | Windows |
|---|---|---|
| 当前会话 | `launchctl setenv` | 写入用户环境变量 |
| 重启持久化 | `~/Library/LaunchAgents/*.plist` | 用户注册表 |
| 生效要求 | 完全退出并重启 ZCode | 完全退出并重启 ZCode |

两端机制不同，但当前语义一致。

### 皮肤 Node.js 来源

- macOS 使用官方 Codex.app 内置的 `cua_node`，校验签名 Team ID、Node 主版本和机器架构。
- Windows 使用 Everything Patch 安装包内置的固定 Node.js 22.23.1，并保留环境变量/PATH 作为开发回退。

这项差异是有意设计，不要求两端使用同一分发方式。

### 前端标题栏

- macOS 渲染自定义拖动区域并预留 traffic lights。
- Windows 使用原生标题栏，不渲染 macOS 拖动层。

当前实现不需要调整。

---

## 验证结果

2026-07-28 在 Windows x64 本机完成：

- `cargo test --lib --locked`：169 passed
- `npm run typecheck`：通过
- `npm --prefix apps/desktop run test:skin-runtime`：4 passed
- 固定 Node 下载、SHA-256 与版本验证：通过，`v22.23.1`
- `npm run build`：通过
- MSI/NSIS 资源清单：均确认包含内置 Node.js

正式 `release.yml` 会在 Windows x64、macOS Apple Silicon、macOS Intel 和 Linux x64
各自的原生 runner 上先执行 Rust 测试，再进入对应平台的签名或打包步骤。
手动 `build.yml` 不执行同等发布验证，其中 macOS Intel 目标在 Apple Silicon runner
上交叉编译，只能作为编译与产物检查。

---

## 剩余风险

1. 当前开发机是 Windows，macOS-only 权限和进程测试需要由 GitHub Actions 原生 macOS runner 执行。
2. ZCode 更新可能改变 bundle id、app.asar 布局或 runtime patch 锚点；当前实现会安全拒绝未知布局，
   但仍需在 ZCode 发布新版本后做兼容回归。
3. 内置 Node 是固定版本，需要在 Node 22 生命周期结束前更新版本与 SHA-256。
4. 手动 `build.yml` 的 macOS 产物仅用于验证，其中 Intel 目标为 ARM runner 交叉编译；
   只有 `release.yml` 使用 Intel 原生 runner，并对 macOS 产物执行正式签名与公证。
5. 产品从 Codex-X 更换了 bundle identifier 和更新地址。已有 Codex-X 安装是否需要无缝迁移，
   仍属于发布策略决策；如需原地升级，需要旧更新通道提供桥接版本。

## 最终状态

| 项目 | 状态 |
|---|---|
| 单一正式发布入口 | 已修复 |
| Windows 内置 Node | 已修复并完成安装器核验 |
| Windows rollout 文件锁 | 已修复并测试 |
| macOS ZCode 安装校验 | 已修复，等待原生 CI 执行测试 |
| macOS LaunchAgent 权限 | 已修复，等待原生 CI 执行测试 |
| macOS ZCode 进程检测 | 已增强，等待原生 CI 执行测试 |
| app.asar 文件/目录检测 | 已修复并测试 |
| 概览跨工具供应商串值 | 已修复，TypeScript 检查通过 |
| ZCode 原生供应商读取/切换 | 已修复并测试；macOS 原生进程检测等待 CI 执行 |
