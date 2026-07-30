use crate::constants::{GROK_AGENTS_FILENAME, GROK_CONFIG_FILENAME, GROK_MANIFEST_FILENAME};
use crate::error::{CodexxError, Result};
use crate::file_io::{
    atomic_write, ensure_directory, io_err, parse_toml_document, read_to_string_if_exists,
    write_json,
};
use crate::paths::app_home;
use crate::prompts::{
    agents_path, claude_instruction_file, claude_memory_path, managed_agents_template_key,
    managed_claude_injection_mode, managed_claude_instruction_filename,
    prompt_template_key_for_instruction, resolve_instruction_path, PromptInjectionMode,
};
use crate::{config_path, string_value};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const ENGINE_CODEX: &str = "codex";
const ENGINE_CLAUDE: &str = "claude";
const ENGINE_ZCODE: &str = "zcode";
const ENGINE_GROK: &str = "grok";

const KEY_CONFIG: &str = "config";
const KEY_AGENTS: &str = "agents";
const KEY_INSTRUCTION: &str = "instruction";
const KEY_MEMORY: &str = "memory";
const KEY_SYSTEM_ROLE: &str = "system-role";
const KEY_LAUNCHER: &str = "launcher";
const KEY_SIDECAR: &str = "sidecar";
const KEY_MANIFEST: &str = "manifest";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptBackupFile {
    key: String,
    existed: bool,
    #[serde(default)]
    original_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptBackupMeta {
    version: u32,
    id: String,
    engine: String,
    action: String,
    created_at: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    injection_mode: Option<String>,
    files: Vec<PromptBackupFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptBackupEntry {
    id: String,
    engine: String,
    action: String,
    created_at: String,
    path: String,
    scope: Option<String>,
    injection_mode: Option<String>,
    file_count: usize,
}

fn validate_engine(engine: &str) -> Result<&str> {
    match engine.trim().to_ascii_lowercase().as_str() {
        ENGINE_CODEX => Ok(ENGINE_CODEX),
        ENGINE_CLAUDE => Ok(ENGINE_CLAUDE),
        ENGINE_ZCODE => Ok(ENGINE_ZCODE),
        ENGINE_GROK => Ok(ENGINE_GROK),
        other => Err(CodexxError::Config(format!("未知的提示词引擎: {other}"))),
    }
}

fn validate_backup_id(id: &str) -> Result<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty()
        || trimmed != id
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
    {
        return Err(CodexxError::Config("备份标识无效".to_string()));
    }
    Ok(trimmed)
}

fn validate_leaf_name(name: &str) -> Result<&str> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed != name
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || Path::new(trimmed)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(trimmed)
    {
        return Err(CodexxError::Config(format!("备份文件名无效: {name}")));
    }
    Ok(trimmed)
}

fn prompt_backup_root(engine: &str) -> Result<PathBuf> {
    Ok(app_home()?
        .join("prompt-backups")
        .join(validate_engine(engine)?))
}

fn snapshot_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.snapshot"))
}

fn capture_file(
    dir: &Path,
    key: &str,
    source: &Path,
    original_name: Option<String>,
) -> Result<PromptBackupFile> {
    let existed = source.is_file();
    if source.exists() && !existed {
        return Err(CodexxError::Config(format!(
            "提示词备份目标不是文件: {}",
            source.display()
        )));
    }
    if existed {
        fs::copy(source, snapshot_path(dir, key)).map_err(|e| io_err(source, e))?;
    }
    Ok(PromptBackupFile {
        key: key.to_string(),
        existed,
        original_name,
    })
}

fn missing_file(key: &str) -> PromptBackupFile {
    PromptBackupFile {
        key: key.to_string(),
        existed: false,
        original_name: None,
    }
}

fn create_snapshot(
    engine: &str,
    action: &str,
    scope: Option<String>,
    injection_mode: Option<PromptInjectionMode>,
    capture: impl FnOnce(&Path) -> Result<Vec<PromptBackupFile>>,
) -> Result<Option<String>> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let engine = validate_engine(engine)?;
    let id = format!(
        "{}-{}-{}",
        Local::now().format("%Y%m%d-%H%M%S-%3f"),
        COUNTER.fetch_add(1, Ordering::Relaxed),
        action
    );
    let dir = prompt_backup_root(engine)?.join(&id);
    ensure_directory(&dir)?;
    let files = capture(&dir)?;
    let meta = PromptBackupMeta {
        version: 1,
        id: id.clone(),
        engine: engine.to_string(),
        action: action.to_string(),
        created_at: Local::now().to_rfc3339(),
        scope,
        injection_mode: injection_mode.map(|mode| mode.as_str().to_string()),
        files,
    };
    write_json(
        &dir.join("meta.json"),
        &serde_json::to_value(meta).expect("prompt backup meta serialize"),
    )?;
    Ok(Some(id))
}

fn codex_managed_instruction(codex_dir: &Path) -> Result<Option<(String, PathBuf)>> {
    let cfg = config_path(codex_dir);
    let text = read_to_string_if_exists(&cfg)?;
    let doc = parse_toml_document(&cfg, &text)?;
    let Some(value) = string_value(&doc, "model_instructions_file") else {
        return Ok(None);
    };
    if prompt_template_key_for_instruction(&value)?.is_none() {
        return Ok(None);
    }
    let path = resolve_instruction_path(codex_dir, &value);
    if path.parent() != Some(codex_dir) {
        return Ok(None);
    }
    let Some(filename) = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
    else {
        return Ok(None);
    };
    validate_leaf_name(&filename)?;
    Ok(Some((filename, path)))
}

pub(crate) fn create_codex_prompt_backup(codex_dir: &Path, action: &str) -> Result<Option<String>> {
    let managed_instruction = codex_managed_instruction(codex_dir)?;
    let injection_mode = if managed_agents_template_key(codex_dir)?.is_some() {
        Some(PromptInjectionMode::Append)
    } else if managed_instruction.is_some() {
        Some(PromptInjectionMode::Replace)
    } else {
        None
    };
    create_snapshot(
        ENGINE_CODEX,
        action,
        Some(codex_dir.display().to_string()),
        injection_mode,
        |dir| {
            let mut files = vec![
                capture_file(dir, KEY_CONFIG, &config_path(codex_dir), None)?,
                capture_file(dir, KEY_AGENTS, &agents_path(codex_dir), None)?,
            ];
            files.push(match managed_instruction.as_ref() {
                Some((filename, path)) => {
                    capture_file(dir, KEY_INSTRUCTION, path, Some(filename.clone()))?
                }
                None => missing_file(KEY_INSTRUCTION),
            });
            Ok(files)
        },
    )
}

fn create_claude_prompt_backup_with_runtime(
    action: &str,
    include_runtime: bool,
) -> Result<Option<String>> {
    let memory = claude_memory_path()?;
    let managed_filename = managed_claude_instruction_filename()?;
    let mode = managed_claude_injection_mode()?;
    create_snapshot(ENGINE_CLAUDE, action, None, mode, |dir| {
        let mut files = vec![capture_file(dir, KEY_MEMORY, &memory, None)?];
        files.push(match managed_filename.as_ref() {
            Some(filename) => capture_file(
                dir,
                KEY_INSTRUCTION,
                &claude_instruction_file(filename)?,
                Some(filename.clone()),
            )?,
            None => missing_file(KEY_INSTRUCTION),
        });
        if include_runtime {
            for (key, path) in crate::claude_runtime::runtime_backup_targets()? {
                files.push(capture_file(dir, &key, &path, None)?);
            }
        }
        Ok(files)
    })
}

pub(crate) fn create_claude_prompt_backup(action: &str) -> Result<Option<String>> {
    // Record absent runtime files as well as existing ones. A later restore can
    // then return Claude to the exact pre-action state instead of leaving a
    // wrapper installed by a newer action.
    create_claude_prompt_backup_with_runtime(action, true)
}

pub(crate) fn create_claude_runtime_backup(action: &str) -> Result<Option<String>> {
    create_claude_prompt_backup_with_runtime(action, true)
}

pub(crate) fn create_zcode_prompt_backup(action: &str) -> Result<Option<String>> {
    let paths = crate::zcode::build_paths()?;
    let mode = crate::zcode::current_install_metadata()?.0;
    create_snapshot(ENGINE_ZCODE, action, None, mode, |dir| {
        Ok(vec![
            capture_file(dir, KEY_SYSTEM_ROLE, &paths.system_file, None)?,
            capture_file(dir, KEY_CONFIG, &paths.config_file, None)?,
            capture_file(dir, KEY_LAUNCHER, &paths.launcher, None)?,
            capture_file(dir, KEY_SIDECAR, &paths.patch_sidecar, None)?,
        ])
    })
}

pub(crate) fn create_grok_prompt_backup(action: &str) -> Result<Option<String>> {
    let mode = crate::grok::current_install_metadata()?.0;
    create_snapshot(ENGINE_GROK, action, None, mode, |dir| {
        Ok(vec![
            capture_file(
                dir,
                KEY_AGENTS,
                &crate::grok::grok_agents_path()?,
                Some(GROK_AGENTS_FILENAME.to_string()),
            )?,
            capture_file(
                dir,
                KEY_CONFIG,
                &crate::grok::grok_config_path()?,
                Some(GROK_CONFIG_FILENAME.to_string()),
            )?,
            capture_file(
                dir,
                KEY_MANIFEST,
                &crate::grok::grok_manifest_path()?,
                Some(GROK_MANIFEST_FILENAME.to_string()),
            )?,
        ])
    })
}

fn read_meta(dir: &Path) -> Result<PromptBackupMeta> {
    let path = dir.join("meta.json");
    let text = fs::read_to_string(&path).map_err(|e| io_err(&path, e))?;
    serde_json::from_str(&text)
        .map_err(|e| CodexxError::Config(format!("提示词备份元数据解析失败: {e}")))
}

fn load_snapshot(engine: &str, backup_id: &str) -> Result<(PathBuf, PromptBackupMeta)> {
    let engine = validate_engine(engine)?;
    let backup_id = validate_backup_id(backup_id)?;
    let dir = prompt_backup_root(engine)?.join(backup_id);
    if !dir.is_dir() {
        return Err(CodexxError::Config(format!("备份不存在: {backup_id}")));
    }
    let meta = read_meta(&dir)?;
    if meta.engine != engine || meta.id != backup_id {
        return Err(CodexxError::Config(
            "提示词备份元数据与请求不匹配".to_string(),
        ));
    }
    Ok((dir, meta))
}

pub(crate) fn list_prompt_backups(
    engine: &str,
    codex_scope: Option<&Path>,
) -> Result<Vec<PromptBackupEntry>> {
    let engine = validate_engine(engine)?;
    let root = prompt_backup_root(engine)?;
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let expected_scope = codex_scope.map(|path| path.display().to_string());
    let mut entries = Vec::new();
    for item in fs::read_dir(&root).map_err(|e| io_err(&root, e))? {
        let item = item.map_err(|e| io_err(&root, e))?;
        let path = item.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(meta) = read_meta(&path) else {
            continue;
        };
        if meta.engine != engine {
            continue;
        }
        if engine == ENGINE_CODEX
            && expected_scope
                .as_ref()
                .is_some_and(|scope| meta.scope.as_ref() != Some(scope))
        {
            continue;
        }
        entries.push(PromptBackupEntry {
            id: meta.id,
            engine: meta.engine,
            action: meta.action,
            created_at: meta.created_at,
            path: path.display().to_string(),
            scope: meta.scope,
            injection_mode: meta.injection_mode,
            file_count: meta.files.iter().filter(|file| file.existed).count(),
        });
    }
    entries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(entries)
}

fn file_meta<'a>(meta: &'a PromptBackupMeta, key: &str) -> Result<&'a PromptBackupFile> {
    meta.files
        .iter()
        .find(|file| file.key == key)
        .ok_or_else(|| CodexxError::Config(format!("备份缺少文件状态: {key}")))
}

fn restore_file(dir: &Path, meta: &PromptBackupMeta, key: &str, target: &Path) -> Result<()> {
    let file = file_meta(meta, key)?;
    if file.existed {
        let snapshot = snapshot_path(dir, key);
        if !snapshot.is_file() {
            return Err(CodexxError::Config(format!(
                "备份文件缺失: {}",
                snapshot.display()
            )));
        }
        if let Some(parent) = target.parent() {
            ensure_directory(parent)?;
        }
        let bytes = fs::read(&snapshot).map_err(|e| io_err(&snapshot, e))?;
        atomic_write(target, &bytes)?;
    } else if target.exists() {
        if !target.is_file() {
            return Err(CodexxError::Config(format!(
                "无法移除非文件目标: {}",
                target.display()
            )));
        }
        fs::remove_file(target).map_err(|e| io_err(target, e))?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        if !path.is_file() {
            return Err(CodexxError::Config(format!(
                "无法移除非文件目标: {}",
                path.display()
            )));
        }
        fs::remove_file(path).map_err(|e| io_err(path, e))?;
    }
    Ok(())
}

fn restore_codex_snapshot(dir: &Path, meta: &PromptBackupMeta, codex_dir: &Path) -> Result<()> {
    let scope = codex_dir.display().to_string();
    if meta.scope.as_deref() != Some(scope.as_str()) {
        return Err(CodexxError::Config(
            "该备份属于另一个 CODEX_HOME，不能恢复到当前目录".to_string(),
        ));
    }
    ensure_directory(codex_dir)?;
    if let Some((_, current)) = codex_managed_instruction(codex_dir)? {
        remove_file_if_exists(&current)?;
    }
    restore_file(dir, meta, KEY_CONFIG, &config_path(codex_dir))?;
    restore_file(dir, meta, KEY_AGENTS, &agents_path(codex_dir))?;
    let instruction = file_meta(meta, KEY_INSTRUCTION)?;
    if let Some(filename) = instruction.original_name.as_deref() {
        validate_leaf_name(filename)?;
        restore_file(dir, meta, KEY_INSTRUCTION, &codex_dir.join(filename))?;
    } else if instruction.existed {
        return Err(CodexxError::Config(
            "备份中的 Codex 提示词文件名缺失".to_string(),
        ));
    }
    Ok(())
}

fn restore_claude_snapshot(dir: &Path, meta: &PromptBackupMeta) -> Result<()> {
    if let Some(filename) = managed_claude_instruction_filename()? {
        remove_file_if_exists(&claude_instruction_file(&filename)?)?;
    }
    restore_file(dir, meta, KEY_MEMORY, &claude_memory_path()?)?;
    let instruction = file_meta(meta, KEY_INSTRUCTION)?;
    if let Some(filename) = instruction.original_name.as_deref() {
        validate_leaf_name(filename)?;
        restore_file(
            dir,
            meta,
            KEY_INSTRUCTION,
            &claude_instruction_file(filename)?,
        )?;
    } else if instruction.existed {
        return Err(CodexxError::Config(
            "备份中的 Claude 提示词文件名缺失".to_string(),
        ));
    }
    for file in &meta.files {
        if !file.key.starts_with("claude-runtime-") {
            continue;
        }
        if let Some(target) = crate::claude_runtime::runtime_target_for_backup_key(&file.key)? {
            restore_file(dir, meta, &file.key, &target)?;
        }
    }
    Ok(())
}

fn restore_zcode_snapshot(dir: &Path, meta: &PromptBackupMeta) -> Result<()> {
    let paths = crate::zcode::build_paths()?;
    restore_file(dir, meta, KEY_SYSTEM_ROLE, &paths.system_file)?;
    restore_file(dir, meta, KEY_CONFIG, &paths.config_file)?;
    restore_file(dir, meta, KEY_LAUNCHER, &paths.launcher)?;
    restore_file(dir, meta, KEY_SIDECAR, &paths.patch_sidecar)?;
    crate::zcode::sync_restored_environment()
}

fn restore_grok_snapshot(dir: &Path, meta: &PromptBackupMeta) -> Result<()> {
    crate::grok::prepare_prompt_backup_restore()?;
    restore_file(dir, meta, KEY_AGENTS, &crate::grok::grok_agents_path()?)?;
    restore_file(dir, meta, KEY_CONFIG, &crate::grok::grok_config_path()?)?;
    restore_file(dir, meta, KEY_MANIFEST, &crate::grok::grok_manifest_path()?)?;
    crate::grok::finalize_prompt_backup_restore()
}

pub(crate) fn restore_prompt_backup(
    engine: &str,
    codex_dir: Option<&Path>,
    backup_id: &str,
) -> Result<Option<String>> {
    let engine = validate_engine(engine)?;
    let (dir, meta) = load_snapshot(engine, backup_id)?;
    let restore_marker = match engine {
        ENGINE_CODEX => create_codex_prompt_backup(
            codex_dir.ok_or_else(|| {
                CodexxError::Config("恢复 Codex 备份时缺少 CODEX_HOME".to_string())
            })?,
            "before-restore",
        )?,
        ENGINE_CLAUDE => create_claude_prompt_backup("before-restore")?,
        ENGINE_ZCODE => create_zcode_prompt_backup("before-restore")?,
        ENGINE_GROK => create_grok_prompt_backup("before-restore")?,
        _ => unreachable!(),
    };
    match engine {
        ENGINE_CODEX => restore_codex_snapshot(
            &dir,
            &meta,
            codex_dir.ok_or_else(|| {
                CodexxError::Config("恢复 Codex 备份时缺少 CODEX_HOME".to_string())
            })?,
        )?,
        ENGINE_CLAUDE => restore_claude_snapshot(&dir, &meta)?,
        ENGINE_ZCODE => restore_zcode_snapshot(&dir, &meta)?,
        ENGINE_GROK => restore_grok_snapshot(&dir, &meta)?,
        _ => unreachable!(),
    }
    Ok(restore_marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_io::write_text;

    #[test]
    fn backup_id_rejects_path_traversal() {
        assert!(validate_backup_id("../backup").is_err());
        assert!(validate_backup_id("folder\\backup").is_err());
        assert!(validate_backup_id("valid-backup").is_ok());
    }

    #[test]
    fn leaf_name_rejects_nested_paths() {
        assert!(validate_leaf_name("../prompt.md").is_err());
        assert!(validate_leaf_name("nested/prompt.md").is_err());
        assert!(validate_leaf_name("prompt.md").is_ok());
    }

    #[test]
    fn codex_snapshot_restores_config_agents_and_managed_prompt() {
        let codex_dir = std::env::temp_dir().join(format!(
            "everything-patch-prompt-backup-{}-{}",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        ensure_directory(&codex_dir).expect("create codex test dir");
        let prompt_path = codex_dir.join("gpt5.5-unrestricted.md");
        write_text(
            &config_path(&codex_dir),
            "model_instructions_file = \"./gpt5.5-unrestricted.md\"\n",
        )
        .expect("write original config");
        write_text(&agents_path(&codex_dir), "# Original agents\n").expect("write original agents");
        write_text(&prompt_path, "# Original prompt\n").expect("write original prompt");

        let backup_id = create_codex_prompt_backup(&codex_dir, "test-restore")
            .expect("create prompt backup")
            .expect("backup id");

        write_text(
            &config_path(&codex_dir),
            "model_instructions_file = \"./gpt5.5-unrestricted.md\"\nmodel = \"changed\"\n",
        )
        .expect("mutate config");
        write_text(&agents_path(&codex_dir), "# Changed agents\n").expect("mutate agents");
        write_text(&prompt_path, "# Changed prompt\n").expect("mutate prompt");

        restore_prompt_backup(ENGINE_CODEX, Some(&codex_dir), &backup_id)
            .expect("restore prompt backup");

        assert_eq!(
            read_to_string_if_exists(&config_path(&codex_dir)).expect("read config"),
            "model_instructions_file = \"./gpt5.5-unrestricted.md\"\n"
        );
        assert_eq!(
            read_to_string_if_exists(&agents_path(&codex_dir)).expect("read agents"),
            "# Original agents\n"
        );
        assert_eq!(
            read_to_string_if_exists(&prompt_path).expect("read prompt"),
            "# Original prompt\n"
        );

        fs::remove_dir_all(codex_dir).expect("remove codex test dir");
    }
}
