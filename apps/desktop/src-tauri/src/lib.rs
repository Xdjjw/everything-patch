use chrono::Local;
#[cfg(test)]
use rusqlite::params;
use rusqlite::Connection;
use serde::Serialize;
#[cfg(test)]
use serde_json::{json, Value};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(any(test, target_os = "windows"))]
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod app_db;
mod backups;
mod ccswitch;
mod config_migration;
mod constants;
mod error;
mod file_io;
mod grok;
mod paths;
mod platform;
mod prompt_backups;
mod prompts;
mod providers;
mod remote;
mod sessions;
mod skills_mcp;
mod skin_presets;
mod skin_runtime;
mod skins;
mod sqlite_utils;
mod state;
mod toml_utils;
mod tool_sessions;
mod tools;
mod updates;
mod zcode;

use backups::{action_backup_root, backups, create_backup, BackupEntry, BackupMeta};
use constants::*;
use error::{CodexxError, Result};
#[cfg(test)]
use file_io::write_json;
use file_io::{
    atomic_write, directory_exists, ensure_directory, io_err, parse_toml_document,
    read_to_string_if_exists, write_text,
};
#[cfg(test)]
use paths::app_home;
use paths::home_dir;
use prompt_backups::{
    create_claude_prompt_backup, create_codex_prompt_backup, create_grok_prompt_backup,
    create_zcode_prompt_backup, list_prompt_backups as prompt_backup_entries,
    restore_prompt_backup as restore_prompt_backup_snapshot, PromptBackupEntry,
};
use prompts::{
    agents_path, builtin_prompt_content, builtin_prompt_status_inner, bundled_prompt_meta,
    claude_builtin_prompt_content, claude_builtin_prompt_status_inner, codex_prompt_id_allowed,
    delete_prompt_inner, get_saved_prompt_inner, install_managed_agents_block,
    install_managed_claude_block, list_saved_prompts_inner, managed_agents_bounds,
    normalize_prompt_filename, prompt_template_key_for_instruction,
    refresh_builtin_prompts_with_active, remember_current_instruction_prompt,
    resolve_instruction_path, save_prompt_inner, uninstall_managed_agents_block,
    uninstall_managed_claude_block, BuiltinPromptStatus, PromptInjectionMode, SavedPrompt,
    ENGINE_CLAUDE, ENGINE_CODEX, ENGINE_GROK, ENGINE_ZCODE,
};
#[cfg(test)]
use prompts::{
    bundled_prompt_metas, cached_prompt_fallback_statuses, delete_cached_prompt_ids,
    github_prompt_catalog_from_entries, jsdelivr_prompt_catalog_from_entries,
    managed_agents_template_key_from_content, prompt_content_source_urls, stable_remote_prompt_id,
    stale_cached_prompt_ids, CachedBuiltinPrompt, GithubContentEntry,
};
use providers::{
    activate_saved_provider_inner, delete_provider_for_app_inner, fetch_provider_models_inner,
    import_ccswitch_codex_providers_inner, import_ccswitch_providers_inner,
    list_saved_providers_for_app_inner, list_zcode_providers_inner,
    provider_by_id_on_connection_for_app, read_ccswitch_official_auth_inner,
    save_official_config_inner, save_provider_inner, save_provider_toml_config_inner,
    switch_official_provider_inner, switch_provider_inner, test_provider_connection_inner,
    ImportResult, OfficialAuthCandidate, OfficialConfigInput, ProviderConnectionResult,
    ProviderInput, ProviderModelsResult, ProviderTomlInput, SavedProvider,
    ToolProviderActionResult,
};
#[cfg(test)]
use providers::{
    build_ccswitch_codex_provider, canonical_provider_base_url, codex_sections_from_config,
    detected_live_custom_provider, is_official_ccswitch_row, list_saved_providers_on_connection,
    merge_duplicate_provider_identities, normalize_saved_provider, provider_by_id_on_connection,
    provider_identity, provider_status_result, read_ccswitch_codex_rows,
    save_manual_provider_on_connection, save_provider_toml_config_with_pre_persist,
    switch_official_provider_with_pre_persist, switch_provider_with_pre_persist,
    upsert_provider_on_connection, CcSwitchCodexRow, ProviderUpsertKind, ProviderUpsertMode,
};
#[cfg(test)]
use sessions::{
    active_session_ids_present, apply_session_changes, backup_sqlite_to_backup,
    hard_delete_sessions_locally, list_session_previews, provider_sync_backup_root,
    prune_provider_sync_backups, restore_session_changes, scan_rollouts, scan_sqlite,
    sqlite_session_db_paths,
};
use sessions::{
    delete_codex_sessions_inner, session_sync_status_inner, sqlite_candidate_paths,
    sync_sessions_provider_inner, SessionDeleteInput, SessionDeleteResult, SessionSyncResult,
    SessionSyncStatus,
};
use skills_mcp::{
    build_skills_mcp_state_inner, build_tool_state_inner, check_skill_updates_inner,
    check_tool_skill_updates_inner, import_existing_skills_mcp_inner, import_tool_resources_inner,
    install_mcp_integration_inner, install_skill_zip_inner, install_tool_skill_zip_inner,
    preview_existing_skills_mcp_inner, preview_tool_import_inner, toggle_codex_mcp_inner,
    toggle_codex_skill_inner, toggle_tool_mcp_inner, toggle_tool_skill_inner,
    McpIntegrationInstallInput, SkillsMcpActionResult, SkillsMcpImportPreview, SkillsMcpState,
};
#[cfg(test)]
use skills_mcp::{
    normalize_legacy_zip_skill_dirs, read_skill_metadata, sort_managed_mcp_servers,
    sort_managed_skills, ManagedMcpServer, ManagedSkill,
};
use skins::{
    create_skin_theme_from_image_inner, enable_skin_theme_inner, export_skin_theme_inner,
    get_skin_center_state_inner, import_skin_theme_zip_inner, pause_skin_theme_inner,
    restore_skin_theme_inner, update_skin_theme_settings_inner, SkinActionResult, SkinCenterState,
    SkinExportResult,
};
#[cfg(test)]
use state::active_saved_provider_id_from_config;
use state::{
    auth_has_material, build_claude_state, build_grok_state, build_state, build_zcode_state,
    ActionResult, ClaudeActionResult, ClaudeState, CodexState, GrokActionResult, GrokState,
    ZcodeActionResult, ZcodeDoctor, ZcodeState, ZcodeVerify,
};
use toml_edit::{value, DocumentMut};
pub(crate) use toml_utils::string_value;
use tool_sessions::{get_tool_sessions_inner, ToolSessionList};
use tools::{get_tool_config_inner, get_tool_statuses_inner, ToolConfigBundle, ToolId, ToolStatus};
use updates::check_app_update;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AboutInfo {
    app_version: String,
    codex_version: Option<String>,
    codex_dir: String,
    project_url: String,
    github_repo: String,
    native_updater_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptRestoreResult {
    ok: bool,
    message: String,
    backup_id: Option<String>,
    engine: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticItem {
    key: String,
    label: String,
    path: Option<String>,
    status: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupDiagnostics {
    codex_dir: String,
    needs_manual_select: bool,
    summary: String,
    items: Vec<DiagnosticItem>,
}

fn open_db() -> Result<Connection> {
    providers::open_store()
}

pub(crate) fn now_rfc3339() -> String {
    Local::now().to_rfc3339()
}

fn active_remote_builtin_prompt_id(config_dir: Option<String>) -> Option<String> {
    let codex_dir = resolve_codex_dir(config_dir).ok()?;
    let state = build_state(codex_dir).ok()?;
    let template_key = state.instruction_template_key.as_deref()?;
    let id = template_key.strip_prefix("builtin:")?.trim();
    if id.is_empty() || !codex_prompt_id_allowed(id) || bundled_prompt_meta(id).is_some() {
        return None;
    }
    Some(id.to_string())
}

fn refresh_builtin_prompts_inner(config_dir: Option<String>) -> Result<Vec<BuiltinPromptStatus>> {
    refresh_builtin_prompts_with_active(|| active_remote_builtin_prompt_id(config_dir))
}
pub(crate) fn sanitize_id(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        format!("provider-{}", Local::now().timestamp_millis())
    } else {
        out
    }
}

fn default_codex_dir() -> Result<PathBuf> {
    if let Ok(value) = std::env::var("CODEX_HOME") {
        if let Some(path) = codex_dir_from_text(&value)? {
            return Ok(path);
        }
    }
    Ok(home_dir()?.join(".codex"))
}

fn codex_dir_from_text(value: &str) -> Result<Option<PathBuf>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let unquoted = if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    if unquoted.trim().is_empty() {
        return Ok(None);
    }
    if unquoted == "~" {
        return Ok(Some(home_dir()?));
    }
    if let Some(rest) = unquoted
        .strip_prefix("~/")
        .or_else(|| unquoted.strip_prefix("~\\"))
    {
        return Ok(Some(home_dir()?.join(rest)));
    }
    Ok(Some(PathBuf::from(unquoted)))
}

#[cfg(target_os = "windows")]
fn resolve_windows_linked_directory(path: PathBuf) -> Result<PathBuf> {
    use std::os::windows::fs::FileTypeExt;

    let original = path.clone();
    let mut current = path;
    let mut followed_link = false;
    let mut visited = HashSet::new();
    for _ in 0..16 {
        if !visited.insert(current.clone()) {
            return Err(CodexxError::Config(format!(
                "当前 Codex 目录链接形成了循环：{}",
                original.display()
            )));
        }
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !followed_link => {
                return Ok(current);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(CodexxError::Config(format!(
                    "当前 Codex 目录链接的目标不存在：{}",
                    original.display()
                )));
            }
            Err(error) => return Err(io_err(&current, error)),
        };
        let file_type = metadata.file_type();
        if metadata.is_dir() && !file_type.is_symlink_dir() {
            return Ok(current);
        }
        if file_type.is_symlink_file() || file_type.is_symlink_dir() || file_type.is_symlink() {
            let target = fs::read_link(&current).map_err(|error| io_err(&current, error))?;
            current = if target.is_absolute() {
                target
            } else {
                current
                    .parent()
                    .map(|parent| parent.join(&target))
                    .unwrap_or(target)
            };
            followed_link = true;
            continue;
        }
        return Err(CodexxError::Config(format!(
            "当前 CODEX_HOME 不是文件夹：{}",
            original.display()
        )));
    }

    Err(CodexxError::Config(format!(
        "当前 Codex 目录链接层级过多：{}",
        original.display()
    )))
}

#[cfg(not(target_os = "windows"))]
fn resolve_windows_linked_directory(path: PathBuf) -> Result<PathBuf> {
    Ok(path)
}

pub(crate) fn resolve_codex_dir(config_dir: Option<String>) -> Result<PathBuf> {
    let path = match config_dir.as_deref().map(codex_dir_from_text).transpose()? {
        Some(Some(path)) => Ok(path),
        _ => default_codex_dir(),
    }?;
    resolve_windows_linked_directory(path)
}

pub(crate) fn config_path(codex_dir: &Path) -> PathBuf {
    codex_dir.join("config.toml")
}

pub(crate) fn auth_path(codex_dir: &Path) -> PathBuf {
    codex_dir.join("auth.json")
}

fn diagnostic_item(
    key: &str,
    label: &str,
    path: Option<&Path>,
    ok: bool,
    manual_when_missing: bool,
) -> DiagnosticItem {
    let status = if ok {
        "ok"
    } else if manual_when_missing {
        "manual"
    } else {
        "missing"
    };
    let message = match status {
        "ok" => "检测通过",
        "manual" => "需要手动选择",
        _ => "未找到",
    };
    DiagnosticItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.map(|p| p.display().to_string()),
        status: status.to_string(),
        message: message.to_string(),
    }
}

fn startup_diagnostics_inner(config_dir: Option<String>) -> Result<StartupDiagnostics> {
    let codex_dir = resolve_codex_dir(config_dir)?;
    let config = config_path(&codex_dir);
    let auth = auth_path(&codex_dir);
    let sqlite_paths = sqlite_candidate_paths(&codex_dir);
    let codex_dir_ok = directory_exists(&codex_dir);
    let config_ok = config.is_file();
    let auth_ok = auth.is_file() && auth_has_material(&auth).unwrap_or(false);
    let sqlite_ok = !sqlite_paths.is_empty();

    let mut items = Vec::new();
    items.push(diagnostic_item(
        "codexHome",
        "CODEX_HOME",
        Some(&codex_dir),
        codex_dir_ok,
        true,
    ));
    items.push(diagnostic_item(
        "config",
        "config.toml",
        Some(&config),
        config_ok,
        false,
    ));
    items.push(diagnostic_item(
        "auth",
        "auth.json",
        Some(&auth),
        auth_ok,
        false,
    ));
    items.push(DiagnosticItem {
        key: "sqlite".to_string(),
        label: "SQLite 会话库".to_string(),
        path: sqlite_paths.first().map(|p| {
            if sqlite_paths.len() > 1 {
                format!("{} 等 {} 个", p.display(), sqlite_paths.len())
            } else {
                p.display().to_string()
            }
        }),
        status: if sqlite_ok { "ok" } else { "missing" }.to_string(),
        message: if sqlite_ok {
            "检测通过"
        } else {
            "未找到"
        }
        .to_string(),
    });

    let ok_count = items.iter().filter(|item| item.status == "ok").count();
    let needs_manual_select = !codex_dir_ok;
    let summary = if ok_count == items.len() {
        "Codex 环境检测通过".to_string()
    } else if needs_manual_select {
        "未找到 CODEX_HOME，需要手动选择 Codex 配置目录".to_string()
    } else {
        format!(
            "已检测到 {ok_count}/{} 项，缺失项不影响部分功能使用",
            items.len()
        )
    };

    Ok(StartupDiagnostics {
        codex_dir: codex_dir.display().to_string(),
        needs_manual_select,
        summary,
        items,
    })
}

#[tauri::command]
async fn get_tool_statuses(config_dir: Option<String>) -> Result<Vec<ToolStatus>> {
    tauri::async_runtime::spawn_blocking(move || get_tool_statuses_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("读取工具状态失败: {e}")))?
}

#[tauri::command]
async fn get_tool_config(tool: ToolId, config_dir: Option<String>) -> Result<ToolConfigBundle> {
    tauri::async_runtime::spawn_blocking(move || get_tool_config_inner(tool, config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("读取工具配置失败: {e}")))?
}

#[tauri::command]
async fn get_tool_sessions(tool: ToolId, config_dir: Option<String>) -> Result<ToolSessionList> {
    tauri::async_runtime::spawn_blocking(move || get_tool_sessions_inner(tool, config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("读取工具会话失败: {e}")))?
}

#[tauri::command]
async fn get_skills_mcp_state(config_dir: Option<String>) -> Result<SkillsMcpState> {
    tauri::async_runtime::spawn_blocking(move || build_skills_mcp_state_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("读取 Skills/MCP 失败: {e}")))?
}

#[tauri::command]
async fn get_tool_skills_mcp_state(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<SkillsMcpState> {
    tauri::async_runtime::spawn_blocking(move || build_tool_state_inner(tool, config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("读取工具 Skills/MCP 失败: {e}")))?
}

#[tauri::command]
async fn preview_tool_skills_mcp_import(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<SkillsMcpImportPreview> {
    tauri::async_runtime::spawn_blocking(move || preview_tool_import_inner(tool, config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("预览工具 Skills/MCP 失败: {e}")))?
}

#[tauri::command]
async fn import_tool_skills_mcp(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<SkillsMcpActionResult> {
    tauri::async_runtime::spawn_blocking(move || import_tool_resources_inner(tool, config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("导入工具 Skills/MCP 失败: {e}")))?
}

#[tauri::command]
async fn toggle_tool_skill(
    tool: ToolId,
    config_dir: Option<String>,
    id: String,
    enabled: bool,
) -> Result<SkillsMcpState> {
    tauri::async_runtime::spawn_blocking(move || {
        toggle_tool_skill_inner(tool, config_dir, id, enabled)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("切换工具 Skill 失败: {e}")))?
}

#[tauri::command]
async fn toggle_tool_mcp(
    tool: ToolId,
    config_dir: Option<String>,
    id: String,
    enabled: bool,
) -> Result<SkillsMcpState> {
    tauri::async_runtime::spawn_blocking(move || {
        toggle_tool_mcp_inner(tool, config_dir, id, enabled)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("切换工具 MCP 失败: {e}")))?
}

#[tauri::command]
async fn install_tool_skill_zip(
    tool: ToolId,
    config_dir: Option<String>,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<SkillsMcpActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        install_tool_skill_zip_inner(tool, config_dir, file_name, bytes)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("安装工具 Skill ZIP 失败: {e}")))?
}

#[tauri::command]
async fn install_mcp_integration(
    tool: ToolId,
    config_dir: Option<String>,
    input: McpIntegrationInstallInput,
) -> Result<SkillsMcpActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        install_mcp_integration_inner(tool, config_dir, input)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("手动配置 MCP 集成失败: {e}")))?
}

#[tauri::command]
async fn check_tool_skill_updates(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<SkillsMcpState> {
    tauri::async_runtime::spawn_blocking(move || check_tool_skill_updates_inner(tool, config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("检查工具 Skill 更新失败: {e}")))?
}

#[tauri::command]
async fn import_existing_skills_mcp(config_dir: Option<String>) -> Result<SkillsMcpActionResult> {
    tauri::async_runtime::spawn_blocking(move || import_existing_skills_mcp_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("导入已有 Skills/MCP 失败: {e}")))?
}

#[tauri::command]
async fn preview_existing_skills_mcp(config_dir: Option<String>) -> Result<SkillsMcpImportPreview> {
    tauri::async_runtime::spawn_blocking(move || preview_existing_skills_mcp_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("预览已有 Skills/MCP 失败: {e}")))?
}

#[tauri::command]
async fn toggle_codex_skill(
    config_dir: Option<String>,
    id: String,
    enabled: bool,
) -> Result<SkillsMcpState> {
    tauri::async_runtime::spawn_blocking(move || toggle_codex_skill_inner(config_dir, id, enabled))
        .await
        .map_err(|e| CodexxError::Config(format!("切换 Skill 失败: {e}")))?
}

#[tauri::command]
async fn toggle_codex_mcp(
    config_dir: Option<String>,
    id: String,
    enabled: bool,
) -> Result<SkillsMcpState> {
    tauri::async_runtime::spawn_blocking(move || toggle_codex_mcp_inner(config_dir, id, enabled))
        .await
        .map_err(|e| CodexxError::Config(format!("切换 MCP 失败: {e}")))?
}

#[tauri::command]
async fn install_skill_zip(
    config_dir: Option<String>,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<SkillsMcpActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        install_skill_zip_inner(config_dir, file_name, bytes)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("ZIP 安装 Skill 失败: {e}")))?
}

#[tauri::command]
async fn get_skin_center_state() -> Result<SkinCenterState> {
    tauri::async_runtime::spawn_blocking(get_skin_center_state_inner)
        .await
        .map_err(|e| CodexxError::Config(format!("读取皮肤中心失败: {e}")))?
}

#[tauri::command]
async fn enable_skin_theme(id: String, restart_existing: bool) -> Result<SkinActionResult> {
    tauri::async_runtime::spawn_blocking(move || enable_skin_theme_inner(id, restart_existing))
        .await
        .map_err(|e| CodexxError::Config(format!("启用皮肤失败: {e}")))?
}

#[tauri::command]
async fn import_skin_theme_zip(file_name: String, bytes: Vec<u8>) -> Result<SkinActionResult> {
    tauri::async_runtime::spawn_blocking(move || import_skin_theme_zip_inner(file_name, bytes))
        .await
        .map_err(|e| CodexxError::Config(format!("导入皮肤失败: {e}")))?
}

#[tauri::command]
async fn create_skin_theme_from_image(
    file_name: String,
    bytes: Vec<u8>,
) -> Result<SkinActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        create_skin_theme_from_image_inner(file_name, bytes)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("从图片创建皮肤失败: {e}")))?
}

#[tauri::command]
async fn update_skin_theme_settings(
    id: String,
    name: String,
    tagline: String,
    surface_opacity: f64,
) -> Result<SkinActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        update_skin_theme_settings_inner(id, name, tagline, surface_opacity)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("保存皮肤设置失败: {e}")))?
}

#[tauri::command]
async fn export_skin_theme(id: String, destination_path: String) -> Result<SkinExportResult> {
    tauri::async_runtime::spawn_blocking(move || export_skin_theme_inner(id, destination_path))
        .await
        .map_err(|e| CodexxError::Config(format!("导出皮肤失败: {e}")))?
}

#[tauri::command]
async fn pause_skin_theme() -> Result<SkinActionResult> {
    tauri::async_runtime::spawn_blocking(pause_skin_theme_inner)
        .await
        .map_err(|e| CodexxError::Config(format!("暂停皮肤失败: {e}")))?
}

#[tauri::command]
async fn restore_skin_theme(restart_existing: bool) -> Result<SkinActionResult> {
    tauri::async_runtime::spawn_blocking(move || restore_skin_theme_inner(restart_existing))
        .await
        .map_err(|e| CodexxError::Config(format!("恢复官方外观失败: {e}")))?
}

#[tauri::command]
async fn check_skill_updates(config_dir: Option<String>) -> Result<SkillsMcpState> {
    tauri::async_runtime::spawn_blocking(move || check_skill_updates_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("检查 Skill 更新失败: {e}")))?
}

#[tauri::command]
async fn get_startup_diagnostics(config_dir: Option<String>) -> Result<StartupDiagnostics> {
    tauri::async_runtime::spawn_blocking(move || startup_diagnostics_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("启动检测失败: {e}")))?
}

#[tauri::command]
async fn get_session_sync_status(
    config_dir: Option<String>,
    target_provider: Option<String>,
) -> Result<SessionSyncStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        session_sync_status_inner(config_dir, target_provider)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("读取会话状态失败: {e}")))?
}

#[tauri::command]
async fn sync_sessions_provider(
    config_dir: Option<String>,
    target_provider: Option<String>,
) -> Result<SessionSyncResult> {
    tauri::async_runtime::spawn_blocking(move || {
        sync_sessions_provider_inner(config_dir, target_provider)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("同步会话失败: {e}")))?
}

#[tauri::command]
async fn delete_codex_sessions(input: SessionDeleteInput) -> Result<SessionDeleteResult> {
    tauri::async_runtime::spawn_blocking(move || delete_codex_sessions_inner(input))
        .await
        .map_err(|e| CodexxError::Config(format!("永久删除会话失败: {e}")))?
}

#[tauri::command]
async fn read_ccswitch_official_auth(
    db_path: Option<String>,
) -> Result<Option<OfficialAuthCandidate>> {
    tauri::async_runtime::spawn_blocking(move || read_ccswitch_official_auth_inner(db_path))
        .await
        .map_err(|e| CodexxError::Config(format!("读取 cc-switch 官方 Auth 失败: {e}")))?
}

#[tauri::command]
async fn import_ccswitch_codex_providers(db_path: Option<String>) -> Result<ImportResult> {
    tauri::async_runtime::spawn_blocking(move || import_ccswitch_codex_providers_inner(db_path))
        .await
        .map_err(|e| CodexxError::Config(format!("导入 cc-switch Provider 失败: {e}")))?
}

#[tauri::command]
async fn import_ccswitch_providers(tool: ToolId, db_path: Option<String>) -> Result<ImportResult> {
    tauri::async_runtime::spawn_blocking(move || import_ccswitch_providers_inner(tool, db_path))
        .await
        .map_err(|e| CodexxError::Config(format!("导入 cc-switch Provider 失败: {e}")))?
}

fn get_about_info_inner(config_dir: Option<String>) -> Result<AboutInfo> {
    let codex_dir = resolve_codex_dir(config_dir)?;
    #[cfg(target_os = "windows")]
    let native_updater_supported = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        .map(|parent| {
            !parent.join("DevConduit.portable").is_file()
                && !parent.join("Everything-Patch.portable").is_file()
        })
        .unwrap_or(true);
    #[cfg(target_os = "linux")]
    let native_updater_supported = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .is_some_and(|path| path.is_file());
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let native_updater_supported = true;
    Ok(AboutInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        codex_version: platform::detect_codex_version(),
        codex_dir: codex_dir.display().to_string(),
        project_url: "https://github.com/Xdjjw/everything-patch".to_string(),
        github_repo: "Xdjjw/everything-patch".to_string(),
        native_updater_supported,
    })
}

#[tauri::command]
async fn get_about_info(config_dir: Option<String>) -> Result<AboutInfo> {
    tauri::async_runtime::spawn_blocking(move || get_about_info_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("读取关于信息失败: {e}")))?
}

#[tauri::command]
async fn list_saved_prompts() -> Result<Vec<SavedPrompt>> {
    tauri::async_runtime::spawn_blocking(move || list_saved_prompts_inner(ENGINE_CODEX))
        .await
        .map_err(|e| CodexxError::Config(format!("读取提示词列表失败: {e}")))?
}

#[tauri::command]
async fn get_builtin_prompt_status() -> Result<Vec<BuiltinPromptStatus>> {
    tauri::async_runtime::spawn_blocking(builtin_prompt_status_inner)
        .await
        .map_err(|e| CodexxError::Config(format!("读取内置提示词状态失败: {e}")))?
}

#[tauri::command]
async fn refresh_builtin_prompts(config_dir: Option<String>) -> Result<Vec<BuiltinPromptStatus>> {
    tauri::async_runtime::spawn_blocking(move || refresh_builtin_prompts_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("提示词后台更新失败: {e}")))?
}

#[tauri::command]
async fn remember_current_instruction(config_dir: Option<String>) -> Result<Option<SavedPrompt>> {
    tauri::async_runtime::spawn_blocking(move || {
        let codex_dir = resolve_codex_dir(config_dir)?;
        remember_current_instruction_prompt(&codex_dir)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("保存当前外部提示词失败: {e}")))?
}

fn save_prompt_command_inner(prompt: SavedPrompt) -> Result<SavedPrompt> {
    let title = prompt.title.trim().to_string();
    if title.is_empty() {
        return Err(CodexxError::Config("提示词名称不能为空".to_string()));
    }
    let content = prompt.content.trim().to_string();
    if content.is_empty() {
        return Err(CodexxError::Config("提示词内容不能为空".to_string()));
    }
    let id = if prompt.id.trim().is_empty() {
        sanitize_id(&title)
    } else {
        sanitize_id(&prompt.id)
    };
    let filename = normalize_prompt_filename(&prompt.filename, &id);
    save_prompt_inner(
        SavedPrompt {
            id,
            title,
            filename,
            content,
        },
        ENGINE_CODEX,
    )
}

#[tauri::command]
async fn save_prompt(prompt: SavedPrompt) -> Result<SavedPrompt> {
    tauri::async_runtime::spawn_blocking(move || save_prompt_command_inner(prompt))
        .await
        .map_err(|e| CodexxError::Config(format!("保存提示词失败: {e}")))?
}

#[tauri::command]
async fn delete_saved_prompt(id: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || delete_prompt_inner(id.trim(), ENGINE_CODEX))
        .await
        .map_err(|e| CodexxError::Config(format!("删除提示词失败: {e}")))?
}

fn managed_model_instruction_path(codex_dir: &Path, doc: &DocumentMut) -> Result<Option<PathBuf>> {
    let Some(current) = string_value(doc, "model_instructions_file") else {
        return Ok(None);
    };
    if prompt_template_key_for_instruction(&current)?.is_none() {
        return Ok(None);
    }
    Ok(Some(resolve_instruction_path(codex_dir, &current)))
}

#[allow(clippy::too_many_arguments)]
fn enable_prompt_content_inner(
    config_dir: Option<String>,
    filename: &str,
    content: &str,
    template_key: &str,
    title: &str,
    content_source: &str,
    injection_mode: PromptInjectionMode,
    action: &str,
) -> Result<ActionResult> {
    if filename.trim().is_empty()
        || !filename.to_ascii_lowercase().ends_with(".md")
        || filename.contains('/')
        || filename.contains('\\')
    {
        return Err(CodexxError::Config("提示词文件名无效".to_string()));
    }
    if template_key.trim().is_empty() || template_key.contains("-->") {
        return Err(CodexxError::Config("提示词模板标识无效".to_string()));
    }

    let codex_dir = resolve_codex_dir(config_dir)?;
    ensure_directory(&codex_dir)?;
    let cfg = config_path(&codex_dir);
    let agents = agents_path(&codex_dir);
    let text = read_to_string_if_exists(&cfg)?;
    let mut doc = parse_toml_document(&cfg, &text)?;
    let agents_text = read_to_string_if_exists(&agents)?;
    managed_agents_bounds(&agents_text)?;
    let previous_managed_file = managed_model_instruction_path(&codex_dir, &doc)?;
    if injection_mode == PromptInjectionMode::Replace {
        let _ = remember_current_instruction_prompt(&codex_dir);
    }
    let backup_id = create_codex_prompt_backup(&codex_dir, action)?;

    match injection_mode {
        PromptInjectionMode::Replace => {
            if doc.get("model").is_none() {
                doc["model"] = value("gpt-5.5");
            }
            doc["model_instructions_file"] = value(format!("./{filename}"));
            write_text(&codex_dir.join(filename), content)?;
            write_text(&cfg, &doc.to_string())?;
            uninstall_managed_agents_block(&codex_dir)?;
        }
        PromptInjectionMode::Append => {
            install_managed_agents_block(&codex_dir, template_key, content)?;
            if previous_managed_file.is_some() {
                doc.as_table_mut().remove("model_instructions_file");
                write_text(&cfg, &doc.to_string())?;
            }
        }
    }

    if let Some(previous) = previous_managed_file {
        let next = codex_dir.join(filename);
        let should_remove = injection_mode == PromptInjectionMode::Append || previous != next;
        if should_remove && previous.parent() == Some(codex_dir.as_path()) && previous.exists() {
            fs::remove_file(&previous).map_err(|e| io_err(&previous, e))?;
        }
    }

    let state = build_state(codex_dir)?;
    Ok(ActionResult {
        ok: true,
        message: format!(
            "已用{}模式启用 {title}（来源：{content_source}）",
            if injection_mode == PromptInjectionMode::Append {
                "追加"
            } else {
                "替换"
            }
        ),
        backup_id,
        state,
    })
}

fn enable_saved_prompt_inner(
    config_dir: Option<String>,
    id: String,
    injection_mode: Option<String>,
) -> Result<ActionResult> {
    let prompt = get_saved_prompt_inner(id.trim(), ENGINE_CODEX)?;
    let mode = PromptInjectionMode::parse(injection_mode.as_deref())?;
    enable_prompt_content_inner(
        config_dir,
        &prompt.filename,
        &prompt.content,
        &format!("saved:{}", prompt.id),
        &prompt.title,
        "本地自定义",
        mode,
        "enable-custom-prompt",
    )
}

#[tauri::command]
async fn enable_saved_prompt(
    config_dir: Option<String>,
    id: String,
    injection_mode: Option<String>,
) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        enable_saved_prompt_inner(config_dir, id, injection_mode)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("启用自定义提示词失败: {e}")))?
}

#[tauri::command]
async fn list_saved_providers(app_type: Option<String>) -> Result<Vec<SavedProvider>> {
    tauri::async_runtime::spawn_blocking(move || {
        let tool = app_type
            .as_deref()
            .map(ToolId::parse)
            .transpose()?
            .unwrap_or(ToolId::Codex);
        if tool == ToolId::Zcode {
            list_zcode_providers_inner()
        } else {
            list_saved_providers_for_app_inner(tool.as_str())
        }
    })
    .await
    .map_err(|e| CodexxError::Config(format!("读取供应商列表失败: {e}")))?
}

fn save_provider_command_inner(provider: SavedProvider) -> Result<SavedProvider> {
    if ToolId::parse(&provider.app_type)? == ToolId::Zcode {
        return Err(CodexxError::Config(
            "ZCode 原生供应商请在 ZCode 中增删改；DevConduit 只负责读取和切换".to_string(),
        ));
    }
    save_provider_inner(provider)
}

#[tauri::command]
async fn save_provider(provider: SavedProvider) -> Result<SavedProvider> {
    tauri::async_runtime::spawn_blocking(move || save_provider_command_inner(provider))
        .await
        .map_err(|e| CodexxError::Config(format!("保存供应商失败: {e}")))?
}

#[tauri::command]
async fn delete_saved_provider(id: String, app_type: Option<String>) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let tool = app_type
            .as_deref()
            .map(ToolId::parse)
            .transpose()?
            .unwrap_or(ToolId::Codex);
        if tool == ToolId::Zcode {
            return Err(CodexxError::Config(
                "ZCode 原生供应商请在 ZCode 中删除".to_string(),
            ));
        }
        delete_provider_for_app_inner(tool.as_str(), id.trim())
    })
    .await
    .map_err(|e| CodexxError::Config(format!("删除供应商失败: {e}")))?
}

#[tauri::command]
async fn activate_saved_provider(
    tool: ToolId,
    id: String,
    model: Option<String>,
    config_dir: Option<String>,
) -> Result<ToolProviderActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        activate_saved_provider_inner(tool, id.trim(), model, config_dir)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("切换供应商失败: {e}")))?
}

#[tauri::command]
async fn get_codex_state(config_dir: Option<String>) -> Result<CodexState> {
    tauri::async_runtime::spawn_blocking(move || {
        let codex_dir = resolve_codex_dir(config_dir)?;
        build_state(codex_dir)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("读取 Codex 状态失败: {e}")))?
}

#[tauri::command]
async fn switch_official_provider(config_dir: Option<String>) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || switch_official_provider_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("切换官方配置失败: {e}")))?
}

#[tauri::command]
async fn save_official_config(input: OfficialConfigInput) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        save_official_config_inner(input.config_dir, input.model, input.auth_json)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("保存官方配置失败: {e}")))?
}

fn enable_instruction_inner(
    config_dir: Option<String>,
    template_id: &str,
    injection_mode: Option<String>,
) -> Result<ActionResult> {
    let resolved_id = if template_id.trim().is_empty() {
        "gpt5.5-unrestricted"
    } else {
        template_id.trim()
    };
    let (filename, _relative, content, content_source) = builtin_prompt_content(resolved_id)?;
    let mode = PromptInjectionMode::parse(injection_mode.as_deref())?;
    enable_prompt_content_inner(
        config_dir,
        &filename,
        &content,
        &format!("builtin:{resolved_id}"),
        &filename,
        &content_source,
        mode,
        "enable-instruct",
    )
}

#[tauri::command]
async fn enable_instruction(
    config_dir: Option<String>,
    injection_mode: Option<String>,
) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        enable_instruction_inner(config_dir, "gpt5.5-unrestricted", injection_mode)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("启用指令提示词失败: {e}")))?
}

#[tauri::command]
async fn enable_instruction_template(
    config_dir: Option<String>,
    template_id: String,
    injection_mode: Option<String>,
) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        enable_instruction_inner(config_dir, &template_id, injection_mode)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("启用指令提示词失败: {e}")))?
}

fn disable_instruction_inner(
    config_dir: Option<String>,
    delete_file: Option<bool>,
) -> Result<ActionResult> {
    let codex_dir = resolve_codex_dir(config_dir)?;
    let cfg = config_path(&codex_dir);
    let agents_text = read_to_string_if_exists(&agents_path(&codex_dir))?;
    managed_agents_bounds(&agents_text)?;
    let backup_id = create_codex_prompt_backup(&codex_dir, "disable-instruct")?;

    let text = read_to_string_if_exists(&cfg)?;
    let mut doc = parse_toml_document(&cfg, &text)?;
    let current = string_value(&doc, "model_instructions_file");
    let managed_model_path = managed_model_instruction_path(&codex_dir, &doc)?;
    let removed_model = managed_model_path.is_some();
    if removed_model {
        doc.as_table_mut().remove("model_instructions_file");
        write_text(&cfg, &doc.to_string())?;
    }
    let removed_agents = uninstall_managed_agents_block(&codex_dir)?;
    if delete_file.unwrap_or(true) {
        if let Some(md) = managed_model_path {
            if md.parent() == Some(codex_dir.as_path()) && md.exists() {
                fs::remove_file(&md).map_err(|e| io_err(&md, e))?;
            }
        }
    }

    let state = build_state(codex_dir)?;
    let removed = removed_model || removed_agents;
    Ok(ActionResult {
        ok: true,
        message: if removed {
            "已禁用指令提示词".to_string()
        } else if current.is_some() {
            "当前使用的是用户自己的提示词，DevConduit 未做修改".to_string()
        } else {
            "当前没有启用 DevConduit 提示词".to_string()
        },
        backup_id,
        state,
    })
}

#[tauri::command]
async fn disable_instruction(
    config_dir: Option<String>,
    delete_file: Option<bool>,
) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || disable_instruction_inner(config_dir, delete_file))
        .await
        .map_err(|e| CodexxError::Config(format!("禁用指令提示词失败: {e}")))?
}

fn disable_external_instruction_inner(config_dir: Option<String>) -> Result<ActionResult> {
    let codex_dir = resolve_codex_dir(config_dir)?;
    let cfg = config_path(&codex_dir);
    let text = read_to_string_if_exists(&cfg)?;
    let mut doc = parse_toml_document(&cfg, &text)?;
    let current = string_value(&doc, "model_instructions_file");
    if let Some(value) = current.as_deref() {
        if prompt_template_key_for_instruction(value)?.is_some() {
            return Err(CodexxError::Config(
                "当前是 DevConduit 管理的提示词，请使用普通禁用按钮".to_string(),
            ));
        }
    }
    let backup_id = create_codex_prompt_backup(&codex_dir, "disable-external-instruct")?;
    if current.is_some() {
        doc.as_table_mut().remove("model_instructions_file");
        write_text(&cfg, &doc.to_string())?;
    }
    let state = build_state(codex_dir)?;
    Ok(ActionResult {
        ok: true,
        message: if current.is_some() {
            "已禁用用户外部提示词，原 md 文件已保留".to_string()
        } else {
            "当前没有外部提示词".to_string()
        },
        backup_id,
        state,
    })
}

#[tauri::command]
async fn disable_external_instruction(config_dir: Option<String>) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || disable_external_instruction_inner(config_dir))
        .await
        .map_err(|e| CodexxError::Config(format!("禁用外部提示词失败: {e}")))?
}

#[tauri::command]
async fn save_provider_toml_config(input: ProviderTomlInput) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || save_provider_toml_config_inner(input))
        .await
        .map_err(|e| CodexxError::Config(format!("保存供应商 TOML 失败: {e}")))?
}

fn stored_provider_api_key(
    api_key: Option<String>,
    tool: Option<ToolId>,
    provider_id: Option<String>,
) -> Result<Option<String>> {
    let explicit = api_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != tools::REDACTED_VALUE);
    if explicit.is_some() {
        return Ok(explicit);
    }
    let (Some(tool), Some(provider_id)) = (tool, provider_id) else {
        return Ok(None);
    };
    let connection = open_db()?;
    Ok(
        provider_by_id_on_connection_for_app(&connection, tool.as_str(), provider_id.trim())?
            .and_then(|provider| provider.api_key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    )
}

#[tauri::command]
async fn test_provider_connection(
    base_url: String,
    api_key: Option<String>,
    tool: Option<ToolId>,
    provider_id: Option<String>,
) -> Result<ProviderConnectionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let api_key = stored_provider_api_key(api_key, tool, provider_id)?;
        test_provider_connection_inner(base_url, api_key)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("测试连接失败: {e}")))?
}

#[tauri::command]
async fn fetch_provider_models(
    base_url: String,
    api_key: Option<String>,
    tool: Option<ToolId>,
    provider_id: Option<String>,
) -> Result<ProviderModelsResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let api_key = stored_provider_api_key(api_key, tool, provider_id)?;
        fetch_provider_models_inner(base_url, api_key)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("获取模型列表失败: {e}")))?
}

#[tauri::command]
async fn switch_provider(input: ProviderInput) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || switch_provider_inner(input))
        .await
        .map_err(|e| CodexxError::Config(format!("切换供应商失败: {e}")))?
}

#[tauri::command]
async fn list_backups() -> Result<Vec<BackupEntry>> {
    tauri::async_runtime::spawn_blocking(backups)
        .await
        .map_err(|e| CodexxError::Config(format!("读取备份列表失败: {e}")))?
}

fn restore_backup_inner(config_dir: Option<String>, backup_id: String) -> Result<ActionResult> {
    let codex_dir = resolve_codex_dir(config_dir)?;
    let dir = action_backup_root(&codex_dir)?.join(&backup_id);
    if !dir.exists() {
        return Err(CodexxError::Config(format!("备份不存在: {backup_id}")));
    }

    let restore_marker = create_backup(&codex_dir, "before-restore")?;
    let cfg = config_path(&codex_dir);
    let auth = auth_path(&codex_dir);
    let agents = agents_path(&codex_dir);
    ensure_directory(&codex_dir)?;

    let backup_meta = fs::read_to_string(dir.join("meta.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<BackupMeta>(&text).ok());

    let backup_cfg = dir.join("config.toml");
    if backup_cfg.exists() {
        let bytes = fs::read(&backup_cfg).map_err(|e| io_err(&backup_cfg, e))?;
        atomic_write(&cfg, &bytes)?;
    } else if cfg.exists() {
        fs::remove_file(&cfg).map_err(|e| io_err(&cfg, e))?;
    }

    let backup_auth = dir.join("auth.json");
    if backup_auth.exists() {
        let bytes = fs::read(&backup_auth).map_err(|e| io_err(&backup_auth, e))?;
        atomic_write(&auth, &bytes)?;
    } else if auth.exists() {
        fs::remove_file(&auth).map_err(|e| io_err(&auth, e))?;
    }

    if backup_meta.as_ref().is_some_and(|meta| meta.tracks_agents) {
        let backup_agents = dir.join(AGENTS_FILENAME);
        if backup_agents.exists() {
            let bytes = fs::read(&backup_agents).map_err(|e| io_err(&backup_agents, e))?;
            atomic_write(&agents, &bytes)?;
        } else if agents.exists() {
            fs::remove_file(&agents).map_err(|e| io_err(&agents, e))?;
        }
    }

    let state = build_state(codex_dir)?;
    Ok(ActionResult {
        ok: true,
        message: format!("已恢复备份 {backup_id}"),
        backup_id: restore_marker,
        state,
    })
}

#[tauri::command]
async fn restore_backup(config_dir: Option<String>, backup_id: String) -> Result<ActionResult> {
    tauri::async_runtime::spawn_blocking(move || restore_backup_inner(config_dir, backup_id))
        .await
        .map_err(|e| CodexxError::Config(format!("恢复备份失败: {e}")))?
}

#[tauri::command]
async fn list_prompt_backups(
    engine: String,
    config_dir: Option<String>,
) -> Result<Vec<PromptBackupEntry>> {
    tauri::async_runtime::spawn_blocking(move || {
        let codex_dir = if engine.trim().eq_ignore_ascii_case("codex") {
            Some(resolve_codex_dir(config_dir)?)
        } else {
            None
        };
        prompt_backup_entries(&engine, codex_dir.as_deref())
    })
    .await
    .map_err(|e| CodexxError::Config(format!("读取提示词备份失败: {e}")))?
}

fn restore_prompt_backup_inner(
    engine: String,
    config_dir: Option<String>,
    backup_id: String,
) -> Result<PromptRestoreResult> {
    let normalized_engine = engine.trim().to_ascii_lowercase();
    let codex_dir = if normalized_engine == "codex" {
        Some(resolve_codex_dir(config_dir)?)
    } else {
        None
    };
    let restore_marker =
        restore_prompt_backup_snapshot(&normalized_engine, codex_dir.as_deref(), &backup_id)?;
    Ok(PromptRestoreResult {
        ok: true,
        message: format!("已恢复 {} 提示词备份 {backup_id}", normalized_engine),
        backup_id: restore_marker,
        engine: normalized_engine,
    })
}

#[tauri::command]
async fn restore_prompt_backup(
    engine: String,
    config_dir: Option<String>,
    backup_id: String,
) -> Result<PromptRestoreResult> {
    tauri::async_runtime::spawn_blocking(move || {
        restore_prompt_backup_inner(engine, config_dir, backup_id)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("恢复提示词备份失败: {e}")))?
}

// ─── Claude Code 指令管理命令 ─────────────────────────────────────────────
// Claude 不走 config.toml/model_instructions_file；追加模式注入 import-block，
// 替换模式则让受管 import-block 成为 CLAUDE.md 的唯一内容。

#[tauri::command]
async fn get_claude_state() -> Result<ClaudeState> {
    tauri::async_runtime::spawn_blocking(build_claude_state)
        .await
        .map_err(|e| CodexxError::Config(format!("读取 Claude 状态失败: {e}")))?
}

#[tauri::command]
async fn list_claude_prompts() -> Result<Vec<SavedPrompt>> {
    tauri::async_runtime::spawn_blocking(move || list_saved_prompts_inner(ENGINE_CLAUDE))
        .await
        .map_err(|e| CodexxError::Config(format!("读取 Claude 提示词列表失败: {e}")))?
}

#[tauri::command]
async fn get_claude_builtin_prompt_status() -> Result<Vec<BuiltinPromptStatus>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = build_claude_state()?;
        claude_builtin_prompt_status_inner(state.instruction_template_key.as_deref())
    })
    .await
    .map_err(|e| CodexxError::Config(format!("读取 Claude 内置提示词状态失败: {e}")))?
}

fn save_claude_prompt_command_inner(prompt: SavedPrompt) -> Result<SavedPrompt> {
    let title = prompt.title.trim().to_string();
    if title.is_empty() {
        return Err(CodexxError::Config("提示词名称不能为空".to_string()));
    }
    let content = prompt.content.trim().to_string();
    if content.is_empty() {
        return Err(CodexxError::Config("提示词内容不能为空".to_string()));
    }
    let id = if prompt.id.trim().is_empty() {
        sanitize_id(&title)
    } else {
        sanitize_id(&prompt.id)
    };
    let filename = normalize_prompt_filename(&prompt.filename, &id);
    save_prompt_inner(
        SavedPrompt {
            id,
            title,
            filename,
            content,
        },
        ENGINE_CLAUDE,
    )
}

#[tauri::command]
async fn save_claude_prompt(prompt: SavedPrompt) -> Result<SavedPrompt> {
    tauri::async_runtime::spawn_blocking(move || save_claude_prompt_command_inner(prompt))
        .await
        .map_err(|e| CodexxError::Config(format!("保存 Claude 提示词失败: {e}")))?
}

#[tauri::command]
async fn delete_claude_prompt(id: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || delete_prompt_inner(id.trim(), ENGINE_CLAUDE))
        .await
        .map_err(|e| CodexxError::Config(format!("删除 Claude 提示词失败: {e}")))?
}

/// Claude 指令启用的核心逻辑：写 keysmith 指令文件 + 注入 CLAUDE.md 受管区块。
fn enable_claude_prompt_content_inner(
    filename: &str,
    content: &str,
    template_key: &str,
    title: &str,
    content_source: &str,
    injection_mode: PromptInjectionMode,
    action: &str,
) -> Result<ClaudeActionResult> {
    if filename.trim().is_empty()
        || !filename.to_ascii_lowercase().ends_with(".md")
        || filename.contains('/')
        || filename.contains('\\')
    {
        return Err(CodexxError::Config("提示词文件名无效".to_string()));
    }
    if template_key.trim().is_empty() || template_key.contains("-->") {
        return Err(CodexxError::Config("提示词模板标识无效".to_string()));
    }

    let backup_id = create_claude_prompt_backup(action)?;
    install_managed_claude_block(template_key, filename, content, injection_mode)?;

    let state = build_claude_state()?;
    Ok(ClaudeActionResult {
        ok: true,
        message: format!(
            "已用{}模式启用 {title}（来源：{content_source}）",
            if injection_mode == PromptInjectionMode::Append {
                "保留"
            } else {
                "替换"
            }
        ),
        backup_id,
        state,
    })
}

fn enable_claude_instruction_inner(
    template_id: &str,
    injection_mode: Option<String>,
) -> Result<ClaudeActionResult> {
    let resolved_id = if template_id.trim().is_empty() {
        "claude-project-rules"
    } else {
        template_id.trim()
    };
    let (filename, _relative, content, content_source) =
        claude_builtin_prompt_content(resolved_id)?;
    let mode = PromptInjectionMode::parse(injection_mode.as_deref())?;
    enable_claude_prompt_content_inner(
        &filename,
        &content,
        &format!("builtin:{resolved_id}"),
        &filename,
        &content_source,
        mode,
        "enable-claude-instruct",
    )
}

#[tauri::command]
async fn enable_claude_instruction(
    template_id: Option<String>,
    injection_mode: Option<String>,
) -> Result<ClaudeActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        enable_claude_instruction_inner(
            template_id.as_deref().unwrap_or("claude-project-rules"),
            injection_mode,
        )
    })
    .await
    .map_err(|e| CodexxError::Config(format!("启用 Claude 指令失败: {e}")))?
}

fn enable_claude_saved_prompt_inner(
    id: String,
    injection_mode: Option<String>,
) -> Result<ClaudeActionResult> {
    let prompt = get_saved_prompt_inner(id.trim(), ENGINE_CLAUDE)?;
    let mode = PromptInjectionMode::parse(injection_mode.as_deref())?;
    enable_claude_prompt_content_inner(
        &prompt.filename,
        &prompt.content,
        &format!("saved:{}", prompt.id),
        &prompt.title,
        "本地自定义",
        mode,
        "enable-claude-custom-prompt",
    )
}

#[tauri::command]
async fn enable_claude_saved_prompt(
    id: String,
    injection_mode: Option<String>,
) -> Result<ClaudeActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        enable_claude_saved_prompt_inner(id, injection_mode)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("启用 Claude 自定义提示词失败: {e}")))?
}

fn disable_claude_instruction_inner(delete_file: Option<bool>) -> Result<ClaudeActionResult> {
    // 默认（delete_file=true 或未传）完整卸载：移除 CLAUDE.md 受管区块并删除
    // 对应的 keysmith 指令文件。delete_file=false 当前行为一致，仅为保持与
    // Codex disable_instruction 命令签名对齐。
    let _ = delete_file;
    let backup_id = create_claude_prompt_backup("disable-claude-instruct")?;
    let removed = uninstall_managed_claude_block()?;
    let state = build_claude_state()?;
    Ok(ClaudeActionResult {
        ok: true,
        message: if removed {
            "已禁用 Claude 指令提示词".to_string()
        } else {
            "当前没有启用 DevConduit 管理的 Claude 指令".to_string()
        },
        backup_id,
        state,
    })
}

#[tauri::command]
async fn disable_claude_instruction(delete_file: Option<bool>) -> Result<ClaudeActionResult> {
    tauri::async_runtime::spawn_blocking(move || disable_claude_instruction_inner(delete_file))
        .await
        .map_err(|e| CodexxError::Config(format!("禁用 Claude 指令失败: {e}")))?
}

// ─── ZCode App 指令管理命令 ───────────────────────────────────────────────
// ZCode 通过 wrapper + 环境变量注入 system-role.md，不走 config.toml/CLAUDE.md。

#[tauri::command]
async fn get_zcode_state() -> Result<ZcodeState> {
    tauri::async_runtime::spawn_blocking(build_zcode_state)
        .await
        .map_err(|e| CodexxError::Config(format!("读取 ZCode 状态失败: {e}")))?
}

#[tauri::command]
async fn list_zcode_prompts() -> Result<Vec<SavedPrompt>> {
    tauri::async_runtime::spawn_blocking(move || list_saved_prompts_inner(ENGINE_ZCODE))
        .await
        .map_err(|e| CodexxError::Config(format!("读取 ZCode 提示词列表失败: {e}")))?
}

#[tauri::command]
async fn get_zcode_builtin_prompt_status() -> Result<Vec<BuiltinPromptStatus>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = build_zcode_state()?;
        let active = state.instruction_template_key.as_deref();
        Ok(vec![BuiltinPromptStatus {
            id: crate::constants::ZCODE_BUILTIN_ID.to_string(),
            filename: crate::constants::ZCODE_BUILTIN_FILENAME.to_string(),
            title: crate::constants::ZCODE_BUILTIN_TITLE.to_string(),
            subtitle: crate::constants::ZCODE_BUILTIN_SUBTITLE.to_string(),
            badge: crate::constants::ZCODE_BUILTIN_BADGE.to_string(),
            source_url: String::new(),
            cached: false,
            updated: false,
            content_source: "打包内置".to_string(),
            sync_issue: None,
            checked_at: None,
            message: if active == Some("builtin:zcode-system-role") {
                "已启用".to_string()
            } else {
                "未启用".to_string()
            },
        }])
    })
    .await
    .map_err(|e| CodexxError::Config(format!("读取 ZCode 内置提示词状态失败: {e}")))?
}

fn save_zcode_prompt_command_inner(prompt: SavedPrompt) -> Result<SavedPrompt> {
    let title = prompt.title.trim().to_string();
    if title.is_empty() {
        return Err(CodexxError::Config("提示词名称不能为空".to_string()));
    }
    let content = prompt.content.trim().to_string();
    if content.is_empty() {
        return Err(CodexxError::Config("提示词内容不能为空".to_string()));
    }
    let id = if prompt.id.trim().is_empty() {
        sanitize_id(&title)
    } else {
        sanitize_id(&prompt.id)
    };
    let filename = normalize_prompt_filename(&prompt.filename, &id);
    save_prompt_inner(
        SavedPrompt {
            id,
            title,
            filename,
            content,
        },
        ENGINE_ZCODE,
    )
}

#[tauri::command]
async fn save_zcode_prompt(prompt: SavedPrompt) -> Result<SavedPrompt> {
    tauri::async_runtime::spawn_blocking(move || save_zcode_prompt_command_inner(prompt))
        .await
        .map_err(|e| CodexxError::Config(format!("保存 ZCode 提示词失败: {e}")))?
}

#[tauri::command]
async fn delete_zcode_prompt(id: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || delete_prompt_inner(id.trim(), ENGINE_ZCODE))
        .await
        .map_err(|e| CodexxError::Config(format!("删除 ZCode 提示词失败: {e}")))?
}

fn install_zcode_instruction_inner(
    template_id: &str,
    injection_mode: Option<String>,
) -> Result<ZcodeActionResult> {
    let resolved_id = if template_id.trim().is_empty() {
        crate::constants::ZCODE_BUILTIN_ID
    } else {
        template_id.trim()
    };
    let (_filename, _relative, content, content_source) =
        zcode::zcode_builtin_content(resolved_id)?;
    let mode = PromptInjectionMode::parse(injection_mode.as_deref())?;
    let backup_id = create_zcode_prompt_backup("install-zcode-instruct")?;
    zcode::install_zcode(
        &content,
        mode,
        &format!("builtin:{resolved_id}"),
        ZCODE_BUILTIN_TITLE,
    )?;
    let state = build_zcode_state()?;
    Ok(ZcodeActionResult {
        ok: true,
        message: format!(
            "已用{}模式安装 ZCode system-role（来源：{content_source}）",
            if mode == PromptInjectionMode::Append {
                "保留"
            } else {
                "替换"
            }
        ),
        backup_id,
        state,
    })
}

#[tauri::command]
async fn install_zcode_instruction(
    template_id: Option<String>,
    injection_mode: Option<String>,
) -> Result<ZcodeActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        install_zcode_instruction_inner(
            template_id
                .as_deref()
                .unwrap_or(crate::constants::ZCODE_BUILTIN_ID),
            injection_mode,
        )
    })
    .await
    .map_err(|e| CodexxError::Config(format!("安装 ZCode 指令失败: {e}")))?
}

fn install_zcode_saved_prompt_inner(
    id: String,
    injection_mode: Option<String>,
) -> Result<ZcodeActionResult> {
    let prompt = get_saved_prompt_inner(id.trim(), ENGINE_ZCODE)?;
    let mode = PromptInjectionMode::parse(injection_mode.as_deref())?;
    let backup_id = create_zcode_prompt_backup("install-zcode-custom-prompt")?;
    zcode::install_zcode(
        &prompt.content,
        mode,
        &format!("saved:{}", prompt.id),
        &prompt.title,
    )?;
    let state = build_zcode_state()?;
    Ok(ZcodeActionResult {
        ok: true,
        message: format!(
            "已用{}模式安装 ZCode system-role：{}",
            if mode == PromptInjectionMode::Append {
                "保留"
            } else {
                "替换"
            },
            prompt.title
        ),
        backup_id,
        state,
    })
}

#[tauri::command]
async fn install_zcode_saved_prompt(
    id: String,
    injection_mode: Option<String>,
) -> Result<ZcodeActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        install_zcode_saved_prompt_inner(id, injection_mode)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("安装 ZCode 自定义提示词失败: {e}")))?
}

fn uninstall_zcode_instruction_inner() -> Result<ZcodeActionResult> {
    let backup_id = create_zcode_prompt_backup("uninstall-zcode-instruct")?;
    let removed = zcode::uninstall_zcode()?;
    let state = build_zcode_state()?;
    Ok(ZcodeActionResult {
        ok: true,
        message: if removed {
            "已卸载 ZCode 受管入口".to_string()
        } else {
            "当前没有安装 ZCode 受管入口".to_string()
        },
        backup_id,
        state,
    })
}

#[tauri::command]
async fn uninstall_zcode_instruction() -> Result<ZcodeActionResult> {
    tauri::async_runtime::spawn_blocking(uninstall_zcode_instruction_inner)
        .await
        .map_err(|e| CodexxError::Config(format!("卸载 ZCode 指令失败: {e}")))?
}

#[tauri::command]
async fn zcode_doctor() -> Result<ZcodeDoctor> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = build_zcode_state()?;
        let paths = zcode::build_paths()?;
        Ok(ZcodeDoctor {
            managed_dir: state.managed_dir.clone(),
            system_file: state.system_file.clone(),
            system_file_exists: state.system_file_exists,
            launcher_exists: paths.launcher.is_file(),
            zcode_app: state.zcode_app.clone(),
            zcode_runtime_exists: state.zcode_runtime_exists,
            runtime_patchable: state.runtime_patchable,
            agent_override_supported: state.agent_override_supported,
            zcode_running: state.zcode_running,
        })
    })
    .await
    .map_err(|e| CodexxError::Config(format!("ZCode 诊断失败: {e}")))?
}

#[tauri::command]
async fn zcode_verify() -> Result<ZcodeVerify> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = build_zcode_state()?;
        let paths = zcode::build_paths()?;
        Ok(ZcodeVerify {
            system_file_exists: state.system_file_exists,
            launcher_exists: paths.launcher.is_file(),
            zcode_app: state.zcode_app.clone(),
            zcode_runtime_exists: state.zcode_runtime_exists,
            runtime_patchable: state.runtime_patchable,
            agent_override_supported: state.agent_override_supported,
            zcode_running: state.zcode_running,
        })
    })
    .await
    .map_err(|e| CodexxError::Config(format!("ZCode 验证失败: {e}")))?
}

// ─── Grok 指令管理命令 ────────────────────────────────────────────────────

#[tauri::command]
async fn get_grok_state() -> Result<GrokState> {
    tauri::async_runtime::spawn_blocking(build_grok_state)
        .await
        .map_err(|e| CodexxError::Config(format!("Grok 状态查询失败: {e}")))?
}

#[tauri::command]
async fn list_grok_prompts() -> Result<Vec<SavedPrompt>> {
    tauri::async_runtime::spawn_blocking(move || list_saved_prompts_inner(ENGINE_GROK))
        .await
        .map_err(|e| CodexxError::Config(format!("Grok 提示词列表失败: {e}")))?
}

#[tauri::command]
async fn get_grok_builtin_prompt_status() -> Result<Vec<BuiltinPromptStatus>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = build_grok_state()?;
        let active_key = state.instruction_template_key.as_deref();
        Ok(vec![BuiltinPromptStatus {
            id: GROK_BUILTIN_ID.to_string(),
            filename: GROK_BUILTIN_FILENAME.to_string(),
            title: GROK_BUILTIN_TITLE.to_string(),
            subtitle: GROK_BUILTIN_SUBTITLE.to_string(),
            badge: GROK_BUILTIN_BADGE.to_string(),
            source_url: String::new(),
            cached: false,
            updated: false,
            content_source: "打包内置".to_string(),
            sync_issue: None,
            checked_at: None,
            message: if active_key == Some("builtin:grok-unrestricted") {
                "已启用".to_string()
            } else {
                "未启用".to_string()
            },
        }])
    })
    .await
    .map_err(|e| CodexxError::Config(format!("Grok 内置模板状态失败: {e}")))?
}

fn save_grok_prompt_command_inner(prompt: SavedPrompt) -> Result<SavedPrompt> {
    let filename = normalize_prompt_filename(&prompt.filename, GROK_BUILTIN_FILENAME);
    save_prompt_inner(
        SavedPrompt {
            id: prompt.id,
            title: prompt.title,
            filename,
            content: prompt.content,
        },
        ENGINE_GROK,
    )
}

#[tauri::command]
async fn save_grok_prompt(prompt: SavedPrompt) -> Result<SavedPrompt> {
    tauri::async_runtime::spawn_blocking(move || save_grok_prompt_command_inner(prompt))
        .await
        .map_err(|e| CodexxError::Config(format!("保存 Grok 提示词失败: {e}")))?
}

#[tauri::command]
async fn delete_grok_prompt(id: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || delete_prompt_inner(id.trim(), ENGINE_GROK))
        .await
        .map_err(|e| CodexxError::Config(format!("删除 Grok 提示词失败: {e}")))?
}

fn install_grok_instruction_inner(
    template_id: &str,
    injection_mode: Option<String>,
) -> Result<GrokActionResult> {
    let resolved_id = if template_id.trim().is_empty() {
        GROK_BUILTIN_ID
    } else {
        template_id.trim()
    };
    let (_filename, _relative, content, content_source) = grok::grok_builtin_content(resolved_id)?;
    let mode = PromptInjectionMode::parse(injection_mode.as_deref())?;
    let backup_id = create_grok_prompt_backup("install-grok-instruct")?;
    grok::install_grok(
        &content,
        mode,
        &format!("builtin:{resolved_id}"),
        GROK_BUILTIN_TITLE,
    )?;
    let state = build_grok_state()?;
    Ok(GrokActionResult {
        ok: true,
        message: format!(
            "已用{}模式安装 Grok AGENTS.md（来源：{content_source}）",
            if mode == PromptInjectionMode::Append {
                "保留"
            } else {
                "替换"
            }
        ),
        backup_id,
        state,
    })
}

#[tauri::command]
async fn install_grok_instruction(
    template_id: Option<String>,
    injection_mode: Option<String>,
) -> Result<GrokActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        install_grok_instruction_inner(
            template_id.as_deref().unwrap_or(GROK_BUILTIN_ID),
            injection_mode,
        )
    })
    .await
    .map_err(|e| CodexxError::Config(format!("安装 Grok 指令失败: {e}")))?
}

fn install_grok_saved_prompt_inner(
    id: String,
    injection_mode: Option<String>,
) -> Result<GrokActionResult> {
    let prompt = get_saved_prompt_inner(id.trim(), ENGINE_GROK)?;
    let mode = PromptInjectionMode::parse(injection_mode.as_deref())?;
    let backup_id = create_grok_prompt_backup("install-grok-custom-prompt")?;
    grok::install_grok(
        &prompt.content,
        mode,
        &format!("saved:{}", prompt.id),
        &prompt.title,
    )?;
    let state = build_grok_state()?;
    Ok(GrokActionResult {
        ok: true,
        message: format!(
            "已用{}模式安装 Grok AGENTS.md：{}",
            if mode == PromptInjectionMode::Append {
                "保留"
            } else {
                "替换"
            },
            prompt.title
        ),
        backup_id,
        state,
    })
}

#[tauri::command]
async fn install_grok_saved_prompt(
    id: String,
    injection_mode: Option<String>,
) -> Result<GrokActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        install_grok_saved_prompt_inner(id, injection_mode)
    })
    .await
    .map_err(|e| CodexxError::Config(format!("安装 Grok 自定义提示词失败: {e}")))?
}

fn uninstall_grok_instruction_inner() -> Result<GrokActionResult> {
    let backup_id = create_grok_prompt_backup("uninstall-grok-instruct")?;
    let removed = grok::uninstall_grok()?;
    let state = build_grok_state()?;
    Ok(GrokActionResult {
        ok: true,
        message: if removed {
            "已卸载 Grok 受管入口".to_string()
        } else {
            "当前没有安装 Grok 受管入口".to_string()
        },
        backup_id,
        state,
    })
}

#[tauri::command]
async fn uninstall_grok_instruction() -> Result<GrokActionResult> {
    tauri::async_runtime::spawn_blocking(uninstall_grok_instruction_inner)
        .await
        .map_err(|e| CodexxError::Config(format!("卸载 Grok 指令失败: {e}")))?
}

#[tauri::command]
async fn restore_grok_hooks_command() -> Result<GrokActionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let backup_id = create_grok_prompt_backup("restore-grok-hooks")?;
        let restored = grok::restore_grok_hooks()?;
        let state = build_grok_state()?;
        Ok(GrokActionResult {
            ok: true,
            message: format!("已恢复 {restored} 个 Grok hooks"),
            backup_id,
            state,
        })
    })
    .await
    .map_err(|e| CodexxError::Config(format!("恢复 Grok hooks 失败: {e}")))?
}

#[tauri::command]
fn open_url(url: String) -> std::result::Result<(), String> {
    let trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        return Err("URL 为空".to_string());
    }

    // Do not wait for the browser process. On Windows, waiting for `cmd /C start` can
    // visibly freeze the WebView for a few seconds before the default browser appears.
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg(&trimmed).spawn();
        }

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let _ = Command::new("cmd")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["/C", "start", ""])
                .arg(&trimmed)
                .spawn();
        }

        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            let _ = Command::new("xdg-open").arg(&trimmed).spawn();
        }
    });

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_about_info,
            check_app_update,
            get_tool_statuses,
            get_tool_config,
            get_tool_sessions,
            get_skills_mcp_state,
            get_tool_skills_mcp_state,
            preview_tool_skills_mcp_import,
            import_tool_skills_mcp,
            toggle_tool_skill,
            toggle_tool_mcp,
            install_tool_skill_zip,
            install_mcp_integration,
            check_tool_skill_updates,
            preview_existing_skills_mcp,
            import_existing_skills_mcp,
            toggle_codex_skill,
            toggle_codex_mcp,
            install_skill_zip,
            get_skin_center_state,
            enable_skin_theme,
            import_skin_theme_zip,
            create_skin_theme_from_image,
            update_skin_theme_settings,
            export_skin_theme,
            pause_skin_theme,
            restore_skin_theme,
            check_skill_updates,
            get_startup_diagnostics,
            get_session_sync_status,
            sync_sessions_provider,
            delete_codex_sessions,
            read_ccswitch_official_auth,
            import_ccswitch_codex_providers,
            import_ccswitch_providers,
            list_saved_prompts,
            get_builtin_prompt_status,
            refresh_builtin_prompts,
            remember_current_instruction,
            save_prompt,
            delete_saved_prompt,
            enable_saved_prompt,
            list_saved_providers,
            save_provider,
            delete_saved_provider,
            activate_saved_provider,
            get_codex_state,
            switch_official_provider,
            save_official_config,
            enable_instruction,
            enable_instruction_template,
            disable_instruction,
            disable_external_instruction,
            switch_provider,
            save_provider_toml_config,
            test_provider_connection,
            fetch_provider_models,
            list_backups,
            restore_backup,
            list_prompt_backups,
            restore_prompt_backup,
            get_claude_state,
            list_claude_prompts,
            get_claude_builtin_prompt_status,
            save_claude_prompt,
            delete_claude_prompt,
            enable_claude_instruction,
            enable_claude_saved_prompt,
            disable_claude_instruction,
            get_zcode_state,
            list_zcode_prompts,
            get_zcode_builtin_prompt_status,
            save_zcode_prompt,
            delete_zcode_prompt,
            install_zcode_instruction,
            install_zcode_saved_prompt,
            uninstall_zcode_instruction,
            zcode_doctor,
            zcode_verify,
            get_grok_state,
            list_grok_prompts,
            get_grok_builtin_prompt_status,
            save_grok_prompt,
            delete_grok_prompt,
            install_grok_instruction,
            install_grok_saved_prompt,
            uninstall_grok_instruction,
            restore_grok_hooks_command,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DevConduit");
}

#[cfg(test)]
mod tests;
