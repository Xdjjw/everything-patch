use crate::constants::AGENTS_FILENAME;
use crate::error::Result;
use crate::file_io::{ensure_directory, io_err, write_json};
use crate::paths::app_home;
use crate::prompts::agents_path;
use crate::{auth_path, config_path};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupMeta {
    pub(crate) id: String,
    pub(crate) action: String,
    pub(crate) created_at: String,
    pub(crate) codex_dir: String,
    pub(crate) config_path: String,
    pub(crate) auth_path: String,
    pub(crate) had_config: bool,
    pub(crate) had_auth: bool,
    #[serde(default)]
    pub(crate) agents_path: String,
    #[serde(default)]
    pub(crate) had_agents: bool,
    #[serde(default)]
    pub(crate) tracks_agents: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupEntry {
    id: String,
    action: String,
    created_at: String,
    path: String,
    had_config: bool,
    had_auth: bool,
    had_agents: bool,
}

fn backup_root() -> Result<PathBuf> {
    Ok(app_home()?.join("backups"))
}

pub(crate) fn action_backup_root(codex_dir: &Path) -> Result<PathBuf> {
    #[cfg(test)]
    {
        Ok(codex_dir.join(".codexx-test-backups"))
    }
    #[cfg(not(test))]
    {
        let _ = codex_dir;
        backup_root()
    }
}

pub(crate) fn create_backup(codex_dir: &Path, action: &str) -> Result<Option<String>> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static BACKUP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let cfg = config_path(codex_dir);
    let auth = auth_path(codex_dir);
    let agents = agents_path(codex_dir);
    let had_config = cfg.exists();
    let had_auth = auth.exists();
    let had_agents = agents.exists();

    let id = format!(
        "{}-{}-{}",
        Local::now().format("%Y%m%d-%H%M%S-%3f"),
        BACKUP_COUNTER.fetch_add(1, Ordering::Relaxed),
        action
    );
    let dir = action_backup_root(codex_dir)?.join(&id);
    ensure_directory(&dir)?;

    if had_config {
        fs::copy(&cfg, dir.join("config.toml")).map_err(|e| io_err(&cfg, e))?;
    }
    if had_auth {
        fs::copy(&auth, dir.join("auth.json")).map_err(|e| io_err(&auth, e))?;
    }
    if had_agents {
        fs::copy(&agents, dir.join(AGENTS_FILENAME)).map_err(|e| io_err(&agents, e))?;
    }

    let meta = BackupMeta {
        id: id.clone(),
        action: action.to_string(),
        created_at: Local::now().to_rfc3339(),
        codex_dir: codex_dir.display().to_string(),
        config_path: cfg.display().to_string(),
        auth_path: auth.display().to_string(),
        had_config,
        had_auth,
        agents_path: agents.display().to_string(),
        had_agents,
        tracks_agents: true,
    };
    write_json(
        &dir.join("meta.json"),
        &serde_json::to_value(meta).expect("meta serialize"),
    )?;
    Ok(Some(id))
}

fn read_backup_entry(dir: &Path) -> Option<BackupEntry> {
    let meta_path = dir.join("meta.json");
    let text = fs::read_to_string(&meta_path).ok()?;
    let meta: BackupMeta = serde_json::from_str(&text).ok()?;
    Some(BackupEntry {
        id: meta.id,
        action: meta.action,
        created_at: meta.created_at,
        path: dir.display().to_string(),
        had_config: meta.had_config,
        had_auth: meta.had_auth,
        had_agents: meta.had_agents,
    })
}

pub(crate) fn backups() -> Result<Vec<BackupEntry>> {
    let root = backup_root()?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&root).map_err(|e| io_err(&root, e))? {
        let entry = entry.map_err(|e| io_err(&root, e))?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(backup) = read_backup_entry(&path) {
                entries.push(backup);
            }
        }
    }
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(entries)
}

pub(crate) fn latest_backup() -> Result<Option<BackupEntry>> {
    Ok(backups()?.into_iter().next())
}

// ─── Claude 专用备份 ─────────────────────────────────────────────────────
// Claude 只需备份 ~/.claude/CLAUDE.md，与 Codex 的 config.toml/auth.json/AGENTS.md
// 完全无关。为避免污染现有 BackupMeta 结构与 Codex 备份目录，Claude 备份存放在
// 独立的 claude-backups 子目录下，meta 复用 BackupMeta 但仅填 claude 相关字段。

use crate::constants::CLAUDE_MEMORY_FILENAME;

fn claude_backup_root() -> Result<PathBuf> {
    Ok(app_home()?.join("claude-backups"))
}

/// 备份 ~/.claude/CLAUDE.md，返回备份 id（仅在文件存在时返回 Some）。
pub(crate) fn create_claude_backup(action: &str) -> Result<Option<String>> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CLAUDE_BACKUP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let memory = crate::prompts::claude_memory_path()?;
    if !memory.exists() {
        return Ok(None);
    }

    let id = format!(
        "{}-{}-{}",
        Local::now().format("%Y%m%d-%H%M%S-%3f"),
        CLAUDE_BACKUP_COUNTER.fetch_add(1, Ordering::Relaxed),
        action
    );
    let dir = claude_backup_root()?.join(&id);
    ensure_directory(&dir)?;
    fs::copy(&memory, dir.join(CLAUDE_MEMORY_FILENAME)).map_err(|e| io_err(&memory, e))?;

    let meta = BackupMeta {
        id: id.clone(),
        action: action.to_string(),
        created_at: Local::now().to_rfc3339(),
        codex_dir: String::new(),
        config_path: String::new(),
        auth_path: String::new(),
        had_config: false,
        had_auth: false,
        agents_path: memory.display().to_string(),
        had_agents: true,
        tracks_agents: false,
    };
    write_json(
        &dir.join("meta.json"),
        &serde_json::to_value(meta).expect("meta serialize"),
    )?;
    Ok(Some(id))
}

// ─── ZCode 专用备份 ───────────────────────────────────────────────────────
// 备份 ~/.zcode-keysmith/ 下的 system-role.md 和 config.json，存放在独立的
// zcode-backups 子目录。

use crate::constants::{ZCODE_CONFIG_FILENAME, ZCODE_SYSTEM_ROLE_FILENAME};
use crate::constants::{GROK_AGENTS_FILENAME, GROK_CONFIG_FILENAME};

fn zcode_backup_root() -> Result<PathBuf> {
    Ok(app_home()?.join("zcode-backups"))
}

/// 备份 ~/.zcode-keysmith/ 下的受管文件，返回备份 id（仅在文件存在时返回 Some）。
pub(crate) fn create_zcode_backup(action: &str) -> Result<Option<String>> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static ZCODE_BACKUP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let managed_dir = crate::zcode::zcode_managed_dir()?;
    let system_file = managed_dir.join(ZCODE_SYSTEM_ROLE_FILENAME);
    let config_file = managed_dir.join(ZCODE_CONFIG_FILENAME);

    if !system_file.exists() && !config_file.exists() {
        return Ok(None);
    }

    let id = format!(
        "{}-{}-{}",
        Local::now().format("%Y%m%d-%H%M%S-%3f"),
        ZCODE_BACKUP_COUNTER.fetch_add(1, Ordering::Relaxed),
        action
    );
    let dir = zcode_backup_root()?.join(&id);
    ensure_directory(&dir)?;

    if system_file.exists() {
        fs::copy(&system_file, dir.join(ZCODE_SYSTEM_ROLE_FILENAME))
            .map_err(|e| io_err(&system_file, e))?;
    }
    if config_file.exists() {
        fs::copy(&config_file, dir.join(ZCODE_CONFIG_FILENAME))
            .map_err(|e| io_err(&config_file, e))?;
    }

    let meta = BackupMeta {
        id: id.clone(),
        action: action.to_string(),
        created_at: Local::now().to_rfc3339(),
        codex_dir: String::new(),
        config_path: config_file.display().to_string(),
        auth_path: String::new(),
        had_config: config_file.exists(),
        had_auth: false,
        agents_path: system_file.display().to_string(),
        had_agents: system_file.exists(),
        tracks_agents: false,
    };
    write_json(
        &dir.join("meta.json"),
        &serde_json::to_value(meta).expect("meta serialize"),
    )?;
    Ok(Some(id))
}

fn grok_backup_root() -> Result<PathBuf> {
    Ok(app_home()?.join("grok-backups"))
}

/// 备份 ~/.grok/ 下的受管文件，返回备份 id（仅在文件存在时返回 Some）。
pub(crate) fn create_grok_backup(action: &str) -> Result<Option<String>> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static GROK_BACKUP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let grok_dir = crate::grok::grok_home_dir()?;
    let agents_file = grok_dir.join(GROK_AGENTS_FILENAME);
    let config_file = grok_dir.join(GROK_CONFIG_FILENAME);

    if !agents_file.exists() && !config_file.exists() {
        return Ok(None);
    }

    let id = format!(
        "{}-{}-{}",
        Local::now().format("%Y%m%d-%H%M%S-%3f"),
        GROK_BACKUP_COUNTER.fetch_add(1, Ordering::Relaxed),
        action
    );
    let dir = grok_backup_root()?.join(&id);
    ensure_directory(&dir)?;

    if agents_file.exists() {
        fs::copy(&agents_file, dir.join(GROK_AGENTS_FILENAME))
            .map_err(|e| io_err(&agents_file, e))?;
    }
    if config_file.exists() {
        fs::copy(&config_file, dir.join(GROK_CONFIG_FILENAME))
            .map_err(|e| io_err(&config_file, e))?;
    }

    let meta = BackupMeta {
        id: id.clone(),
        action: action.to_string(),
        created_at: Local::now().to_rfc3339(),
        codex_dir: String::new(),
        config_path: config_file.display().to_string(),
        auth_path: String::new(),
        had_config: config_file.exists(),
        had_auth: false,
        agents_path: agents_file.display().to_string(),
        had_agents: agents_file.exists(),
        tracks_agents: false,
    };
    write_json(
        &dir.join("meta.json"),
        &serde_json::to_value(meta).expect("meta serialize"),
    )?;
    Ok(Some(id))
}
