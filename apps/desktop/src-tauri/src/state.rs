use crate::backups::{latest_backup, BackupEntry};
use crate::config_migration::migrate_legacy_prompt_config;
use crate::error::Result;
use crate::file_io::{io_err, json_err, parse_toml_document, read_to_string_if_exists};
use crate::prompts::{
    agents_path, claude_builtin_prompt_meta, claude_memory_path, codex_prompt_id_allowed,
    list_saved_prompts_inner, managed_agents_template_key, managed_claude_injection_mode,
    managed_claude_template_key, prompt_template_key_for_instruction, ENGINE_CLAUDE,
};
use crate::providers::{list_saved_providers_inner, SavedProvider};
use crate::tools::{redact_json_value, redacted_json_text, redacted_toml_text};
use crate::{auth_path, config_path, string_value};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSummary {
    id: String,
    name: Option<String>,
    base_url: Option<String>,
    wire_api: Option<String>,
    requires_openai_auth: Option<bool>,
    is_current: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexState {
    codex_dir: String,
    config_path: String,
    auth_path: String,
    config_exists: bool,
    auth_exists: bool,
    official_auth_available: bool,
    pub(crate) model: Option<String>,
    pub(crate) model_provider: Option<String>,
    instruction_file: Option<String>,
    pub(crate) instruction_enabled: bool,
    instruction_status: String,
    inactive_instruction_file: Option<String>,
    pub(crate) instruction_injection_mode: Option<String>,
    pub(crate) instruction_template_key: Option<String>,
    agents_path: String,
    active_saved_provider_id: Option<String>,
    providers: Vec<ProviderSummary>,
    #[serde(skip_serializing)]
    pub(crate) config_text: String,
    #[serde(rename = "configText")]
    config_preview: String,
    auth_preview: Option<Value>,
    #[serde(skip_serializing)]
    auth_text: String,
    #[serde(rename = "authText")]
    auth_text_preview: String,
    last_backup: Option<BackupEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionResult {
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) backup_id: Option<String>,
    pub(crate) state: CodexState,
}

fn redacted_auth_preview(path: &Path) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    let mut value: Value = serde_json::from_str(&text).map_err(|e| json_err(path, e))?;
    redact_json_value(&mut value);
    Ok(Some(value))
}

pub(crate) fn auth_has_material(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let text = fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| json_err(path, e))?;
    let Some(obj) = value.as_object() else {
        return Ok(false);
    };
    Ok(obj.iter().any(|(key, value)| {
        if key == "auth_mode" {
            return false;
        }
        match value {
            Value::Null => false,
            Value::String(s) => !s.trim().is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            _ => true,
        }
    }))
}

fn bool_from_item(item: Option<&Item>) -> Option<bool> {
    item.and_then(|i| i.as_bool())
}

fn extract_providers(doc: &DocumentMut, current: Option<&str>) -> Vec<ProviderSummary> {
    let Some(providers) = doc.get("model_providers").and_then(|i| i.as_table()) else {
        return Vec::new();
    };

    providers
        .iter()
        .filter_map(|(id, item)| {
            let table = item.as_table()?;
            Some(ProviderSummary {
                id: id.to_string(),
                name: table
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string),
                base_url: table
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string),
                wire_api: table
                    .get("wire_api")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string),
                requires_openai_auth: bool_from_item(table.get("requires_openai_auth")),
                is_current: current.is_some_and(|c| c == id),
            })
        })
        .collect()
}

fn normalized_provider_toml_for_match(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .filter(|line| !line.trim_start().starts_with("experimental_bearer_token"))
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn inactive_managed_instruction_file(codex_dir: &Path) -> Result<Option<String>> {
    let entries = match fs::read_dir(codex_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_err(codex_dir, error)),
    };
    let mut filenames = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| io_err(codex_dir, error))?;
        if !entry
            .file_type()
            .map_err(|error| io_err(&entry.path(), error))?
            .is_file()
        {
            continue;
        }
        let Some(filename) = entry.file_name().to_str().map(ToString::to_string) else {
            continue;
        };
        if Path::new(&filename)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            filenames.push(filename);
        }
    }
    filenames.sort_unstable();

    for filename in filenames {
        let Some(template_key) = prompt_template_key_for_instruction(&filename)? else {
            continue;
        };
        if !template_key.starts_with("saved:external-") {
            return Ok(Some(filename));
        }
    }
    Ok(None)
}

fn codex_instruction_status(
    instruction_enabled: bool,
    instruction_file: Option<&str>,
    inactive_instruction_file: Option<&str>,
) -> &'static str {
    if instruction_enabled {
        "active"
    } else if instruction_file.is_some() {
        "external"
    } else if inactive_instruction_file.is_some() {
        "inactive"
    } else {
        "none"
    }
}

pub(crate) fn active_saved_provider_id_from_config(
    config_text: &str,
    providers: &[SavedProvider],
) -> Option<String> {
    let live = normalized_provider_toml_for_match(config_text);
    if live.is_empty() {
        return None;
    }
    let matches = providers
        .iter()
        .filter(|provider| {
            provider
                .toml_config
                .as_deref()
                .is_some_and(|toml| normalized_provider_toml_for_match(toml) == live)
        })
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0].id.clone())
}

pub(crate) fn build_state(codex_dir: PathBuf) -> Result<CodexState> {
    let cfg = config_path(&codex_dir);
    let auth = auth_path(&codex_dir);
    migrate_legacy_prompt_config(&codex_dir)?;
    let text = read_to_string_if_exists(&cfg)?;
    let doc = parse_toml_document(&cfg, &text)?;
    let model = string_value(&doc, "model");
    let model_provider = string_value(&doc, "model_provider");
    let instruction_file = string_value(&doc, "model_instructions_file");
    let model_template_key = instruction_file
        .as_deref()
        .map(prompt_template_key_for_instruction)
        .transpose()?
        .flatten();
    let agents_template_key = managed_agents_template_key(&codex_dir)?.filter(|key| {
        key.strip_prefix("builtin:")
            .map(codex_prompt_id_allowed)
            .unwrap_or(true)
    });
    let (instruction_injection_mode, instruction_template_key) =
        if let Some(key) = agents_template_key {
            (Some("append".to_string()), Some(key))
        } else if let Some(key) = model_template_key {
            (Some("replace".to_string()), Some(key))
        } else {
            (None, None)
        };
    let instruction_enabled = instruction_template_key.is_some();
    let inactive_instruction_file = if instruction_enabled || instruction_file.is_some() {
        None
    } else {
        inactive_managed_instruction_file(&codex_dir)?
    };
    let instruction_status = codex_instruction_status(
        instruction_enabled,
        instruction_file.as_deref(),
        inactive_instruction_file.as_deref(),
    )
    .to_string();
    let providers = extract_providers(&doc, model_provider.as_deref());
    let active_saved_provider_id = if model_provider.as_deref() == Some("openai") {
        None
    } else {
        active_saved_provider_id_from_config(&text, &list_saved_providers_inner()?)
    };
    let auth_text = read_to_string_if_exists(&auth)?;

    Ok(CodexState {
        codex_dir: codex_dir.display().to_string(),
        config_path: cfg.display().to_string(),
        auth_path: auth.display().to_string(),
        config_exists: cfg.exists(),
        auth_exists: auth.exists(),
        official_auth_available: auth_has_material(&auth)?,
        model,
        model_provider,
        instruction_file,
        instruction_enabled,
        instruction_status,
        inactive_instruction_file,
        instruction_injection_mode,
        instruction_template_key,
        agents_path: agents_path(&codex_dir).display().to_string(),
        active_saved_provider_id,
        providers,
        config_preview: redacted_toml_text(&text),
        config_text: text,
        auth_preview: redacted_auth_preview(&auth)?,
        auth_text_preview: redacted_json_text(&auth_text),
        auth_text,
        last_backup: latest_backup()?,
    })
}

/// Claude 指令状态，对应 `~/.claude/CLAUDE.md` 的受管 import-block。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeState {
    claude_dir: String,
    memory_path: String,
    memory_exists: bool,
    pub(crate) instruction_enabled: bool,
    pub(crate) instruction_injection_mode: Option<String>,
    pub(crate) instruction_template_key: Option<String>,
    pub(crate) active_instruction_title: Option<String>,
    pub(crate) runtime: crate::claude_runtime::ClaudeRuntimeState,
}

/// Claude 操作结果，与 `ActionResult` 对应但携带 `ClaudeState`。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeActionResult {
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) backup_id: Option<String>,
    pub(crate) state: ClaudeState,
}

/// 根据 template_key 反查启用的指令标题。
fn claude_instruction_title(template_key: &str) -> Option<String> {
    if let Some(id) = template_key.strip_prefix("builtin:") {
        let meta = claude_builtin_prompt_meta();
        return if id == meta.id {
            Some(meta.title.to_string())
        } else {
            None
        };
    }
    if let Some(id) = template_key.strip_prefix("saved:") {
        return list_saved_prompts_inner(ENGINE_CLAUDE)
            .ok()?
            .into_iter()
            .find(|p| p.id == id)
            .map(|p| p.title);
    }
    None
}

/// 构建 Claude 指令状态：读取 ~/.claude/CLAUDE.md，提取受管区块 template_key。
pub(crate) fn build_claude_state() -> Result<ClaudeState> {
    let memory = claude_memory_path()?;
    let memory_exists = memory.is_file();
    let template_key = managed_claude_template_key()?;
    let instruction_enabled = template_key.is_some();
    let instruction_injection_mode =
        managed_claude_injection_mode()?.map(|mode| mode.as_str().to_string());
    let active_instruction_title = template_key.as_deref().and_then(claude_instruction_title);
    let runtime = crate::claude_runtime::build_runtime_state()?;
    Ok(ClaudeState {
        claude_dir: memory
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        memory_path: memory.display().to_string(),
        memory_exists,
        instruction_enabled,
        instruction_injection_mode,
        instruction_template_key: template_key,
        active_instruction_title,
        runtime,
    })
}

/// ZCode 指令状态，对应 ~/.zcode-keysmith 的受管 system-role 入口。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZcodeState {
    pub(crate) managed_dir: String,
    pub(crate) system_file: String,
    pub(crate) system_file_exists: bool,
    pub(crate) instruction_enabled: bool,
    pub(crate) instruction_injection_mode: Option<String>,
    pub(crate) instruction_template_key: Option<String>,
    pub(crate) zcode_app: Option<String>,
    pub(crate) zcode_runtime_exists: bool,
    pub(crate) runtime_patchable: bool,
    pub(crate) agent_override_supported: bool,
    pub(crate) zcode_running: bool,
    pub(crate) active_instruction_title: Option<String>,
}

/// ZCode 操作结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZcodeActionResult {
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) backup_id: Option<String>,
    pub(crate) state: ZcodeState,
}

/// ZCode 诊断信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZcodeDoctor {
    pub managed_dir: String,
    pub system_file: String,
    pub system_file_exists: bool,
    pub launcher_exists: bool,
    pub zcode_app: Option<String>,
    pub zcode_runtime_exists: bool,
    pub runtime_patchable: bool,
    pub agent_override_supported: bool,
    pub zcode_running: bool,
}

/// ZCode 验证信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZcodeVerify {
    pub system_file_exists: bool,
    pub launcher_exists: bool,
    pub zcode_app: Option<String>,
    pub zcode_runtime_exists: bool,
    pub runtime_patchable: bool,
    pub agent_override_supported: bool,
    pub zcode_running: bool,
}

/// 构建 ZCode 指令状态。
pub(crate) fn build_zcode_state() -> Result<ZcodeState> {
    let paths = crate::zcode::build_paths()?;
    let system_file_exists = paths.system_file.is_file();
    let launcher_exists = paths.launcher.is_file();

    let zcode_app = crate::zcode::discover_zcode_app().ok();
    let (zcode_runtime, _node) = zcode_app
        .as_ref()
        .map(|app| crate::zcode::resolve_runtime_and_node(app))
        .unwrap_or_default();
    let zcode_runtime_exists = zcode_runtime.exists();
    let runtime_patchable = crate::zcode::runtime_patchable(&zcode_runtime);
    let agent_override_supported = zcode_app
        .as_ref()
        .map(|app| crate::zcode::app_supports_agent_override(app))
        .unwrap_or(false);
    let zcode_running = crate::zcode::is_zcode_running();
    let (injection_mode, instruction_template_key, configured_title) =
        crate::zcode::current_install_metadata()?;

    // 指令已启用 = system-role.md 存在 + launcher 存在
    let instruction_enabled = system_file_exists && launcher_exists;
    let active_instruction_title = if instruction_enabled {
        configured_title.or_else(|| Some(crate::constants::ZCODE_BUILTIN_TITLE.to_string()))
    } else {
        None
    };

    Ok(ZcodeState {
        managed_dir: paths.managed_dir.display().to_string(),
        system_file: paths.system_file.display().to_string(),
        system_file_exists,
        instruction_enabled,
        instruction_injection_mode: injection_mode.map(|mode| mode.as_str().to_string()),
        instruction_template_key,
        zcode_app: zcode_app.map(|p| p.display().to_string()),
        zcode_runtime_exists,
        runtime_patchable,
        agent_override_supported,
        zcode_running,
        active_instruction_title,
    })
}

/// Grok 指令状态，对应 ~/.grok 的受管 AGENTS.md 部署。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokState {
    pub(crate) grok_dir: String,
    pub(crate) grok_dir_exists: bool,
    pub(crate) agents_md_exists: bool,
    pub(crate) config_toml_exists: bool,
    pub(crate) compat_block_injected: bool,
    pub(crate) active_hooks_count: usize,
    pub(crate) disabled_hooks_count: usize,
    pub(crate) manifest_exists: bool,
    pub(crate) instruction_enabled: bool,
    pub(crate) instruction_injection_mode: Option<String>,
    pub(crate) instruction_template_key: Option<String>,
    pub(crate) active_instruction_title: Option<String>,
}

/// Grok 操作结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokActionResult {
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) backup_id: Option<String>,
    pub(crate) state: GrokState,
}

/// 构建 Grok 指令状态。
pub(crate) fn build_grok_state() -> Result<GrokState> {
    let status = crate::grok::grok_status()?;
    let grok_dir = crate::grok::grok_home_dir()?;
    // 指令已启用 = AGENTS.md 存在 + manifest 存在
    let instruction_enabled = status.agents_md_exists && status.manifest_exists;
    let (injection_mode, instruction_template_key, configured_title) =
        crate::grok::current_install_metadata()?;
    let active_instruction_title = if instruction_enabled {
        configured_title.or_else(|| Some(crate::constants::GROK_BUILTIN_TITLE.to_string()))
    } else {
        None
    };
    Ok(GrokState {
        grok_dir: grok_dir.display().to_string(),
        grok_dir_exists: status.grok_dir_exists,
        agents_md_exists: status.agents_md_exists,
        config_toml_exists: status.config_toml_exists,
        compat_block_injected: status.compat_block_injected,
        active_hooks_count: status.active_hooks_count,
        disabled_hooks_count: status.disabled_hooks_count,
        manifest_exists: status.manifest_exists,
        instruction_enabled,
        instruction_injection_mode: injection_mode.map(|mode| mode.as_str().to_string()),
        instruction_template_key,
        active_instruction_title,
    })
}

#[cfg(test)]
mod tests {
    use super::codex_instruction_status;

    #[test]
    fn codex_instruction_status_distinguishes_profile_switches_from_external_prompts() {
        assert_eq!(
            codex_instruction_status(true, Some("./managed.md"), None),
            "active"
        );
        assert_eq!(
            codex_instruction_status(false, Some("./external.md"), None),
            "external"
        );
        assert_eq!(
            codex_instruction_status(false, None, Some("managed.md")),
            "inactive"
        );
        assert_eq!(codex_instruction_status(false, None, None), "none");
    }
}
