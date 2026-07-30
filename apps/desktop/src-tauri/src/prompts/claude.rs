//! Claude Code `CLAUDE.md` 受管 import-block 管理。
//!
//! 与 `managed_agents.rs` 的 AGENTS.md BEGIN/END 区块模式对齐，但 Claude 没有
//! `model_instructions_file` 等价物，只有往 CLAUDE.md 注入 import block 这一种
//! 注入方式。指令文件存放在 `~/.claude/keysmith/<name>.md`，import target 形如
//! `@keysmith/<name>.md`。

use crate::constants::{
    CLAUDE_BUILTIN_BADGE, CLAUDE_BUILTIN_CONTENT, CLAUDE_BUILTIN_FILENAME, CLAUDE_BUILTIN_ID,
    CLAUDE_BUILTIN_SUBTITLE, CLAUDE_BUILTIN_TITLE, CLAUDE_HOME_DIRNAME, CLAUDE_KEYSMITH_DIRNAME,
    CLAUDE_MANAGED_BEGIN, CLAUDE_MANAGED_END, CLAUDE_MEMORY_FILENAME, CLAUDE_MODE_PREFIX,
    CLAUDE_TEMPLATE_PREFIX, LEGACY_CLAUDE_MANAGED_BEGIN, LEGACY_CLAUDE_MANAGED_END,
    LEGACY_CLAUDE_TEMPLATE_PREFIX,
};
use crate::error::{CodexxError, Result};
use crate::file_io::{ensure_directory, io_err, read_to_string_if_exists, write_text};
use crate::paths::home_dir;
use crate::prompts::types::{BuiltinPromptStatus, BundledPromptMeta, PromptInjectionMode};
use std::fs;
use std::path::{Path, PathBuf};

/// Claude user scope 根目录：`~/.claude`。
pub(crate) fn claude_home_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(CLAUDE_HOME_DIRNAME))
}

/// Claude 记忆文件：`~/.claude/CLAUDE.md`。
pub(crate) fn claude_memory_path() -> Result<PathBuf> {
    Ok(claude_home_dir()?.join(CLAUDE_MEMORY_FILENAME))
}

/// keysmith 指令文件目录：`~/.claude/keysmith`。
pub(crate) fn claude_keysmith_dir() -> Result<PathBuf> {
    Ok(claude_home_dir()?.join(CLAUDE_KEYSMITH_DIRNAME))
}

/// 某个 keysmith 指令文件的完整路径：`~/.claude/keysmith/<md_filename>`。
pub(crate) fn claude_instruction_file(md_filename: &str) -> Result<PathBuf> {
    Ok(claude_keysmith_dir()?.join(md_filename))
}

/// import target 字符串，写入 CLAUDE.md 受管区块：`@keysmith/<md_filename>`。
fn import_target_for(md_filename: &str) -> String {
    format!("@{}/{}", CLAUDE_KEYSMITH_DIRNAME, md_filename)
}

/// 在 CLAUDE.md 内容中定位受管区块，返回 (start, end_exclusive)。
///
/// 要求恰好 1 个 BEGIN 和 1 个 END 且 BEGIN < END，否则报错；
/// 两者都没有则返回 None。与 `managed_agents_bounds` 一致的健壮性策略。
fn marker_bounds(content: &str, begin: &str, end: &str) -> Result<Option<(usize, usize)>> {
    let begins = content
        .match_indices(begin)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let ends = content
        .match_indices(end)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    if begins.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if begins.len() != 1 || ends.len() != 1 || begins[0] >= ends[0] {
        return Err(CodexxError::Config(
            "CLAUDE.md 中的 DevConduit 受管区块标记不完整或重复，请先修复 BEGIN/END 标记"
                .to_string(),
        ));
    }
    Ok(Some((begins[0], ends[0] + end.len())))
}

pub(crate) fn managed_claude_bounds(content: &str) -> Result<Option<(usize, usize)>> {
    let current = marker_bounds(content, CLAUDE_MANAGED_BEGIN, CLAUDE_MANAGED_END)?;
    let legacy = marker_bounds(
        content,
        LEGACY_CLAUDE_MANAGED_BEGIN,
        LEGACY_CLAUDE_MANAGED_END,
    )?;
    match (current, legacy) {
        (Some(_), Some(_)) => Err(CodexxError::Config(
            "CLAUDE.md 中存在多个 DevConduit 受管区块，请只保留一个".to_string(),
        )),
        (Some(bounds), None) | (None, Some(bounds)) => Ok(Some(bounds)),
        (None, None) => Ok(None),
    }
}

/// 从受管区块内容里提取 template_key。
fn managed_claude_template_key_from_content(content: &str) -> Option<String> {
    let (start, end) = managed_claude_bounds(content).ok().flatten()?;
    content[start..end].lines().find_map(|line| {
        [CLAUDE_TEMPLATE_PREFIX, LEGACY_CLAUDE_TEMPLATE_PREFIX]
            .iter()
            .find_map(|prefix| line.trim().strip_prefix(prefix))
            .and_then(|value| value.strip_suffix("-->"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

/// 读取 CLAUDE.md 并提取当前受管区块的 template_key。
pub(crate) fn managed_claude_template_key() -> Result<Option<String>> {
    let path = claude_memory_path()?;
    let content = read_to_string_if_exists(&path)?;
    Ok(managed_claude_template_key_from_content(&content))
}

fn managed_claude_injection_mode_from_content(content: &str) -> Option<PromptInjectionMode> {
    let (start, end) = managed_claude_bounds(content).ok().flatten()?;
    let mode = content[start..end].lines().find_map(|line| {
        line.trim()
            .strip_prefix(CLAUDE_MODE_PREFIX)
            .and_then(|value| value.strip_suffix("-->"))
            .map(str::trim)
            .and_then(|value| PromptInjectionMode::parse(Some(value)).ok())
    });
    // Older managed blocks were always additive import blocks.
    Some(mode.unwrap_or(PromptInjectionMode::Append))
}

pub(crate) fn managed_claude_injection_mode() -> Result<Option<PromptInjectionMode>> {
    let path = claude_memory_path()?;
    let content = read_to_string_if_exists(&path)?;
    Ok(managed_claude_injection_mode_from_content(&content))
}

/// 从受管区块内容里提取 import target（`@keysmith/<name>.md` 行）。
fn managed_claude_import_target_from_content(content: &str) -> Option<String> {
    let (start, end) = managed_claude_bounds(content).ok().flatten()?;
    content[start..end].lines().find_map(|line| {
        line.trim()
            .strip_prefix('@')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
    })
}

/// 从 import target 反解出 md 文件名（如 `keysmith/foo.md` -> `foo.md`）。
fn md_filename_from_import_target(import_target: &str) -> Option<String> {
    import_target.rsplit('/').next().map(ToString::to_string)
}

pub(crate) fn managed_claude_instruction_filename() -> Result<Option<String>> {
    let path = claude_memory_path()?;
    let content = read_to_string_if_exists(&path)?;
    Ok(managed_claude_import_target_from_content(&content)
        .as_deref()
        .and_then(md_filename_from_import_target))
}

/// 移除受管区块，返回 (剩余内容, 是否移除)。
fn remove_managed_claude_block(content: &str) -> Result<(String, bool)> {
    let Some((start, end)) = managed_claude_bounds(content)? else {
        return Ok((content.to_string(), false));
    };
    let before = content[..start].trim_end();
    let after = content[end..].trim_start();
    let merged = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{}\n", before),
        (true, false) => format!("{}\n", after.trim_end()),
        (false, false) => format!("{}\n\n{}\n", before, after.trim_end()),
    };
    Ok((merged, true))
}

fn compose_claude_memory(base: &str, managed: &str, injection_mode: PromptInjectionMode) -> String {
    match injection_mode {
        PromptInjectionMode::Replace => format!("{managed}\n"),
        PromptInjectionMode::Append if base.trim().is_empty() => format!("{managed}\n"),
        PromptInjectionMode::Append => format!("{}\n\n{managed}\n", base.trim_end()),
    }
}

/// 写入/替换受管区块：先 remove 旧区块，再写入 keysmith 指令文件，最后拼接新区块。
///
/// `template_key` 形如 `builtin:claude-project-rules` 或 `saved:<id>`，
/// `md_filename` 为 keysmith 目录下的文件名（如 `claude-project-rules.md`），
/// `content` 为指令正文。
pub(crate) fn install_managed_claude_block(
    template_key: &str,
    md_filename: &str,
    content: &str,
    injection_mode: PromptInjectionMode,
) -> Result<()> {
    let memory_path = claude_memory_path()?;
    let existing = read_to_string_if_exists(&memory_path)?;
    let previous_filename = managed_claude_import_target_from_content(&existing)
        .as_deref()
        .and_then(md_filename_from_import_target);
    let (base, _) = remove_managed_claude_block(&existing)?;

    // 写入 keysmith 指令文件
    let keysmith_dir = claude_keysmith_dir()?;
    ensure_directory(&keysmith_dir)?;
    let instruction_path = keysmith_dir.join(md_filename);
    write_text(&instruction_path, content)?;

    let import_target = import_target_for(md_filename);
    let managed = format!(
        "{CLAUDE_MANAGED_BEGIN}\n{CLAUDE_TEMPLATE_PREFIX} {template_key} -->\n{CLAUDE_MODE_PREFIX} {} -->\n{import_target}\n{CLAUDE_MANAGED_END}",
        injection_mode.as_str(),
    );
    let next = compose_claude_memory(&base, &managed, injection_mode);
    ensure_directory(memory_path.parent().unwrap_or(Path::new(".")))?;
    write_text(&memory_path, &next)?;

    if let Some(previous) = previous_filename {
        if previous != md_filename {
            let previous_path = claude_instruction_file(&previous)?;
            if previous_path.exists() {
                fs::remove_file(&previous_path).map_err(|e| io_err(&previous_path, e))?;
            }
        }
    }
    Ok(())
}

/// 移除 CLAUDE.md 受管区块，并删除对应的 keysmith 指令文件。
///
/// 返回是否实际移除了区块。若移除后 CLAUDE.md 为空则删除文件，否则写回。
pub(crate) fn uninstall_managed_claude_block() -> Result<bool> {
    let memory_path = claude_memory_path()?;
    let existing = read_to_string_if_exists(&memory_path)?;

    // 先取出 import target，用于清理 keysmith 指令文件
    let import_target = managed_claude_import_target_from_content(&existing);
    let md_filename = import_target
        .as_deref()
        .and_then(md_filename_from_import_target);

    let (next, removed) = remove_managed_claude_block(&existing)?;
    if !removed {
        return Ok(false);
    }

    if next.trim().is_empty() {
        if memory_path.exists() {
            fs::remove_file(&memory_path).map_err(|e| io_err(&memory_path, e))?;
        }
    } else {
        write_text(&memory_path, &next)?;
    }

    // 清理对应的 keysmith 指令文件
    if let Some(filename) = md_filename {
        let instruction_path = claude_instruction_file(&filename)?;
        if instruction_path.exists() {
            fs::remove_file(&instruction_path).map_err(|e| io_err(&instruction_path, e))?;
        }
    }
    Ok(true)
}

/// 唯一的 Claude 内置模板元数据。
pub(crate) fn claude_builtin_prompt_meta() -> BundledPromptMeta {
    BundledPromptMeta {
        id: CLAUDE_BUILTIN_ID,
        filename: CLAUDE_BUILTIN_FILENAME,
        title: CLAUDE_BUILTIN_TITLE,
        subtitle: CLAUDE_BUILTIN_SUBTITLE,
        badge: CLAUDE_BUILTIN_BADGE,
        content: CLAUDE_BUILTIN_CONTENT,
    }
}

/// 返回内置模板的 (filename, relative, content, content_source)。
///
/// Claude 目前不拉取远程目录，content_source 固定为打包内置。
pub(crate) fn claude_builtin_prompt_content(
    template_id: &str,
) -> Result<(String, String, String, String)> {
    let meta = claude_builtin_prompt_meta();
    let id = if template_id.trim().is_empty() {
        CLAUDE_BUILTIN_ID
    } else {
        template_id.trim()
    };
    if id != meta.id {
        return Err(CodexxError::Config(format!("未知的 Claude 内置模板: {id}")));
    }
    Ok((
        meta.filename.to_string(),
        format!("./{}", meta.filename),
        meta.content.to_string(),
        "打包内置".to_string(),
    ))
}

/// Claude 内置模板状态列表（当前仅 1 个）。`active_template_key` 用于标记启用项。
pub(crate) fn claude_builtin_prompt_status_inner(
    active_template_key: Option<&str>,
) -> Result<Vec<BuiltinPromptStatus>> {
    let meta = claude_builtin_prompt_meta();
    let active_id = active_template_key
        .and_then(|key| key.strip_prefix("builtin:"))
        .map(str::trim);
    Ok(vec![BuiltinPromptStatus {
        id: meta.id.to_string(),
        filename: meta.filename.to_string(),
        title: meta.title.to_string(),
        subtitle: meta.subtitle.to_string(),
        badge: meta.badge.to_string(),
        source_url: String::new(),
        cached: false,
        updated: false,
        content_source: "打包内置".to_string(),
        sync_issue: None,
        checked_at: None,
        message: if active_id == Some(meta.id) {
            "已启用".to_string()
        } else {
            "未启用".to_string()
        },
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_managed_block_remains_readable() {
        let content = format!(
            "{LEGACY_CLAUDE_MANAGED_BEGIN}\n{LEGACY_CLAUDE_TEMPLATE_PREFIX} builtin:legacy -->\n@keysmith/legacy.md\n{LEGACY_CLAUDE_MANAGED_END}\n"
        );

        assert!(managed_claude_bounds(&content)
            .expect("parse legacy managed block")
            .is_some());
        assert_eq!(
            managed_claude_template_key_from_content(&content).as_deref(),
            Some("builtin:legacy")
        );
    }

    #[test]
    fn mixed_current_and_legacy_blocks_are_rejected() {
        let content = format!(
            "{CLAUDE_MANAGED_BEGIN}\n{CLAUDE_MANAGED_END}\n{LEGACY_CLAUDE_MANAGED_BEGIN}\n{LEGACY_CLAUDE_MANAGED_END}\n"
        );

        assert!(managed_claude_bounds(&content).is_err());
    }

    #[test]
    fn legacy_block_defaults_to_append_mode() {
        let content = format!(
            "{LEGACY_CLAUDE_MANAGED_BEGIN}\n{LEGACY_CLAUDE_TEMPLATE_PREFIX} builtin:legacy -->\n@keysmith/legacy.md\n{LEGACY_CLAUDE_MANAGED_END}\n"
        );

        assert_eq!(
            managed_claude_injection_mode_from_content(&content),
            Some(PromptInjectionMode::Append)
        );
    }

    #[test]
    fn current_block_reads_replace_mode() {
        let content = format!(
            "{CLAUDE_MANAGED_BEGIN}\n{CLAUDE_TEMPLATE_PREFIX} builtin:test -->\n{CLAUDE_MODE_PREFIX} replace -->\n@keysmith/test.md\n{CLAUDE_MANAGED_END}\n"
        );

        assert_eq!(
            managed_claude_injection_mode_from_content(&content),
            Some(PromptInjectionMode::Replace)
        );
    }

    #[test]
    fn append_mode_preserves_existing_memory() {
        assert_eq!(
            compose_claude_memory(
                "# Existing",
                "<!-- managed -->",
                PromptInjectionMode::Append,
            ),
            "# Existing\n\n<!-- managed -->\n"
        );
    }

    #[test]
    fn replace_mode_discards_existing_memory_from_live_file() {
        assert_eq!(
            compose_claude_memory(
                "# Existing",
                "<!-- managed -->",
                PromptInjectionMode::Replace,
            ),
            "<!-- managed -->\n"
        );
    }
}
