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
use crate::prompts::PromptInjectionMode;
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

fn read_manifest() -> Result<Option<serde_json::Value>> {
    let path = grok_manifest_path()?;
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_to_string_if_exists(&path)?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| CodexxError::Config(format!("manifest 解析失败: {e}")))
}

fn manifest_injection_mode(manifest: &serde_json::Value) -> PromptInjectionMode {
    manifest
        .get("injection_mode")
        .and_then(|value| value.as_str())
        .and_then(|value| PromptInjectionMode::parse(Some(value)).ok())
        // Legacy Grok installs replaced AGENTS.md wholesale.
        .unwrap_or(PromptInjectionMode::Replace)
}

pub(crate) fn current_install_metadata(
) -> Result<(Option<PromptInjectionMode>, Option<String>, Option<String>)> {
    let Some(manifest) = read_manifest()? else {
        return Ok((None, None, None));
    };
    let mode = manifest_injection_mode(&manifest);
    let template_key = manifest
        .get("template_key")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let title = manifest
        .get("prompt_name")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    Ok((Some(mode), template_key, title))
}

// ─── AGENTS.md 受管区块 ──────────────────────────────────────────────────

fn managed_prompt_bounds(content: &str) -> Result<Option<(usize, usize)>> {
    let begins = content
        .match_indices(GROK_PROMPT_BEGIN)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let ends = content
        .match_indices(GROK_PROMPT_END)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if begins.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if begins.len() != 1 || ends.len() != 1 || begins[0] >= ends[0] {
        return Err(CodexxError::Config(
            "Grok AGENTS.md 中的 Everything Patch 受管区块不完整或重复".to_string(),
        ));
    }
    Ok(Some((begins[0], ends[0] + GROK_PROMPT_END.len())))
}

fn remove_managed_prompt_block(content: &str) -> Result<(String, bool)> {
    let Some((start, end)) = managed_prompt_bounds(content)? else {
        return Ok((content.to_string(), false));
    };
    let before = content[..start].trim_end();
    let after = content[end..].trim_start();
    let merged = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{before}\n"),
        (true, false) => format!("{}\n", after.trim_end()),
        (false, false) => format!("{before}\n\n{}\n", after.trim_end()),
    };
    Ok((merged, true))
}

fn render_append_prompt(existing: &str, content: &str, template_key: &str) -> Result<String> {
    let (base, _) = remove_managed_prompt_block(existing)?;
    let managed = format!(
        "{GROK_PROMPT_BEGIN}\n{GROK_PROMPT_TEMPLATE_PREFIX} {template_key} -->\n{GROK_PROMPT_MODE_PREFIX} append -->\n{}\n{GROK_PROMPT_END}",
        content.trim_end(),
    );
    Ok(if base.trim().is_empty() {
        format!("{managed}\n")
    } else {
        format!("{}\n\n{managed}\n", base.trim_end())
    })
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
        GROK_COMPAT_BEGIN_MARKER, GROK_COMPAT_BLOCK, GROK_COMPAT_END_MARKER,
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
        let end_line_end = text[end..]
            .find('\n')
            .map(|i| end + i + 1)
            .unwrap_or(text.len());
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
                && !p
                    .file_name()
                    .map_or(false, |n| n.to_string_lossy().ends_with(".disabled"))
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
                && p.file_name()
                    .map_or(false, |n| n.to_string_lossy().ends_with(".disabled"))
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
            let archive = disabled.with_file_name(format!(
                "{}.keysmith-archive-{}",
                disabled
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("hook"),
                ts
            ));
            fs::rename(&disabled, &archive).map_err(|e| io_err(&disabled, e))?;
        }
        fs::rename(&hook, &disabled).map_err(|e| io_err(&hook, e))?;
        pairs.push((hook, disabled));
    }
    Ok(pairs)
}

fn hook_pair_paths(hook: &serde_json::Value, hooks_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let original = PathBuf::from(hook.get("original")?.as_str()?);
    let disabled = PathBuf::from(hook.get("disabled")?.as_str()?);
    if original.parent() != Some(hooks_dir) || disabled.parent() != Some(hooks_dir) {
        return None;
    }
    let original_name = original.file_name()?.to_str()?;
    let disabled_name = disabled.file_name()?.to_str()?;
    if !original_name.ends_with(".json") || disabled_name != format!("{original_name}.disabled") {
        return None;
    }
    Some((original, disabled))
}

/// 恢复 hooks：将 `*.json.disabled` 改回 `*.json`。
/// 返回 (disabled, restored) 对列表。
fn restore_hooks_from_pairs(hooks: &[serde_json::Value]) -> Result<usize> {
    let hooks_dir = grok_hooks_dir()?;
    let mut restored = 0;
    for h in hooks {
        let Some((original, disabled)) = hook_pair_paths(h, &hooks_dir) else {
            continue;
        };
        if disabled.exists() {
            if original.exists() {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let archive = original.with_file_name(format!(
                    "{}.uninstall-conflict-{}",
                    original
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("hook"),
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

fn isolate_hooks_from_pairs(hooks: &[serde_json::Value]) -> Result<usize> {
    let hooks_dir = grok_hooks_dir()?;
    let mut isolated = 0;
    for hook in hooks {
        let Some((original, disabled)) = hook_pair_paths(hook, &hooks_dir) else {
            continue;
        };
        if disabled.exists() {
            if original.exists() {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let archive = original.with_file_name(format!(
                    "{}.restore-conflict-{}",
                    original
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("hook"),
                    ts
                ));
                fs::rename(&original, &archive).map_err(|e| io_err(&original, e))?;
            }
            continue;
        }
        if original.exists() {
            fs::rename(&original, &disabled).map_err(|e| io_err(&original, e))?;
            isolated += 1;
        }
    }
    Ok(isolated)
}

pub(crate) fn prepare_prompt_backup_restore() -> Result<()> {
    let Some(manifest) = read_manifest()? else {
        return Ok(());
    };
    let hooks = manifest
        .get("hooks")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    restore_hooks_from_pairs(&hooks)?;
    Ok(())
}

pub(crate) fn finalize_prompt_backup_restore() -> Result<()> {
    let Some(manifest) = read_manifest()? else {
        return Ok(());
    };
    let hooks = manifest
        .get("hooks")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    isolate_hooks_from_pairs(&hooks)?;
    Ok(())
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
            let archive =
                original.with_file_name(format!("{}.keysmith-conflict-{}", original_name, ts));
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

fn backup_path_from_manifest(manifest: &serde_json::Value, key: &str) -> Option<PathBuf> {
    manifest
        .get("backups")
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
}

fn original_existed(manifest: &serde_json::Value, key: &str) -> bool {
    manifest
        .get("originals")
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| backup_path_from_manifest(manifest, key).is_some())
}

fn restore_original_file(manifest: &serde_json::Value, key: &str, target: &Path) -> Result<()> {
    if original_existed(manifest, key) {
        let backup = backup_path_from_manifest(manifest, key)
            .ok_or_else(|| CodexxError::Config(format!("Grok 原文件备份缺失: {key}")))?;
        if !backup.is_file() {
            return Err(CodexxError::Config(format!(
                "Grok 原文件备份不存在: {}",
                backup.display()
            )));
        }
        let bytes = fs::read(&backup).map_err(|e| io_err(&backup, e))?;
        crate::file_io::atomic_write(target, &bytes)?;
    } else if target.exists() {
        fs::remove_file(target).map_err(|e| io_err(target, e))?;
    }
    Ok(())
}

/// 安装 Grok 指令：写 AGENTS.md + patch config.toml + 隔离 hooks + 写 manifest。
pub(crate) fn install_grok(
    content: &str,
    injection_mode: PromptInjectionMode,
    template_key: &str,
    title: &str,
) -> Result<()> {
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
    let previous_manifest = read_manifest()?;

    let deploy_id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let deployed_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let original_agents_existed = previous_manifest
        .as_ref()
        .map(|manifest| original_existed(manifest, "agents_md"))
        .unwrap_or_else(|| agents_path.is_file());
    let original_config_existed = previous_manifest
        .as_ref()
        .map(|manifest| original_existed(manifest, "config_toml"))
        .unwrap_or_else(|| config_path.is_file());

    // The first install owns the original backups. Later prompt switches keep them.
    let previous_agents_backup = previous_manifest
        .as_ref()
        .and_then(|manifest| backup_path_from_manifest(manifest, "agents_md"));
    let agents_backup = match previous_agents_backup {
        Some(path) => Some(path),
        None => backup_file(&agents_path)?,
    };
    let previous_config_backup = previous_manifest
        .as_ref()
        .and_then(|manifest| backup_path_from_manifest(manifest, "config_toml"));
    let config_backup = match previous_config_backup {
        Some(path) => Some(path),
        None => backup_file(&config_path)?,
    };

    // 1. 写 AGENTS.md
    let previous_mode = previous_manifest.as_ref().map(manifest_injection_mode);
    let existing_agents = if injection_mode == PromptInjectionMode::Append
        && previous_mode == Some(PromptInjectionMode::Replace)
    {
        match agents_backup.as_ref().filter(|_| original_agents_existed) {
            Some(path) => read_to_string_if_exists(path)?,
            None => String::new(),
        }
    } else {
        read_to_string_if_exists(&agents_path)?
    };
    let next_agents = match injection_mode {
        PromptInjectionMode::Append => {
            render_append_prompt(&existing_agents, content, template_key)?
        }
        PromptInjectionMode::Replace => format!("{}\n", content.trim_end()),
    };
    write_text(&agents_path, &next_agents)?;

    // 2. patch config.toml
    let config_content = read_to_string_if_exists(&config_path)?;
    let new_config = config_add_compat_block(&config_content);
    ensure_directory(config_path.parent().unwrap_or(Path::new(".")))?;
    write_text(&config_path, &new_config)?;

    // 3. 隔离 hooks
    let isolated = isolate_hooks(&hooks_dir)?;
    let mut hooks_json = previous_manifest
        .as_ref()
        .and_then(|manifest| manifest.get("hooks"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let new_hooks: Vec<serde_json::Value> = isolated
        .iter()
        .map(|(orig, dis)| {
            json!({
                "original": orig.display().to_string(),
                "disabled": dis.display().to_string(),
            })
        })
        .collect();
    for hook in new_hooks {
        let original = hook.get("original").and_then(|value| value.as_str());
        if !hooks_json
            .iter()
            .any(|item| item.get("original").and_then(|value| value.as_str()) == original)
        {
            hooks_json.push(hook);
        }
    }

    // 4. 写 manifest
    let mut backups_map = serde_json::Map::new();
    if let Some(b) = &agents_backup {
        backups_map.insert("agents_md".to_string(), json!(b.display().to_string()));
    }
    if let Some(b) = &config_backup {
        backups_map.insert("config_toml".to_string(), json!(b.display().to_string()));
    }

    // 归档旧 manifest，但保留原文件备份与 hooks 恢复关系。
    if manifest_path.exists() {
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let archive =
            manifest_path.with_file_name(format!("{}.archived-{}", GROK_MANIFEST_FILENAME, ts));
        fs::copy(&manifest_path, &archive).map_err(|e| io_err(&manifest_path, e))?;
    }

    let manifest = json!({
        "tool": "grok-keysmith",
        "version": "0.2.0",
        "deployment_id": deploy_id,
        "deployed_at": deployed_at,
        "prompt_source": "everything-patch",
        "prompt_name": title,
        "template_key": template_key,
        "injection_mode": injection_mode.as_str(),
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
        "originals": {
            "agents_md": original_agents_existed,
            "config_toml": original_config_existed,
        },
    });
    write_text(
        &manifest_path,
        &serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )?;

    Ok(())
}

/// 卸载 Grok 指令并恢复安装前状态。
pub(crate) fn uninstall_grok() -> Result<bool> {
    let manifest_path = grok_manifest_path()?;
    if !manifest_path.exists() {
        return Ok(false);
    }

    let manifest = read_manifest()?
        .ok_or_else(|| CodexxError::Config("Grok manifest 在卸载前消失".to_string()))?;
    let injection_mode = manifest_injection_mode(&manifest);

    // 1. 追加模式只移除受管区块，替换模式恢复首个原文件快照。
    let agents_path = grok_agents_path()?;
    if injection_mode == PromptInjectionMode::Append && agents_path.is_file() {
        let current = read_to_string_if_exists(&agents_path)?;
        let (next, removed) = remove_managed_prompt_block(&current)?;
        if removed {
            if next.trim().is_empty() && !original_existed(&manifest, "agents_md") {
                fs::remove_file(&agents_path).map_err(|e| io_err(&agents_path, e))?;
            } else {
                write_text(&agents_path, &next)?;
            }
        } else {
            restore_original_file(&manifest, "agents_md", &agents_path)?;
        }
    } else {
        restore_original_file(&manifest, "agents_md", &agents_path)?;
    }

    // 2. config compat 注入会替换同名表，必须恢复原始快照。
    let config_path = grok_config_path()?;
    restore_original_file(&manifest, "config_toml", &config_path)?;

    // 3. 恢复 hooks
    let hooks = manifest
        .get("hooks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let _restored = restore_hooks_from_pairs(&hooks)?;

    // 4. 归档 manifest
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let archive =
        manifest_path.with_file_name(format!("{}.uninstalled-{}", GROK_MANIFEST_FILENAME, ts));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_prompt_preserves_existing_content() {
        let rendered =
            render_append_prompt("# Existing rules\n", "# Managed rules\n", "saved:test")
                .expect("render append prompt");

        assert!(rendered.starts_with("# Existing rules\n\n"));
        assert!(rendered.contains(GROK_PROMPT_BEGIN));
        assert!(rendered.contains("saved:test"));
        assert!(rendered.contains("# Managed rules"));
    }

    #[test]
    fn replacing_append_block_keeps_single_managed_block() {
        let first = render_append_prompt("", "first", "saved:first").expect("render first prompt");
        let second =
            render_append_prompt(&first, "second", "saved:second").expect("render second prompt");

        assert_eq!(second.matches(GROK_PROMPT_BEGIN).count(), 1);
        assert!(!second.contains("first"));
        assert!(second.contains("second"));
    }

    #[test]
    fn removing_append_block_restores_original_content() {
        let rendered = render_append_prompt("original\n", "managed", "saved:test")
            .expect("render append prompt");
        let (restored, removed) =
            remove_managed_prompt_block(&rendered).expect("remove managed block");

        assert!(removed);
        assert_eq!(restored, "original\n");
    }

    #[test]
    fn legacy_manifest_defaults_to_replace() {
        let manifest = json!({ "version": "0.1.1" });
        assert_eq!(
            manifest_injection_mode(&manifest),
            PromptInjectionMode::Replace
        );
    }
}
