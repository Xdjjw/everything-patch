use crate::error::{CodexxError, Result};
use crate::file_io::{atomic_write, ensure_directory, io_err};
use crate::paths::{app_home, home_dir};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SEARCH_RESULTS: usize = 48;
const CE_BRIDGE_NAME: &str = "DevConduit-ce_mcp_bridge.lua";
const X64DBG_X64_PLUGIN: &str = "MCPx64dbg.dp64";
const X64DBG_X32_PLUGIN: &str = "MCPx64dbg.dp32";

static HOST_INSTALL_LOCK: Mutex<()> = Mutex::new(());
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpHostInstallTarget {
    pub(crate) path: String,
    pub(crate) source: String,
    pub(crate) operation: String,
    pub(crate) exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpHostInstallPlan {
    pub(crate) integration_id: String,
    pub(crate) status: String,
    pub(crate) host_name: String,
    pub(crate) host_path: Option<String>,
    pub(crate) targets: Vec<McpHostInstallTarget>,
    pub(crate) can_restore: bool,
    pub(crate) message: String,
    pub(crate) next_step: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct HostInstallReport {
    pub(crate) plan: McpHostInstallPlan,
    pub(crate) installed: usize,
    pub(crate) backup_location: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    integration_id: String,
    created_at: String,
    restored: bool,
    entries: Vec<BackupEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupEntry {
    target: String,
    backup: Option<String>,
    #[serde(default)]
    installed_sha256: Option<String>,
}

#[derive(Debug, Clone)]
struct InstallTarget {
    target: PathBuf,
    source: PathBuf,
    operation: &'static str,
}

fn is_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn is_plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn validate_integration_id(integration_id: &str) -> Result<&str> {
    let integration_id = integration_id.trim();
    match integration_id {
        "ida-pro-mcp" | "cheatengine-mcp" | "x64dbg-mcp" | "burp-suite-mcp" => Ok(integration_id),
        _ => Err(CodexxError::Config(format!(
            "未知的 MCP 宿主软件: {integration_id}"
        ))),
    }
}

fn clean_host_path(raw: Option<&str>) -> Result<Option<PathBuf>> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if raw.len() > 4096
        || raw
            .chars()
            .any(|character| character == '\0' || character == '\r' || character == '\n')
    {
        return Err(CodexxError::Config("宿主软件路径无效".to_string()));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(CodexxError::Config(
            "宿主软件路径必须使用绝对路径".to_string(),
        ));
    }
    if !path.exists() {
        return Err(CodexxError::Config(format!(
            "宿主软件路径不存在: {}",
            path.display()
        )));
    }
    Ok(Some(path))
}

fn search_roots() -> Vec<(PathBuf, usize)> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |path: PathBuf, depth: usize| {
        if seen.insert(path_key(&path)) {
            roots.push((path, depth));
        }
    };
    if let Ok(home) = home_dir() {
        push(home.join("Applications"), 3);
        push(home.join("Desktop"), 3);
        push(home.join("Downloads"), 3);
        push(home.join(".local/bin"), 2);
    }

    #[cfg(target_os = "macos")]
    {
        for path in [
            "/Applications",
            "/Applications/Utilities",
            "/opt/homebrew/bin",
            "/usr/local/bin",
        ] {
            push(PathBuf::from(path), 3);
        }
    }

    #[cfg(target_os = "windows")]
    {
        for variable in [
            "ProgramFiles",
            "ProgramFiles(x86)",
            "LOCALAPPDATA",
            "APPDATA",
        ] {
            if let Ok(value) = env::var(variable) {
                push(PathBuf::from(value), 4);
            }
        }
    }

    roots
}

fn collect_named_files(root: &Path, names: &[&str], depth: usize, output: &mut Vec<PathBuf>) {
    if depth == 0 || output.len() >= MAX_SEARCH_RESULTS || !is_plain_directory(root) {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if output.len() >= MAX_SEARCH_RESULTS {
            return;
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_named_files(&path, names, depth - 1, output);
        } else if metadata.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    names
                        .iter()
                        .any(|candidate| name.eq_ignore_ascii_case(candidate))
                })
        {
            output.push(path);
        }
    }
}

fn path_command_candidates(names: &[&str]) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let Some(path) = env::var_os("PATH") else {
        return candidates;
    };
    for directory in env::split_paths(&path) {
        for name in names {
            let candidate = directory.join(name);
            if is_plain_file(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

fn find_executable(names: &[&str]) -> Option<PathBuf> {
    let mut candidates = path_command_candidates(names);
    if let Some(path) = candidates.first() {
        return Some(path.clone());
    }
    let roots = search_roots();
    for (root, depth) in roots {
        collect_named_files(&root, names, depth, &mut candidates);
        if let Some(path) = candidates.first() {
            return Some(path.clone());
        }
    }
    candidates.into_iter().next()
}

#[cfg(target_os = "macos")]
fn find_application(terms: &[&str]) -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Ok(home) = home_dir() {
        roots.push(home.join("Applications"));
    }
    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_app = path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("app"));
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            if is_app
                && terms
                    .iter()
                    .any(|term| lower.contains(&term.to_ascii_lowercase()))
            {
                return Some(path);
            }
        }
    }
    None
}

fn application_root(path: &Path) -> PathBuf {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("app"))
    {
        return path.to_path_buf();
    }
    if path.is_file() {
        return path.parent().unwrap_or(path).to_path_buf();
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if name.eq_ignore_ascii_case("bin") {
        return path.parent().unwrap_or(path).to_path_buf();
    }
    path.to_path_buf()
}

fn ancestors(path: &Path, limit: usize) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut current = Some(path);
    for _ in 0..limit {
        let Some(value) = current else {
            break;
        };
        result.push(value.to_path_buf());
        current = value.parent();
    }
    result
}

fn first_existing_directory(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| is_plain_directory(path))
}

fn ce_autorun_directory(host: &Path) -> PathBuf {
    let root = application_root(host);
    let candidates = if root
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("bin"))
    {
        vec![root.join("autorun")]
    } else {
        vec![
            root.join("bin/autorun"),
            root.join("autorun"),
            root.join("bin").join("autorun"),
        ]
    };
    first_existing_directory(candidates).unwrap_or_else(|| root.join("bin/autorun"))
}

fn x64dbg_plugin_directories(host: &Path) -> Vec<(String, PathBuf)> {
    let root = application_root(host);
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for base in ancestors(&root, 7) {
        let base_name = base
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let direct_architecture = if base_name.contains("x32") {
            Some("x32")
        } else if base_name.contains("x64") {
            Some("x64")
        } else {
            None
        };
        let mut candidates = vec![
            ("x64", base.join("release/x64/plugins")),
            ("x32", base.join("release/x32/plugins")),
            ("x64", base.join("x64/plugins")),
            ("x32", base.join("x32/plugins")),
        ];
        if let Some(architecture) = direct_architecture {
            candidates.push((architecture, base.join("plugins")));
        }
        for (architecture, directory) in candidates {
            let architecture_root = directory.parent().unwrap_or(&directory);
            if is_plain_directory(architecture_root) && seen.insert(path_key(&directory)) {
                result.push((architecture.to_string(), directory));
            }
        }
    }

    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    if result.is_empty() && (lower.contains("x64") || lower.contains("x32")) {
        result.push((
            if lower.contains("x32") { "x32" } else { "x64" }.to_string(),
            root.join("plugins"),
        ));
    }
    result
}

fn detect_host_path(integration_id: &str, override_path: Option<&str>) -> Result<Option<PathBuf>> {
    if let Some(path) = clean_host_path(override_path)? {
        return Ok(Some(path));
    }
    match integration_id {
        "ida-pro-mcp" => {
            #[cfg(target_os = "macos")]
            if let Some(app) = find_application(&["ida professional", "ida pro"]) {
                return Ok(Some(app));
            }
            let names = if cfg!(target_os = "windows") {
                &["ida64.exe", "ida.exe", "idaq64.exe"][..]
            } else {
                &["ida64", "ida", "idaq64"][..]
            };
            Ok(find_executable(names))
        }
        "cheatengine-mcp" => {
            if !cfg!(target_os = "windows") {
                return Ok(None);
            }
            Ok(find_executable(&[
                "cheatengine-x86_64.exe",
                "cheatengine-i386.exe",
                "cheatengine.exe",
            ]))
        }
        "x64dbg-mcp" => {
            if !cfg!(target_os = "windows") {
                return Ok(None);
            }
            Ok(find_executable(&["x64dbg.exe", "x32dbg.exe"]))
        }
        "burp-suite-mcp" => {
            #[cfg(target_os = "macos")]
            if let Some(app) = find_application(&["burp suite"]) {
                return Ok(Some(app));
            }
            if cfg!(target_os = "windows") {
                Ok(find_executable(&[
                    "BurpSuitePro.exe",
                    "BurpSuiteCommunity.exe",
                    "burpsuite.exe",
                ]))
            } else {
                Ok(None)
            }
        }
        _ => Err(CodexxError::Config(format!(
            "未知的 MCP 宿主软件: {integration_id}"
        ))),
    }
}

fn source_path_for(
    integration_id: &str,
    managed_root: &Path,
    architecture: Option<&str>,
) -> Option<PathBuf> {
    match integration_id {
        "cheatengine-mcp" => Some(managed_root.join("project/MCP_Server/ce_mcp_bridge.lua")),
        "x64dbg-mcp" => Some(managed_root.join("plugins").join(match architecture {
            Some("x32") => X64DBG_X32_PLUGIN,
            _ => X64DBG_X64_PLUGIN,
        })),
        _ => None,
    }
}

fn build_install_targets(
    integration_id: &str,
    host_path: Option<&Path>,
    managed_root: Option<&Path>,
) -> Vec<InstallTarget> {
    let Some(host_path) = host_path else {
        return Vec::new();
    };
    let preview_root = PathBuf::from("<DevConduit-managed-source>");
    let managed_root = managed_root.unwrap_or(&preview_root);
    match integration_id {
        "cheatengine-mcp" if cfg!(target_os = "windows") => {
            vec![InstallTarget {
                target: ce_autorun_directory(host_path).join(CE_BRIDGE_NAME),
                source: source_path_for(integration_id, managed_root, None)
                    .expect("CE source path"),
                operation: "copy-to-autorun",
            }]
        }
        "x64dbg-mcp" if cfg!(target_os = "windows") => x64dbg_plugin_directories(host_path)
            .into_iter()
            .filter_map(|(architecture, directory)| {
                Some(InstallTarget {
                    target: directory.join(match architecture.as_str() {
                        "x32" => X64DBG_X32_PLUGIN,
                        _ => X64DBG_X64_PLUGIN,
                    }),
                    source: source_path_for(
                        integration_id,
                        managed_root,
                        Some(architecture.as_str()),
                    )?,
                    operation: "copy-to-plugins",
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn plan_from_targets(
    integration_id: &str,
    mode: &str,
    host_path: Option<PathBuf>,
    targets: Vec<InstallTarget>,
) -> McpHostInstallPlan {
    let host_name = match integration_id {
        "ida-pro-mcp" => "IDA Pro",
        "cheatengine-mcp" => "Cheat Engine",
        "x64dbg-mcp" => "x64dbg / x32dbg",
        "burp-suite-mcp" => "Burp Suite",
        _ => "宿主软件",
    };
    let host_path_text = host_path.as_ref().map(|path| path.display().to_string());
    let can_restore = restorable_backup_exists(integration_id);
    let target_views = targets
        .iter()
        .map(|target| McpHostInstallTarget {
            path: target.target.display().to_string(),
            source: target.source.display().to_string(),
            operation: target.operation.to_string(),
            exists: target.target.exists(),
        })
        .collect::<Vec<_>>();

    if matches!(mode, "remote") && matches!(integration_id, "cheatengine-mcp" | "x64dbg-mcp") {
        return McpHostInstallPlan {
            integration_id: integration_id.to_string(),
            status: "remote".to_string(),
            host_name: host_name.to_string(),
            host_path: host_path_text,
            targets: target_views,
            can_restore,
            message: format!("{host_name} 运行在远程 Windows，当前 Mac 无法直接写入远程主机"),
            next_step: Some(
                "远程 Windows 端仍需加载桥接文件；本机 MCP 配置会使用远程地址".to_string(),
            ),
        };
    }

    let Some(host_path) = host_path_text else {
        return McpHostInstallPlan {
            integration_id: integration_id.to_string(),
            status: "missing".to_string(),
            host_name: host_name.to_string(),
            host_path: None,
            targets: target_views,
            can_restore,
            message: format!("未检测到 {host_name} 安装目录"),
            next_step: Some("请安装宿主软件，或指定其安装目录".to_string()),
        };
    };

    match integration_id {
        "ida-pro-mcp" => McpHostInstallPlan {
            integration_id: integration_id.to_string(),
            status: "detected".to_string(),
            host_name: host_name.to_string(),
            host_path: Some(host_path),
            targets: target_views,
            can_restore,
            message: "已检测到 IDA Pro；MCP 使用 idalib，不需要复制插件文件".to_string(),
            next_step: Some("首次使用前确认 idalib 已激活，并重启 IDA 与 MCP 客户端".to_string()),
        },
        "burp-suite-mcp" => McpHostInstallPlan {
            integration_id: integration_id.to_string(),
            status: "detected".to_string(),
            host_name: host_name.to_string(),
            host_path: Some(host_path),
            targets: target_views,
            can_restore,
            message: "已检测到 Burp Suite；官方扩展已下载到 DevConduit 托管目录".to_string(),
            next_step: Some(
                "Burp 没有可靠的外部自动加载接口，首次仍需在 Extensions 中点击 Add".to_string(),
            ),
        },
        "cheatengine-mcp" | "x64dbg-mcp" if target_views.is_empty() => McpHostInstallPlan {
            integration_id: integration_id.to_string(),
            status: "manual".to_string(),
            host_name: host_name.to_string(),
            host_path: Some(host_path),
            targets: target_views,
            can_restore,
            message: format!("已找到 {host_name}，但没有找到可写入的插件目录"),
            next_step: Some("请指定软件根目录或确认便携版目录结构".to_string()),
        },
        _ => McpHostInstallPlan {
            integration_id: integration_id.to_string(),
            status: "ready".to_string(),
            host_name: host_name.to_string(),
            host_path: Some(host_path),
            targets: target_views,
            can_restore,
            message: format!("已找到 {host_name}，安装时会自动备份并写入对应目录"),
            next_step: None,
        },
    }
}

pub(crate) fn detect_mcp_host_inner(
    integration_id: String,
    mode: Option<String>,
    host_path: Option<String>,
) -> Result<McpHostInstallPlan> {
    let integration_id = validate_integration_id(&integration_id)?;
    let mode = mode.as_deref().unwrap_or(if cfg!(target_os = "windows") {
        "local"
    } else {
        "remote"
    });
    let host = detect_host_path(integration_id, host_path.as_deref())?;
    let targets = build_install_targets(integration_id, host.as_deref(), None);
    Ok(plan_from_targets(integration_id, mode, host, targets))
}

fn timestamp_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{nanos}-{}", TEMP_COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn reject_target_link(path: &Path) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(CodexxError::Config(format!(
                "拒绝覆盖宿主软件中的符号链接: {}",
                path.display()
            )));
        }
        if !metadata.is_file() {
            return Err(CodexxError::Config(format!(
                "宿主软件目标不是普通文件: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn copy_staged(source: &Path, target: &Path) -> Result<()> {
    if !is_plain_file(source) {
        return Err(CodexxError::Config(format!(
            "MCP 安装源文件不存在或不是普通文件: {}",
            source.display()
        )));
    }
    reject_target_link(target)?;
    let parent = target.parent().ok_or_else(|| {
        CodexxError::Config(format!("MCP 安装目标没有父目录: {}", target.display()))
    })?;
    ensure_directory(parent)?;
    let temp = parent.join(format!(
        ".{}.tmp.{}.{}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mcp"),
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_file(&temp);
    fs::copy(source, &temp).map_err(|error| io_err(&temp, error))?;
    if target.exists() {
        fs::remove_file(target).map_err(|error| io_err(target, error))?;
    }
    if let Err(error) = fs::rename(&temp, target) {
        let _ = fs::remove_file(&temp);
        return Err(io_err(target, error));
    }
    Ok(())
}

fn plain_file_sha256(path: &Path) -> Result<String> {
    if !is_plain_file(path) {
        return Err(CodexxError::Config(format!(
            "MCP 文件不存在或不是普通文件: {}",
            path.display()
        )));
    }
    let bytes = fs::read(path).map_err(|error| io_err(path, error))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn backup_root(integration_id: &str) -> Result<PathBuf> {
    let root = app_home()?
        .join("mcp-backups")
        .join(integration_id)
        .join(timestamp_id());
    ensure_directory(&root)?;
    Ok(root)
}

fn restorable_backup_exists(integration_id: &str) -> bool {
    let Ok(home) = app_home() else {
        return false;
    };
    let root = home.join("mcp-backups").join(integration_id);
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if !is_plain_directory(&path) {
            return false;
        }
        fs::read_to_string(path.join("manifest.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<BackupManifest>(&text).ok())
            .is_some_and(|manifest| manifest.integration_id == integration_id && !manifest.restored)
    })
}

fn restore_entries(entries: &[BackupEntry]) -> Result<()> {
    let mut failures = Vec::new();
    for entry in entries.iter().rev() {
        let target = PathBuf::from(&entry.target);
        let result = if let Some(backup) = &entry.backup {
            let backup = PathBuf::from(backup);
            if !is_plain_file(&backup) {
                Err(CodexxError::Config(format!(
                    "MCP 宿主备份文件缺失: {}",
                    backup.display()
                )))
            } else {
                copy_staged(&backup, &target)
            }
        } else {
            match fs::symlink_metadata(&target) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                    Err(CodexxError::Config(format!(
                        "MCP 宿主恢复目标被目录占用: {}",
                        target.display()
                    )))
                }
                Ok(_) => fs::remove_file(&target).map_err(|error| io_err(&target, error)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(io_err(&target, error)),
            }
        };
        if let Err(error) = result {
            failures.push(error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(CodexxError::Config(format!(
            "MCP 宿主文件恢复不完整: {}",
            failures.join("；")
        )))
    }
}

fn verify_installed_targets(entries: &[BackupEntry]) -> Result<()> {
    let mut changed = Vec::new();
    for entry in entries {
        let Some(expected) = entry.installed_sha256.as_deref() else {
            continue;
        };
        let target = PathBuf::from(&entry.target);
        match fs::symlink_metadata(&target) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_err(&target, error)),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                changed.push(target.display().to_string());
            }
            Ok(_) => {
                let actual = plain_file_sha256(&target)?;
                if actual != expected {
                    changed.push(target.display().to_string());
                }
            }
        }
    }
    if changed.is_empty() {
        Ok(())
    } else {
        Err(CodexxError::Config(format!(
            "宿主文件已在安装后被修改，已停止覆盖: {}",
            changed.join("；")
        )))
    }
}

pub(crate) fn apply_mcp_host_install(
    integration_id: &str,
    mode: &str,
    host_path: Option<&str>,
    managed_root: &Path,
) -> Result<HostInstallReport> {
    let _guard = HOST_INSTALL_LOCK
        .lock()
        .map_err(|_| CodexxError::Config("宿主软件安装锁已损坏，请重启 DevConduit".to_string()))?;
    let integration_id = validate_integration_id(integration_id)?;
    let host = detect_host_path(integration_id, host_path)?;
    let targets = build_install_targets(integration_id, host.as_deref(), Some(managed_root));
    let plan = plan_from_targets(integration_id, mode, host, targets.clone());
    if targets.is_empty() {
        return Ok(HostInstallReport {
            plan,
            installed: 0,
            backup_location: None,
        });
    }

    let source_hashes = targets
        .iter()
        .map(|target| plain_file_sha256(&target.source))
        .collect::<Result<Vec<_>>>()?;
    let backup = backup_root(integration_id)?;
    let mut entries = Vec::new();
    for (target, installed_sha256) in targets.iter().zip(source_hashes) {
        let backup_path = if target.target.exists() {
            if let Err(error) = reject_target_link(&target.target) {
                let _ = fs::remove_dir_all(&backup);
                return Err(error);
            }
            let path = backup.join(format!("original-{}", entries.len()));
            if let Err(error) = fs::copy(&target.target, &path) {
                let _ = fs::remove_dir_all(&backup);
                return Err(io_err(&path, error));
            }
            Some(path.display().to_string())
        } else {
            None
        };
        entries.push(BackupEntry {
            target: target.target.display().to_string(),
            backup: backup_path,
            installed_sha256: Some(installed_sha256),
        });
    }

    let manifest = BackupManifest {
        integration_id: integration_id.to_string(),
        created_at: crate::now_rfc3339(),
        restored: false,
        entries,
    };
    let manifest_path = backup.join("manifest.json");
    let mut text = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| CodexxError::Config(format!("生成 MCP 备份清单失败: {error}")))?;
    text.push(b'\n');
    if let Err(error) = atomic_write(&manifest_path, &text) {
        let _ = fs::remove_dir_all(&backup);
        return Err(error);
    }

    for target in &targets {
        if let Err(error) = copy_staged(&target.source, &target.target) {
            return match restore_entries(&manifest.entries) {
                Ok(()) => {
                    let _ = fs::remove_dir_all(&backup);
                    Err(error)
                }
                Err(rollback_error) => Err(CodexxError::Config(format!(
                    "{error}；自动回滚不完整，备份保留在 {}：{rollback_error}",
                    backup.display()
                ))),
            };
        }
    }

    let final_plan = manifest_plan(plan, &backup);
    Ok(HostInstallReport {
        plan: final_plan,
        installed: targets.len(),
        backup_location: Some(backup),
    })
}

fn manifest_plan(mut plan: McpHostInstallPlan, backup: &Path) -> McpHostInstallPlan {
    plan.can_restore = true;
    plan.message = format!("{}；已自动安装并创建备份", plan.message);
    let backup_message = format!("备份位置：{}", backup.display());
    plan.next_step = Some(match plan.next_step.take() {
        Some(next_step) => format!("{next_step}；{backup_message}"),
        None => backup_message,
    });
    plan
}

pub(crate) fn rollback_mcp_host_install(report: &HostInstallReport) {
    let Some(backup) = &report.backup_location else {
        return;
    };
    let manifest_path = backup.join("manifest.json");
    let Ok(text) = fs::read_to_string(&manifest_path) else {
        return;
    };
    let Ok(manifest) = serde_json::from_str::<BackupManifest>(&text) else {
        return;
    };
    if restore_entries(&manifest.entries).is_ok() {
        let _ = fs::remove_dir_all(backup);
    }
}

pub(crate) fn restore_latest_mcp_host_install_inner(integration_id: String) -> Result<String> {
    let _guard = HOST_INSTALL_LOCK
        .lock()
        .map_err(|_| CodexxError::Config("宿主软件恢复锁已损坏，请重启 DevConduit".to_string()))?;
    let integration_id = validate_integration_id(&integration_id)?;
    let root = app_home()?.join("mcp-backups").join(integration_id);
    if !is_plain_directory(&root) {
        return Err(CodexxError::Config("没有可恢复的 MCP 宿主备份".to_string()));
    }
    let mut candidates = fs::read_dir(&root)
        .map_err(|error| io_err(&root, error))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_plain_directory(path) && path.join("manifest.json").is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    let backup = candidates
        .into_iter()
        .rev()
        .find(|path| {
            fs::read_to_string(path.join("manifest.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<BackupManifest>(&text).ok())
                .is_some_and(|manifest| {
                    manifest.integration_id == integration_id && !manifest.restored
                })
        })
        .ok_or_else(|| CodexxError::Config("没有可恢复的 MCP 宿主备份".to_string()))?;
    let manifest_path = backup.join("manifest.json");
    let text = fs::read_to_string(&manifest_path).map_err(|error| io_err(&manifest_path, error))?;
    let mut manifest: BackupManifest = serde_json::from_str(&text)
        .map_err(|error| CodexxError::Config(format!("MCP 备份清单无效: {error}")))?;
    if manifest.integration_id != integration_id {
        return Err(CodexxError::Config(
            "MCP 备份清单与当前集成不匹配".to_string(),
        ));
    }
    verify_installed_targets(&manifest.entries)?;
    restore_entries(&manifest.entries)?;
    manifest.restored = true;
    let mut serialized = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| CodexxError::Config(format!("更新 MCP 备份清单失败: {error}")))?;
    serialized.push(b'\n');
    atomic_write(&manifest_path, &serialized)?;
    Ok(format!(
        "已恢复 MCP 宿主文件，备份位置：{}",
        backup.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ce_autorun_path_uses_installation_bin() {
        let root = PathBuf::from(if cfg!(target_os = "windows") {
            r"C:\Program Files\Cheat Engine 7.5"
        } else {
            "/tmp/Cheat Engine"
        });
        assert_eq!(ce_autorun_directory(&root), root.join("bin/autorun"));
    }

    #[test]
    fn x64dbg_targets_existing_architecture_plugin_dirs() {
        let root = std::env::temp_dir().join(format!(
            "devconduit-host-plan-{}",
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("release/x64/plugins")).expect("create x64 plugins");
        fs::create_dir_all(root.join("release/x32/plugins")).expect("create x32 plugins");
        let targets = x64dbg_plugin_directories(&root);
        assert!(targets.iter().any(|(arch, _)| arch == "x64"));
        assert!(targets.iter().any(|(arch, _)| arch == "x32"));
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn host_file_install_creates_backup_and_restores_original() {
        let root = std::env::temp_dir().join(format!(
            "devconduit-host-install-{}",
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let source = root.join("source.lua");
        let target = root.join("host/bin/autorun").join(CE_BRIDGE_NAME);
        fs::create_dir_all(target.parent().expect("target parent")).expect("create target parent");
        fs::write(&source, b"new").expect("write source");
        fs::write(&target, b"old").expect("write original");
        let backup = root.join("backup");
        fs::create_dir_all(&backup).expect("create backup");
        let original = backup.join("original");
        fs::copy(&target, &original).expect("backup original");
        copy_staged(&source, &target).expect("install file");
        assert_eq!(fs::read(&target).expect("read installed"), b"new");
        restore_entries(&[BackupEntry {
            target: target.display().to_string(),
            backup: Some(original.display().to_string()),
            installed_sha256: None,
        }])
        .expect("restore host file");
        assert_eq!(fs::read(&target).expect("read restored"), b"old");
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn restore_refuses_to_overwrite_a_changed_host_file() {
        let root = std::env::temp_dir().join(format!(
            "devconduit-host-changed-{}",
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create test root");
        let target = root.join("MCPx64dbg.dp64");
        fs::write(&target, b"installed").expect("write installed file");
        let installed_sha256 = plain_file_sha256(&target).expect("hash installed file");
        fs::write(&target, b"updated by user").expect("change installed file");

        let error = verify_installed_targets(&[BackupEntry {
            target: target.display().to_string(),
            backup: None,
            installed_sha256: Some(installed_sha256),
        }])
        .expect_err("changed host file must be preserved");

        assert!(error.to_string().contains("已在安装后被修改"));
        assert_eq!(
            fs::read(&target).expect("read changed file"),
            b"updated by user"
        );
        fs::remove_dir_all(root).expect("remove test root");
    }
}
