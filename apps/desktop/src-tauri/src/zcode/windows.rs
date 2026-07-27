//! Windows 专属：ZCode 路径发现、环境变量持久化（注册表 + WM_SETTINGCHANGE 广播）、进程检测。

use crate::constants::{
    ZCODE_AGENT_ARGS_JSON, ZCODE_AGENT_OVERRIDE_NEEDLE, ZCODE_RUNTIME_RELPATH,
};
use crate::error::{CodexxError, Result};
use crate::platform::program_command;
use crate::zcode::{EnvVars, ZcodeInstallPlan};
use std::path::{Path, PathBuf};
use std::process::Command;

const ZCODE_EXE_NAME: &str = "ZCode.exe";
const ZCODE_APP_DIRNAME: &str = "ZCode";

/// 发现 ZCode 安装路径：环境变量 > LocalAppData > AppData > ProgramFiles。
pub(crate) fn discover_zcode_app() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("ZCODE_APP_PATH") {
        let p = PathBuf::from(path);
        if p.exists() && p.is_dir() && validate_zcode_root(&p) {
            return Ok(p);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        candidates.push(PathBuf::from(&local_app_data).join("Programs").join(ZCODE_APP_DIRNAME));
        candidates.push(PathBuf::from(&local_app_data).join(ZCODE_APP_DIRNAME));
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        candidates.push(PathBuf::from(&app_data).join(ZCODE_APP_DIRNAME));
    }
    if let Ok(pf) = std::env::var("ProgramFiles") {
        candidates.push(PathBuf::from(&pf).join(ZCODE_APP_DIRNAME));
    }
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        candidates.push(PathBuf::from(&pf86).join(ZCODE_APP_DIRNAME));
    }
    for candidate in &candidates {
        if candidate.exists() && candidate.is_dir() && validate_zcode_root(candidate) {
            return Ok(candidate.clone());
        }
    }
    Err(CodexxError::Config(
        "未找到 ZCode 安装目录，可设置 ZCODE_APP_PATH 环境变量指定路径".to_string(),
    ))
}

/// 校验目录是否为 ZCode 安装根（含 ZCode.exe 和 resources/glm/zcode.cjs）。
fn validate_zcode_root(root: &Path) -> bool {
    root.join(ZCODE_EXE_NAME).exists() && root.join(ZCODE_RUNTIME_RELPATH).exists()
}

/// 从 app 根目录解析 runtime 与 node 命令。
/// Windows 上 node 命令就是 ZCode.exe（设 ELECTRON_RUN_AS_NODE=1 后当 node 用）。
pub(crate) fn resolve_runtime_and_node(app_root: &Path) -> (PathBuf, PathBuf) {
    let runtime = app_root.join(ZCODE_RUNTIME_RELPATH);
    let node = app_root.join(ZCODE_EXE_NAME);
    (runtime, node)
}

/// 检查 ZCode 是否支持 agent-server 覆盖。
/// Windows 上 app.asar 是目录，搜索 out/host/index.js 内是否含环境变量名。
pub(crate) fn app_supports_agent_override(app_root: &Path) -> bool {
    let asar_dir = app_root.join("resources/app.asar");
    if !asar_dir.exists() || !asar_dir.is_dir() {
        // 某些版本可能是文件
        let asar_file = app_root.join("resources/app.asar");
        if asar_file.exists() && asar_file.is_file() {
            return std::fs::read(&asar_file)
                .map(|bytes| bytes.windows(ZCODE_AGENT_OVERRIDE_NEEDLE.len()).any(|w| w == ZCODE_AGENT_OVERRIDE_NEEDLE.as_bytes()))
                .unwrap_or(false);
        }
        return false;
    }
    search_dir_for_needle(&asar_dir, ZCODE_AGENT_OVERRIDE_NEEDLE, 3)
}

/// 递归搜索目录内文件是否包含某字符串。
fn search_dir_for_needle(dir: &Path, needle: &str, max_depth: usize) -> bool {
    if max_depth == 0 {
        return false;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if search_dir_for_needle(&path, needle, max_depth - 1) {
                return true;
            }
        } else if path.is_file() {
            if let Ok(metadata) = path.metadata() {
                if metadata.len() < 50 * 1024 * 1024 {
                    if let Ok(bytes) = std::fs::read(&path) {
                        if bytes.windows(needle.len()).any(|w| w == needle.as_bytes()) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// 检查 ZCode 主进程是否在运行。
pub(crate) fn is_zcode_running() -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {}", ZCODE_EXE_NAME), "/NH"])
        .output();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains(ZCODE_EXE_NAME)
        }
        Err(_) => false,
    }
}

/// 构建环境变量键值表。
/// Windows 上 ZCODE_AGENT_SERVER_COMMAND 设为 ZCode.exe（Electron node），
/// launcher.js 路径放进 ZCODE_AGENT_SERVER_ARGS_JSON 首项。
pub(crate) fn env_values(plan: &ZcodeInstallPlan) -> Result<EnvVars> {
    let wrapper_path = plan.paths.launcher.display().to_string();
    // args JSON: ["<launcher.js>", "app-server", "--stdio"]
    let args_json = format!(
        "[{},{}]",
        serde_json::to_string(&wrapper_path).unwrap_or_default(),
        ZCODE_AGENT_ARGS_JSON.trim_start_matches('[').trim_end_matches(']')
    );

    let mut vars = EnvVars::new();
    vars.insert("ZCODE_AGENT_SERVER_COMMAND".to_string(), plan.node_command.display().to_string());
    vars.insert("ZCODE_AGENT_SERVER_ARGS_JSON".to_string(), args_json);
    vars.insert("ZCODE_KEYSMITH_SYSTEM_FILE".to_string(), plan.paths.system_file.display().to_string());
    vars.insert("ZCODE_KEYSMITH_ORIGINAL".to_string(), plan.zcode_runtime.display().to_string());
    vars.insert("ZCODE_KEYSMITH_NODE_COMMAND".to_string(), plan.node_command.display().to_string());
    vars.insert("ZCODE_KEYSMITH_CACHE_DIR".to_string(), plan.paths.cache_dir.display().to_string());
    vars.insert("ZCODE_KEYSMITH_LOG_DIR".to_string(), plan.paths.log_dir.display().to_string());
    Ok(vars)
}

/// 激活当前会话：通过 PowerShell [Environment]::SetEnvironmentVariable 写注册表 + 广播。
pub(crate) fn activate_current_session(vars: &EnvVars) -> Result<Vec<String>> {
    let mut results = Vec::new();
    let ps = powershell_path();
    for (key, value) in vars {
        let script = format!(
            "[Environment]::SetEnvironmentVariable('{}', '{}', 'User')",
            key.replace('\'', "''"),
            value.replace('\'', "''")
        );
        let output = program_command(&ps, &["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .map_err(|e| CodexxError::Config(format!("设置环境变量 {key} 失败: {e}")))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(CodexxError::Config(format!("设置环境变量 {key} 失败: {detail}")));
        }
        results.push(format!("setenv {}: ok", key));
    }
    Ok(results)
}

/// 清除当前会话环境变量：通过 PowerShell 设为 $null。
pub(crate) fn unset_current_session_env() -> Result<Vec<String>> {
    let keys = [
        "ZCODE_AGENT_SERVER_COMMAND",
        "ZCODE_AGENT_SERVER_ARGS_JSON",
        "ZCODE_KEYSMITH_SYSTEM_FILE",
        "ZCODE_KEYSMITH_ORIGINAL",
        "ZCODE_KEYSMITH_NODE_COMMAND",
        "ZCODE_KEYSMITH_CACHE_DIR",
        "ZCODE_KEYSMITH_LOG_DIR",
    ];
    let ps = powershell_path();
    let mut results = Vec::new();
    for key in keys {
        let script = format!(
            "[Environment]::SetEnvironmentVariable('{}', $null, 'User')",
            key
        );
        let _ = program_command(&ps, &["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &script]).output();
        results.push(format!("unsetenv {}: ok", key));
    }
    Ok(results)
}

/// PowerShell 可执行文件路径。
fn powershell_path() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| root.join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("powershell.exe"))
}
