pub(crate) const CODEX_KEYSMITH_BUILTIN_ID: &str = "codex-keysmith";
pub(crate) const CODEX_KEYSMITH_BUILTIN_FILENAME: &str = "codex-keysmith.md";
pub(crate) const CODEX_KEYSMITH_BUILTIN_CONTENT: &str =
    include_str!("../../../../examples/codex-keysmith.md");

pub(crate) const INSTRUCTION_FILENAME: &str = "gpt5.5-unrestricted.md";
pub(crate) const INSTRUCTION_CONTENT: &str =
    include_str!("../../../../examples/gpt5.5-unrestricted.md");

pub(crate) const INSTRUCTION_54_FILENAME: &str = "gpt5.4-unrestricted.md";
pub(crate) const INSTRUCTION_54_CONTENT: &str =
    include_str!("../../../../examples/gpt5.4-unrestricted.md");

pub(crate) const INSTRUCTION_JELI_FILENAME: &str = "gpt5.5-jeli.md";
pub(crate) const INSTRUCTION_JELI_CONTENT: &str =
    include_str!("../../../../examples/gpt5.5-jeli.md");

pub(crate) const INSTRUCTION_56_SOL_FILENAME: &str = "gpt-5.6-sol-unrestricted.md";
pub(crate) const INSTRUCTION_56_SOL_CONTENT: &str =
    include_str!("../../../../examples/gpt-5.6-sol-unrestricted.md");

pub(crate) const INSTRUCTION_SEAGULL_FILENAME: &str = "海鸥3.0破甲.md";
pub(crate) const INSTRUCTION_SEAGULL_CONTENT: &str =
    include_str!("../../../../examples/海鸥3.0破甲.md");

pub(crate) const AGENTS_FILENAME: &str = "AGENTS.md";
pub(crate) const AGENTS_MANAGED_BEGIN: &str = "<!-- EVERYTHING-PATCH:INSTRUCTIONS:BEGIN -->";
pub(crate) const AGENTS_MANAGED_END: &str = "<!-- EVERYTHING-PATCH:INSTRUCTIONS:END -->";
pub(crate) const AGENTS_TEMPLATE_PREFIX: &str = "<!-- EVERYTHING-PATCH:TEMPLATE:";
pub(crate) const LEGACY_AGENTS_MANAGED_BEGIN: &str = "<!-- CODEX-X:INSTRUCTIONS:BEGIN -->";
pub(crate) const LEGACY_AGENTS_MANAGED_END: &str = "<!-- CODEX-X:INSTRUCTIONS:END -->";
pub(crate) const LEGACY_AGENTS_TEMPLATE_PREFIX: &str = "<!-- CODEX-X:TEMPLATE:";
pub(crate) const JSDELIVR_EXAMPLES_API: &str =
    "https://data.jsdelivr.com/v1/packages/gh/Xdjjw/everything-patch@main?structure=flat";
pub(crate) const JSDELIVR_EXAMPLES_BASE: &str =
    "https://cdn.jsdelivr.net/gh/Xdjjw/everything-patch@main/examples/";
pub(crate) const GITHUB_EXAMPLES_API: &str =
    "https://api.github.com/repos/Xdjjw/everything-patch/contents/examples?ref=main";
pub(crate) const GITHUB_EXAMPLES_BASE: &str =
    "https://raw.githubusercontent.com/Xdjjw/everything-patch/main/examples/";

pub(crate) const MAX_SKILL_ZIP_BYTES: u64 = 20 * 1024 * 1024;

// ─── Claude Code 指令管理常量 ─────────────────────────────────────────────
// Claude 的 user scope 固定为 ~/.claude，指令文件存放在 ~/.claude/keysmith/。
pub(crate) const CLAUDE_HOME_DIRNAME: &str = ".claude";
pub(crate) const CLAUDE_MEMORY_FILENAME: &str = "CLAUDE.md";
pub(crate) const CLAUDE_KEYSMITH_DIRNAME: &str = "keysmith";
// Claude CLI runtime injection stays separate from the historical keysmith
// instruction directory so uninstalling either layer cannot remove the other.
pub(crate) const CLAUDE_RUNTIME_DIRNAME: &str = "devconduit";
pub(crate) const CLAUDE_RUNTIME_PROMPT_FILENAME: &str = "runtime-prompt.md";

// CLAUDE.md 受管 import-block 标记，与 AGENTS.md 的 BEGIN/END 模式对齐。
pub(crate) const CLAUDE_MANAGED_BEGIN: &str = "<!-- EVERYTHING-PATCH:CLAUDE:BEGIN -->";
pub(crate) const CLAUDE_MANAGED_END: &str = "<!-- EVERYTHING-PATCH:CLAUDE:END -->";
pub(crate) const CLAUDE_TEMPLATE_PREFIX: &str = "<!-- EVERYTHING-PATCH:CLAUDE:TEMPLATE:";
pub(crate) const CLAUDE_MODE_PREFIX: &str = "<!-- EVERYTHING-PATCH:CLAUDE:MODE:";
pub(crate) const LEGACY_CLAUDE_MANAGED_BEGIN: &str = "<!-- CODEX-X:CLAUDE:BEGIN -->";
pub(crate) const LEGACY_CLAUDE_MANAGED_END: &str = "<!-- CODEX-X:CLAUDE:END -->";
pub(crate) const LEGACY_CLAUDE_TEMPLATE_PREFIX: &str = "<!-- CODEX-X:CLAUDE:TEMPLATE:";
pub(crate) const CLAUDE_RUNTIME_BEGIN: &str = "# >>> DevConduit Claude runtime >>>";
pub(crate) const CLAUDE_RUNTIME_END: &str = "# <<< DevConduit Claude runtime <<<";

// keysmith 默认模板，编译进二进制。
pub(crate) const CLAUDE_BUILTIN_ID: &str = "claude-project-rules";
pub(crate) const CLAUDE_BUILTIN_TITLE: &str = "Claude 项目规则";
pub(crate) const CLAUDE_BUILTIN_SUBTITLE: &str = "同步 claude-keysmith v5 项目规则，离线内置";
pub(crate) const CLAUDE_BUILTIN_BADGE: &str = "默认";
pub(crate) const CLAUDE_BUILTIN_FILENAME: &str = "claude-project-rules.md";
pub(crate) const CLAUDE_BUILTIN_CONTENT: &str =
    include_str!("../../../../examples/claude-project-rules.md");

// ─── ZCode App 指令管理常量 ───────────────────────────────────────────────
// ZCode 的受管目录为 ~/.zcode-keysmith，system-role.md 作为 runtime system prompt。
pub(crate) const ZCODE_KEYSMITH_DIRNAME: &str = ".zcode-keysmith";
pub(crate) const ZCODE_SYSTEM_ROLE_FILENAME: &str = "system-role.md";
pub(crate) const ZCODE_CONFIG_FILENAME: &str = "config.json";
pub(crate) const ZCODE_LAUNCHER_NAME: &str = "launcher.js";
pub(crate) const ZCODE_ENV_SCRIPT_NAME: &str = "zcode-keysmith-env.sh";
pub(crate) const ZCODE_LAUNCH_AGENT_LABEL: &str = "com.jia.zcode-keysmith.env";
pub(crate) const ZCODE_LOG_DIRNAME: &str = "logs";
pub(crate) const ZCODE_PATCH_SIDECAR_NAME: &str = "patch.js";
pub(crate) const ZCODE_CACHE_DIRNAME: &str = "cache";
pub(crate) const ZCODE_LAUNCHER_LOG_NAME: &str = "launcher-start.jsonl";

// runtime patch 锚点与 agent-server 参数（跨平台一致）。
pub(crate) const ZCODE_RUNTIME_RELPATH: &str = "resources/glm/zcode.cjs";
pub(crate) const ZCODE_APP_ASAR_RELPATH: &str = "resources/app.asar";
pub(crate) const ZCODE_AGENT_OVERRIDE_NEEDLE: &str = "ZCODE_AGENT_SERVER_COMMAND";
pub(crate) const ZCODE_PATCH_NEEDLE: &str = "customSystemPrompt:this.config.systemPrompt,language:";
pub(crate) const ZCODE_AGENT_ARGS_JSON: &str = "[\"app-server\",\"--stdio\"]";

// keysmith 默认模板，编译进二进制。
pub(crate) const ZCODE_BUILTIN_ID: &str = "zcode-system-role";
pub(crate) const ZCODE_BUILTIN_TITLE: &str = "ZCode 系统 Prompt";
pub(crate) const ZCODE_BUILTIN_SUBTITLE: &str = "同步 zcode-keysmith system-role，离线内置";
pub(crate) const ZCODE_BUILTIN_BADGE: &str = "默认";
pub(crate) const ZCODE_BUILTIN_FILENAME: &str = "zcode-system-role.md";
pub(crate) const ZCODE_BUILTIN_CONTENT: &str =
    include_str!("../../../../examples/zcode-system-role.md");

// ─── Grok Build 指令管理常量 ─────────────────────────────────────────────
// Grok 的受管目录为 ~/.grok，AGENTS.md 作为全局 project rules。
pub(crate) const GROK_HOME_DIRNAME: &str = ".grok";
pub(crate) const GROK_AGENTS_FILENAME: &str = "AGENTS.md";
pub(crate) const GROK_CONFIG_FILENAME: &str = "config.toml";
pub(crate) const GROK_HOOKS_DIRNAME: &str = "hooks";
pub(crate) const GROK_MANIFEST_FILENAME: &str = ".grok-keysmith-manifest.json";
pub(crate) const GROK_COMPAT_BEGIN_MARKER: &str = "# === grok-keysmith compat isolation begin ===";
pub(crate) const GROK_COMPAT_END_MARKER: &str = "# === grok-keysmith compat isolation end ===";
pub(crate) const GROK_PROMPT_BEGIN: &str = "<!-- EVERYTHING-PATCH:GROK:BEGIN -->";
pub(crate) const GROK_PROMPT_END: &str = "<!-- EVERYTHING-PATCH:GROK:END -->";
pub(crate) const GROK_PROMPT_TEMPLATE_PREFIX: &str = "<!-- EVERYTHING-PATCH:GROK:TEMPLATE:";
pub(crate) const GROK_PROMPT_MODE_PREFIX: &str = "<!-- EVERYTHING-PATCH:GROK:MODE:";

// compat 隔离块内容（关闭 Claude/Cursor/Codex 兼容层）。
pub(crate) const GROK_COMPAT_BLOCK: &str = "[compat.claude]\nskills = false\nrules = false\nagents = false\nmcps = false\nhooks = false\nsessions = false\n\n[compat.cursor]\nskills = false\nrules = false\nagents = false\nmcps = false\nhooks = false\nsessions = false\n\n[compat.codex]\nsessions = false";

// keysmith 默认模板，编译进二进制。
pub(crate) const GROK_BUILTIN_ID: &str = "grok-unrestricted";
pub(crate) const GROK_BUILTIN_TITLE: &str = "Grok 无限制模式";
pub(crate) const GROK_BUILTIN_SUBTITLE: &str = "keysmith 默认 Grok 指令，全局 AGENTS.md";
pub(crate) const GROK_BUILTIN_BADGE: &str = "默认";
pub(crate) const GROK_BUILTIN_FILENAME: &str = "grok-unrestricted.md";
pub(crate) const GROK_BUILTIN_CONTENT: &str =
    include_str!("../../../../examples/grok-unrestricted.md");
