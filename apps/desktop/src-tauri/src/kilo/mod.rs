//! Kilo Code managed global instructions.
//!
//! Kilo reads global instructions from `~/.config/kilo/AGENTS.md`. DevConduit
//! keeps a fixed-path snapshot from the first install so prompt switches and
//! uninstall can always return to the original file.

use crate::constants::*;
use crate::error::{CodexxError, Result};
use crate::file_io::{
    atomic_write, ensure_directory, io_err, read_to_string_if_exists, write_text,
};
use crate::paths::home_dir;
use crate::prompts::PromptInjectionMode;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
struct FixedFileSnapshot {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

fn capture_fixed_file(path: PathBuf) -> Result<FixedFileSnapshot> {
    let bytes = match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(CodexxError::Config(format!(
                "Kilo 受管路径不是普通文件: {}",
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
            "Kilo 文件回滚不完整: {}",
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

pub(crate) fn kilo_home_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config").join("kilo"))
}

pub(crate) fn kilo_agents_path() -> Result<PathBuf> {
    Ok(kilo_home_dir()?.join(KILO_AGENTS_FILENAME))
}

pub(crate) fn kilo_manifest_path() -> Result<PathBuf> {
    Ok(kilo_home_dir()?.join(KILO_MANIFEST_FILENAME))
}

pub(crate) fn kilo_original_agents_path() -> Result<PathBuf> {
    Ok(kilo_home_dir()?.join(KILO_ORIGINAL_AGENTS_FILENAME))
}

fn read_manifest() -> Result<Option<serde_json::Value>> {
    let path = kilo_manifest_path()?;
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_to_string_if_exists(&path)?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| CodexxError::Config(format!("Kilo manifest 解析失败: {error}")))
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
        .match_indices(KILO_PROMPT_BEGIN)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let ends = content
        .match_indices(KILO_PROMPT_END)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if begins.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if begins.len() != 1 || ends.len() != 1 || begins[0] >= ends[0] {
        return Err(CodexxError::Config(
            "Kilo AGENTS.md 中的 DevConduit 受管区块不完整或重复".to_string(),
        ));
    }
    Ok(Some((begins[0], ends[0] + KILO_PROMPT_END.len())))
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
        "{KILO_PROMPT_BEGIN}\n{KILO_PROMPT_TEMPLATE_PREFIX} {template_key} -->\n{KILO_PROMPT_MODE_PREFIX} append -->\n{}\n{KILO_PROMPT_END}",
        content.trim_end(),
    );
    Ok(if base.trim().is_empty() {
        format!("{managed}\n")
    } else {
        format!("{}\n\n{managed}\n", base.trim_end())
    })
}

fn original_agents_content(original_existed: bool, snapshot: &PathBuf) -> Result<String> {
    if !original_existed {
        return Ok(String::new());
    }
    if !snapshot.is_file() {
        return Err(CodexxError::Config(format!(
            "Kilo 原始 AGENTS.md 快照缺失: {}",
            snapshot.display()
        )));
    }
    read_to_string_if_exists(snapshot)
}

pub(crate) fn install_kilo(
    content: &str,
    injection_mode: PromptInjectionMode,
    template_key: &str,
    title: &str,
) -> Result<()> {
    if content.trim().is_empty() {
        return Err(CodexxError::Config("Kilo 指令内容为空".to_string()));
    }

    let kilo_dir = kilo_home_dir()?;
    ensure_directory(&kilo_dir)?;
    let agents_path = kilo_agents_path()?;
    let manifest_path = kilo_manifest_path()?;
    let original_path = kilo_original_agents_path()?;
    let snapshots = [
        capture_fixed_file(agents_path.clone())?,
        capture_fixed_file(manifest_path.clone())?,
        capture_fixed_file(original_path.clone())?,
    ];

    let result = (|| {
        let previous_manifest = read_manifest()?;
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
                "Kilo 原始 AGENTS.md 快照缺失，已停止覆盖: {}",
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
            "tool": "devconduit-kilo",
            "version": 1,
            "deployed_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "prompt_name": title,
            "template_key": template_key,
            "injection_mode": injection_mode.as_str(),
            "original_agents_existed": original_existed,
        });
        write_text(
            &manifest_path,
            &serde_json::to_string_pretty(&manifest).map_err(|error| {
                CodexxError::Config(format!("Kilo manifest 序列化失败: {error}"))
            })?,
        )?;
        Ok(())
    })();
    finish_with_rollback(result, &snapshots)
}

pub(crate) fn uninstall_kilo() -> Result<bool> {
    let manifest_path = kilo_manifest_path()?;
    if !manifest_path.is_file() {
        return Ok(false);
    }
    let manifest = read_manifest()?
        .ok_or_else(|| CodexxError::Config("Kilo manifest 在卸载前消失".to_string()))?;
    let agents_path = kilo_agents_path()?;
    let original_path = kilo_original_agents_path()?;
    let snapshots = [
        capture_fixed_file(agents_path.clone())?,
        capture_fixed_file(manifest_path.clone())?,
        capture_fixed_file(original_path.clone())?,
    ];
    let result = (|| {
        if manifest_original_existed(&manifest) {
            if !original_path.is_file() {
                return Err(CodexxError::Config(format!(
                    "Kilo 原始 AGENTS.md 快照缺失，无法安全卸载: {}",
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

#[derive(Debug, Clone)]
pub(crate) struct KiloStatus {
    pub kilo_dir_exists: bool,
    pub agents_md_exists: bool,
    pub manifest_exists: bool,
    pub original_snapshot_exists: bool,
}

pub(crate) fn kilo_status() -> Result<KiloStatus> {
    Ok(KiloStatus {
        kilo_dir_exists: kilo_home_dir()?.is_dir(),
        agents_md_exists: kilo_agents_path()?.is_file(),
        manifest_exists: kilo_manifest_path()?.is_file(),
        original_snapshot_exists: kilo_original_agents_path()?.is_file(),
    })
}

pub(crate) fn kilo_builtin_content(template_id: &str) -> Result<(String, String, String, String)> {
    let id = if template_id.trim().is_empty() {
        KILO_BUILTIN_ID
    } else {
        template_id.trim()
    };
    if id != KILO_BUILTIN_ID {
        return Err(CodexxError::Config(format!("未知的 Kilo 内置模板: {id}")));
    }
    Ok((
        KILO_BUILTIN_FILENAME.to_string(),
        format!("./{KILO_BUILTIN_FILENAME}"),
        KILO_BUILTIN_CONTENT.replacen("Codex operates", "Kilo Code operates", 1),
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
            "devconduit-kilo-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create Kilo test directory");
        path
    }

    #[test]
    fn append_prompt_preserves_existing_content() {
        let rendered = render_append_prompt("# Existing\n", "# Managed\n", "saved:test")
            .expect("render append prompt");
        assert!(rendered.starts_with("# Existing\n\n"));
        assert!(rendered.contains(KILO_PROMPT_BEGIN));
        assert!(rendered.contains("saved:test"));
        assert!(rendered.contains("# Managed"));
    }

    #[test]
    fn replacing_append_prompt_keeps_one_managed_block() {
        let first = render_append_prompt("", "first", "saved:first").expect("render first");
        let second = render_append_prompt(&first, "second", "saved:second").expect("render second");
        assert_eq!(second.matches(KILO_PROMPT_BEGIN).count(), 1);
        assert!(!second.contains("first"));
        assert!(second.contains("second"));
    }

    #[test]
    fn malformed_managed_markers_are_rejected() {
        let malformed = format!("before\n{KILO_PROMPT_BEGIN}\nmissing end\n");
        assert!(remove_managed_prompt_block(&malformed).is_err());
    }

    #[test]
    fn removing_append_prompt_restores_existing_text() {
        let rendered = render_append_prompt("original\n", "managed", "saved:test")
            .expect("render append prompt");
        let (restored, removed) =
            remove_managed_prompt_block(&rendered).expect("remove managed prompt");
        assert!(removed);
        assert_eq!(restored, "original\n");
    }

    #[test]
    fn fixed_file_rollback_restores_content_and_absence() {
        let root = temp_dir("rollback");
        let existing = root.join("AGENTS.md");
        let absent = root.join("manifest.json");
        fs::write(&existing, b"original").expect("write original file");
        let snapshots = [
            capture_fixed_file(existing.clone()).expect("capture existing file"),
            capture_fixed_file(absent.clone()).expect("capture absent file"),
        ];

        fs::write(&existing, b"changed").expect("change original file");
        fs::write(&absent, b"created").expect("create absent file");
        restore_fixed_files(&snapshots).expect("restore snapshots");

        assert_eq!(
            fs::read(&existing).expect("read restored file"),
            b"original"
        );
        assert!(!absent.exists());
        fs::remove_dir_all(root).expect("remove Kilo test directory");
    }

    #[test]
    fn builtin_prompt_is_adapted_for_kilo() {
        let (_, _, content, _) = kilo_builtin_content(KILO_BUILTIN_ID).expect("load builtin");
        assert!(content.starts_with("Kilo Code operates"));
        assert!(!content.contains("Codex operates"));
    }
}
