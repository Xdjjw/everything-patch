//! Pi managed global instructions.
//!
//! Pi reads global instructions from `~/.pi/agent/AGENTS.md`. DevConduit keeps
//! a fixed-path snapshot from the first install so prompt switches and
//! uninstall can always return to the original file.

mod mcp_adapter;

pub(crate) use mcp_adapter::{
    ensure_mcp_adapter_installed, mcp_adapter_installed, mcp_config_path,
    rollback_mcp_adapter_install, PiMcpAdapterInstall,
};

use crate::constants::*;
use crate::error::{CodexxError, Result};
use crate::file_io::{
    atomic_write, ensure_directory, io_err, read_to_string_if_exists, write_text,
};
use crate::paths::home_dir;
use crate::prompts::PromptInjectionMode;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct FixedFileSnapshot {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

fn capture_fixed_file(path: PathBuf) -> Result<FixedFileSnapshot> {
    let bytes = match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(CodexxError::Config(format!(
                "Pi 受管路径不是普通文件: {}",
                path.display()
            )));
        }
        Ok(_) => Some(fs::read(&path).map_err(|error| io_err(&path, error))?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_err(&path, error)),
    };
    Ok(FixedFileSnapshot { path, bytes })
}

fn restore_fixed_files(snapshots: &[FixedFileSnapshot]) -> Result<()> {
    let mut failures = Vec::new();
    for snapshot in snapshots.iter().rev() {
        let result = match &snapshot.bytes {
            Some(bytes) => atomic_write(&snapshot.path, bytes),
            None => match fs::symlink_metadata(&snapshot.path) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Err(
                    CodexxError::Config(format!("回滚目标被目录占用: {}", snapshot.path.display())),
                ),
                Ok(_) => {
                    fs::remove_file(&snapshot.path).map_err(|error| io_err(&snapshot.path, error))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(io_err(&snapshot.path, error)),
            },
        };
        if let Err(error) = result {
            failures.push(error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(CodexxError::Config(format!(
            "Pi 文件回滚不完整: {}",
            failures.join("；")
        )))
    }
}

fn finish_with_rollback(result: Result<()>, snapshots: &[FixedFileSnapshot]) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => match restore_fixed_files(snapshots) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CodexxError::Config(format!("{error}；{rollback_error}"))),
        },
    }
}

pub(crate) fn pi_home_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".pi").join("agent"))
}

pub(crate) fn pi_agents_path() -> Result<PathBuf> {
    Ok(pi_home_dir()?.join(PI_AGENTS_FILENAME))
}

pub(crate) fn pi_manifest_path() -> Result<PathBuf> {
    Ok(pi_home_dir()?.join(PI_MANIFEST_FILENAME))
}

pub(crate) fn pi_original_agents_path() -> Result<PathBuf> {
    Ok(pi_home_dir()?.join(PI_ORIGINAL_AGENTS_FILENAME))
}

fn read_manifest_at(path: &Path) -> Result<Option<serde_json::Value>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(CodexxError::Config(format!(
                "Pi manifest 不是普通文件: {}",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_err(path, error)),
    }
    let text = read_to_string_if_exists(path)?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| CodexxError::Config(format!("Pi manifest 解析失败: {error}")))
}

fn read_manifest() -> Result<Option<serde_json::Value>> {
    read_manifest_at(&pi_manifest_path()?)
}

fn manifest_injection_mode(manifest: &serde_json::Value) -> PromptInjectionMode {
    manifest
        .get("injection_mode")
        .and_then(|value| value.as_str())
        .and_then(|value| PromptInjectionMode::parse(Some(value)).ok())
        .unwrap_or(PromptInjectionMode::Replace)
}

fn manifest_original_existed(manifest: &serde_json::Value) -> bool {
    manifest
        .get("original_agents_existed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(crate) fn current_install_metadata(
) -> Result<(Option<PromptInjectionMode>, Option<String>, Option<String>)> {
    let Some(manifest) = read_manifest()? else {
        return Ok((None, None, None));
    };
    Ok((
        Some(manifest_injection_mode(&manifest)),
        manifest
            .get("template_key")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        manifest
            .get("prompt_name")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
    ))
}

fn managed_prompt_bounds(content: &str) -> Result<Option<(usize, usize)>> {
    let begins = content
        .match_indices(PI_PROMPT_BEGIN)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let ends = content
        .match_indices(PI_PROMPT_END)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if begins.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if begins.len() != 1 || ends.len() != 1 || begins[0] >= ends[0] {
        return Err(CodexxError::Config(
            "Pi AGENTS.md 中的 DevConduit 受管区块不完整或重复".to_string(),
        ));
    }
    Ok(Some((begins[0], ends[0] + PI_PROMPT_END.len())))
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
        "{PI_PROMPT_BEGIN}\n{PI_PROMPT_TEMPLATE_PREFIX} {template_key} -->\n{PI_PROMPT_MODE_PREFIX} append -->\n{}\n{PI_PROMPT_END}",
        content.trim_end(),
    );
    Ok(if base.trim().is_empty() {
        format!("{managed}\n")
    } else {
        format!("{}\n\n{managed}\n", base.trim_end())
    })
}

fn original_agents_content(original_existed: bool, snapshot: &Path) -> Result<String> {
    if !original_existed {
        return Ok(String::new());
    }
    if !snapshot.is_file() {
        return Err(CodexxError::Config(format!(
            "Pi 原始 AGENTS.md 快照缺失: {}",
            snapshot.display()
        )));
    }
    read_to_string_if_exists(snapshot)
}

fn install_at(
    pi_dir: &Path,
    content: &str,
    injection_mode: PromptInjectionMode,
    template_key: &str,
    title: &str,
) -> Result<()> {
    if content.trim().is_empty() {
        return Err(CodexxError::Config("Pi 指令内容为空".to_string()));
    }

    ensure_directory(pi_dir)?;
    let agents_path = pi_dir.join(PI_AGENTS_FILENAME);
    let manifest_path = pi_dir.join(PI_MANIFEST_FILENAME);
    let original_path = pi_dir.join(PI_ORIGINAL_AGENTS_FILENAME);
    let snapshots = [
        capture_fixed_file(agents_path.clone())?,
        capture_fixed_file(manifest_path.clone())?,
        capture_fixed_file(original_path.clone())?,
    ];

    let result = (|| {
        let previous_manifest = read_manifest_at(&manifest_path)?;
        let original_existed = previous_manifest
            .as_ref()
            .map(manifest_original_existed)
            .unwrap_or_else(|| agents_path.is_file());
        if previous_manifest.is_none() {
            if original_existed {
                let bytes = fs::read(&agents_path).map_err(|error| io_err(&agents_path, error))?;
                atomic_write(&original_path, &bytes)?;
            } else if original_path.is_file() {
                fs::remove_file(&original_path).map_err(|error| io_err(&original_path, error))?;
            }
        } else if original_existed && !original_path.is_file() {
            return Err(CodexxError::Config(format!(
                "Pi 原始 AGENTS.md 快照缺失，已停止覆盖: {}",
                original_path.display()
            )));
        }

        let previous_mode = previous_manifest.as_ref().map(manifest_injection_mode);
        let existing = if injection_mode == PromptInjectionMode::Append
            && previous_mode == Some(PromptInjectionMode::Replace)
        {
            original_agents_content(original_existed, &original_path)?
        } else {
            read_to_string_if_exists(&agents_path)?
        };
        let next = match injection_mode {
            PromptInjectionMode::Append => render_append_prompt(&existing, content, template_key)?,
            PromptInjectionMode::Replace => format!("{}\n", content.trim_end()),
        };
        write_text(&agents_path, &next)?;

        let manifest = json!({
            "tool": "devconduit-pi",
            "version": 1,
            "deployed_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "prompt_name": title,
            "template_key": template_key,
            "injection_mode": injection_mode.as_str(),
            "original_agents_existed": original_existed,
        });
        write_text(
            &manifest_path,
            &serde_json::to_string_pretty(&manifest)
                .map_err(|error| CodexxError::Config(format!("Pi manifest 序列化失败: {error}")))?,
        )?;
        Ok(())
    })();
    finish_with_rollback(result, &snapshots)
}

pub(crate) fn install_pi(
    content: &str,
    injection_mode: PromptInjectionMode,
    template_key: &str,
    title: &str,
) -> Result<()> {
    install_at(
        &pi_home_dir()?,
        content,
        injection_mode,
        template_key,
        title,
    )
}

fn uninstall_at(pi_dir: &Path) -> Result<bool> {
    let manifest_path = pi_dir.join(PI_MANIFEST_FILENAME);
    if !manifest_path.is_file() {
        return Ok(false);
    }
    let manifest = read_manifest_at(&manifest_path)?
        .ok_or_else(|| CodexxError::Config("Pi manifest 在卸载前消失".to_string()))?;
    let agents_path = pi_dir.join(PI_AGENTS_FILENAME);
    let original_path = pi_dir.join(PI_ORIGINAL_AGENTS_FILENAME);
    let snapshots = [
        capture_fixed_file(agents_path.clone())?,
        capture_fixed_file(manifest_path.clone())?,
        capture_fixed_file(original_path.clone())?,
    ];
    let result = (|| {
        if manifest_original_existed(&manifest) {
            if !original_path.is_file() {
                return Err(CodexxError::Config(format!(
                    "Pi 原始 AGENTS.md 快照缺失，无法安全卸载: {}",
                    original_path.display()
                )));
            }
            let bytes = fs::read(&original_path).map_err(|error| io_err(&original_path, error))?;
            atomic_write(&agents_path, &bytes)?;
        } else if agents_path.exists() {
            fs::remove_file(&agents_path).map_err(|error| io_err(&agents_path, error))?;
        }

        fs::remove_file(&manifest_path).map_err(|error| io_err(&manifest_path, error))?;
        if original_path.is_file() {
            fs::remove_file(&original_path).map_err(|error| io_err(&original_path, error))?;
        }
        Ok(())
    })();
    finish_with_rollback(result, &snapshots)?;
    Ok(true)
}

pub(crate) fn uninstall_pi() -> Result<bool> {
    uninstall_at(&pi_home_dir()?)
}

#[derive(Debug, Clone)]
pub(crate) struct PiStatus {
    pub pi_dir_exists: bool,
    pub agents_md_exists: bool,
    pub manifest_exists: bool,
    pub original_snapshot_exists: bool,
}

pub(crate) fn pi_status() -> Result<PiStatus> {
    Ok(PiStatus {
        pi_dir_exists: pi_home_dir()?.is_dir(),
        agents_md_exists: pi_agents_path()?.is_file(),
        manifest_exists: pi_manifest_path()?.is_file(),
        original_snapshot_exists: pi_original_agents_path()?.is_file(),
    })
}

pub(crate) fn pi_builtin_content(template_id: &str) -> Result<(String, String, String, String)> {
    let id = if template_id.trim().is_empty() {
        PI_BUILTIN_ID
    } else {
        template_id.trim()
    };
    if id != PI_BUILTIN_ID {
        return Err(CodexxError::Config(format!("未知的 Pi 内置模板: {id}")));
    }
    Ok((
        PI_BUILTIN_FILENAME.to_string(),
        format!("./{PI_BUILTIN_FILENAME}"),
        PI_BUILTIN_CONTENT.to_string(),
        "打包内置".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "devconduit-pi-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create Pi test directory");
        path
    }

    #[test]
    fn append_install_and_uninstall_restore_original_agents() {
        let root = temp_dir("restore");
        fs::write(root.join(PI_AGENTS_FILENAME), "# Existing\n").expect("write original");

        install_at(
            &root,
            "# Managed",
            PromptInjectionMode::Append,
            "saved:test",
            "Test",
        )
        .expect("install Pi prompt");
        let installed = fs::read_to_string(root.join(PI_AGENTS_FILENAME)).expect("read installed");
        assert!(installed.contains("# Existing"));
        assert!(installed.contains(PI_PROMPT_BEGIN));
        assert!(installed.contains("# Managed"));

        assert!(uninstall_at(&root).expect("uninstall Pi prompt"));
        assert_eq!(
            fs::read_to_string(root.join(PI_AGENTS_FILENAME)).expect("read restored"),
            "# Existing\n"
        );
        assert!(!root.join(PI_MANIFEST_FILENAME).exists());
        assert!(!root.join(PI_ORIGINAL_AGENTS_FILENAME).exists());
        fs::remove_dir_all(root).expect("remove Pi test directory");
    }

    #[test]
    fn replace_then_append_uses_original_agents_as_base() {
        let root = temp_dir("switch-mode");
        fs::write(root.join(PI_AGENTS_FILENAME), "# Existing\n").expect("write original");

        install_at(
            &root,
            "# Replacement",
            PromptInjectionMode::Replace,
            "saved:replacement",
            "Replacement",
        )
        .expect("replace Pi prompt");
        install_at(
            &root,
            "# Appended",
            PromptInjectionMode::Append,
            "saved:append",
            "Append",
        )
        .expect("append Pi prompt");

        let installed = fs::read_to_string(root.join(PI_AGENTS_FILENAME)).expect("read installed");
        assert!(installed.contains("# Existing"));
        assert!(installed.contains("# Appended"));
        assert!(!installed.contains("# Replacement"));
        fs::remove_dir_all(root).expect("remove Pi test directory");
    }

    #[test]
    fn malformed_managed_markers_are_rejected() {
        let malformed = format!("before\n{PI_PROMPT_BEGIN}\nmissing end\n");
        assert!(remove_managed_prompt_block(&malformed).is_err());
    }

    #[test]
    fn builtin_prompt_is_a_pi_named_copy_of_codex_keysmith() {
        let (_, _, content, _) = pi_builtin_content(PI_BUILTIN_ID).expect("load builtin");
        let expected = CODEX_KEYSMITH_BUILTIN_CONTENT.replacen("Codex operates", "Pi operates", 1);
        assert_eq!(content, expected);
        assert!(content.starts_with("Pi operates"));
        assert!(!content.contains("Codex"));
    }

    #[cfg(unix)]
    #[test]
    fn manifest_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = temp_dir("manifest-symlink");
        let target = root.join("target.json");
        let manifest = root.join(PI_MANIFEST_FILENAME);
        fs::write(&target, "{}\n").expect("write manifest target");
        symlink(&target, &manifest).expect("create manifest symlink");

        assert!(read_manifest_at(&manifest).is_err());
        fs::remove_dir_all(root).expect("remove Pi test directory");
    }
}
