//! ZCode App 受管 system-role 入口管理（跨平台 macOS + Windows）。
//!
//! 机制：写 `~/.zcode-keysmith/system-role.md` + 生成 Node.js launcher +
//! 设置 `ZCODE_AGENT_SERVER_COMMAND` 等环境变量，让 ZCode 启动 agent-server 时
//! 通过 launcher（使用 ZCode 自带 Electron node 执行）运行 patched runtime。
//!
//! 平台分发仿 `skin_runtime/`：mod.rs 统一入口 + #[cfg] 块分派到 macos/windows。

mod wrapper;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as platform;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;

use crate::constants::*;
use crate::error::{CodexxError, Result};
use crate::file_io::{ensure_directory, io_err, read_to_string_if_exists, write_text};
use crate::paths::home_dir;
use crate::prompts::PromptInjectionMode;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 受管安装路径集合。
#[derive(Debug, Clone)]
pub(crate) struct ZcodePaths {
    pub managed_dir: PathBuf,
    pub system_file: PathBuf,
    pub config_file: PathBuf,
    pub launcher: PathBuf,
    pub patch_sidecar: PathBuf,
    pub log_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub launcher_log: PathBuf,
}

/// 安装计划（写入前收集的全部信息）。
#[derive(Debug, Clone)]
pub(crate) struct ZcodeInstallPlan {
    pub paths: ZcodePaths,
    pub zcode_runtime: PathBuf,
    pub node_command: PathBuf,
    pub zcode_app: PathBuf,
}

/// 环境变量键值表。
pub(crate) type EnvVars = BTreeMap<String, String>;

/// 受管目录：`~/.zcode-keysmith`。
pub(crate) fn zcode_managed_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(ZCODE_KEYSMITH_DIRNAME))
}

/// 构建受管路径集合。
pub(crate) fn build_paths() -> Result<ZcodePaths> {
    let managed_dir = zcode_managed_dir()?;
    let bin_dir = managed_dir.join("bin");
    let log_dir = managed_dir.join(ZCODE_LOG_DIRNAME);
    let cache_dir = managed_dir.join(ZCODE_CACHE_DIRNAME);
    Ok(ZcodePaths {
        launcher_log: log_dir.join(ZCODE_LAUNCHER_LOG_NAME),
        system_file: managed_dir.join(ZCODE_SYSTEM_ROLE_FILENAME),
        config_file: managed_dir.join(ZCODE_CONFIG_FILENAME),
        launcher: bin_dir.join(ZCODE_LAUNCHER_NAME),
        patch_sidecar: bin_dir.join(ZCODE_PATCH_SIDECAR_NAME),
        managed_dir,
        log_dir,
        cache_dir,
    })
}

/// 发现 ZCode 安装路径（平台分发）。
pub(crate) fn discover_zcode_app() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        platform::discover_zcode_app()
    }
    #[cfg(target_os = "windows")]
    {
        platform::discover_zcode_app()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        unsupported::discover_zcode_app()
    }
}

pub(crate) fn detect_zcode_version() -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        discover_zcode_app()
            .ok()
            .and_then(|app_root| platform::detect_zcode_version(&app_root))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        discover_zcode_app()
            .ok()
            .and_then(|app_root| unsupported::detect_zcode_version(&app_root))
    }
}

/// 从 app 根目录解析 runtime 与 node 命令路径（平台分发）。
pub(crate) fn resolve_runtime_and_node(app_root: &Path) -> (PathBuf, PathBuf) {
    #[cfg(target_os = "macos")]
    {
        platform::resolve_runtime_and_node(app_root)
    }
    #[cfg(target_os = "windows")]
    {
        platform::resolve_runtime_and_node(app_root)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app_root;
        (PathBuf::new(), PathBuf::new())
    }
}

/// 检查 ZCode 是否支持 agent-server 环境变量覆盖（平台分发）。
pub(crate) fn app_supports_agent_override(app_root: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        platform::app_supports_agent_override(app_root)
    }
    #[cfg(target_os = "windows")]
    {
        platform::app_supports_agent_override(app_root)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app_root;
        false
    }
}

/// 检查 ZCode 主进程是否正在运行（平台分发）。
pub(crate) fn is_zcode_running() -> bool {
    #[cfg(target_os = "macos")]
    {
        platform::is_zcode_running()
    }
    #[cfg(target_os = "windows")]
    {
        platform::is_zcode_running()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// 流式判断文件是否包含某段字节，避免把 app.asar（常见 100MB+）整个读进内存。
///
/// 两个平台的 `app_supports_agent_override` 共用；块间保留 needle-1 字节重叠，
/// 保证跨块边界的匹配不会漏掉。
pub(crate) fn file_contains_needle(path: &Path, needle: &str) -> bool {
    use std::io::Read;

    let needle = needle.as_bytes();
    if needle.is_empty() {
        return true;
    }
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut reader = std::io::BufReader::new(file);
    let overlap = needle.len() - 1;
    let chunk_size = (1024 * 1024).max(needle.len() * 2);
    let mut buffer = vec![0_u8; overlap + chunk_size];
    let mut filled = 0_usize;
    loop {
        let read = match reader.read(&mut buffer[filled..]) {
            Ok(0) => 0,
            Ok(count) => count,
            Err(_) => return false,
        };
        if read == 0 {
            return buffer[..filled].windows(needle.len()).any(|w| w == needle);
        }
        filled += read;
        if filled < buffer.len() {
            continue;
        }
        if buffer[..filled].windows(needle.len()).any(|w| w == needle) {
            return true;
        }
        buffer.copy_within(filled - overlap..filled, 0);
        filled = overlap;
    }
}

/// 递归搜索目录内是否有文件包含某段字节（用于 asar 被解包成目录的安装）。
pub(crate) fn dir_contains_needle(dir: &Path, needle: &str, max_depth: usize) -> bool {
    if max_depth == 0 {
        return false;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            directories.push(path);
        } else if file_type.is_file() && file_contains_needle(&path, needle) {
            return true;
        }
    }
    directories
        .into_iter()
        .any(|child| dir_contains_needle(&child, needle, max_depth - 1))
}

/// 检查 runtime 是否包含 patch 锚点。
pub(crate) fn runtime_patchable(runtime_path: &Path) -> bool {
    if !runtime_path.exists() || !runtime_path.is_file() {
        return false;
    }
    fs::read_to_string(runtime_path)
        .map(|text| text.contains(ZCODE_PATCH_NEEDLE))
        .unwrap_or(false)
}

/// 构建安装计划：发现 ZCode 安装 + 解析路径 + 校验。
pub(crate) fn build_install_plan() -> Result<ZcodeInstallPlan> {
    let paths = build_paths()?;
    let zcode_app = discover_zcode_app()?;
    let (zcode_runtime, node_command) = resolve_runtime_and_node(&zcode_app);
    if !zcode_runtime.exists() {
        return Err(CodexxError::Config(format!(
            "未找到 ZCode runtime: {}",
            zcode_runtime.display()
        )));
    }
    if !node_command.exists() {
        return Err(CodexxError::Config(format!(
            "未找到 ZCode node 命令: {}",
            node_command.display()
        )));
    }
    Ok(ZcodeInstallPlan {
        paths,
        zcode_runtime,
        node_command,
        zcode_app,
    })
}

/// 构建环境变量键值表（平台分发）。
pub(crate) fn env_values(plan: &ZcodeInstallPlan) -> Result<EnvVars> {
    #[cfg(target_os = "macos")]
    {
        platform::env_values(plan)
    }
    #[cfg(target_os = "windows")]
    {
        platform::env_values(plan)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = plan;
        Ok(EnvVars::new())
    }
}

/// 激活当前会话环境变量（平台分发）。
pub(crate) fn activate_current_session(vars: &EnvVars) -> Result<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        platform::activate_current_session(vars)
    }
    #[cfg(target_os = "windows")]
    {
        platform::activate_current_session(vars)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = vars;
        Ok(vec!["skipped (unsupported platform)".to_string()])
    }
}

/// 清除当前会话环境变量（平台分发）。
pub(crate) fn unset_current_session_env() -> Result<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        platform::unset_current_session_env()
    }
    #[cfg(target_os = "windows")]
    {
        platform::unset_current_session_env()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(vec!["skipped (unsupported platform)".to_string()])
    }
}

/// 安装受管入口：写 system-role.md + config.json + wrapper + 激活环境变量。
pub(crate) fn install_zcode(
    system_content: &str,
    injection_mode: PromptInjectionMode,
    template_key: &str,
    title: &str,
) -> Result<()> {
    let plan = build_install_plan()?;
    let normalized = wrapper::normalize_system_prompt_content(system_content);
    if normalized.trim().is_empty() {
        return Err(CodexxError::Config("system-role 内容为空".to_string()));
    }
    if !runtime_patchable(&plan.zcode_runtime) {
        return Err(CodexxError::Config(format!(
            "ZCode runtime patch 锚点未找到，可能版本不兼容: {}",
            plan.zcode_runtime.display()
        )));
    }

    let paths = &plan.paths;
    let launcher_parent = paths
        .launcher
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    for dir in [
        &paths.managed_dir,
        &launcher_parent,
        &paths.log_dir,
        &paths.cache_dir,
    ] {
        ensure_directory(dir)?;
    }

    // 写 system-role.md
    write_text(&paths.system_file, &normalized)?;
    // 写 config.json
    let config = wrapper::render_config(&plan, injection_mode, template_key, title);
    write_text(&paths.config_file, &config)?;
    // 写 launcher.js（Node.js 脚本，无需 Python）
    let launcher_content = wrapper::render_launcher();
    write_text(&paths.launcher, &launcher_content)?;
    // 写 patch.js sidecar（patch 参数）
    let sidecar_content = wrapper::render_patch_sidecar(injection_mode);
    write_text(&paths.patch_sidecar, &sidecar_content)?;

    // 激活环境变量
    let vars = env_values(&plan)?;
    let _ = activate_current_session(&vars)?;

    // macOS：额外写 env 脚本与 LaunchAgent
    #[cfg(target_os = "macos")]
    {
        platform::write_env_artifacts(&plan)?;
    }

    Ok(())
}

/// 卸载受管入口：备份并移除受管文件 + 清除环境变量。
pub(crate) fn uninstall_zcode() -> Result<bool> {
    let paths = build_paths()?;
    let mut removed = false;
    for path in [
        &paths.system_file,
        &paths.config_file,
        &paths.launcher,
        &paths.patch_sidecar,
    ] {
        if path.exists() {
            backup_zcode_file(path)?;
            removed = true;
        }
    }
    // macOS：移除 env 脚本与 LaunchAgent
    #[cfg(target_os = "macos")]
    {
        let env_script = paths
            .launcher
            .parent()
            .unwrap_or(Path::new("."))
            .join(ZCODE_ENV_SCRIPT_NAME);
        if env_script.exists() {
            backup_zcode_file(&env_script)?;
            removed = true;
        }
        let launch_agent = platform::launch_agent_path();
        if launch_agent.exists() {
            backup_zcode_file(&launch_agent)?;
            removed = true;
        }
    }
    let _ = unset_current_session_env()?;
    Ok(removed)
}

pub(crate) fn sync_restored_environment() -> Result<()> {
    let paths = build_paths()?;
    let active = paths.system_file.is_file()
        && paths.config_file.is_file()
        && paths.launcher.is_file()
        && paths.patch_sidecar.is_file();
    if active {
        let plan = build_install_plan()?;
        let vars = env_values(&plan)?;
        activate_current_session(&vars)?;
        #[cfg(target_os = "macos")]
        {
            platform::write_env_artifacts(&plan)?;
        }
    } else {
        unset_current_session_env()?;
        #[cfg(target_os = "macos")]
        {
            let env_script = paths
                .launcher
                .parent()
                .unwrap_or(Path::new("."))
                .join(ZCODE_ENV_SCRIPT_NAME);
            for artifact in [env_script, platform::launch_agent_path()] {
                if artifact.exists() {
                    fs::remove_file(&artifact).map_err(|e| io_err(&artifact, e))?;
                }
            }
        }
    }
    Ok(())
}

/// 备份文件（改名为 .bak_时间戳）。
fn backup_zcode_file(path: &Path) -> Result<()> {
    use chrono::Local;
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    let backup = path.with_file_name(format!(
        "{}.bak_{}",
        path.file_name().and_then(|e| e.to_str()).unwrap_or("file"),
        ts
    ));
    fs::rename(path, &backup).map_err(|e| io_err(path, e))?;
    Ok(())
}

/// 当前 system-role.md 内容（若存在）。
pub(crate) fn current_system_role_content() -> Result<Option<String>> {
    let paths = build_paths()?;
    if !paths.system_file.exists() {
        return Ok(None);
    }
    let content = read_to_string_if_exists(&paths.system_file)?;
    if content.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(content))
    }
}

pub(crate) fn current_install_metadata(
) -> Result<(Option<PromptInjectionMode>, Option<String>, Option<String>)> {
    let paths = build_paths()?;
    if !paths.config_file.is_file() {
        return Ok((None, None, None));
    }
    let text = read_to_string_if_exists(&paths.config_file)?;
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        // A damaged diagnostic config must not block backup, uninstall, or repair.
        return Ok((Some(PromptInjectionMode::Append), None, None));
    };
    let mode = value
        .get("injection_mode")
        .and_then(|item| item.as_str())
        .and_then(|item| PromptInjectionMode::parse(Some(item)).ok())
        // Older managed installs preserved an existing native system prompt.
        .or(Some(PromptInjectionMode::Append));
    let template_key = value
        .get("template_key")
        .and_then(|item| item.as_str())
        .map(ToString::to_string);
    let title = value
        .get("title")
        .and_then(|item| item.as_str())
        .map(ToString::to_string);
    Ok((mode, template_key, title))
}

/// 内置模板元数据。
pub(crate) fn zcode_builtin_content(template_id: &str) -> Result<(String, String, String, String)> {
    let id = if template_id.trim().is_empty() {
        ZCODE_BUILTIN_ID
    } else {
        template_id.trim()
    };
    if id != ZCODE_BUILTIN_ID {
        return Err(CodexxError::Config(format!("未知的 ZCode 内置模板: {id}")));
    }
    Ok((
        ZCODE_BUILTIN_FILENAME.to_string(),
        format!("./{}", ZCODE_BUILTIN_FILENAME),
        ZCODE_BUILTIN_CONTENT.to_string(),
        "打包内置".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "everything-patch-zcode-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    #[test]
    fn file_search_matches_across_streaming_chunk_boundaries() {
        let root = temp_dir("stream-boundary");
        let path = root.join("app.asar");
        let needle = ZCODE_AGENT_OVERRIDE_NEEDLE;
        let overlap = needle.len() - 1;
        let buffer_len = 1024 * 1024 + overlap;
        let mut bytes = vec![b'x'; buffer_len - 5];
        bytes.extend_from_slice(needle.as_bytes());
        bytes.extend_from_slice(b"tail");
        fs::write(&path, bytes).expect("write asar fixture");

        assert!(file_contains_needle(&path, needle));
        assert!(!file_contains_needle(&path, "missing-agent-override"));

        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn directory_search_handles_unpacked_asar_layout() {
        let root = temp_dir("unpacked-asar");
        let host = root.join("out/host");
        fs::create_dir_all(&host).expect("create unpacked asar directories");
        fs::write(
            host.join("index.js"),
            format!("const override = '{ZCODE_AGENT_OVERRIDE_NEEDLE}';"),
        )
        .expect("write unpacked asar fixture");

        assert!(dir_contains_needle(&root, ZCODE_AGENT_OVERRIDE_NEEDLE, 3));
        assert!(!dir_contains_needle(&root, ZCODE_AGENT_OVERRIDE_NEEDLE, 2));

        fs::remove_dir_all(root).expect("remove test directory");
    }
}
