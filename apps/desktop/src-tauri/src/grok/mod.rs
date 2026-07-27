//! Grok Build 受管 AGENTS.md 指令部署（跨平台，无平台分发）。
//!
//! 机制：写 `~/.grok/AGENTS.md`（全局 project rules）+ 在 `~/.grok/config.toml`
//! 注入 compat 隔离块（关闭 Claude/Cursor/Codex 兼容层）+ 隔离 hooks/*.json +
//! 写 `.grok-keysmith-manifest.json` 记录部署信息。
//!
//! 仿 `prompts/claude.rs` 模式，单文件模块，不需要 macos/windows 分发。

use crate::constants::*;
use crate::error::{CodexxError, Result};
use crate::file_io::{ensure_directory, io_err, read_to_string_if_exists, write_text};
use crate::paths::home_dir;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

// ─── 路径辅助 ─────────────────────────────────────────────────────────────

/// Grok 配置根目录：`~/.grok`。
pub(crate) fn grok_home_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(GROK_HOME_DIRNAME))
}

/// AGENTS.md 路径：`~/.grok/AGENTS.md`。
pub(crate) fn grok_agents_path() -> Result<PathBuf> {
    Ok(grok_home_dir()?.join(GROK_AGENTS_FILENAME))
}

/// config.toml 路径：`~/.grok/config.toml`。
pub(crate) fn grok_config_path() -> Result<PathBuf> {
    Ok(grok_home_dir()?.join(GROK_CONFIG_FILENAME))
}

/// hooks 目录：`~/.grok/hooks`。
pub(crate) fn grok_hooks_dir() -> Result<PathBuf> {
    Ok(grok_home_dir()?.join(GROK_HOOKS_DIRNAME))
}

/// manifest 路径：`~/.grok/.grok-keysmith-manifest.json`。
pub(crate) fn grok_manifest_path() -> Result<PathBuf> {
    Ok(grok_home_dir()?.join(GROK_MANIFEST_FILENAME))
}

// ─── config.toml compat 块操作 ────────────────────────────────────────────

/// 检查 config.toml 内容是否包含 compat 隔离块。
pub(crate) fn config_has_compat_block(content: &str) -> bool {
    content.contains(GROK_COMPAT_BEGIN_MARKER) && content.contains(GROK_COMPAT_END_MARKER)
}

/// 向 config.toml 内容注入 compat 隔离块。
///
/// 先移除已有的 `[compat.claude]`/`[compat.cursor]`/`[compat.codex]` 段
///（避免 TOML 重复表头），再移除旧 keysmith 块，最后追加带标记的新块。
pub(crate) fn config_add_compat_block(content: &str) -> String {
    let mut text = config_strip_external_compat_sections(content);
    text = config_remove_compat_block(&text);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "\n{}\n{}\n{}\n",
        GROK_COMPAT_BEGIN_MARKER,
        GROK_COMPAT_BLOCK,
        GROK_COMPAT_END_MARKER,
    ));
    text
}

/// 移除 config.toml 内容中的 compat 隔离块（按 BEGIN/END 标记定位）。
pub(crate) fn config_remove_compat_block(content: &str) -> String {
    let mut text = content.to_string();
    loop {
        let Some(begin) = text.find(GROK_COMPAT_BEGIN_MARKER) else {
            break;
        };
        let Some(end) = text.find(GROK_COMPAT_END_MARKER) else {
            break;
        };
        let end_line_end = text[end..].find('\n').map(|i| end + i + 1).unwrap_or(text.len());
        text = format!("{}{}", &text[..begin], &text[end_line_end..]);
    }
    // 清理多余空行
    while text.ends_with("\n\n\n") {
        text.truncate(text.len() - 1);
    }
    text
}

/// 移除已有的 `[compat.claude]`/`[compat.cursor]`/`[compat.codex]` 段。
///
/// 按行扫描：遇到这三个表头时开始跳过，直到遇到下一个表头或 keysmith 标记。
fn config_strip_external_compat_sections(content: &str) -> String {
    let compat_headers = ["[compat.claude]", "[compat.cursor]", "[compat.codex]"];
    let mut out = String::with_capacity(content.len());
    let mut skipping = false;
    for line in content.lines() {
        let stripped = line.trim();
        if skipping {
            // 遇到新表头或 keysmith 标记时停止跳过
            if (stripped.starts_with('[') && (stripped.ends_with(']') || stripped.ends_with("].")))
                || stripped == GROK_COMPAT_BEGIN_MARKER
            {
                skipping = false;
            } else {
                continue;
            }
        }
        if !skipping && compat_headers.contains(&stripped) {
            skipping = true;
            continue;
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    // 折叠多余空行
    while out.contains("\n\n\n\n") {
        out = out.replace("\n\n\n\n", "\n\n\n");
    }
    out
}

// ─── hooks 隔离 ───────────────────────────────────────────────────────────

/// 列出活跃的 hook JSON 文件（`hooks/*.json`，排除 `.disabled`）。
fn list_active_hooks(hooks_dir: &Path) -> Vec<PathBuf> {
    if !hooks_dir.exists() {
        return vec![];
    }
    let mut result: Vec<PathBuf> = fs::read_dir(hooks_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().map_or(false, |ext| ext == "json")
                && !p.file_name().map_or(false, |n| n.to_string_lossy().ends_with(".disabled"))
        })
        .collect();
    result.sort();
    result
}

/// 列出已隔离的 hook 文件（`hooks/*.json.disabled`）。
fn list_disabled_hooks(hooks_dir: &Path) -> Vec<PathBuf> {
    if !hooks_dir.exists() {
        return vec![];
    }
    let mut result: Vec<PathBuf> = fs::read_dir(hooks_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name().map_or(false, |n| n.to_string_lossy().ends_with(".disabled"))
        })
        .collect();
    result.sort();
    result
}

/// 隔离 hooks：将每个 `*.json` 改名为 `*.json.disabled`。
/// 返回 (original, disabled) 对列表。
fn isolate_hooks(hooks_dir: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut pairs = Vec::new();
    for hook in list_active_hooks(hooks_dir) {
        let disabled = hook.with_extension("json.disabled");
        if disabled.exists() {
            // 已存在的 .disabled 文件先归档
            let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let archive = disabled.with_name(format!(
                "{}.keysmith-archive-{}",
                disabled.file_name().and_then(|n| n.to_str()).unwrap_or("hook"),
                ts
            ));
            fs::rename(&disabled, &archive).map_err(|e| io_err(&disabled, e))?;
        }
        fs::rename(&hook, &disabled).map_err(|e| io_err(&hook, e))?;
        pairs.push((hook, disabled));
    }
    Ok(pairs)
}

/// 恢复 hooks：将 `*.json.disabled` 改回 `*.json`。
/// 返回 (disabled, restored) 对列表。
fn restore_hooks_from_pairs(hooks: &[serde_json::Value]) -> Result<usize> {
    let mut restored = 0;
    for h in hooks {
        let disabled_str = h.get("disabled").and_then(|v| v.as_str()).unwrap_or("");
        let original_str = h.get("original").and_then(|v| v.as_str()).unwrap_or("");
        if disabled_str.is_empty() || original_str.is_empty() {
            continue;
        }
        let disabled = PathBuf::from(disabled_str);
        let original = PathBuf::from(original_str);
        if disabled.exists() {
            if original.exists() {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let archive = original.with_name(format!(
                    "{}.uninstall-conflict-{}",
                    original.file_name().and_then(|n| n.to_str()).unwrap_or("hook"),
                    ts
                ));
                fs::rename(&original, &archive).map_err(|e| io_err(&original, e))?;
            }
            fs::rename(&disabled, &original).map_err(|e| io_err(&disabled, e))?;
            restored += 1;
        }
    }
    Ok(restored)
}

/// 恢复所有已隔离的 hooks（独立操作，不依赖 manifest）。
pub(crate) fn restore_grok_hooks() -> Result<usize> {
    let hooks_dir = grok_hooks_dir()?;
    let mut restored = 0;
    for disabled in list_disabled_hooks(&hooks_dir) {
        // foo.json.disabled -> foo.json
        let name = disabled.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let original_name = name.strip_suffix(".disabled").unwrap_or(name);
        let original = disabled.with_file_name(original_name);
        if original.exists() {
            let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let archive = original.with_name(format!(
                "{}.keysmith-conflict-{}",
                original_name, ts
            ));
            fs::rename(&original, &archive).map_err(|e| io_err(&original, e))?;
        }
        fs::rename(&disabled, &original).map_err(|e| io_err(&disabled, e))?;
        restored += 1;
    }
    Ok(restored)
}

// ─── 备份辅助 ─────────────────────────────────────────────────────────────

/// 备份文件（复制为 `.keysmith-backup-时间戳`），返回备份路径。
fn backup_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() || !path.is_file() {
        return Ok(None);
    }
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!(
        "{}.keysmith-backup-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        ts
    );
    let backup_path = path.parent().unwrap_or(Path::new(".")).join(backup_name);
    fs::copy(path, &backup_path).map_err(|e| io_err(path, e))?;
    Ok(Some(backup_path))
}

// ─── 安装 / 卸载 ──────────────────────────────────────────────────────────

/// 安装 Grok 指令：写 AGENTS.md + patch config.toml + 隔离 hooks + 写 manifest。
pub(crate) fn install_grok(content: &str) -> Result<()> {
    let grok_dir = grok_home_dir()?;
    if !grok_dir.exists() {
        return Err(CodexxError::Config(
            "未找到 ~/.grok 目录，请先运行 grok 至少一次".to_string(),
        ));
    }
    if content.trim().is_empty() {
        return Err(CodexxError::Config("Grok 指令内容为空".to_string()));
    }

    let agents_path = grok_agents_path()?;
    let config_path = grok_config_path()?;
    let hooks_dir = grok_hooks_dir()?;
    let manifest_path = grok_manifest_path()?;

    let deploy_id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let deployed_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // 1. 备份 + 写 AGENTS.md
    let agents_backup = backup_file(&agents_path)?;
    write_text(&agents_path, content)?;

    // 2. 备份 + patch config.toml
    let config_backup = backup_file(&config_path)?;
    let config_content = read_to_string_if_exists(&config_path)?;
    let new_config = if config_content.is_empty() {
        format!("{}\n", GROK_COMPAT_BLOCK)
    } else {
        config_add_compat_block(&config_content)
    };
    ensure_directory(config_path.parent().unwrap_or(Path::new(".")))?;
    write_text(&config_path, &new_config)?;

    // 3. 隔离 hooks
    let isolated = isolate_hooks(&hooks_dir)?;
    let hooks_json: Vec<serde_json::Value> = isolated
        .iter()
        .map(|(orig, dis)| {
            json!({
                "original": orig.display().to_string(),
                "disabled": dis.display().to_string(),
            })
        })
        .collect();

    // 4. 写 manifest
    let mut backups_map = serde_json::Map::new();
    if let Some(b) = &agents_backup {
        backups_map.insert("agents_md".to_string(), json!(b.display().to_string()));
    }
    if let Some(b) = &config_backup {
        backups_map.insert("config_toml".to_string(), json!(b.display().to_string()));
    }

    // 归档旧 manifest
    if manifest_path.exists() {
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let archive = manifest_path.with_name(format!(
            "{}.archived-{}",
            GROK_MANIFEST_FILENAME, ts
        ));
        fs::rename(&manifest_path, &archive).map_err(|e| io_err(&manifest_path, e))?;
    }

    let manifest = json!({
        "tool": "grok-keysmith",
        "version": "0.1.1",
        "deployment_id": deploy_id,
        "deployed_at": deployed_at,
        "prompt_source": "codex-x",
        "prompt_name": "grok-unrestricted",
        "agents_md": {
            "path": agents_path.display().to_string(),
            "exists": true,
        },
        "config_toml": {
            "path": config_path.display().to_string(),
            "exists": true,
        },
        "hooks": hooks_json,
        "backups": backups_map,
    });
    write_text(&manifest_path, &serde_json::to_string_pretty(&manifest).unwrap_or_default())?;

    Ok(())
}

/// 卸载 Grok 指令：删 AGENTS.md + 移除 compat 块 + 恢复 hooks + 归档 manifest。
pub(crate) fn uninstall_grok() -> Result<bool> {
    let manifest_path = grok_manifest_path()?;
    if !manifest_path.exists() {
        return Ok(false);
    }

    let manifest_content = read_to_string_if_exists(&manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content)
        .map_err(|e| CodexxError::Config(format!("manifest 解析失败: {e}")))?;

    // 1. 删除 AGENTS.md
    let agents_path = grok_agents_path()?;
    if agents_path.exists() {
        fs::remove_file(&agents_path).map_err(|e| io_err(&agents_path, e))?;
    }

    // 2. 移除 config.toml compat 块
    let config_path = grok_config_path()?;
    if config_path.exists() {
        let content = read_to_string_if_exists(&config_path)?;
        let new_content = config_remove_compat_block(&content);
        write_text(&config_path, &new_content)?;
    }

    // 3. 恢复 hooks
    let hooks = manifest.get("hooks").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let _restored = restore_hooks_from_pairs(&hooks)?;

    // 4. 归档 manifest
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let archive = manifest_path.with_name(format!(
        "{}.uninstalled-{}",
        GROK_MANIFEST_FILENAME, ts
    ));
    fs::rename(&manifest_path, &archive).map_err(|e| io_err(&manifest_path, e))?;

    Ok(true)
}

// ─── 状态查询 ─────────────────────────────────────────────────────────────

/// Grok 状态信息（供 state.rs 使用）。
#[derive(Debug, Clone)]
pub(crate) struct GrokStatus {
    pub grok_dir_exists: bool,
    pub agents_md_exists: bool,
    pub config_toml_exists: bool,
    pub compat_block_injected: bool,
    pub active_hooks_count: usize,
    pub disabled_hooks_count: usize,
    pub manifest_exists: bool,
}

/// 查询 Grok 当前状态。
pub(crate) fn grok_status() -> Result<GrokStatus> {
    let grok_dir = grok_home_dir()?;
    let grok_dir_exists = grok_dir.exists();
    let agents_path = grok_agents_path()?;
    let config_path = grok_config_path()?;
    let hooks_dir = grok_hooks_dir()?;
    let manifest_path = grok_manifest_path()?;

    let agents_md_exists = agents_path.is_file();
    let config_toml_exists = config_path.is_file();
    let config_content = read_to_string_if_exists(&config_path)?;
    let compat_block_injected = config_has_compat_block(&config_content);
    let active_hooks_count = list_active_hooks(&hooks_dir).len();
    let disabled_hooks_count = list_disabled_hooks(&hooks_dir).len();
    let manifest_exists = manifest_path.is_file();

    Ok(GrokStatus {
        grok_dir_exists,
        agents_md_exists,
        config_toml_exists,
        compat_block_injected,
        active_hooks_count,
        disabled_hooks_count,
        manifest_exists,
    })
}

// ─── 内置模板 ─────────────────────────────────────────────────────────────

/// 返回内置模板的 (filename, relative, content, content_source)。
pub(crate) fn grok_builtin_content(template_id: &str) -> Result<(String, String, String, String)> {
    let id = if template_id.trim().is_empty() {
        GROK_BUILTIN_ID
    } else {
        template_id.trim()
    };
    if id != GROK_BUILTIN_ID {
        return Err(CodexxError::Config(format!("未知的 Grok 内置模板: {id}")));
    }
    Ok((
        GROK_BUILTIN_FILENAME.to_string(),
        format!("./{}", GROK_BUILTIN_FILENAME),
        GROK_BUILTIN_CONTENT.to_string(),
        "打包内置".to_string(),
    ))
}
