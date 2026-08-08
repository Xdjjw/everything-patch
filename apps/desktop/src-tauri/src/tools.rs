use crate::error::{CodexxError, Result};
use crate::file_io::read_to_string_if_exists;
use crate::paths::home_dir;
use crate::platform;
use crate::resolve_codex_dir;
use crate::state::{
    build_claude_state, build_grok_state, build_kilo_state, build_pi_state, build_state,
    build_zcode_state,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Value as TomlValue};

pub(crate) const REDACTED_VALUE: &str = "[REDACTED]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToolId {
    Codex,
    #[serde(alias = "claude-code", alias = "claudecode")]
    Claude,
    #[serde(alias = "grok-build", alias = "grokbuild")]
    Grok,
    #[serde(alias = "z-code")]
    Zcode,
    #[serde(alias = "kilo-code", alias = "kilocode")]
    Kilo,
    #[serde(alias = "pi-agent", alias = "piagent")]
    Pi,
}

impl ToolId {
    pub(crate) const ALL: [ToolId; 6] = [
        ToolId::Codex,
        ToolId::Claude,
        ToolId::Grok,
        ToolId::Zcode,
        ToolId::Kilo,
        ToolId::Pi,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ToolId::Codex => "codex",
            ToolId::Claude => "claude",
            ToolId::Grok => "grok",
            ToolId::Zcode => "zcode",
            ToolId::Kilo => "kilo",
            ToolId::Pi => "pi",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ToolId::Codex => "Codex",
            ToolId::Claude => "Claude Code",
            ToolId::Grok => "Grok Build",
            ToolId::Zcode => "ZCode",
            ToolId::Kilo => "Kilo Code",
            ToolId::Pi => "Pi",
        }
    }

    pub(crate) fn ccswitch_app_type(self) -> &'static str {
        match self {
            ToolId::Codex => "codex",
            ToolId::Claude => "claude",
            ToolId::Grok => "grokbuild",
            ToolId::Zcode => "zcode",
            ToolId::Kilo => "kilo",
            ToolId::Pi => "pi",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "codex" => Ok(ToolId::Codex),
            "claude" | "claude-code" | "claudecode" => Ok(ToolId::Claude),
            "grok" | "grok-build" | "grokbuild" => Ok(ToolId::Grok),
            "zcode" | "z-code" => Ok(ToolId::Zcode),
            "kilo" | "kilo-code" | "kilocode" => Ok(ToolId::Kilo),
            "pi" | "pi-agent" | "piagent" => Ok(ToolId::Pi),
            other => Err(CodexxError::Config(format!("不支持的工具: {other}"))),
        }
    }

    pub(crate) fn home_dir(self, codex_override: Option<String>) -> Result<PathBuf> {
        match self {
            ToolId::Codex => resolve_codex_dir(codex_override),
            ToolId::Claude => Ok(home_dir()?.join(".claude")),
            ToolId::Grok => Ok(home_dir()?.join(".grok")),
            ToolId::Zcode => Ok(home_dir()?.join(".zcode")),
            ToolId::Kilo => Ok(home_dir()?.join(".config").join("kilo")),
            ToolId::Pi => Ok(home_dir()?.join(".pi").join("agent")),
        }
    }

    pub(crate) fn config_path(self, codex_override: Option<String>) -> Result<PathBuf> {
        let root = self.home_dir(codex_override)?;
        Ok(match self {
            ToolId::Codex | ToolId::Grok => root.join("config.toml"),
            ToolId::Claude => root.join("settings.json"),
            ToolId::Zcode => root.join("cli").join("config.json"),
            ToolId::Kilo => root.join("kilo.jsonc"),
            ToolId::Pi => root.join("settings.json"),
        })
    }

    pub(crate) fn skills_dir(self, codex_override: Option<String>) -> Result<PathBuf> {
        match self {
            ToolId::Kilo => return Ok(home_dir()?.join(".kilo").join("skills")),
            ToolId::Pi => return Ok(home_dir()?.join(".pi").join("agent").join("skills")),
            _ => {}
        }
        Ok(self.home_dir(codex_override)?.join("skills"))
    }
}

impl std::fmt::Display for ToolId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCapabilities {
    providers: bool,
    sessions: bool,
    session_sync: bool,
    session_delete: bool,
    skills: bool,
    mcp: bool,
    config: bool,
    prompts: bool,
}

impl ToolCapabilities {
    fn for_tool(tool: ToolId) -> Self {
        Self {
            providers: tool != ToolId::Kilo,
            sessions: tool != ToolId::Kilo,
            session_sync: tool == ToolId::Codex,
            session_delete: tool == ToolId::Codex,
            skills: true,
            mcp: true,
            config: true,
            prompts: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolStatus {
    id: ToolId,
    label: String,
    installed: bool,
    version: Option<String>,
    home_dir: String,
    config_path: String,
    config_format: String,
    config_exists: bool,
    auth_path: Option<String>,
    auth_exists: bool,
    instruction_path: String,
    native_instruction_path: String,
    diagnostic_path: Option<String>,
    instruction_exists: bool,
    instruction_enabled: bool,
    model: Option<String>,
    provider: Option<String>,
    provider_id: Option<String>,
    notice: Option<String>,
    capabilities: ToolCapabilities,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolConfigFile {
    id: String,
    label: String,
    path: String,
    format: String,
    exists: bool,
    native: bool,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolConfigBundle {
    tool: ToolId,
    label: String,
    primary_file_id: String,
    files: Vec<ToolConfigFile>,
    notice: Option<String>,
}

pub(crate) fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "apikey",
        "token",
        "secret",
        "password",
        "authorization",
        "cookie",
        "credential",
        "accesstoken",
        "refreshtoken",
        "idtoken",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

pub(crate) fn redact_json_value(value: &mut JsonValue) {
    match value {
        JsonValue::Object(object) => {
            for (key, nested) in object {
                if is_sensitive_key(key) {
                    if !nested.is_null()
                        && !nested.as_str().is_some_and(|value| value.trim().is_empty())
                    {
                        *nested = JsonValue::String(REDACTED_VALUE.to_string());
                    }
                } else {
                    redact_json_value(nested);
                }
            }
        }
        JsonValue::Array(values) => {
            for nested in values {
                redact_json_value(nested);
            }
        }
        _ => {}
    }
}

pub(crate) fn redacted_json_text(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let Ok(mut value) = serde_json::from_str::<JsonValue>(text) else {
        return "{\n  \"preview\": \"Hidden because this JSON is invalid\"\n}".to_string();
    };
    redact_json_value(&mut value);
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

fn redact_toml_value(value: &mut TomlValue) {
    if let Some(table) = value.as_inline_table_mut() {
        for (key, nested) in table.iter_mut() {
            if is_sensitive_key(&key) {
                *nested = TomlValue::from(REDACTED_VALUE);
            } else {
                redact_toml_value(nested);
            }
        }
        return;
    }
    if let Some(array) = value.as_array_mut() {
        for nested in array.iter_mut() {
            redact_toml_value(nested);
        }
    }
}

fn redact_toml_item(item: &mut Item) {
    if let Some(table) = item.as_table_mut() {
        for (key, nested) in table.iter_mut() {
            if is_sensitive_key(&key) {
                *nested = toml_edit::value(REDACTED_VALUE);
            } else {
                redact_toml_item(nested);
            }
        }
        return;
    }
    if let Some(tables) = item.as_array_of_tables_mut() {
        for table in tables.iter_mut() {
            for (key, nested) in table.iter_mut() {
                if is_sensitive_key(&key) {
                    *nested = toml_edit::value(REDACTED_VALUE);
                } else {
                    redact_toml_item(nested);
                }
            }
        }
        return;
    }
    if let Some(value) = item.as_value_mut() {
        redact_toml_value(value);
    }
}

pub(crate) fn redacted_toml_text(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let Ok(mut document) = text.parse::<DocumentMut>() else {
        return "# Preview hidden because this TOML is invalid.\n".to_string();
    };
    for (key, item) in document.as_table_mut().iter_mut() {
        if is_sensitive_key(&key) {
            *item = toml_edit::value(REDACTED_VALUE);
        } else {
            redact_toml_item(item);
        }
    }
    document.to_string()
}

pub(crate) fn redacted_config_text(path: &Path, text: &str) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => redacted_json_text(text),
        Some("jsonc") => redacted_jsonc_text(text),
        Some("toml") => redacted_toml_text(text),
        Some("md") | Some("markdown") | Some("txt") => truncate_preview(text),
        Some("yaml") | Some("yml") | Some("ini") | Some("env") | Some("conf") => {
            truncate_preview(&redacted_line_text(text))
        }
        _ => String::new(),
    }
}

fn parse_jsonc_value(text: &str) -> Option<JsonValue> {
    if text.trim().is_empty() {
        return Some(JsonValue::Object(Default::default()));
    }
    jsonc_parser::cst::CstRootNode::parse(text, &Default::default())
        .ok()?
        .to_serde_value()
}

fn redacted_jsonc_text(text: &str) -> String {
    let Some(mut value) = parse_jsonc_value(text) else {
        return "// Preview hidden because this JSONC is invalid.\n".to_string();
    };
    redact_json_value(&mut value);
    serde_json::to_string_pretty(&value)
        .map(|value| format!("{value}\n"))
        .unwrap_or_default()
}

/// 预览文本上限，避免把超大文件整个塞进 IPC 负载。
const CONFIG_PREVIEW_LIMIT: usize = 200 * 1024;

fn truncate_preview(text: &str) -> String {
    if text.len() <= CONFIG_PREVIEW_LIMIT {
        return text.to_string();
    }
    let mut end = CONFIG_PREVIEW_LIMIT;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n… [预览已截断]\n", &text[..end])
}

/// 逐行脱敏：命中敏感 key 的 `key: value` / `key=value` 行值替换为 [REDACTED]。
fn redacted_line_text(text: &str) -> String {
    text.lines()
        .map(|line| {
            let Some(separator) = line.find([':', '=']) else {
                return line.to_string();
            };
            let key = line[..separator].trim().trim_matches(['"', '\'', '-', ' ']);
            if key.is_empty() || !is_sensitive_key(key) {
                return line.to_string();
            }
            format!(
                "{}{} {REDACTED_VALUE}",
                &line[..separator],
                &line[separator..separator + 1]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn json_string_at(value: &JsonValue, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        value
            .pointer(pointer)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn toml_root_string(document: &DocumentMut, key: &str) -> Option<String> {
    document
        .get(key)
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn codex_model_provider(path: &Path) -> (Option<String>, Option<String>) {
    let text = read_to_string_if_exists(path).unwrap_or_default();
    let Ok(document) = text.parse::<DocumentMut>() else {
        return (None, None);
    };
    (
        toml_root_string(&document, "model"),
        toml_root_string(&document, "model_provider"),
    )
}

fn claude_model_provider(path: &Path) -> (Option<String>, Option<String>) {
    let text = read_to_string_if_exists(path).unwrap_or_default();
    let Ok(value) = serde_json::from_str::<JsonValue>(&text) else {
        return (None, None);
    };
    let model = json_string_at(
        &value,
        &[
            "/env/ANTHROPIC_MODEL",
            "/model",
            "/env/ANTHROPIC_DEFAULT_SONNET_MODEL",
        ],
    );
    let provider = json_string_at(&value, &["/env/ANTHROPIC_BASE_URL"])
        .or_else(|| Some("Anthropic Official".to_string()));
    (model, provider)
}

fn grok_model_provider(path: &Path) -> (Option<String>, Option<String>) {
    let text = read_to_string_if_exists(path).unwrap_or_default();
    let Ok(document) = text.parse::<DocumentMut>() else {
        return (None, None);
    };
    let model = document
        .get("models")
        .and_then(Item::as_table)
        .and_then(|table| table.get("default"))
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let provider = model.as_deref().and_then(|model_id| {
        document
            .get("model")
            .and_then(Item::as_table)
            .and_then(|models| models.get(model_id))
            .and_then(Item::as_table)
            .and_then(|model_table| {
                model_table
                    .get("name")
                    .and_then(Item::as_str)
                    .or_else(|| model_table.get("base_url").and_then(Item::as_str))
            })
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    });
    (model, provider)
}

fn kilo_model_provider(path: &Path) -> (Option<String>, Option<String>) {
    let text = read_to_string_if_exists(path).unwrap_or_default();
    let Some(value) = parse_jsonc_value(&text) else {
        return (None, None);
    };
    let model = json_string_at(&value, &["/model", "/small_model"]);
    let provider = model.as_deref().and_then(|model| {
        model
            .split_once('/')
            .map(|(provider, _)| provider.to_string())
    });
    (model, provider)
}

fn pi_model_provider(path: &Path) -> (Option<String>, Option<String>) {
    let text = read_to_string_if_exists(path).unwrap_or_default();
    let Ok(value) = serde_json::from_str::<JsonValue>(&text) else {
        return (None, None);
    };
    let model = value
        .get("defaultModel")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let provider = value
        .get("defaultProvider")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    (model, provider)
}

fn command_version(candidates: &[PathBuf]) -> Option<String> {
    let mut seen = HashSet::new();
    for candidate in candidates {
        let key = if cfg!(target_os = "windows") {
            candidate.to_string_lossy().to_ascii_lowercase()
        } else {
            candidate.to_string_lossy().to_string()
        };
        if !seen.insert(key) {
            continue;
        }
        if candidate.components().count() > 1 && !candidate.is_file() {
            continue;
        }
        let Ok(output) = platform::program_command(candidate, &["--version"]).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|line| {
                !line.is_empty() && line.chars().any(|character| character.is_ascii_digit())
            });
        if let Some(line) = line {
            return Some(line.to_string());
        }
    }
    None
}

fn generic_command_candidates(names: &[&str]) -> Vec<PathBuf> {
    let mut candidates = names.iter().map(PathBuf::from).collect::<Vec<_>>();
    let home = dirs::home_dir().unwrap_or_default();
    for name in names {
        candidates.push(home.join(".local").join("bin").join(name));
        candidates.push(home.join(".npm-global").join("bin").join(name));
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(name));
        candidates.push(PathBuf::from("/usr/local/bin").join(name));
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            for name in names {
                candidates.push(PathBuf::from(&appdata).join("npm").join(name));
            }
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            for name in names {
                candidates.push(
                    PathBuf::from(&localappdata)
                        .join("Microsoft")
                        .join("WindowsApps")
                        .join(name),
                );
            }
        }
    }
    candidates
}

fn tool_version(tool: ToolId) -> Option<String> {
    match tool {
        ToolId::Codex => platform::detect_codex_version(),
        ToolId::Claude => command_version(&generic_command_candidates(&[
            "claude",
            "claude.exe",
            "claude.cmd",
        ])),
        ToolId::Grok => command_version(&generic_command_candidates(&[
            "grok",
            "grok.exe",
            "grok.cmd",
            "grok-build",
            "grok-build.exe",
        ])),
        ToolId::Zcode => crate::zcode::detect_zcode_version().or_else(|| {
            crate::zcode::discover_zcode_app()
                .ok()
                .map(|_| "installed".to_string())
        }),
        ToolId::Kilo => command_version(&generic_command_candidates(&[
            "kilo", "kilo.exe", "kilo.cmd",
        ])),
        ToolId::Pi => command_version(&generic_command_candidates(&["pi", "pi.exe", "pi.cmd"])),
    }
}

fn config_file(
    id: &str,
    label: &str,
    path: PathBuf,
    format: &str,
    native: bool,
) -> Result<ToolConfigFile> {
    let exists = path.is_file();
    let text = read_to_string_if_exists(&path)?;
    let text = if format.eq_ignore_ascii_case("jsonc") {
        redacted_jsonc_text(&text)
    } else {
        redacted_config_text(&path, &text)
    };
    Ok(ToolConfigFile {
        id: id.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        format: format.to_string(),
        exists,
        native,
        text,
    })
}

/// 扫描时跳过的目录名（缓存 / 会话 / 二进制产物），小写比较。
const CONFIG_SCAN_SKIP_DIRS: &[&str] = &[
    ".git",
    "agents",
    "artifacts",
    "backups",
    "blob_storage",
    "cache",
    "cachestorage",
    "caches",
    "code cache",
    "crashpad",
    "dawncache",
    "downloads",
    "gpucache",
    "history",
    "images",
    "indexeddb",
    "local storage",
    "logs",
    "node_modules",
    "projects",
    "rollout",
    "rollouts",
    "screenshots",
    "service worker",
    "session storage",
    "sessions",
    "shell-snapshots",
    "statsig",
    "temp",
    "tmp",
    "todos",
    "transcripts",
];

/// 会被收录进配置列表的扩展名。
const CONFIG_SCAN_EXTENSIONS: &[&str] =
    &["json", "toml", "yaml", "yml", "md", "ini", "conf", "env"];

/// 结构化存储：列出但不做文本预览（`read_to_string` 会在二进制上直接失败）。
const CONFIG_SCAN_BINARY_EXTENSIONS: &[&str] = &["sqlite", "sqlite3", "db"];

/// SQLite 文件头魔数，用于确认扩展名之外的真实类型。
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

fn looks_like_sqlite(path: &Path) -> bool {
    use std::io::Read;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 16];
    file.read_exact(&mut header).is_ok() && header.as_slice() == SQLITE_MAGIC
}

/// 为二进制存储构造一个"只列出、不预览"的条目。
fn binary_config_file(id: &str, label: &str, path: PathBuf, format: &str) -> ToolConfigFile {
    let exists = path.is_file();
    let size = path.metadata().map(|meta| meta.len()).unwrap_or(0);
    let text = if exists {
        format!(
            "# {label}\n# 路径: {}\n# 类型: {format}（二进制存储，不做文本预览）\n# 大小: {size} 字节\n",
            path.display()
        )
    } else {
        String::new()
    };
    ToolConfigFile {
        id: id.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        format: format.to_string(),
        exists,
        native: true,
        text,
    }
}

/// 单个文件超过该体积就不再读取预览内容。
const CONFIG_SCAN_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// 一次最多返回多少个配置文件，避免 IPC 负载失控。
const CONFIG_SCAN_MAX_FILES: usize = 40;

#[derive(Debug, Clone)]
struct ScannedConfig {
    id: String,
    label: String,
    path: PathBuf,
    native: bool,
    binary: bool,
    rank: u8,
}

fn scan_id(label: &str) -> String {
    let id = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = id.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "config".to_string()
    } else {
        trimmed
    }
}

/// 排序权重：主配置 < 供应商/模型/凭据 < 其它结构化配置 < 文档。
fn scan_rank(relative: &str) -> u8 {
    let lower = relative.to_ascii_lowercase();
    let name = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    if matches!(
        name,
        "config.json" | "config.toml" | "settings.json" | "settings.toml"
    ) {
        return 0;
    }
    if [
        "provider",
        "model",
        "auth",
        "account",
        "credential",
        "channel",
    ]
    .iter()
    .any(|needle| name.contains(needle))
    {
        return 1;
    }
    if name.ends_with(".json") || name.ends_with(".toml") {
        return 2;
    }
    3
}

/// 判断一个文件是否要收录：`Some(false)` 文本配置，`Some(true)` 二进制存储。
fn scan_kind(path: &Path) -> Option<bool> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)?;
    if CONFIG_SCAN_EXTENSIONS.contains(&extension.as_str()) {
        return Some(false);
    }
    if CONFIG_SCAN_BINARY_EXTENSIONS.contains(&extension.as_str()) && looks_like_sqlite(path) {
        return Some(true);
    }
    None
}

/// 递归收集一个目录下的配置文件。`prefix` 会作为标签前缀区分来源。
fn scan_config_dir(
    root: &Path,
    current: &Path,
    prefix: &str,
    native: bool,
    depth_left: usize,
    out: &mut Vec<ScannedConfig>,
) {
    if out.len() >= CONFIG_SCAN_MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    let mut children = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // 不跟随符号链接，避免走出目标目录。
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if depth_left > 0 && !CONFIG_SCAN_SKIP_DIRS.contains(&name.as_str()) {
                children.push(path);
            }
            continue;
        }
        let Some(binary) = scan_kind(&path) else {
            continue;
        };
        // 二进制存储不做文本预览，因此不受预览体积上限约束。
        if !binary
            && entry
                .metadata()
                .is_ok_and(|metadata| metadata.len() > CONFIG_SCAN_MAX_FILE_BYTES)
        {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let label = if prefix.is_empty() {
            relative.clone()
        } else {
            format!("{prefix}/{relative}")
        };
        out.push(ScannedConfig {
            id: scan_id(&label),
            // 二进制存储通常就是供应商/账号库，排在文本配置之后、杂项之前。
            rank: if binary { 1 } else { scan_rank(&relative) },
            label,
            path,
            native,
            binary,
        });
        if out.len() >= CONFIG_SCAN_MAX_FILES {
            return;
        }
    }
    children.sort();
    for child in children {
        scan_config_dir(root, &child, prefix, native, depth_left - 1, out);
        if out.len() >= CONFIG_SCAN_MAX_FILES {
            return;
        }
    }
}

/// ZCode 桌面端（Electron）的应用数据目录，供应商 / 模型通道通常存在这里。
fn zcode_app_data_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Ok(app_data) = std::env::var("APPDATA") {
            dirs.push(PathBuf::from(&app_data).join("ZCode"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(&local).join("ZCode"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(home.join("Library/Application Support/ZCode"));
        dirs.push(home.join("Library/Preferences"));
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        dirs.push(home.join(".config").join("ZCode"));
    }
    let _ = home;
    dirs.retain(|dir| dir.is_dir());
    dirs
}

/// 扫描 ZCode 的全部可见配置：`~/.zcode` + 桌面端应用数据 + 受管注入产物。
fn zcode_config_files(tool_home: &Path, home: &Path) -> Result<Vec<ToolConfigFile>> {
    let mut scanned = Vec::new();

    if tool_home.is_dir() {
        scan_config_dir(tool_home, tool_home, "", true, 3, &mut scanned);
    }

    for dir in zcode_app_data_dirs(home) {
        let prefix = if dir.file_name().and_then(|name| name.to_str()) == Some("Preferences") {
            "preferences"
        } else {
            "appdata"
        };
        // 应用数据目录很杂，深度收得更紧。
        scan_config_dir(&dir, &dir, prefix, true, 1, &mut scanned);
    }

    let managed_dir = home.join(crate::constants::ZCODE_KEYSMITH_DIRNAME);
    if managed_dir.is_dir() {
        scan_config_dir(
            &managed_dir,
            &managed_dir,
            "keysmith",
            false,
            1,
            &mut scanned,
        );
    }

    // 去重（同一路径可能被多个根目录覆盖）后按权重与标签排序。
    let mut seen_paths = HashSet::new();
    scanned.retain(|item| seen_paths.insert(item.path.clone()));
    scanned.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| left.label.cmp(&right.label))
    });

    // 主配置永远排第一，即使当前不存在也要占位，便于用户知道该去哪找。
    let primary = tool_home.join("cli").join("config.json");
    if !scanned.iter().any(|item| item.path == primary) {
        scanned.insert(
            0,
            ScannedConfig {
                id: "cli-config-json".to_string(),
                label: "cli/config.json".to_string(),
                path: primary,
                native: true,
                binary: false,
                rank: 0,
            },
        );
    }

    let mut seen_ids = HashSet::new();
    let mut files = Vec::new();
    for item in scanned.into_iter().take(CONFIG_SCAN_MAX_FILES) {
        let mut id = item.id.clone();
        let mut suffix = 2;
        while !seen_ids.insert(id.clone()) {
            id = format!("{}-{suffix}", item.id);
            suffix += 1;
        }
        if item.binary {
            files.push(binary_config_file(&id, &item.label, item.path, "sqlite"));
            continue;
        }
        // 扫描到的文件来源不可控（可能是非 UTF-8 或读取期间被删除）。
        // 单个文件读失败只跳过该文件，不能让整个配置页报错。
        match config_file(
            &id,
            &item.label,
            item.path,
            &format_for_path_ext(&item.label),
            item.native,
        ) {
            Ok(file) => files.push(file),
            Err(_) => continue,
        }
    }
    Ok(files)
}

fn format_for_path_ext(label: &str) -> String {
    label
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .filter(|extension| CONFIG_SCAN_EXTENSIONS.contains(&extension.as_str()))
        .unwrap_or_else(|| "text".to_string())
}

pub(crate) fn get_tool_config_inner(
    tool: ToolId,
    codex_override: Option<String>,
) -> Result<ToolConfigBundle> {
    let home = home_dir()?;
    let tool_home = tool.home_dir(codex_override.clone())?;
    let mut files = Vec::new();
    let notice = match tool {
        ToolId::Codex => {
            files.push(config_file(
                "config",
                "config.toml",
                tool_home.join("config.toml"),
                "toml",
                true,
            )?);
            files.push(config_file(
                "auth",
                "auth.json",
                tool_home.join("auth.json"),
                "json",
                true,
            )?);
            None
        }
        ToolId::Claude => {
            files.push(config_file(
                "settings",
                "settings.json",
                tool_home.join("settings.json"),
                "json",
                true,
            )?);
            files.push(config_file(
                "user",
                ".claude.json",
                home.join(".claude.json"),
                "json",
                true,
            )?);
            None
        }
        ToolId::Grok => {
            files.push(config_file(
                "config",
                "config.toml",
                tool_home.join("config.toml"),
                "toml",
                true,
            )?);
            files.push(config_file(
                "auth",
                "auth.json",
                tool_home.join("auth.json"),
                "json",
                true,
            )?);
            Some("Grok Build 的原生配置位于 ~/.grok；预览中的凭据已脱敏。".to_string())
        }
        ToolId::Zcode => {
            files.extend(zcode_config_files(&tool_home, &home)?);
            let scanned = files.len();
            Some(format!(
                "已扫描 {scanned} 个配置文件：~/.zcode（CLI 与 MCP）、ZCode 桌面端应用数据目录（供应商与模型通道）、\
                 以及 keysmith/ 前缀的 DevConduit 注入产物（诊断用，不是 ZCode 主配置）。凭据均已脱敏。"
            ))
        }
        ToolId::Kilo => {
            files.push(config_file(
                "config",
                "kilo.jsonc",
                tool_home.join("kilo.jsonc"),
                "jsonc",
                true,
            )?);
            files.push(config_file(
                "instructions",
                "AGENTS.md",
                tool_home.join("AGENTS.md"),
                "markdown",
                true,
            )?);
            Some(
                "Kilo Code 的全局配置位于 ~/.config/kilo，全局 Skills 位于 ~/.kilo/skills；预览中的凭据已脱敏。"
                    .to_string(),
            )
        }
        ToolId::Pi => {
            files.push(config_file(
                "settings",
                "settings.json",
                tool_home.join("settings.json"),
                "json",
                true,
            )?);
            files.push(config_file(
                "models",
                "models.json",
                tool_home.join("models.json"),
                "jsonc",
                true,
            )?);
            files.push(config_file(
                "instructions",
                "AGENTS.md",
                tool_home.join("AGENTS.md"),
                "markdown",
                true,
            )?);
            files.push(config_file(
                "mcp",
                "mcp.json",
                tool_home.join("mcp.json"),
                "jsonc",
                true,
            )?);
            Some(
                "Pi 的全局配置、指令、Skills、Extensions 与 MCP adapter 配置位于 ~/.pi/agent；auth.json 不进入预览。"
                    .to_string(),
            )
        }
    };
    let primary_file_id = files
        .first()
        .map(|file| file.id.clone())
        .unwrap_or_else(|| "config".to_string());
    Ok(ToolConfigBundle {
        tool,
        label: tool.label().to_string(),
        primary_file_id,
        files,
        notice,
    })
}

fn status_for_tool(tool: ToolId, codex_override: Option<String>) -> Result<ToolStatus> {
    let root = tool.home_dir(codex_override.clone())?;
    let config = tool.config_path(codex_override.clone())?;
    let home = home_dir()?;
    let (
        auth_path,
        instruction_path,
        native_instruction_path,
        diagnostic_path,
        instruction_enabled,
        instruction_exists,
        notice,
    ) = match tool {
        ToolId::Codex => {
            let state = build_state(root.clone())?;
            let instruction = root.join("AGENTS.md");
            (
                Some(root.join("auth.json")),
                instruction.clone(),
                instruction,
                None,
                state.instruction_enabled,
                state.instruction_enabled,
                None,
            )
        }
        ToolId::Claude => {
            let state = build_claude_state()?;
            let instruction = root.join("CLAUDE.md");
            (
                Some(home.join(".claude.json")),
                instruction.clone(),
                instruction.clone(),
                None,
                state.instruction_enabled,
                instruction.is_file(),
                None,
            )
        }
        ToolId::Grok => {
            let state = build_grok_state()?;
            let instruction = root.join("AGENTS.md");
            (
                Some(root.join("auth.json")),
                instruction.clone(),
                instruction.clone(),
                None,
                state.instruction_enabled,
                instruction.is_file(),
                None,
            )
        }
        ToolId::Zcode => {
            let state = build_zcode_state()?;
            let native_instruction = root.join("AGENTS.md");
            let managed = PathBuf::from(&state.system_file);
            let diagnostic = home.join(".zcode-keysmith").join("config.json");
            (
                None,
                managed.clone(),
                native_instruction,
                Some(diagnostic),
                state.instruction_enabled,
                managed.is_file(),
                Some(
                    "ZCode 提示词通过受管 system-role 注入；更改环境变量后需完全重启 ZCode。"
                        .to_string(),
                ),
            )
        }
        ToolId::Kilo => {
            let state = build_kilo_state()?;
            let instruction = PathBuf::from(&state.agents_path);
            (
                None,
                instruction.clone(),
                instruction.clone(),
                None,
                state.instruction_enabled,
                instruction.is_file(),
                Some(
                    "Kilo Code 使用 ~/.config/kilo/AGENTS.md 作为全局提示词，修改后可在 Kilo 中执行 /reload。"
                        .to_string(),
                ),
            )
        }
        ToolId::Pi => {
            let state = build_pi_state()?;
            let instruction = PathBuf::from(&state.agents_path);
            (
                Some(root.join("auth.json")),
                instruction.clone(),
                instruction.clone(),
                None,
                state.instruction_enabled,
                instruction.is_file(),
                Some(
                    "Pi 使用 ~/.pi/agent/AGENTS.md 作为全局指令，修改后可在 Pi 中执行 /reload。"
                        .to_string(),
                ),
            )
        }
    };
    let (model, provider, provider_id) = match tool {
        ToolId::Codex => {
            let (model, provider) = codex_model_provider(&config);
            (model, provider.clone(), provider)
        }
        ToolId::Claude => {
            let (model, provider) = claude_model_provider(&config);
            (model, provider, None)
        }
        ToolId::Grok => {
            let (model, provider) = grok_model_provider(&config);
            (model, provider, None)
        }
        ToolId::Zcode => match crate::providers::current_zcode_provider_inner()? {
            Some(current) => (
                (!current.model.is_empty()).then_some(current.model),
                Some(current.provider_name),
                Some(current.provider_id),
            ),
            None => (None, None, None),
        },
        ToolId::Kilo => {
            let (model, provider) = kilo_model_provider(&config);
            (model, provider, None)
        }
        ToolId::Pi => {
            let (model, provider) = pi_model_provider(&config);
            (model, provider.clone(), provider)
        }
    };
    let version = tool_version(tool);
    let installed = root.is_dir()
        || version.is_some()
        || (tool == ToolId::Zcode && crate::zcode::discover_zcode_app().is_ok());
    let auth_exists = auth_path.as_ref().is_some_and(|path| path.is_file());
    Ok(ToolStatus {
        id: tool,
        label: tool.label().to_string(),
        installed,
        version,
        home_dir: root.display().to_string(),
        config_path: config.display().to_string(),
        config_format: match tool {
            ToolId::Codex | ToolId::Grok => "toml",
            ToolId::Claude | ToolId::Zcode => "json",
            ToolId::Kilo => "jsonc",
            ToolId::Pi => "json",
        }
        .to_string(),
        config_exists: config.is_file(),
        auth_path: auth_path.map(|path| path.display().to_string()),
        auth_exists,
        instruction_path: instruction_path.display().to_string(),
        native_instruction_path: native_instruction_path.display().to_string(),
        diagnostic_path: diagnostic_path.map(|path| path.display().to_string()),
        instruction_exists,
        instruction_enabled,
        model,
        provider,
        provider_id,
        notice,
        capabilities: ToolCapabilities::for_tool(tool),
    })
}

fn fallback_status(
    tool: ToolId,
    codex_override: Option<String>,
    error: &CodexxError,
) -> Result<ToolStatus> {
    let root = tool.home_dir(codex_override.clone())?;
    let config = tool.config_path(codex_override)?;
    let home = home_dir()?;
    let (auth_path, instruction_path, native_instruction_path, diagnostic_path) = match tool {
        ToolId::Codex => {
            let instruction = root.join("AGENTS.md");
            (
                Some(root.join("auth.json")),
                instruction.clone(),
                instruction,
                None,
            )
        }
        ToolId::Claude => {
            let instruction = root.join("CLAUDE.md");
            (
                Some(home.join(".claude.json")),
                instruction.clone(),
                instruction,
                None,
            )
        }
        ToolId::Grok => {
            let instruction = root.join("AGENTS.md");
            (
                Some(root.join("auth.json")),
                instruction.clone(),
                instruction,
                None,
            )
        }
        ToolId::Zcode => (
            None,
            home.join(".zcode-keysmith").join("system-role.md"),
            root.join("AGENTS.md"),
            Some(home.join(".zcode-keysmith").join("config.json")),
        ),
        ToolId::Kilo => {
            let instruction = root.join("AGENTS.md");
            (None, instruction.clone(), instruction, None)
        }
        ToolId::Pi => {
            let instruction = root.join("AGENTS.md");
            (
                Some(root.join("auth.json")),
                instruction.clone(),
                instruction,
                None,
            )
        }
    };
    let version = tool_version(tool);
    let installed = root.is_dir()
        || version.is_some()
        || (tool == ToolId::Zcode && crate::zcode::discover_zcode_app().is_ok());
    let instruction_exists = instruction_path.is_file();
    let auth_exists = auth_path.as_ref().is_some_and(|path| path.is_file());
    Ok(ToolStatus {
        id: tool,
        label: tool.label().to_string(),
        installed,
        version,
        home_dir: root.display().to_string(),
        config_path: config.display().to_string(),
        config_format: match tool {
            ToolId::Codex | ToolId::Grok => "toml",
            ToolId::Claude | ToolId::Zcode => "json",
            ToolId::Kilo => "jsonc",
            ToolId::Pi => "json",
        }
        .to_string(),
        config_exists: config.is_file(),
        auth_path: auth_path.map(|path| path.display().to_string()),
        auth_exists,
        instruction_path: instruction_path.display().to_string(),
        native_instruction_path: native_instruction_path.display().to_string(),
        diagnostic_path: diagnostic_path.map(|path| path.display().to_string()),
        instruction_exists,
        instruction_enabled: false,
        model: None,
        provider: None,
        provider_id: None,
        notice: Some(format!(
            "{} 状态读取失败，其他工具不受影响：{error}",
            tool.label()
        )),
        capabilities: ToolCapabilities::for_tool(tool),
    })
}

pub(crate) fn get_tool_statuses_inner(codex_override: Option<String>) -> Result<Vec<ToolStatus>> {
    ToolId::ALL
        .into_iter()
        .map(|tool| {
            status_for_tool(tool, codex_override.clone())
                .or_else(|error| fallback_status(tool, codex_override.clone(), &error))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_aliases_are_normalized() {
        assert_eq!(ToolId::parse("Claude-Code").unwrap(), ToolId::Claude);
        assert_eq!(ToolId::parse("grokbuild").unwrap(), ToolId::Grok);
        assert_eq!(ToolId::parse("z-code").unwrap(), ToolId::Zcode);
        assert_eq!(ToolId::parse("kilo-code").unwrap(), ToolId::Kilo);
        assert_eq!(ToolId::parse("pi-agent").unwrap(), ToolId::Pi);
    }

    #[test]
    fn json_redaction_is_recursive() {
        let mut value = json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "secret",
                "nested": [{"refresh_token": "refresh"}],
                "model": "claude"
            }
        });
        redact_json_value(&mut value);
        assert_eq!(
            value
                .pointer("/env/ANTHROPIC_AUTH_TOKEN")
                .and_then(JsonValue::as_str),
            Some(REDACTED_VALUE)
        );
        assert_eq!(
            value
                .pointer("/env/nested/0/refresh_token")
                .and_then(JsonValue::as_str),
            Some(REDACTED_VALUE)
        );
        assert_eq!(
            value.pointer("/env/model").and_then(JsonValue::as_str),
            Some("claude")
        );
    }

    #[test]
    fn jsonc_redaction_accepts_comments_and_trailing_commas() {
        let redacted = redacted_jsonc_text(
            "{\n  // keep parsing\n  \"apiKey\": \"secret\",\n  \"model\": \"gpt\",\n}\n",
        );
        assert!(redacted.contains("\"apiKey\": \"[REDACTED]\""));
        assert!(redacted.contains("\"model\": \"gpt\""));
        assert!(!redacted.contains("\"secret\""));
    }

    #[test]
    fn toml_redaction_preserves_non_secret_values() {
        let redacted = redacted_toml_text(
            "model = \"gpt\"\napi_key = \"secret\"\n[server]\nbase_url = \"https://example.com\"\ntoken = \"nested\"\n",
        );
        assert!(redacted.contains("model = \"gpt\""));
        assert!(redacted.contains("api_key = \"[REDACTED]\""));
        assert!(redacted.contains("base_url = \"https://example.com\""));
        assert!(redacted.contains("token = \"[REDACTED]\""));
        assert!(!redacted.contains("\"secret\""));
        assert!(!redacted.contains("\"nested\""));
    }
}
