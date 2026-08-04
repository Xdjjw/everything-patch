//! Read-only deployment previews shared by all prompt engines.

use crate::error::{CodexxError, Result};
use crate::paths::app_home;
use crate::prompts::{
    agents_path, claude_instruction_file, claude_memory_path, managed_claude_instruction_filename,
};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimePreviewTarget {
    pub(crate) label: String,
    pub(crate) path: String,
    pub(crate) operation: String,
    pub(crate) exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptRuntimePreview {
    pub(crate) engine: String,
    pub(crate) operation: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) backup_location: String,
    pub(crate) restart_hint: Option<String>,
    pub(crate) targets: Vec<RuntimePreviewTarget>,
}

fn validate_engine(engine: &str) -> Result<&str> {
    match engine.trim().to_ascii_lowercase().as_str() {
        "codex" => Ok("codex"),
        "claude" => Ok("claude"),
        "zcode" => Ok("zcode"),
        "grok" => Ok("grok"),
        "kilo" => Ok("kilo"),
        _ => Err(CodexxError::Config(format!("未知的运行时引擎: {engine}"))),
    }
}

fn validate_operation(operation: &str) -> Result<&str> {
    match operation.trim() {
        "install" | "uninstall" => Ok(operation.trim()),
        _ => Err(CodexxError::Config(format!(
            "未知的运行时操作: {operation}"
        ))),
    }
}

fn target(label: &str, path: impl AsRef<Path>, operation: &str) -> RuntimePreviewTarget {
    let path = path.as_ref();
    RuntimePreviewTarget {
        label: label.to_string(),
        path: path.display().to_string(),
        operation: operation.to_string(),
        exists: path.is_file() || path.is_dir(),
    }
}

fn profile_restart_hint() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        Some(
            "Open a new zsh terminal or run source ~/.zshrc to reload the Claude wrapper."
                .to_string(),
        )
    }
    #[cfg(target_os = "windows")]
    {
        Some(
            "Open a new PowerShell window or reload $PROFILE to apply the Claude wrapper."
                .to_string(),
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

fn preview_codex(
    operation: &str,
    codex_dir: Option<&Path>,
) -> Result<(String, String, Vec<RuntimePreviewTarget>, Option<String>)> {
    let codex_dir = codex_dir.ok_or_else(|| {
        CodexxError::Config("预览 Codex 运行时部署时缺少当前 CODEX_HOME".to_string())
    })?;
    let file_operation = if operation == "install" {
        "update"
    } else {
        "restore/remove"
    };
    Ok((
        "Native config deployment".to_string(),
        "Codex uses config.toml and its managed AGENTS.md block; no shell command is replaced."
            .to_string(),
        vec![
            target(
                "Codex config",
                crate::config_path(codex_dir),
                file_operation,
            ),
            target("Managed AGENTS.md", agents_path(codex_dir), file_operation),
        ],
        None,
    ))
}

fn preview_claude(
    operation: &str,
) -> Result<(String, String, Vec<RuntimePreviewTarget>, Option<String>)> {
    let file_operation = if operation == "install" {
        "update"
    } else {
        "restore/remove"
    };
    let mut targets = vec![target(
        "Claude memory",
        claude_memory_path()?,
        file_operation,
    )];
    if let Some(filename) = managed_claude_instruction_filename()? {
        targets.push(target(
            "Managed Claude instruction",
            claude_instruction_file(&filename)?,
            file_operation,
        ));
    }
    let runtime = crate::claude_runtime::build_runtime_state()?;
    if runtime.active || runtime.status == "partial" || runtime.status == "needs-repair" {
        targets.push(target(
            "Claude runtime prompt",
            Path::new(&runtime.prompt_path),
            "sync/remove",
        ));
        targets.extend(runtime.profiles.iter().map(|profile| RuntimePreviewTarget {
            label: "Claude shell profile".to_string(),
            path: profile.path.clone(),
            operation: "preserve managed block".to_string(),
            exists: profile.exists,
        }));
    }
    Ok((
        "Native CLAUDE.md deployment".to_string(),
        "Claude instructions are managed through a CLAUDE.md import block. The optional CLI wrapper has a separate install control."
            .to_string(),
        targets,
        None,
    ))
}

fn preview_zcode(
    operation: &str,
) -> Result<(String, String, Vec<RuntimePreviewTarget>, Option<String>)> {
    let paths = crate::zcode::build_paths()?;
    let file_operation = if operation == "install" {
        "write"
    } else {
        "remove"
    };
    Ok((
        "Launcher runtime deployment".to_string(),
        "ZCode uses a managed launcher and environment bridge instead of changing the application bundle."
            .to_string(),
        vec![
            target("System role", paths.system_file, file_operation),
            target("Runtime config", paths.config_file, file_operation),
            target("Launcher", paths.launcher, file_operation),
            target("Patch sidecar", paths.patch_sidecar, file_operation),
        ],
        Some("Restart ZCode after changing its runtime deployment.".to_string()),
    ))
}

fn preview_grok(
    operation: &str,
) -> Result<(String, String, Vec<RuntimePreviewTarget>, Option<String>)> {
    let file_operation = if operation == "install" {
        "update"
    } else {
        "restore/remove"
    };
    Ok((
        "Native global rules deployment".to_string(),
        "Grok uses its AGENTS.md, config compatibility block, hook isolation, and a deployment manifest."
            .to_string(),
        vec![
            target("Grok AGENTS.md", crate::grok::grok_agents_path()?, file_operation),
            target("Grok config", crate::grok::grok_config_path()?, file_operation),
            target("Grok hooks", crate::grok::grok_hooks_dir()?, file_operation),
            target("Deployment manifest", crate::grok::grok_manifest_path()?, file_operation),
        ],
        Some("Restart Grok after changing global rules or hook isolation.".to_string()),
    ))
}

fn preview_kilo(
    operation: &str,
) -> Result<(String, String, Vec<RuntimePreviewTarget>, Option<String>)> {
    let file_operation = if operation == "install" {
        "update"
    } else {
        "restore/remove"
    };
    Ok((
        "Native global AGENTS.md deployment".to_string(),
        "Kilo uses its official global AGENTS.md path. DevConduit keeps the first original file in a fixed local snapshot for uninstall and recovery."
            .to_string(),
        vec![
            target(
                "Kilo AGENTS.md",
                crate::kilo::kilo_agents_path()?,
                file_operation,
            ),
            target(
                "Original AGENTS.md snapshot",
                crate::kilo::kilo_original_agents_path()?,
                "preserve/restore",
            ),
            target(
                "Deployment manifest",
                crate::kilo::kilo_manifest_path()?,
                file_operation,
            ),
        ],
        Some("Restart Kilo or run /reload after changing global instructions.".to_string()),
    ))
}

pub(crate) fn preview_prompt_runtime(
    engine: &str,
    operation: &str,
    codex_dir: Option<&Path>,
) -> Result<PromptRuntimePreview> {
    let engine = validate_engine(engine)?;
    let operation = validate_operation(operation)?;
    let (title, summary, targets, restart_hint) = match engine {
        "codex" => preview_codex(operation, codex_dir)?,
        "claude" => preview_claude(operation)?,
        "zcode" => preview_zcode(operation)?,
        "grok" => preview_grok(operation)?,
        "kilo" => preview_kilo(operation)?,
        _ => unreachable!(),
    };
    let action_title = if operation == "install" {
        "Install deployment"
    } else {
        "Uninstall deployment"
    };
    Ok(PromptRuntimePreview {
        engine: engine.to_string(),
        operation: operation.to_string(),
        title: format!("{action_title}: {title}"),
        summary,
        backup_location: app_home()?
            .join("prompt-backups")
            .join(engine)
            .display()
            .to_string(),
        restart_hint,
        targets,
    })
}

pub(crate) fn preview_claude_runtime() -> Result<PromptRuntimePreview> {
    let runtime = crate::claude_runtime::build_runtime_state()?;
    if !runtime.supported {
        return Err(CodexxError::Config(
            "当前平台不支持 Claude CLI runtime（仅支持 macOS 和 Windows）".to_string(),
        ));
    }
    let operation =
        if runtime.active || runtime.status == "partial" || runtime.status == "needs-repair" {
            "uninstall"
        } else {
            "install"
        };
    let target_operation = if operation == "install" {
        "write"
    } else {
        "remove"
    };
    let mut targets = vec![target(
        "Claude runtime prompt",
        Path::new(&runtime.prompt_path),
        target_operation,
    )];
    targets.extend(runtime.profiles.iter().map(|profile| RuntimePreviewTarget {
        label: "Claude shell profile".to_string(),
        path: profile.path.clone(),
        operation: target_operation.to_string(),
        exists: profile.exists,
    }));
    Ok(PromptRuntimePreview {
        engine: "claude".to_string(),
        operation: operation.to_string(),
        title: if operation == "install" {
            "Install Claude CLI runtime".to_string()
        } else {
            "Remove Claude CLI runtime".to_string()
        },
        summary: "The wrapper appends the currently managed DevConduit Claude prompt to every claude command. It does not modify Claude settings, credentials, binaries, or MCP configuration."
            .to_string(),
        backup_location: app_home()?
            .join("prompt-backups")
            .join("claude")
            .display()
            .to_string(),
        restart_hint: profile_restart_hint(),
        targets,
    })
}
