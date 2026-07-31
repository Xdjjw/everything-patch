//! Optional Claude CLI runtime wrapper.
//!
//! The native `CLAUDE.md` import block remains the primary instruction layer.
//! This module adds a separately managed shell wrapper only when the user opts
//! in. It appends the currently managed DevConduit prompt at CLI runtime,
//! without changing Claude settings, credentials, or the Claude executable.

use crate::constants::{
    CLAUDE_HOME_DIRNAME, CLAUDE_RUNTIME_BEGIN, CLAUDE_RUNTIME_DIRNAME, CLAUDE_RUNTIME_END,
    CLAUDE_RUNTIME_PROMPT_FILENAME,
};
use crate::error::{CodexxError, Result};
use crate::file_io::{atomic_write, io_err, read_to_string_if_exists};
use crate::paths::home_dir;
use crate::prompts::{claude_instruction_file, managed_claude_instruction_filename};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const KEY_RUNTIME_PROMPT: &str = "claude-runtime-prompt";
#[cfg(any(target_os = "macos", test))]
const KEY_RUNTIME_ZSHRC: &str = "claude-runtime-zshrc";
#[cfg(any(target_os = "windows", test))]
const KEY_RUNTIME_POWERSHELL: &str = "claude-runtime-powershell";
#[cfg(any(target_os = "windows", test))]
const KEY_RUNTIME_WINDOWS_POWERSHELL: &str = "claude-runtime-windows-powershell";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeRuntimeProfile {
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) managed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeRuntimeState {
    pub(crate) supported: bool,
    pub(crate) platform: String,
    pub(crate) shell: Option<String>,
    pub(crate) prompt_path: String,
    pub(crate) prompt_exists: bool,
    pub(crate) profiles: Vec<ClaudeRuntimeProfile>,
    pub(crate) status: String,
    pub(crate) active: bool,
}

#[derive(Debug, Clone)]
struct RuntimeFileSnapshot {
    path: PathBuf,
    original: Option<Vec<u8>>,
}

pub(crate) fn runtime_prompt_path() -> Result<PathBuf> {
    Ok(home_dir()?
        .join(CLAUDE_HOME_DIRNAME)
        .join(CLAUDE_RUNTIME_DIRNAME)
        .join(CLAUDE_RUNTIME_PROMPT_FILENAME))
}

#[cfg(any(target_os = "macos", test))]
fn macos_runtime_profile_paths(home: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![(KEY_RUNTIME_ZSHRC, home.join(".zshrc"))]
}

#[cfg(any(target_os = "windows", test))]
fn windows_runtime_profile_paths(home: &Path) -> Vec<(&'static str, PathBuf)> {
    let profiles = home.join("Documents");
    vec![
        (
            KEY_RUNTIME_POWERSHELL,
            profiles
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ),
        (
            KEY_RUNTIME_WINDOWS_POWERSHELL,
            profiles
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ),
    ]
}

fn runtime_profile_paths() -> Result<Vec<(&'static str, PathBuf)>> {
    let home = home_dir()?;
    #[cfg(target_os = "macos")]
    {
        Ok(macos_runtime_profile_paths(&home))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(windows_runtime_profile_paths(&home))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = home;
        Ok(Vec::new())
    }
}

fn runtime_platform() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "unsupported"
    }
}

fn runtime_shell() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        Some("zsh".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        Some("PowerShell 7 + Windows PowerShell".to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

fn marker_bounds(content: &str) -> Result<Option<(usize, usize)>> {
    let begins = content
        .match_indices(CLAUDE_RUNTIME_BEGIN)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let ends = content
        .match_indices(CLAUDE_RUNTIME_END)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    if begins.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if begins.len() != 1 || ends.len() != 1 || begins[0] >= ends[0] {
        return Err(CodexxError::Config(
            "Claude runtime 的 DevConduit 标记不完整或重复，请先修复 profile 中的标记".to_string(),
        ));
    }

    let mut end = ends[0] + CLAUDE_RUNTIME_END.len();
    if content[end..].starts_with("\r\n") {
        end += 2;
    } else if content[end..].starts_with('\n') {
        end += 1;
    }
    Ok(Some((begins[0], end)))
}

fn is_managed_profile(path: &Path) -> Result<bool> {
    let content = read_to_string_if_exists(path)?;
    Ok(marker_bounds(&content)?.is_some())
}

#[cfg(any(target_os = "macos", test))]
fn shell_single_quote(value: &Path) -> String {
    format!("'{}'", value.display().to_string().replace('\'', "'\"'\"'"))
}

#[cfg(any(target_os = "windows", test))]
fn powershell_single_quote(value: &Path) -> String {
    format!("'{}'", value.display().to_string().replace('\'', "''"))
}

#[cfg(any(target_os = "macos", test))]
fn render_zsh_wrapper(prompt_path: &Path) -> String {
    let prompt = shell_single_quote(prompt_path);
    format!(
        "{CLAUDE_RUNTIME_BEGIN}\n# Managed by DevConduit. Remove from DevConduit instead of editing this block.\nclaude() {{\n  command claude --append-system-prompt-file {prompt} \"$@\"\n}}\n{CLAUDE_RUNTIME_END}\n"
    )
}

#[cfg(any(target_os = "windows", test))]
fn render_powershell_wrapper(prompt_path: &Path) -> String {
    let prompt = powershell_single_quote(prompt_path);
    format!(
        "{CLAUDE_RUNTIME_BEGIN}\n# Managed by DevConduit. Remove from DevConduit instead of editing this block.\nfunction global:claude {{\n  $devConduitClaude = Get-Command claude -CommandType Application -ErrorAction Stop | Select-Object -First 1\n  & $devConduitClaude.Path --append-system-prompt-file {prompt} @args\n}}\n{CLAUDE_RUNTIME_END}\n"
    )
}

fn render_wrapper(prompt_path: &Path) -> String {
    #[cfg(target_os = "macos")]
    {
        render_zsh_wrapper(prompt_path)
    }
    #[cfg(target_os = "windows")]
    {
        render_powershell_wrapper(prompt_path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = prompt_path;
        String::new()
    }
}

fn upsert_managed_block(content: &str, block: &str) -> Result<String> {
    match marker_bounds(content)? {
        Some((start, end)) => Ok(format!("{}{}{}", &content[..start], block, &content[end..])),
        None if content.is_empty() => Ok(block.to_string()),
        None => {
            let separator = if content.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            Ok(format!("{content}{separator}{block}"))
        }
    }
}

fn remove_managed_block(content: &str) -> Result<(String, bool)> {
    let Some((start, end)) = marker_bounds(content)? else {
        return Ok((content.to_string(), false));
    };
    let prefix = &content[..start];
    let suffix = &content[end..];
    let next = match (prefix.trim_end().is_empty(), suffix.trim_start().is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{}\n", prefix.trim_end()),
        (true, false) => format!("{}\n", suffix.trim_start().trim_end()),
        (false, false) => format!("{}\n\n{}", prefix.trim_end(), suffix.trim_start()),
    };
    Ok((next, true))
}

fn snapshot_paths(paths: impl IntoIterator<Item = PathBuf>) -> Result<Vec<RuntimeFileSnapshot>> {
    paths
        .into_iter()
        .map(|path| {
            if !path.exists() {
                return Ok(RuntimeFileSnapshot {
                    path,
                    original: None,
                });
            }
            if !path.is_file() {
                return Err(CodexxError::Config(format!(
                    "Claude runtime 目标不是普通文件: {}",
                    path.display()
                )));
            }
            Ok(RuntimeFileSnapshot {
                original: Some(fs::read(&path).map_err(|error| io_err(&path, error))?),
                path,
            })
        })
        .collect()
}

fn restore_snapshots(snapshots: &[RuntimeFileSnapshot]) {
    for snapshot in snapshots.iter().rev() {
        match &snapshot.original {
            Some(bytes) => {
                let _ = atomic_write(&snapshot.path, bytes);
            }
            None if snapshot.path.exists() => {
                let _ = fs::remove_file(&snapshot.path);
            }
            None => {}
        }
    }
}

fn write_transaction(updates: &[(PathBuf, Option<Vec<u8>>)]) -> Result<()> {
    let snapshots = snapshot_paths(updates.iter().map(|(path, _)| path.clone()))?;
    for (path, next) in updates {
        let result = match next {
            Some(bytes) => atomic_write(path, bytes),
            None if path.exists() => fs::remove_file(path).map_err(|error| io_err(path, error)),
            None => Ok(()),
        };
        if let Err(error) = result {
            restore_snapshots(&snapshots);
            return Err(error);
        }
    }
    Ok(())
}

fn active_instruction_content() -> Result<String> {
    let filename = managed_claude_instruction_filename()?.ok_or_else(|| {
        CodexxError::Config(
            "请先在 DevConduit 中启用一个 Claude 指令，再安装 CLI runtime".to_string(),
        )
    })?;
    let path = claude_instruction_file(&filename)?;
    let content = read_to_string_if_exists(&path)?;
    if content.trim().is_empty() {
        return Err(CodexxError::Config(format!(
            "当前 Claude 指令文件为空或不存在: {}",
            path.display()
        )));
    }
    Ok(content)
}

pub(crate) fn build_runtime_state() -> Result<ClaudeRuntimeState> {
    let prompt = runtime_prompt_path()?;
    let profiles = runtime_profile_paths()?;
    let mut managed_count = 0;
    let mut needs_repair = false;
    let profile_states = profiles
        .into_iter()
        .map(|(_, path)| {
            let managed = match is_managed_profile(&path) {
                Ok(managed) => managed,
                Err(_) => {
                    needs_repair = true;
                    false
                }
            };
            if managed {
                managed_count += 1;
            }
            Ok(ClaudeRuntimeProfile {
                path: path.display().to_string(),
                exists: path.is_file(),
                managed,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let supported = !profile_states.is_empty();
    let prompt_exists = prompt.is_file();
    let active = supported && prompt_exists && managed_count == profile_states.len();
    let status = if !supported {
        "unsupported"
    } else if needs_repair {
        "needs-repair"
    } else if active {
        "active"
    } else if prompt_exists || managed_count > 0 {
        "partial"
    } else {
        "inactive"
    };

    Ok(ClaudeRuntimeState {
        supported,
        platform: runtime_platform().to_string(),
        shell: runtime_shell(),
        prompt_path: prompt.display().to_string(),
        prompt_exists,
        profiles: profile_states,
        status: status.to_string(),
        active,
    })
}

pub(crate) fn runtime_backup_targets() -> Result<Vec<(String, PathBuf)>> {
    let mut targets = vec![(KEY_RUNTIME_PROMPT.to_string(), runtime_prompt_path()?)];
    targets.extend(
        runtime_profile_paths()?
            .into_iter()
            .map(|(key, path)| (key.to_string(), path)),
    );
    Ok(targets)
}

pub(crate) fn runtime_target_for_backup_key(key: &str) -> Result<Option<PathBuf>> {
    if key == KEY_RUNTIME_PROMPT {
        return Ok(Some(runtime_prompt_path()?));
    }
    Ok(runtime_profile_paths()?
        .into_iter()
        .find_map(|(candidate, path)| (candidate == key).then_some(path)))
}

pub(crate) fn install_runtime() -> Result<()> {
    let profiles = runtime_profile_paths()?;
    if profiles.is_empty() {
        return Err(CodexxError::Config(
            "当前平台不支持 Claude CLI runtime（仅支持 macOS 和 Windows）".to_string(),
        ));
    }
    let prompt_path = runtime_prompt_path()?;
    let prompt_content = active_instruction_content()?;
    let wrapper = render_wrapper(&prompt_path);
    let mut updates = vec![(prompt_path, Some(prompt_content.into_bytes()))];
    for (_, profile) in profiles {
        let current = read_to_string_if_exists(&profile)?;
        let next = upsert_managed_block(&current, &wrapper)?;
        updates.push((profile, Some(next.into_bytes())));
    }
    write_transaction(&updates)
}

pub(crate) fn sync_runtime_prompt_if_active() -> Result<bool> {
    let state = build_runtime_state()?;
    if !state.active {
        return Ok(false);
    }
    let content = active_instruction_content()?;
    atomic_write(&runtime_prompt_path()?, content.as_bytes())?;
    Ok(true)
}

pub(crate) fn uninstall_runtime() -> Result<bool> {
    let profiles = runtime_profile_paths()?;
    if profiles.is_empty() {
        return Ok(false);
    }
    let prompt_path = runtime_prompt_path()?;
    let mut updates = Vec::new();
    let mut removed = prompt_path.is_file();
    if prompt_path.exists() && !prompt_path.is_file() {
        return Err(CodexxError::Config(format!(
            "Claude runtime prompt 不是普通文件: {}",
            prompt_path.display()
        )));
    }
    updates.push((prompt_path, None));
    for (_, profile) in profiles {
        let current = read_to_string_if_exists(&profile)?;
        let (next, changed) = remove_managed_block(&current)?;
        removed |= changed;
        if changed {
            updates.push((profile, Some(next.into_bytes())));
        }
    }
    write_transaction(&updates)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_replaces_only_the_managed_block() {
        let original =
            format!("# user\n\n{CLAUDE_RUNTIME_BEGIN}\nold\n{CLAUDE_RUNTIME_END}\n\n# keep\n");
        let updated = upsert_managed_block(&original, "new block\n").expect("upsert runtime block");
        assert_eq!(updated, "# user\n\nnew block\n\n# keep\n");
    }

    #[test]
    fn remove_preserves_unmanaged_profile_content() {
        let original =
            format!("# user\n\n{CLAUDE_RUNTIME_BEGIN}\nblock\n{CLAUDE_RUNTIME_END}\n\n# keep\n");
        let (updated, removed) = remove_managed_block(&original).expect("remove runtime block");
        assert!(removed);
        assert_eq!(updated, "# user\n\n# keep\n");
    }

    #[test]
    fn malformed_marker_is_rejected() {
        let content = format!("{CLAUDE_RUNTIME_BEGIN}\nmissing end");
        assert!(marker_bounds(&content).is_err());
    }

    #[test]
    fn runtime_profiles_cover_macos_and_both_windows_powershell_families() {
        let home = Path::new("/Users/test");
        assert_eq!(
            macos_runtime_profile_paths(home),
            vec![(KEY_RUNTIME_ZSHRC, home.join(".zshrc"))]
        );

        let windows = windows_runtime_profile_paths(Path::new(r"C:\Users\Test"));
        assert_eq!(windows.len(), 2);
        assert!(windows[0]
            .1
            .ends_with("PowerShell/Microsoft.PowerShell_profile.ps1"));
        assert!(windows[1]
            .1
            .ends_with("WindowsPowerShell/Microsoft.PowerShell_profile.ps1"));
    }

    #[test]
    fn zsh_wrapper_preserves_arguments_and_escapes_prompt_path() {
        let wrapper = render_zsh_wrapper(Path::new("/Users/test/O'Brien/runtime prompt.md"));
        assert!(wrapper.contains("command claude --append-system-prompt-file"));
        assert!(wrapper.contains("'\"'\"'"));
        assert!(wrapper.contains("\"$@\""));
    }

    #[test]
    fn powershell_wrapper_avoids_function_recursion_and_escapes_prompt_path() {
        let wrapper = render_powershell_wrapper(Path::new(r"C:\Users\O'Brien\runtime prompt.md"));
        assert!(wrapper.contains("Get-Command claude -CommandType Application"));
        assert!(wrapper.contains("--append-system-prompt-file"));
        assert!(wrapper.contains("O''Brien"));
        assert!(wrapper.contains("@args"));
    }
}
