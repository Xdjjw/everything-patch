use super::catalog_download::{acquire_mcp_source, AcquiredMcpSource};
use super::tool::install_tool_mcp_config_inner;
use super::types::{SkillsMcpActionResult, SkillsMcpState};
use crate::error::{CodexxError, Result};
use crate::tools::ToolId;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_COMMAND_LENGTH: usize = 1024;
const MAX_PATH_LENGTH: usize = 4096;
const MAX_ENDPOINT_LENGTH: usize = 2048;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpIntegrationInstallInput {
    pub(crate) integration_id: String,
    pub(crate) source_path: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) endpoint: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) source_mode: Option<String>,
}

#[derive(Debug)]
struct PreparedIntegration {
    id: &'static str,
    name: &'static str,
    config: Value,
}

fn clean_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn validate_text(value: &str, label: &str, max_length: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CodexxError::Config(format!("{label}不能为空")));
    }
    if value.len() > max_length {
        return Err(CodexxError::Config(format!("{label}过长")));
    }
    if value
        .chars()
        .any(|character| character == '\0' || character == '\r' || character == '\n')
    {
        return Err(CodexxError::Config(format!("{label}包含非法控制字符")));
    }
    Ok(value.to_string())
}

fn command_or_default(input: &McpIntegrationInstallInput, fallback: &str) -> Result<String> {
    validate_text(
        clean_optional(input.command.as_deref()).unwrap_or(fallback),
        "MCP 命令",
        MAX_COMMAND_LENGTH,
    )
}

fn source_path(input: &McpIntegrationInstallInput, label: &str) -> Result<PathBuf> {
    let raw = clean_optional(input.source_path.as_deref())
        .ok_or_else(|| CodexxError::Config(format!("请选择{label}")))?;
    let raw = validate_text(raw, label, MAX_PATH_LENGTH)?;
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(CodexxError::Config(format!("{label}必须使用绝对路径")));
    }
    if !path.exists() {
        return Err(CodexxError::Config(format!(
            "{label}不存在: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn has_file_name(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn expected_file(
    source: &Path,
    candidates: &[&str],
    expected_name: &str,
    label: &str,
) -> Result<PathBuf> {
    if source.is_file() {
        if has_file_name(source, expected_name) {
            return Ok(source.to_path_buf());
        }
        return Err(CodexxError::Config(format!(
            "{label}应选择 {expected_name}: {}",
            source.display()
        )));
    }
    for candidate in candidates {
        let path = source.join(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(CodexxError::Config(format!(
        "{label}中没有找到 {expected_name}: {}",
        source.display()
    )))
}

fn mode_or_default<'a>(
    input: &'a McpIntegrationInstallInput,
    fallback: &'a str,
    allowed: &[&str],
) -> Result<&'a str> {
    let mode = clean_optional(input.mode.as_deref()).unwrap_or(fallback);
    if allowed.contains(&mode) {
        Ok(mode)
    } else {
        Err(CodexxError::Config(format!(
            "不支持的 MCP 安装模式: {mode}"
        )))
    }
}

fn source_mode(input: &McpIntegrationInstallInput) -> Result<&str> {
    match clean_optional(input.source_mode.as_deref()).unwrap_or("manual") {
        mode @ ("managed" | "manual") => Ok(mode),
        mode => Err(CodexxError::Config(format!(
            "不支持的 MCP 文件来源模式: {mode}"
        ))),
    }
}

fn managed_acquisition_settings(
    tool: ToolId,
    input: &McpIntegrationInstallInput,
) -> Result<(String, String)> {
    match input.integration_id.trim() {
        "ida-pro-mcp" => Ok(("local".to_string(), command_or_default(input, "uv")?)),
        "cheatengine-mcp" => {
            let fallback = if cfg!(target_os = "windows") {
                "local"
            } else {
                "remote"
            };
            let mode = mode_or_default(input, fallback, &["local", "remote"])?;
            let command = command_or_default(
                input,
                if cfg!(target_os = "windows") {
                    "python"
                } else {
                    "python3"
                },
            )?;
            Ok((mode.to_string(), command))
        }
        "x64dbg-mcp" => {
            let fallback = if cfg!(target_os = "windows") {
                "local"
            } else {
                "remote"
            };
            let mode = mode_or_default(input, fallback, &["local", "remote"])?;
            let command = command_or_default(
                input,
                if cfg!(target_os = "windows") {
                    "python"
                } else {
                    "python3"
                },
            )?;
            Ok((mode.to_string(), command))
        }
        "burp-suite-mcp" => {
            let fallback = if matches!(tool, ToolId::Claude | ToolId::Zcode) {
                "direct"
            } else {
                "proxy"
            };
            let mode = mode_or_default(input, fallback, &["direct", "proxy"])?;
            Ok((mode.to_string(), command_or_default(input, "java")?))
        }
        integration => Err(CodexxError::Config(format!(
            "未知的 MCP 集成: {integration}"
        ))),
    }
}

fn http_endpoint(
    input: &McpIntegrationInstallInput,
    fallback: &str,
    trailing_slash: bool,
) -> Result<String> {
    let raw = clean_optional(input.endpoint.as_deref()).unwrap_or(fallback);
    let raw = validate_text(raw, "MCP 服务地址", MAX_ENDPOINT_LENGTH)?;
    let parsed = reqwest::Url::parse(&raw)
        .map_err(|error| CodexxError::Config(format!("MCP 服务地址无效: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(CodexxError::Config(
            "MCP 服务地址必须是完整的 http/https URL".to_string(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(CodexxError::Config(
            "MCP 服务地址不能包含用户名或密码".to_string(),
        ));
    }
    let mut normalized = parsed.to_string();
    if trailing_slash && !normalized.ends_with('/') {
        normalized.push('/');
    }
    Ok(normalized)
}

fn tcp_endpoint(input: &McpIntegrationInstallInput) -> Result<(String, String)> {
    let raw = clean_optional(input.endpoint.as_deref()).unwrap_or("127.0.0.1:9876");
    let raw = validate_text(raw, "TCP 桥接地址", MAX_ENDPOINT_LENGTH)?;
    let authority = raw.strip_prefix("tcp://").unwrap_or(&raw);
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (host, suffix) = rest
            .split_once(']')
            .ok_or_else(|| CodexxError::Config("TCP IPv6 地址缺少 ]".to_string()))?;
        let port = suffix
            .strip_prefix(':')
            .ok_or_else(|| CodexxError::Config("TCP 桥接地址缺少端口".to_string()))?;
        (host, port)
    } else {
        authority
            .rsplit_once(':')
            .ok_or_else(|| CodexxError::Config("TCP 桥接地址格式应为 host:port".to_string()))?
    };
    if host.is_empty()
        || !host.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | ':')
        })
    {
        return Err(CodexxError::Config("TCP 桥接主机名无效".to_string()));
    }
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| CodexxError::Config("TCP 桥接端口无效".to_string()))?;
    Ok((host.to_string(), port.to_string()))
}

fn ida_integration(input: &McpIntegrationInstallInput) -> Result<PreparedIntegration> {
    let selected = source_path(input, "IDA Pro MCP 项目目录")?;
    let root = if selected.is_file() && has_file_name(&selected, "pyproject.toml") {
        selected
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| CodexxError::Config("IDA Pro MCP 项目路径无效".to_string()))?
    } else if selected.is_dir() {
        selected
    } else {
        return Err(CodexxError::Config(
            "IDA Pro MCP 请选择项目目录或 pyproject.toml".to_string(),
        ));
    };
    let pyproject = root.join("pyproject.toml");
    let text = fs::read_to_string(&pyproject)
        .map_err(|error| crate::file_io::io_err(&pyproject, error))?;
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| CodexxError::Toml {
            path: pyproject.display().to_string(),
            message: error.to_string(),
        })?;
    if document
        .get("project")
        .and_then(|item| item.get("name"))
        .and_then(toml_edit::Item::as_str)
        != Some("ida-pro-mcp")
    {
        return Err(CodexxError::Config(format!(
            "所选目录不是 mrexodia/ida-pro-mcp: {}",
            root.display()
        )));
    }
    let command = command_or_default(input, "uv")?;
    Ok(PreparedIntegration {
        id: "ida-pro-mcp",
        name: "IDA Pro MCP",
        config: json!({
            "command": command,
            "args": [
                "run",
                "--offline",
                "--no-sync",
                "--project",
                root.display().to_string(),
                "idalib-mcp",
                "--stdio"
            ]
        }),
    })
}

fn cheat_engine_integration(input: &McpIntegrationInstallInput) -> Result<PreparedIntegration> {
    let fallback_mode = if cfg!(target_os = "windows") {
        "local"
    } else {
        "remote"
    };
    let mode = mode_or_default(input, fallback_mode, &["local", "remote"])?;
    if mode == "local" && !cfg!(target_os = "windows") {
        return Err(CodexxError::Config(
            "Cheat Engine 本地 Named Pipe 模式仅支持 Windows，macOS 请选择远程 TCP".to_string(),
        ));
    }
    let selected = source_path(input, "Cheat Engine MCP 项目目录")?;
    let script = expected_file(
        &selected,
        &["MCP_Server/mcp_cheatengine.py", "mcp_cheatengine.py"],
        "mcp_cheatengine.py",
        "Cheat Engine MCP 项目",
    )?;
    let lua = script
        .parent()
        .map(|parent| parent.join("ce_mcp_bridge.lua"))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            CodexxError::Config(format!(
                "Cheat Engine MCP 项目缺少 ce_mcp_bridge.lua: {}",
                script.display()
            ))
        })?;
    let command = command_or_default(
        input,
        if cfg!(target_os = "windows") {
            "python"
        } else {
            "python3"
        },
    )?;
    let mut config = json!({
        "command": command,
        "args": [script.display().to_string()]
    });
    if mode == "remote" {
        let (host, port) = tcp_endpoint(input)?;
        config["env"] = json!({
            "CE_MCP_TRANSPORT": "tcp",
            "CE_MCP_HOST": host,
            "CE_MCP_PORT": port
        });
    }
    let _ = lua;
    Ok(PreparedIntegration {
        id: "cheatengine-mcp",
        name: "Cheat Engine MCP Bridge",
        config,
    })
}

fn x64dbg_integration(input: &McpIntegrationInstallInput) -> Result<PreparedIntegration> {
    let fallback_mode = if cfg!(target_os = "windows") {
        "local"
    } else {
        "remote"
    };
    let mode = mode_or_default(input, fallback_mode, &["local", "remote"])?;
    if mode == "local" && !cfg!(target_os = "windows") {
        return Err(CodexxError::Config(
            "x64dbg 本地模式仅支持 Windows，macOS 请选择远程桥接".to_string(),
        ));
    }
    let selected = source_path(input, "x64dbg MCP Python 桥接脚本")?;
    let script = expected_file(
        &selected,
        &["src/x64dbg.py", "x64dbg.py"],
        "x64dbg.py",
        "x64dbg MCP 项目",
    )?;
    let command = command_or_default(
        input,
        if cfg!(target_os = "windows") {
            "python"
        } else {
            "python3"
        },
    )?;
    let endpoint = http_endpoint(input, "http://127.0.0.1:8888/", true)?;
    Ok(PreparedIntegration {
        id: "x64dbg-mcp",
        name: "x64dbg MCP",
        config: json!({
            "command": command,
            "args": [script.display().to_string()],
            "env": { "X64DBG_URL": endpoint }
        }),
    })
}

fn burp_integration(
    tool: ToolId,
    input: &McpIntegrationInstallInput,
) -> Result<PreparedIntegration> {
    let fallback = if matches!(tool, ToolId::Claude | ToolId::Zcode) {
        "direct"
    } else {
        "proxy"
    };
    let mode = mode_or_default(input, fallback, &["direct", "proxy"])?;
    if mode == "direct" && !matches!(tool, ToolId::Claude | ToolId::Zcode) {
        return Err(CodexxError::Config(format!(
            "{} 不支持 Burp 的传统 SSE 直连，请使用官方 stdio 代理 mcp-proxy-all.jar",
            tool.label()
        )));
    }
    let endpoint = http_endpoint(input, "http://127.0.0.1:9876/sse", false)?;
    let config = if mode == "proxy" {
        let selected = source_path(input, "Burp MCP stdio 代理 JAR")?;
        if !selected.is_file()
            || selected
                .extension()
                .and_then(|extension| extension.to_str())
                .is_none_or(|extension| !extension.eq_ignore_ascii_case("jar"))
            || selected
                .file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| !name.to_ascii_lowercase().contains("mcp-proxy"))
        {
            return Err(CodexxError::Config(
                "请选择 Burp MCP 扩展导出的 mcp-proxy-all.jar".to_string(),
            ));
        }
        let command = command_or_default(input, "java")?;
        json!({
            "command": command,
            "args": ["-jar", selected.display().to_string(), "--sse-url", endpoint]
        })
    } else {
        json!({ "type": "sse", "url": endpoint })
    };
    Ok(PreparedIntegration {
        id: "burp-suite-mcp",
        name: "Burp Suite MCP Server",
        config,
    })
}

fn prepare_integration(
    tool: ToolId,
    input: &McpIntegrationInstallInput,
) -> Result<PreparedIntegration> {
    match input.integration_id.trim() {
        "ida-pro-mcp" => ida_integration(input),
        "cheatengine-mcp" => cheat_engine_integration(input),
        "x64dbg-mcp" => x64dbg_integration(input),
        "burp-suite-mcp" => burp_integration(tool, input),
        integration => Err(CodexxError::Config(format!(
            "未知的 MCP 集成: {integration}"
        ))),
    }
}

pub(crate) fn install_mcp_integration_inner(
    tool: ToolId,
    config_dir: Option<String>,
    mut input: McpIntegrationInstallInput,
) -> Result<SkillsMcpActionResult> {
    let acquired: Option<AcquiredMcpSource> = if source_mode(&input)? == "managed" {
        let (mode, command) = managed_acquisition_settings(tool, &input)?;
        let source = acquire_mcp_source(input.integration_id.trim(), &mode, &command)?;
        input.source_path = source
            .source_path
            .as_ref()
            .map(|path| path.display().to_string());
        if let Some(runtime_command) = source.runtime_command.as_ref() {
            input.command = Some(runtime_command.display().to_string());
        }
        Some(source)
    } else {
        None
    };
    let prepared = prepare_integration(tool, &input)?;
    let state: SkillsMcpState = install_tool_mcp_config_inner(
        tool,
        config_dir,
        prepared.id,
        prepared.name,
        prepared.config,
    )?;
    Ok(SkillsMcpActionResult {
        imported_skills: 0,
        imported_mcp: 1,
        message: if let Some(source) = acquired {
            let mut message = format!(
                "已获取并校验 {}，为 {} 配置 {}；托管文件位于 {}",
                source.version,
                tool.label(),
                prepared.name,
                source.managed_root.display()
            );
            if let Some(next_step) = source.next_step {
                message.push_str("；");
                message.push_str(&next_step);
            }
            message
        } else {
            format!(
                "已使用所选本地文件为 {} 配置 {}",
                tool.label(),
                prepared.name
            )
        },
        state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "everything-patch-mcp-catalog-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp directory");
        path
    }

    fn input(id: &str) -> McpIntegrationInstallInput {
        McpIntegrationInstallInput {
            integration_id: id.to_string(),
            source_path: None,
            command: None,
            endpoint: None,
            mode: None,
            source_mode: None,
        }
    }

    #[test]
    fn ida_uses_selected_project_without_running_installer() {
        let root = temp_dir("ida");
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nname = \"ida-pro-mcp\"\nversion = \"2.0.0\"\n",
        )
        .expect("write pyproject");
        let mut request = input("ida-pro-mcp");
        request.source_path = Some(root.display().to_string());

        let prepared = prepare_integration(ToolId::Codex, &request).expect("prepare IDA");

        assert_eq!(prepared.id, "ida-pro-mcp");
        assert_eq!(prepared.config["command"], "uv");
        assert_eq!(prepared.config["args"][0], "run");
        assert_eq!(prepared.config["args"][1], "--offline");
        assert_eq!(prepared.config["args"][2], "--no-sync");
        assert_eq!(prepared.config["args"][3], "--project");
        assert_eq!(prepared.config["args"][6], "--stdio");
        fs::remove_dir_all(root).expect("remove temp directory");
    }

    #[test]
    fn cheat_engine_remote_mode_sets_tcp_environment() {
        let root = temp_dir("ce");
        let server = root.join("MCP_Server");
        fs::create_dir_all(&server).expect("create server directory");
        fs::write(server.join("mcp_cheatengine.py"), "# bridge\n").expect("write bridge");
        fs::write(server.join("ce_mcp_bridge.lua"), "-- bridge\n").expect("write lua");
        let mut request = input("cheatengine-mcp");
        request.source_path = Some(root.display().to_string());
        request.mode = Some("remote".to_string());
        request.endpoint = Some("192.0.2.10:4567".to_string());

        let prepared = prepare_integration(ToolId::Claude, &request).expect("prepare CE");

        assert_eq!(prepared.config["env"]["CE_MCP_TRANSPORT"], "tcp");
        assert_eq!(prepared.config["env"]["CE_MCP_HOST"], "192.0.2.10");
        assert_eq!(prepared.config["env"]["CE_MCP_PORT"], "4567");
        fs::remove_dir_all(root).expect("remove temp directory");
    }

    #[test]
    fn x64dbg_uses_python_bridge_and_normalized_endpoint() {
        let root = temp_dir("x64dbg");
        let script = root.join("x64dbg.py");
        fs::write(&script, "# bridge\n").expect("write bridge");
        let mut request = input("x64dbg-mcp");
        request.source_path = Some(script.display().to_string());
        request.mode = Some("remote".to_string());
        request.endpoint = Some("http://debug-host.example:8888".to_string());

        let prepared = prepare_integration(ToolId::Grok, &request).expect("prepare x64dbg");

        assert_eq!(
            prepared.config["env"]["X64DBG_URL"],
            "http://debug-host.example:8888/"
        );
        assert_eq!(prepared.config["args"][0], script.display().to_string());
        fs::remove_dir_all(root).expect("remove temp directory");
    }

    #[test]
    fn burp_direct_transport_is_used_for_sse_capable_targets() {
        let request = input("burp-suite-mcp");

        let claude = prepare_integration(ToolId::Claude, &request).expect("prepare Claude Burp");
        let zcode = prepare_integration(ToolId::Zcode, &request).expect("prepare ZCode Burp");

        assert_eq!(claude.config["url"], "http://127.0.0.1:9876/sse");
        assert_eq!(claude.config["type"], "sse");
        assert_eq!(zcode.config, claude.config);
    }

    #[test]
    fn burp_direct_transport_rejects_targets_without_legacy_sse_support() {
        let mut request = input("burp-suite-mcp");
        request.mode = Some("direct".to_string());

        for tool in [ToolId::Codex, ToolId::Grok] {
            let error = prepare_integration(tool, &request).expect_err("reject direct SSE");
            assert!(error.to_string().contains("mcp-proxy-all.jar"));
        }
    }

    #[test]
    fn burp_proxy_uses_selected_jar_without_running_it() {
        let root = temp_dir("burp-proxy");
        let proxy = root.join("mcp-proxy-all.jar");
        fs::write(&proxy, "test fixture").expect("write proxy fixture");
        let mut request = input("burp-suite-mcp");
        request.mode = Some("proxy".to_string());
        request.source_path = Some(proxy.display().to_string());
        request.command = Some("java".to_string());
        request.endpoint = Some("http://127.0.0.1:9876/sse".to_string());

        let prepared = prepare_integration(ToolId::Zcode, &request).expect("prepare Burp proxy");

        assert_eq!(prepared.config["command"], "java");
        assert_eq!(prepared.config["args"][0], "-jar");
        assert_eq!(prepared.config["args"][1], proxy.display().to_string());
        assert_eq!(prepared.config["args"][2], "--sse-url");
        assert_eq!(prepared.config["args"][3], "http://127.0.0.1:9876/sse");
        fs::remove_dir_all(root).expect("remove temp directory");
    }

    #[test]
    fn source_paths_must_be_absolute() {
        let mut request = input("x64dbg-mcp");
        request.source_path = Some("src/x64dbg.py".to_string());
        request.mode = Some("remote".to_string());

        let error = prepare_integration(ToolId::Codex, &request).expect_err("reject relative path");

        assert!(error.to_string().contains("绝对路径"));
    }

    #[test]
    fn old_clients_keep_manual_source_behavior() {
        let request = input("ida-pro-mcp");
        assert_eq!(
            source_mode(&request).expect("default source mode"),
            "manual"
        );
    }

    #[test]
    fn source_mode_rejects_unknown_values() {
        let mut request = input("ida-pro-mcp");
        request.source_mode = Some("automatic".to_string());

        let error = source_mode(&request).expect_err("reject unknown source mode");

        assert!(error.to_string().contains("文件来源模式"));
    }

    #[test]
    fn managed_burp_transport_matches_target_capabilities() {
        let request = input("burp-suite-mcp");

        let (claude_mode, _) =
            managed_acquisition_settings(ToolId::Claude, &request).expect("Claude settings");
        let (codex_mode, _) =
            managed_acquisition_settings(ToolId::Codex, &request).expect("Codex settings");

        assert_eq!(claude_mode, "direct");
        assert_eq!(codex_mode, "proxy");
    }
}
