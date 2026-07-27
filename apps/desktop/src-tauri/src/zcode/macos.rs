//! macOS 专属：ZCode 路径发现、环境变量持久化（launchctl + LaunchAgent）、进程检测。

use crate::constants::{
    ZCODE_AGENT_ARGS_JSON, ZCODE_AGENT_OVERRIDE_NEEDLE, ZCODE_ENV_SCRIPT_NAME,
    ZCODE_LAUNCH_AGENT_LABEL,
};
use crate::error::{CodexxError, Result};
use crate::zcode::{EnvVars, ZcodeInstallPlan};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_ZCODE_APP: &str = "/Applications/ZCode.app";

/// 发现 ZCode.app 路径：环境变量 > mdfind > 默认。
pub(crate) fn discover_zcode_app() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("ZCODE_APP_PATH") {
        let p = PathBuf::from(path);
        if p.exists() && p.is_dir() {
            return Ok(p);
        }
    }
    // mdfind 查找 bundle
    let output = Command::new("mdfind")
        .args(["kMDItemCFBundleIdentifier == 'dev.zcode.app'"])
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let p = PathBuf::from(line.trim());
                if p.exists() && p.is_dir() {
                    return Ok(p);
                }
            }
        }
    }
    let default = PathBuf::from(DEFAULT_ZCODE_APP);
    if default.exists() {
        Ok(default)
    } else {
        Err(CodexxError::Config(format!(
            "未找到 ZCode.app，可设置 ZCODE_APP_PATH 环境变量指定路径"
        )))
    }
}

/// 从 app 根目录解析 runtime 与 node 命令。
pub(crate) fn resolve_runtime_and_node(app_root: &Path) -> (PathBuf, PathBuf) {
    let runtime = app_root.join("Contents/Resources/glm/zcode.cjs");
    let helper = app_root
        .join("Contents/Frameworks/ZCode Helper.app/Contents/MacOS/ZCode Helper");
    let main = app_root.join("Contents/MacOS/ZCode");
    let node = if helper.exists() { helper } else { main };
    (runtime, node)
}

/// 检查 app.asar 是否包含 agent-server 覆盖入口。
pub(crate) fn app_supports_agent_override(app_root: &Path) -> bool {
    let asar = app_root.join("Contents/Resources/app.asar");
    if !asar.exists() || !asar.is_file() {
        return false;
    }
    fs::read(&asar)
        .map(|bytes| bytes.windows(ZCODE_AGENT_OVERRIDE_NEEDLE.len()).any(|w| w == ZCODE_AGENT_OVERRIDE_NEEDLE.as_bytes()))
        .unwrap_or(false)
}

/// 检查 ZCode 主进程是否在运行。
pub(crate) fn is_zcode_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "ZCode"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 构建环境变量键值表（command 直接是 wrapper 路径，靠 shebang 执行）。
pub(crate) fn env_values(plan: &ZcodeInstallPlan) -> Result<EnvVars> {
    let mut vars = EnvVars::new();
    vars.insert("ZCODE_AGENT_SERVER_COMMAND".to_string(), plan.node_command.display().to_string());
    let args_json = format!(
        "[{},{}]",
        serde_json::to_string(&plan.paths.launcher.display().to_string()).unwrap_or_default(),
        ZCODE_AGENT_ARGS_JSON.trim_start_matches('[').trim_end_matches(']')
    );
    vars.insert("ZCODE_AGENT_SERVER_ARGS_JSON".to_string(), args_json);
    vars.insert("ZCODE_KEYSMITH_SYSTEM_FILE".to_string(), plan.paths.system_file.display().to_string());
    vars.insert("ZCODE_KEYSMITH_ORIGINAL".to_string(), plan.zcode_runtime.display().to_string());
    vars.insert("ZCODE_KEYSMITH_NODE_COMMAND".to_string(), plan.node_command.display().to_string());
    vars.insert("ZCODE_KEYSMITH_CACHE_DIR".to_string(), plan.paths.cache_dir.display().to_string());
    vars.insert("ZCODE_KEYSMITH_LOG_DIR".to_string(), plan.paths.log_dir.display().to_string());
    Ok(vars)
}

/// 激活当前会话：launchctl setenv 逐个设置。
pub(crate) fn activate_current_session(vars: &EnvVars) -> Result<Vec<String>> {
    let mut results = Vec::new();
    for (key, value) in vars {
        let output = Command::new("launchctl")
            .args(["setenv", key, value])
            .output()
            .map_err(|e| CodexxError::Config(format!("launchctl setenv {key} 失败: {e}")))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(CodexxError::Config(format!("launchctl setenv {key} 失败: {detail}")));
        }
        results.push(format!("launchctl setenv {}: ok", key));
    }
    Ok(results)
}

/// 清除当前会话环境变量。
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
    let mut results = Vec::new();
    for key in keys {
        let _ = Command::new("launchctl").args(["unsetenv", key]).output();
        results.push(format!("launchctl unsetenv {}: ok", key));
    }
    Ok(results)
}

/// LaunchAgent plist 路径。
pub(crate) fn launch_agent_path() -> PathBuf {
    crate::paths::home_dir()
        .unwrap_or_else(|_| PathBuf::from("~"))
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", ZCODE_LAUNCH_AGENT_LABEL))
}

/// 生成 env 脚本与 LaunchAgent plist 内容。
/// 返回 (env_script_path, env_script_content, launch_agent_content)。
pub(crate) fn render_env_artifacts(plan: &ZcodeInstallPlan) -> Result<(PathBuf, String, String)> {
    let vars = env_values(plan)?;
    let env_script_path = plan.paths.launcher.parent().unwrap_or(Path::new(".")).join(ZCODE_ENV_SCRIPT_NAME);

    let mut script_lines = vec!["#!/bin/sh".to_string(), "set -eu".to_string()];
    for (key, value) in &vars {
        script_lines.push(format!("launchctl setenv {} {}", key, sh_single_quote(value)));
    }
    script_lines.push(String::new());
    let env_script = script_lines.join("\n");

    let log_dir = plan.paths.log_dir.display().to_string();
    let env_script_str = env_script_path.display().to_string();
    let launch_agent = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{script}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log_dir}/launchagent.out.log</string>
    <key>StandardErrorPath</key>
    <string>{log_dir}/launchagent.err.log</string>
</dict>
</plist>
"#,
        label = ZCODE_LAUNCH_AGENT_LABEL,
        script = env_script_str,
        log_dir = log_dir,
    );

    Ok((env_script_path, env_script, launch_agent))
}

fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
