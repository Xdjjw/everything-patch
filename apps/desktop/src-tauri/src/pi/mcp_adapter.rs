use crate::error::{CodexxError, Result};
use crate::file_io::{atomic_write, ensure_directory, io_err, read_to_string_if_exists};
use crate::paths::{app_home, home_dir};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const PI_MCP_ADAPTER_PACKAGE: &str = "pi-mcp-adapter";
pub(crate) const PI_MCP_ADAPTER_VERSION: &str = "2.21.0";
pub(crate) const PI_MCP_ADAPTER_SPEC: &str = "npm:pi-mcp-adapter@2.21.0";
pub(crate) const PI_MCP_ADAPTER_INTEGRITY: &str =
    "sha512-4oLrU5qTdbMnDNU8ECGADX3H2V50DCtgIjqFf+BWA31c9mw5zvSnCJfplyaf8v55NpfgBvi/Rli7ES4DflckfA==";

static PI_MCP_ADAPTER_INSTALL_LOCK: Mutex<()> = Mutex::new(());

fn package_source(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.as_object()?.get("source")?.as_str())
}

fn package_identity(source: &str) -> &str {
    let source = source.trim().strip_prefix("npm:").unwrap_or(source.trim());
    if let Some(rest) = source.strip_prefix('@') {
        return rest
            .rfind('@')
            .map(|index| &source[..index + 1])
            .unwrap_or(source);
    }
    source.split('@').next().unwrap_or(source)
}

fn package_is_pinned(value: &Value) -> bool {
    let Some(source) = package_source(value) else {
        return false;
    };
    let normalized = source.trim().strip_prefix("npm:").unwrap_or(source.trim());
    let expected = format!("{PI_MCP_ADAPTER_PACKAGE}@{PI_MCP_ADAPTER_VERSION}");
    if normalized == expected {
        return true;
    }
    package_identity(source) == PI_MCP_ADAPTER_PACKAGE
        && value
            .as_object()
            .and_then(|package| package.get("version"))
            .and_then(Value::as_str)
            == Some(PI_MCP_ADAPTER_VERSION)
}

pub(crate) fn settings_has_mcp_adapter(value: &Value) -> bool {
    value
        .get("packages")
        .and_then(Value::as_array)
        .is_some_and(|packages| packages.iter().any(package_is_pinned))
}

pub(crate) fn mcp_config_path() -> Result<PathBuf> {
    Ok(super::pi_home_dir()?.join("mcp.json"))
}

pub(crate) fn mcp_adapter_installed() -> Result<bool> {
    let settings = super::pi_home_dir()?.join("settings.json");
    let text = read_to_string_if_exists(&settings)?;
    if text.trim().is_empty() {
        return Ok(false);
    }
    let value = serde_json::from_str::<Value>(&text)
        .map_err(|error| CodexxError::Config(format!("Pi settings.json 解析失败: {error}")))?;
    Ok(settings_has_mcp_adapter(&value))
}

fn command_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("pi"),
        PathBuf::from("pi.cmd"),
        PathBuf::from("pi.exe"),
    ];
    let home = home_dir().unwrap_or_default();
    candidates.extend([
        home.join(".local/bin/pi"),
        home.join(".npm-global/bin/pi"),
        home.join("Library/pnpm/pi"),
        PathBuf::from("/opt/homebrew/bin/pi"),
        PathBuf::from("/usr/local/bin/pi"),
    ]);
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            candidates.push(PathBuf::from(appdata).join("npm/pi.cmd"));
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            candidates.push(PathBuf::from(localappdata).join("Microsoft/WindowsApps/pi.exe"));
        }
    }
    candidates
}

fn candidate_key(candidate: &Path) -> String {
    let value = candidate.to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn find_pi_command() -> Result<PathBuf> {
    let mut seen = HashSet::new();
    for candidate in command_candidates() {
        if !seen.insert(candidate_key(&candidate)) {
            continue;
        }
        if candidate.components().count() > 1 && !candidate.is_file() {
            continue;
        }
        let Ok(output) = crate::platform::program_command(&candidate, &["--version"]).output()
        else {
            continue;
        };
        if output.status.success() {
            return Ok(candidate);
        }
    }
    Err(CodexxError::Config(
        "未找到 Pi CLI，无法安装 MCP adapter".to_string(),
    ))
}

fn backup_settings(settings: &Path) -> Result<PathBuf> {
    let id = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let directory = app_home()?.join("backups/pi-mcp-adapter").join(id);
    ensure_directory(&directory)?;
    let existed = settings.is_file();
    let metadata = serde_json::json!({
        "version": 1,
        "package": PI_MCP_ADAPTER_SPEC,
        "packageVersion": PI_MCP_ADAPTER_VERSION,
        "integrity": PI_MCP_ADAPTER_INTEGRITY,
        "settingsExisted": existed,
    });
    atomic_write(
        &directory.join("meta.json"),
        serde_json::to_vec_pretty(&metadata)
            .map_err(|error| CodexxError::Config(error.to_string()))?
            .as_slice(),
    )?;
    if existed {
        fs::copy(settings, directory.join("settings.json.snapshot"))
            .map_err(|error| io_err(settings, error))?;
    }
    Ok(directory)
}

fn restore_settings(settings: &Path, backup: &Path) -> Result<()> {
    let snapshot = backup.join("settings.json.snapshot");
    if snapshot.is_file() {
        let bytes = fs::read(&snapshot).map_err(|error| io_err(&snapshot, error))?;
        atomic_write(settings, &bytes)
    } else if settings.exists() {
        let metadata = fs::symlink_metadata(settings).map_err(|error| io_err(settings, error))?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(settings).map_err(|error| io_err(settings, error))
        } else {
            Err(CodexxError::Config(format!(
                "Pi MCP adapter 回滚目标不是普通文件: {}",
                settings.display()
            )))
        }
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PiMcpAdapterInstall {
    pub(crate) installed_now: bool,
    pub(crate) backup_path: Option<PathBuf>,
}

pub(crate) fn rollback_mcp_adapter_install(install: &PiMcpAdapterInstall) -> Result<()> {
    if !install.installed_now {
        return Ok(());
    }
    let Some(backup) = install.backup_path.as_deref() else {
        return Err(CodexxError::Config(
            "Pi MCP adapter 安装记录缺少恢复快照".to_string(),
        ));
    };
    restore_settings(&super::pi_home_dir()?.join("settings.json"), backup)
}

pub(crate) fn ensure_mcp_adapter_installed() -> Result<PiMcpAdapterInstall> {
    let _guard = PI_MCP_ADAPTER_INSTALL_LOCK.lock().map_err(|_| {
        CodexxError::Config("Pi MCP adapter 安装锁已损坏，请重启 DevConduit".to_string())
    })?;
    if mcp_adapter_installed()? {
        return Ok(PiMcpAdapterInstall {
            installed_now: false,
            backup_path: None,
        });
    }
    let pi_dir = super::pi_home_dir()?;
    ensure_directory(&pi_dir)?;
    let settings = pi_dir.join("settings.json");
    match fs::symlink_metadata(&settings) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(CodexxError::Config(format!(
                "Pi settings.json 不是普通文件，已停止安装 MCP adapter: {}",
                settings.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_err(&settings, error)),
    }
    let backup = backup_settings(&settings)?;
    let program = find_pi_command()?;
    let output = crate::platform::program_command(
        &program,
        &["install", PI_MCP_ADAPTER_SPEC, "--no-approve"],
    )
    .env("PI_CODING_AGENT_DIR", &pi_dir)
    .current_dir(home_dir()?)
    .output()
    .map_err(|error| CodexxError::Config(format!("启动 Pi MCP adapter 安装失败: {error}")))?;
    if !output.status.success() {
        let restore_error = restore_settings(&settings, &backup).err();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            stderr
        };
        return Err(CodexxError::Config(match restore_error {
            Some(error) => format!("Pi MCP adapter 安装失败: {detail}；{error}"),
            None => format!("Pi MCP adapter 安装失败: {detail}"),
        }));
    }
    if !mcp_adapter_installed()? {
        let restore_error = restore_settings(&settings, &backup).err();
        return Err(CodexxError::Config(match restore_error {
            Some(error) => {
                format!("Pi 安装命令完成，但 settings.json 中未找到 {PI_MCP_ADAPTER_SPEC}；{error}")
            }
            None => format!("Pi 安装命令完成，但 settings.json 中未找到 {PI_MCP_ADAPTER_SPEC}"),
        }));
    }
    Ok(PiMcpAdapterInstall {
        installed_now: true,
        backup_path: Some(backup),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn adapter_detection_requires_the_pinned_version() {
        assert!(settings_has_mcp_adapter(&json!({
            "packages": ["npm:pi-mcp-adapter@2.21.0"]
        })));
        assert!(settings_has_mcp_adapter(&json!({
            "packages": [{"source": "npm:pi-mcp-adapter", "version": "2.21.0", "skills": []}]
        })));
        assert!(!settings_has_mcp_adapter(&json!({
            "packages": ["npm:pi-mcp-adapter@2.20.0"]
        })));
        assert!(!settings_has_mcp_adapter(&json!({
            "packages": [{"source": "npm:pi-mcp-adapter", "skills": []}]
        })));
        assert!(!settings_has_mcp_adapter(&json!({
            "packages": ["npm:another-package@1.0.0"]
        })));
    }

    #[test]
    fn scoped_package_identity_keeps_scope() {
        assert_eq!(
            package_identity("npm:@scope/package@1.2.3"),
            "@scope/package"
        );
        assert_eq!(
            package_identity("pi-mcp-adapter@2.21.0"),
            PI_MCP_ADAPTER_PACKAGE
        );
    }
}
