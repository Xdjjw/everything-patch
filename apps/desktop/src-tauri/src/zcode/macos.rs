//! macOS 专属：ZCode 路径发现、环境变量持久化（launchctl + LaunchAgent）、进程检测。

use crate::constants::{
    ZCODE_AGENT_ARGS_JSON, ZCODE_AGENT_OVERRIDE_NEEDLE, ZCODE_ENV_SCRIPT_NAME,
    ZCODE_LAUNCH_AGENT_LABEL,
};
use crate::error::{CodexxError, Result};
use crate::file_io::{io_err, write_text};
use crate::zcode::{EnvVars, ZcodeInstallPlan};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_ZCODE_APP: &str = "/Applications/ZCode.app";

/// 校验目录是否为 ZCode 安装根（含 ZCode 可执行文件与 glm/zcode.cjs runtime）。
///
/// 与 Windows 的 `validate_zcode_root` 对齐：没有这层校验时，`ZCODE_APP_PATH`
/// 或 mdfind 返回的任意目录都会被当成有效安装，后续才在 `build_install_plan`
/// 里报出难以理解的 "未找到 ZCode runtime"。
fn validate_zcode_root(root: &Path) -> bool {
    let (runtime, _) = resolve_runtime_and_node(root);
    runtime.is_file() && root.join("Contents/MacOS/ZCode").is_file()
}

/// 发现 ZCode.app 路径：环境变量 > mdfind > 默认。
pub(crate) fn discover_zcode_app() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("ZCODE_APP_PATH") {
        let p = PathBuf::from(path);
        if p.is_dir() && validate_zcode_root(&p) {
            return Ok(p);
        }
    }
    // mdfind 查找 bundle
    let output = Command::new("/usr/bin/mdfind")
        .args(["kMDItemCFBundleIdentifier == 'dev.zcode.app'"])
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let p = PathBuf::from(line.trim());
                if p.is_dir() && validate_zcode_root(&p) {
                    return Ok(p);
                }
            }
        }
    }
    let default = PathBuf::from(DEFAULT_ZCODE_APP);
    if default.is_dir() && validate_zcode_root(&default) {
        Ok(default)
    } else {
        Err(CodexxError::Config(
            "未找到 ZCode.app，可设置 ZCODE_APP_PATH 环境变量指定路径".to_string(),
        ))
    }
}

pub(crate) fn detect_zcode_version(app_root: &Path) -> Option<String> {
    let plist = app_root.join("Contents").join("Info.plist");
    for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
        let output = Command::new("/usr/bin/plutil")
            .arg("-extract")
            .arg(key)
            .arg("raw")
            .arg("-o")
            .arg("-")
            .arg(&plist)
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !version.is_empty() && version != "(null)" {
            return Some(format!("ZCode {version}"));
        }
    }
    None
}

/// 从 app 根目录解析 runtime 与 node 命令。
pub(crate) fn resolve_runtime_and_node(app_root: &Path) -> (PathBuf, PathBuf) {
    let runtime = app_root.join("Contents/Resources/glm/zcode.cjs");
    let helper = app_root.join("Contents/Frameworks/ZCode Helper.app/Contents/MacOS/ZCode Helper");
    let main = app_root.join("Contents/MacOS/ZCode");
    let node = if helper.is_file() { helper } else { main };
    (runtime, node)
}

/// 检查 app.asar 是否包含 agent-server 覆盖入口。
///
/// 与 Windows 实现对齐：asar 既可能是单个文件，也可能被解包成目录。
pub(crate) fn app_supports_agent_override(app_root: &Path) -> bool {
    let asar = app_root.join("Contents/Resources/app.asar");
    if asar.is_file() {
        return crate::zcode::file_contains_needle(&asar, ZCODE_AGENT_OVERRIDE_NEEDLE);
    }
    if asar.is_dir() {
        return crate::zcode::dir_contains_needle(&asar, ZCODE_AGENT_OVERRIDE_NEEDLE, 3);
    }
    false
}

/// 检查 ZCode 主进程是否在运行。
pub(crate) fn is_zcode_running() -> bool {
    if Command::new("/usr/bin/pgrep")
        .args(["-x", "ZCode"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    let Ok(app_root) = discover_zcode_app() else {
        return false;
    };
    let executable = app_root.join("Contents/MacOS/ZCode");
    let Ok(output) = Command::new("/bin/ps").args(["-axo", "command="]).output() else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| command_line_starts_with_executable(line, &executable))
}

fn command_line_starts_with_executable(line: &str, executable: &Path) -> bool {
    let expected = executable.to_string_lossy();
    let trimmed = line.trim_start();
    let remainder = trimmed.strip_prefix(expected.as_ref()).or_else(|| {
        let quoted = format!("\"{expected}\"");
        trimmed.strip_prefix(quoted.as_str())
    });
    remainder
        .is_some_and(|rest| rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace))
}

/// 构建环境变量键值表（command 使用 ZCode 内置 Electron，launcher 路径放在 args 中）。
pub(crate) fn env_values(plan: &ZcodeInstallPlan) -> Result<EnvVars> {
    let mut vars = EnvVars::new();
    vars.insert(
        "ZCODE_AGENT_SERVER_COMMAND".to_string(),
        plan.node_command.display().to_string(),
    );
    let args_json = format!(
        "[{},{}]",
        serde_json::to_string(&plan.paths.launcher.display().to_string()).unwrap_or_default(),
        ZCODE_AGENT_ARGS_JSON
            .trim_start_matches('[')
            .trim_end_matches(']')
    );
    vars.insert("ZCODE_AGENT_SERVER_ARGS_JSON".to_string(), args_json);
    vars.insert(
        "ZCODE_KEYSMITH_SYSTEM_FILE".to_string(),
        plan.paths.system_file.display().to_string(),
    );
    vars.insert(
        "ZCODE_KEYSMITH_ORIGINAL".to_string(),
        plan.zcode_runtime.display().to_string(),
    );
    vars.insert(
        "ZCODE_KEYSMITH_NODE_COMMAND".to_string(),
        plan.node_command.display().to_string(),
    );
    vars.insert(
        "ZCODE_KEYSMITH_CACHE_DIR".to_string(),
        plan.paths.cache_dir.display().to_string(),
    );
    vars.insert(
        "ZCODE_KEYSMITH_LOG_DIR".to_string(),
        plan.paths.log_dir.display().to_string(),
    );
    Ok(vars)
}

/// 激活当前会话：launchctl setenv 逐个设置。
pub(crate) fn activate_current_session(vars: &EnvVars) -> Result<Vec<String>> {
    let mut results = Vec::new();
    for (key, value) in vars {
        let output = Command::new("/bin/launchctl")
            .args(["setenv", key, value])
            .output()
            .map_err(|e| CodexxError::Config(format!("launchctl setenv {key} 失败: {e}")))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(CodexxError::Config(format!(
                "launchctl setenv {key} 失败: {detail}"
            )));
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
        let _ = Command::new("/bin/launchctl")
            .args(["unsetenv", key])
            .output();
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
    let env_script_path = plan
        .paths
        .launcher
        .parent()
        .unwrap_or(Path::new("."))
        .join(ZCODE_ENV_SCRIPT_NAME);

    let mut script_lines = vec!["#!/bin/sh".to_string(), "set -eu".to_string()];
    for (key, value) in &vars {
        script_lines.push(format!(
            "/bin/launchctl setenv {} {}",
            key,
            sh_single_quote(value)
        ));
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
        label = xml_text(ZCODE_LAUNCH_AGENT_LABEL),
        script = xml_text(&env_script_str),
        log_dir = xml_text(&log_dir),
    );

    Ok((env_script_path, env_script, launch_agent))
}

pub(crate) fn write_env_artifacts(plan: &ZcodeInstallPlan) -> Result<()> {
    let (env_script_path, env_script, launch_agent) = render_env_artifacts(plan)?;
    write_executable_text(&env_script_path, &env_script)?;
    write_text(&launch_agent_path(), &launch_agent)
}

fn write_executable_text(path: &Path, content: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    write_text(path, content)?;
    let mut permissions = fs::metadata(path)
        .map_err(|error| io_err(path, error))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| io_err(path, error))
}

fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zcode::ZcodePaths;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "everything-patch-zcode-macos-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    fn test_plan(root: &Path) -> ZcodeInstallPlan {
        let managed = root.join("managed&config");
        let bin = managed.join("bin");
        let log = managed.join("logs");
        ZcodeInstallPlan {
            paths: ZcodePaths {
                managed_dir: managed.clone(),
                system_file: managed.join("system-role.md"),
                config_file: managed.join("config.json"),
                launcher: bin.join("launcher.js"),
                patch_sidecar: bin.join("patch.js"),
                log_dir: log.clone(),
                cache_dir: managed.join("cache"),
                launcher_log: log.join("launcher-start.jsonl"),
            },
            zcode_runtime: root.join("ZCode.app/Contents/Resources/glm/zcode.cjs"),
            node_command: root.join("ZCode.app/Contents/MacOS/ZCode"),
            zcode_app: root.join("ZCode.app"),
        }
    }

    #[test]
    fn validates_a_complete_zcode_bundle() {
        let root = temp_dir("validation").join("ZCode.app");
        let runtime = root.join("Contents/Resources/glm/zcode.cjs");
        let executable = root.join("Contents/MacOS/ZCode");
        fs::create_dir_all(runtime.parent().expect("runtime parent"))
            .expect("create runtime parent");
        fs::create_dir_all(executable.parent().expect("executable parent"))
            .expect("create executable parent");
        fs::write(&runtime, "runtime").expect("write runtime");
        fs::write(&executable, "binary").expect("write executable");

        assert!(validate_zcode_root(&root));
        fs::remove_file(runtime).expect("remove runtime");
        assert!(!validate_zcode_root(&root));

        fs::remove_dir_all(root.parent().expect("test root")).expect("remove test directory");
    }

    #[test]
    fn process_matching_accepts_the_main_executable_only() {
        let executable = Path::new("/Applications/ZCode.app/Contents/MacOS/ZCode");
        assert!(command_line_starts_with_executable(
            "/Applications/ZCode.app/Contents/MacOS/ZCode --flag",
            executable
        ));
        assert!(command_line_starts_with_executable(
            "\"/Applications/ZCode.app/Contents/MacOS/ZCode\" --flag",
            executable
        ));
        assert!(!command_line_starts_with_executable(
            "/Applications/ZCode.app/Contents/MacOS/ZCode Helper --type=renderer",
            executable
        ));
    }

    #[test]
    fn env_artifacts_escape_xml_and_write_an_executable_script() {
        let root = temp_dir("env-artifacts");
        let plan = test_plan(&root);
        let (script_path, script, plist) = render_env_artifacts(&plan).expect("render artifacts");
        assert!(script.contains("/bin/launchctl setenv"));
        assert!(plist.contains("managed&amp;config"));

        write_executable_text(&script_path, &script).expect("write executable script");
        let mode = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);

        fs::remove_dir_all(root).expect("remove test directory");
    }
}
