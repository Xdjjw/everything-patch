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
pub(crate) fn install_zcode(system_content: &str) -> Result<()> {
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
    let launcher_parent = paths.launcher.parent().unwrap_or(Path::new(".")).to_path_buf();
    for dir in [&paths.managed_dir, &launcher_parent, &paths.log_dir, &paths.cache_dir] {
        ensure_directory(dir)?;
    }

    // 写 system-role.md
    write_text(&paths.system_file, &normalized)?;
    // 写 config.json
    let config = wrapper::render_config(&plan);
    write_text(&paths.config_file, &config)?;
    // 写 launcher.js（Node.js 脚本，无需 Python）
    let launcher_content = wrapper::render_launcher();
    write_text(&paths.launcher, &launcher_content)?;
    // 写 patch.js sidecar（patch 参数）
    let sidecar_content = wrapper::render_patch_sidecar();
    write_text(&paths.patch_sidecar, &sidecar_content)?;

    // 激活环境变量
    let vars = env_values(&plan)?;
    let _ = activate_current_session(&vars)?;

    // macOS：额外写 env 脚本与 LaunchAgent
    #[cfg(target_os = "macos")]
    {
        let (env_script_path, env_script, launch_agent) = platform::render_env_artifacts(&plan)?;
        write_text(&env_script_path, &env_script)?;
        let launch_agent_path = platform::launch_agent_path();
        write_text(&launch_agent_path, &launch_agent)?;
    }

    Ok(())
}

/// 卸载受管入口：备份并移除受管文件 + 清除环境变量。
pub(crate) fn uninstall_zcode() -> Result<bool> {
    let paths = build_paths()?;
    let mut removed = false;
    for path in [&paths.system_file, &paths.config_file, &paths.launcher, &paths.patch_sidecar] {
        if path.exists() {
            backup_zcode_file(path)?;
            removed = true;
        }
    }
    // macOS：移除 env 脚本与 LaunchAgent
    #[cfg(target_os = "macos")]
    {
        let env_script = paths.launcher.parent().unwrap_or(Path::new(".")).join(ZCODE_ENV_SCRIPT_NAME);
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

/// 备份文件（改名为 .bak_时间戳）。
fn backup_zcode_file(path: &Path) -> Result<()> {
    use chrono::Local;
    let ts = Local::now().format("%Y%m%d_%H%M%S");
    let backup = path.with_name(format!(
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
