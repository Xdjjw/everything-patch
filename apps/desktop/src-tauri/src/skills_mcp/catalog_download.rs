use crate::error::{CodexxError, Result};
use crate::file_io::{atomic_write, ensure_directory, io_err};
use crate::paths::app_home;
use crate::platform;
use crate::remote::ensure_crypto_provider;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const MAX_DOWNLOAD_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 96 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 10_000;
const MAX_COMMAND_ERROR_BYTES: usize = 64 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

const IDA_VERSION: &str = "f82e6e2";
const IDA_URL: &str =
    "https://codeload.github.com/mrexodia/ida-pro-mcp/zip/f82e6e2517a161b77e738951c3071cd446480ba0";
const IDA_SHA256: &str = "3d511bb2f1439270f56e6350f9b35d4540483beb416cb8cda3905f1880a2f741";

const CE_VERSION: &str = "588813f";
const CE_URL: &str = "https://codeload.github.com/miscusi-peek/cheatengine-mcp-bridge/zip/588813f3edfd2a7e0574e73d882f3383203c6343";
const CE_SHA256: &str = "fdb12a0e55643ef10a04e6598cf8aef540475fd1b3779f17fe4ef92f63159416";

const X64DBG_VERSION: &str = "build1.1";
const X64DBG_PY_URL: &str =
    "https://github.com/Wasdubya/x64dbgMCP/releases/download/build1.1/x64dbg.py";
const X64DBG_PY_SHA256: &str = "6fe64ec6ea9e5df253b94ffa0274b59fd4744fb639467305ca8835288d606f25";
const X64DBG_PLUGINS_URL: &str =
    "https://github.com/Wasdubya/x64dbgMCP/releases/download/build1.1/MCP_Plugins.zip";
const X64DBG_PLUGINS_SHA256: &str =
    "20d0c69d0b7f2d7f251e5479cf6728be8bb5da76d3e20c9e1feb28bfbc56ce3e";

const BURP_VERSION: &str = "v1.3.0";
const BURP_EXTENSION_URL: &str =
    "https://github.com/PortSwigger/mcp-server/releases/download/v1.3.0/burp-mcp-all.jar";
const BURP_EXTENSION_SHA256: &str =
    "c4011245ee7da0cb901b9c0435aba3d8458ab5b0e2078e1a87fd025ed93c7892";
const BURP_PROXY_URL: &str = "https://raw.githubusercontent.com/PortSwigger/mcp-server/5f76126409780ecba2b766c7f7388f465c5b5f94/libs/mcp-proxy-all.jar";
const BURP_PROXY_SHA256: &str = "b376b860f114f67e8301e50b06760f1edd23dd99e860c3646cbeac144ce7821a";

static MCP_SOURCE_LOCK: Mutex<()> = Mutex::new(());
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
struct Artifact {
    label: &'static str,
    filename: &'static str,
    url: &'static str,
    sha256: &'static str,
}

#[derive(Debug)]
pub(super) struct AcquiredMcpSource {
    pub(super) source_path: Option<PathBuf>,
    pub(super) managed_root: PathBuf,
    pub(super) version: &'static str,
    pub(super) runtime_command: Option<PathBuf>,
    pub(super) next_step: Option<String>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn file_sha256(path: &Path) -> Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_err(path, error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CodexxError::Config(format!(
            "MCP 托管文件不是普通文件: {}",
            path.display()
        )));
    }
    if metadata.len() > MAX_DOWNLOAD_BYTES {
        return Err(CodexxError::Config(format!(
            "MCP 托管文件超过大小限制: {}",
            path.display()
        )));
    }
    let bytes = fs::read(path).map_err(|error| io_err(path, error))?;
    Ok(Some(sha256_hex(&bytes)))
}

fn plain_child_directory(parent: &Path, name: &str) -> Result<PathBuf> {
    ensure_directory(parent)?;
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(path),
        Ok(_) => Err(CodexxError::Config(format!(
            "MCP 托管目录被文件或链接占用: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).map_err(|error| io_err(&path, error))?;
            Ok(path)
        }
        Err(error) => Err(io_err(&path, error)),
    }
}

fn managed_root(integration_id: &str, version: &str) -> Result<PathBuf> {
    let home = app_home()?;
    let sources = plain_child_directory(&home, "mcp-integrations")?;
    let integration = plain_child_directory(&sources, integration_id)?;
    plain_child_directory(&integration, version)
}

fn download_client() -> Result<reqwest::blocking::Client> {
    ensure_crypto_provider();
    reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent("DevConduit MCP source installer")
        .build()
        .map_err(|_| CodexxError::Config("MCP 下载客户端初始化失败".to_string()))
}

fn download_verified(artifact: Artifact) -> Result<Vec<u8>> {
    let mut response = download_client()?
        .get(artifact.url)
        .send()
        .map_err(|error| {
            let reason = if error.is_timeout() {
                "请求超时"
            } else if error.is_connect() {
                "网络连接失败"
            } else {
                "网络请求失败"
            };
            CodexxError::Config(format!("{}下载失败：{reason}", artifact.label))
        })?;
    if !response.status().is_success() {
        return Err(CodexxError::Config(format!(
            "{}下载失败（HTTP {}）",
            artifact.label,
            response.status().as_u16()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOWNLOAD_BYTES)
    {
        return Err(CodexxError::Config(format!(
            "{}超过下载大小限制",
            artifact.label
        )));
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CodexxError::Config(format!("{}读取失败", artifact.label)))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err(CodexxError::Config(format!(
            "{}为空或超过下载大小限制",
            artifact.label
        )));
    }
    let actual = sha256_hex(&bytes);
    if actual != artifact.sha256 {
        return Err(CodexxError::Config(format!(
            "{} SHA-256 校验失败，未写入任何工具配置",
            artifact.label
        )));
    }
    Ok(bytes)
}

fn ensure_artifact(root: &Path, artifact: Artifact) -> Result<PathBuf> {
    let path = root.join(artifact.filename);
    if file_sha256(&path)?.as_deref() == Some(artifact.sha256) {
        return Ok(path);
    }
    let bytes = download_verified(artifact)?;
    atomic_write(&path, &bytes)?;
    Ok(path)
}

fn archive_relative_path(
    path: &Path,
    strip_first_component: bool,
    expected_prefix: &mut Option<std::ffi::OsString>,
) -> Result<Option<PathBuf>> {
    if !strip_first_component {
        return Ok(Some(path.to_path_buf()));
    }
    let mut components = path.components();
    let Some(Component::Normal(prefix)) = components.next() else {
        return Err(CodexxError::Config(
            "MCP 源码压缩包缺少安全的顶层目录".to_string(),
        ));
    };
    match expected_prefix {
        Some(expected) if expected != prefix => {
            return Err(CodexxError::Config(
                "MCP 源码压缩包含多个顶层目录".to_string(),
            ));
        }
        None => *expected_prefix = Some(prefix.to_os_string()),
        _ => {}
    }
    let relative = components.as_path();
    if relative.as_os_str().is_empty() {
        Ok(None)
    } else {
        Ok(Some(relative.to_path_buf()))
    }
}

fn extract_zip(bytes: &[u8], destination: &Path, strip_first_component: bool) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| CodexxError::Config(format!("MCP 压缩包无效: {error}")))?;
    if archive.len() > MAX_ARCHIVE_FILES {
        return Err(CodexxError::Config("MCP 压缩包文件数量过多".to_string()));
    }
    fs::create_dir(destination).map_err(|error| io_err(destination, error))?;
    let mut total_size = 0_u64;
    let mut prefix = None;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| CodexxError::Config(format!("MCP 压缩包读取失败: {error}")))?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| CodexxError::Config("MCP 压缩包包含越界路径".to_string()))?;
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(CodexxError::Config(
                "MCP 压缩包不能包含符号链接".to_string(),
            ));
        }
        let Some(relative) = archive_relative_path(&enclosed, strip_first_component, &mut prefix)?
        else {
            continue;
        };
        if relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(CodexxError::Config("MCP 压缩包包含非法路径".to_string()));
        }
        let output = destination.join(relative);
        if file.is_dir() {
            ensure_directory(&output)?;
            continue;
        }
        let remaining = MAX_EXTRACTED_BYTES.saturating_sub(total_size);
        if file.size() > remaining {
            return Err(CodexxError::Config(
                "MCP 压缩包解压后超过大小限制".to_string(),
            ));
        }
        if let Some(parent) = output.parent() {
            ensure_directory(parent)?;
        }
        let mut target = fs::File::create(&output).map_err(|error| io_err(&output, error))?;
        let copied = std::io::copy(&mut file.take(remaining + 1), &mut target)
            .map_err(|error| io_err(&output, error))?;
        if copied > remaining {
            return Err(CodexxError::Config(
                "MCP 压缩包解压后超过大小限制".to_string(),
            ));
        }
        total_size += copied;
    }
    Ok(())
}

fn regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn runnable_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn extraction_ready(root: &Path, marker: &Path, digest: &str, required: &[&str]) -> bool {
    fs::read_to_string(marker).is_ok_and(|value| value.trim() == digest)
        && required
            .iter()
            .all(|relative| regular_file(&root.join(relative)))
}

fn remove_plain_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_err(path, error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(CodexxError::Config(format!(
            "拒绝替换非普通 MCP 托管目录: {}",
            path.display()
        )));
    }
    fs::remove_dir_all(path).map_err(|error| io_err(path, error))
}

fn ensure_extracted(
    root: &Path,
    archive_path: &Path,
    destination_name: &str,
    digest: &str,
    strip_first_component: bool,
    required: &[&str],
) -> Result<PathBuf> {
    let destination = root.join(destination_name);
    let marker = root.join(format!(".{destination_name}.sha256"));
    if extraction_ready(&destination, &marker, digest, required) {
        return Ok(destination);
    }
    let bytes = fs::read(archive_path).map_err(|error| io_err(archive_path, error))?;
    if sha256_hex(&bytes) != digest {
        return Err(CodexxError::Config(
            "MCP 本地下载缓存校验失败，请重试".to_string(),
        ));
    }
    let staging = root.join(format!(
        ".{destination_name}.tmp.{}.{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let extracted = extract_zip(&bytes, &staging, strip_first_component);
    if let Err(error) = extracted {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if !required
        .iter()
        .all(|relative| regular_file(&staging.join(relative)))
    {
        let _ = fs::remove_dir_all(&staging);
        return Err(CodexxError::Config(
            "MCP 下载内容缺少预期文件，未写入任何工具配置".to_string(),
        ));
    }
    match fs::symlink_metadata(&destination) {
        Ok(_) => remove_plain_directory(&destination)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_err(&destination, error)),
    }
    fs::rename(&staging, &destination).map_err(|error| io_err(&destination, error))?;
    atomic_write(&marker, format!("{digest}\n").as_bytes())?;
    Ok(destination)
}

fn command_error_detail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let clean = text
        .chars()
        .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
        .collect::<String>();
    let mut characters = clean.chars().rev().take(1600).collect::<Vec<_>>();
    characters.reverse();
    characters
        .into_iter()
        .collect::<String>()
        .trim()
        .to_string()
}

fn read_tail(mut reader: impl Read, limit: usize) -> Vec<u8> {
    let mut tail = Vec::with_capacity(limit);
    let mut chunk = [0_u8; 8 * 1024];
    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        if read >= limit {
            tail.clear();
            tail.extend_from_slice(&chunk[read - limit..read]);
            continue;
        }
        let overflow = tail.len().saturating_add(read).saturating_sub(limit);
        if overflow > 0 {
            tail.drain(..overflow);
        }
        tail.extend_from_slice(&chunk[..read]);
    }
    tail
}

fn run_checked(program: &str, args: &[String], cwd: &Path, label: &str) -> Result<()> {
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mut command = platform::program_command(Path::new(program), &refs);
    let mut child = command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| CodexxError::Config(format!("无法启动 {label}，请检查命令：{program}")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CodexxError::Config(format!("无法读取 {label} 的错误输出")))?;
    let stderr_reader = thread::spawn(move || read_tail(stderr, MAX_COMMAND_ERROR_BYTES));
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr = stderr_reader.join().unwrap_or_default();
                if status.success() {
                    return Ok(());
                }
                let detail = command_error_detail(&stderr);
                return Err(CodexxError::Config(if detail.is_empty() {
                    format!("{label}失败")
                } else {
                    format!("{label}失败：{detail}")
                }));
            }
            Ok(None) if started.elapsed() >= INSTALL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_reader.join();
                return Err(CodexxError::Config(format!("{label}超时，已停止进程")));
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_reader.join();
                return Err(io_err(Path::new(program), error));
            }
        }
    }
}

fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

fn prepare_requirements_venv(
    python: &str,
    root: &Path,
    marker_value: &str,
    requirements: Option<&Path>,
    packages: &[&str],
) -> Result<PathBuf> {
    let venv = root.join(".venv");
    let runtime = venv_python(&venv);
    let marker = root.join(".python-environment");
    if runnable_file(&runtime)
        && fs::read_to_string(&marker).is_ok_and(|value| value.trim() == marker_value)
    {
        return Ok(runtime);
    }
    match fs::symlink_metadata(&venv) {
        Ok(_) => remove_plain_directory(&venv)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_err(&venv, error)),
    }
    run_checked(
        python,
        &["-m".into(), "venv".into(), venv.display().to_string()],
        root,
        "创建 MCP Python 环境",
    )?;
    let mut args = vec![
        "-m".to_string(),
        "pip".to_string(),
        "install".to_string(),
        "--disable-pip-version-check".to_string(),
    ];
    if let Some(requirements) = requirements {
        args.push("--requirement".to_string());
        args.push(requirements.display().to_string());
    }
    args.extend(packages.iter().map(|package| (*package).to_string()));
    run_checked(
        &runtime.display().to_string(),
        &args,
        root,
        "安装 MCP Python 依赖",
    )?;
    atomic_write(&marker, format!("{marker_value}\n").as_bytes())?;
    Ok(runtime)
}

fn acquire_ida(command: &str) -> Result<AcquiredMcpSource> {
    let root = managed_root("ida-pro-mcp", IDA_VERSION)?;
    let archive = ensure_artifact(
        &root,
        Artifact {
            label: "IDA Pro MCP 源码",
            filename: "source.zip",
            url: IDA_URL,
            sha256: IDA_SHA256,
        },
    )?;
    let project = ensure_extracted(
        &root,
        &archive,
        "project",
        IDA_SHA256,
        true,
        &["pyproject.toml", "uv.lock"],
    )?;
    let environment_marker = root.join(".uv-environment");
    if (!runnable_file(&project.join(".venv/bin/python"))
        && !runnable_file(&project.join(".venv/Scripts/python.exe")))
        || !fs::read_to_string(&environment_marker).is_ok_and(|value| value.trim() == IDA_SHA256)
    {
        run_checked(
            command,
            &[
                "sync".into(),
                "--locked".into(),
                "--project".into(),
                project.display().to_string(),
            ],
            &project,
            "安装 IDA Pro MCP 依赖",
        )?;
        atomic_write(&environment_marker, format!("{IDA_SHA256}\n").as_bytes())?;
    }
    Ok(AcquiredMcpSource {
        source_path: Some(project),
        managed_root: root,
        version: IDA_VERSION,
        runtime_command: None,
        next_step: Some("启动前请确认 IDA Pro 的 idalib 已完成激活".to_string()),
    })
}

fn acquire_ce(mode: &str, command: &str) -> Result<AcquiredMcpSource> {
    let root = managed_root("cheatengine-mcp", CE_VERSION)?;
    let archive = ensure_artifact(
        &root,
        Artifact {
            label: "Cheat Engine MCP 源码",
            filename: "source.zip",
            url: CE_URL,
            sha256: CE_SHA256,
        },
    )?;
    let project = ensure_extracted(
        &root,
        &archive,
        "project",
        CE_SHA256,
        true,
        &[
            "MCP_Server/mcp_cheatengine.py",
            "MCP_Server/ce_mcp_bridge.lua",
            "MCP_Server/requirements.txt",
            "MCP_Server/requirements-tcp.txt",
        ],
    )?;
    let requirements_name = if mode == "local" {
        "requirements.txt"
    } else {
        "requirements-tcp.txt"
    };
    let lua = project.join("MCP_Server").join("ce_mcp_bridge.lua");
    let requirements = project.join("MCP_Server").join(requirements_name);
    let runtime = prepare_requirements_venv(
        command,
        &root,
        &format!("{CE_SHA256}:{mode}"),
        Some(&requirements),
        &[],
    )?;
    Ok(AcquiredMcpSource {
        source_path: Some(project),
        managed_root: root,
        version: CE_VERSION,
        runtime_command: Some(runtime),
        next_step: Some(if cfg!(target_os = "windows") {
            format!("请在 Cheat Engine 中执行 Lua 桥：{}", lua.display())
        } else {
            format!(
                "请把 Lua 桥复制到远程 Windows 并在 Cheat Engine 中执行：{}",
                lua.display()
            )
        }),
    })
}

fn acquire_x64dbg(command: &str) -> Result<AcquiredMcpSource> {
    let root = managed_root("x64dbg-mcp", X64DBG_VERSION)?;
    let script = ensure_artifact(
        &root,
        Artifact {
            label: "x64dbg MCP Python 桥",
            filename: "x64dbg.py",
            url: X64DBG_PY_URL,
            sha256: X64DBG_PY_SHA256,
        },
    )?;
    let plugins_archive = ensure_artifact(
        &root,
        Artifact {
            label: "x64dbg MCP 插件",
            filename: "MCP_Plugins.zip",
            url: X64DBG_PLUGINS_URL,
            sha256: X64DBG_PLUGINS_SHA256,
        },
    )?;
    let plugins = ensure_extracted(
        &root,
        &plugins_archive,
        "plugins",
        X64DBG_PLUGINS_SHA256,
        false,
        &["MCPx64dbg.dp32", "MCPx64dbg.dp64"],
    )?;
    let runtime = prepare_requirements_venv(
        command,
        &root,
        X64DBG_PY_SHA256,
        None,
        &["mcp>=1.0.0", "requests>=2.31.0"],
    )?;
    Ok(AcquiredMcpSource {
        source_path: Some(script),
        managed_root: root,
        version: X64DBG_VERSION,
        runtime_command: Some(runtime),
        next_step: Some(format!(
            "请将对应架构插件装入 x64dbg/x32dbg：{}",
            plugins.display()
        )),
    })
}

fn acquire_burp(mode: &str) -> Result<AcquiredMcpSource> {
    let root = managed_root("burp-suite-mcp", BURP_VERSION)?;
    let extension = ensure_artifact(
        &root,
        Artifact {
            label: "Burp MCP 官方扩展",
            filename: "burp-mcp-all.jar",
            url: BURP_EXTENSION_URL,
            sha256: BURP_EXTENSION_SHA256,
        },
    )?;
    let proxy = if mode == "proxy" {
        Some(ensure_artifact(
            &root,
            Artifact {
                label: "Burp MCP 官方 stdio 代理",
                filename: "mcp-proxy-all.jar",
                url: BURP_PROXY_URL,
                sha256: BURP_PROXY_SHA256,
            },
        )?)
    } else {
        None
    };
    Ok(AcquiredMcpSource {
        source_path: proxy,
        managed_root: root,
        version: BURP_VERSION,
        runtime_command: None,
        next_step: Some(format!(
            "请在 Burp 的 Extensions 页面加载官方扩展：{}",
            extension.display()
        )),
    })
}

pub(super) fn acquire_mcp_source(
    integration_id: &str,
    mode: &str,
    command: &str,
) -> Result<AcquiredMcpSource> {
    let _guard = MCP_SOURCE_LOCK
        .lock()
        .map_err(|_| CodexxError::Config("MCP 下载锁已损坏，请重启 DevConduit".to_string()))?;
    match integration_id {
        "ida-pro-mcp" => acquire_ida(command),
        "cheatengine-mcp" => acquire_ce(mode, command),
        "x64dbg-mcp" => acquire_x64dbg(command),
        "burp-suite-mcp" => acquire_burp(mode),
        _ => Err(CodexxError::Config(format!(
            "未知的 MCP 自动来源: {integration_id}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_fixture(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(name, SimpleFileOptions::default())
                .expect("start fixture file");
            writer.write_all(bytes).expect("write fixture file");
        }
        writer.finish().expect("finish fixture").into_inner()
    }

    fn test_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "devconduit-mcp-download-{label}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn github_archive_extraction_strips_single_root() {
        let bytes = zip_fixture(&[
            ("repo-commit/pyproject.toml", b"[project]\n"),
            ("repo-commit/src/server.py", b"print('ok')\n"),
        ]);
        let destination = test_directory("strip-root");

        extract_zip(&bytes, &destination, true).expect("extract archive");

        assert!(destination.join("pyproject.toml").is_file());
        assert!(destination.join("src/server.py").is_file());
        assert!(!destination.join("repo-commit").exists());
        fs::remove_dir_all(destination).expect("remove fixture");
    }

    #[test]
    fn archive_extraction_rejects_parent_traversal() {
        let bytes = zip_fixture(&[("repo/../../outside.txt", b"no")]);
        let destination = test_directory("traversal");

        let error = extract_zip(&bytes, &destination, true).expect_err("reject traversal");

        assert!(error.to_string().contains("越界路径") || error.to_string().contains("非法路径"));
        let _ = fs::remove_dir_all(destination);
    }

    #[test]
    fn checked_artifact_hashes_are_exact_sha256_values() {
        for digest in [
            IDA_SHA256,
            CE_SHA256,
            X64DBG_PY_SHA256,
            X64DBG_PLUGINS_SHA256,
            BURP_EXTENSION_SHA256,
            BURP_PROXY_SHA256,
        ] {
            assert_eq!(digest.len(), 64);
            assert!(digest
                .chars()
                .all(|character| character.is_ascii_hexdigit()));
        }
    }

    #[test]
    #[ignore = "requires GitHub network access"]
    fn pinned_artifacts_are_downloadable_and_verified() {
        for artifact in [
            Artifact {
                label: "IDA fixture",
                filename: "source.zip",
                url: IDA_URL,
                sha256: IDA_SHA256,
            },
            Artifact {
                label: "Cheat Engine fixture",
                filename: "source.zip",
                url: CE_URL,
                sha256: CE_SHA256,
            },
            Artifact {
                label: "x64dbg bridge fixture",
                filename: "x64dbg.py",
                url: X64DBG_PY_URL,
                sha256: X64DBG_PY_SHA256,
            },
            Artifact {
                label: "x64dbg plugin fixture",
                filename: "MCP_Plugins.zip",
                url: X64DBG_PLUGINS_URL,
                sha256: X64DBG_PLUGINS_SHA256,
            },
            Artifact {
                label: "Burp extension fixture",
                filename: "burp-mcp-all.jar",
                url: BURP_EXTENSION_URL,
                sha256: BURP_EXTENSION_SHA256,
            },
            Artifact {
                label: "Burp proxy fixture",
                filename: "mcp-proxy-all.jar",
                url: BURP_PROXY_URL,
                sha256: BURP_PROXY_SHA256,
            },
        ] {
            assert!(!download_verified(artifact)
                .expect("download pinned artifact")
                .is_empty());
        }
    }

    #[test]
    #[ignore = "downloads sources and installs Python dependencies"]
    fn managed_sources_prepare_runnable_layouts() {
        let home = app_home().expect("resolve test app home");
        let _ = fs::remove_dir_all(&home);
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(home.clone());
        let python = if cfg!(target_os = "windows") {
            "python"
        } else {
            "python3"
        };
        let result = (|| -> Result<()> {
            let ida = acquire_ida("uv")?;
            assert!(regular_file(
                &ida.source_path.expect("IDA project").join("pyproject.toml")
            ));

            let ce = acquire_ce("remote", python)?;
            assert!(runnable_file(
                &ce.runtime_command.expect("CE runtime command")
            ));

            let x64dbg = acquire_x64dbg(python)?;
            assert!(runnable_file(
                &x64dbg.runtime_command.expect("x64dbg runtime command")
            ));
            assert!(regular_file(
                &x64dbg.managed_root.join("plugins/MCPx64dbg.dp64")
            ));

            let burp = acquire_burp("proxy")?;
            assert!(regular_file(&burp.source_path.expect("Burp proxy path")));
            Ok(())
        })();
        result.expect("prepare managed MCP sources");
    }

    #[test]
    fn command_error_detail_is_bounded() {
        let detail = command_error_detail("x".repeat(5_000).as_bytes());
        assert_eq!(detail.chars().count(), 1_600);
    }

    #[test]
    fn command_reader_keeps_only_the_tail() {
        let bytes = (0_u8..100).collect::<Vec<_>>();
        assert_eq!(
            read_tail(Cursor::new(bytes), 16),
            (84_u8..100).collect::<Vec<_>>()
        );
    }

    #[test]
    fn venv_runtime_path_matches_platform() {
        let root = Path::new("managed").join(".venv");
        let runtime = venv_python(&root);
        if cfg!(target_os = "windows") {
            assert!(runtime.ends_with("Scripts/python.exe"));
        } else {
            assert!(runtime.ends_with("bin/python"));
        }
    }
}
