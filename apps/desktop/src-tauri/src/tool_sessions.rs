use crate::error::Result;
use crate::paths::home_dir;
use crate::sessions::session_sync_status_inner;
use crate::tools::ToolId;
use chrono::DateTime;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const MAX_SESSIONS: usize = 5_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolSession {
    id: String,
    title: String,
    summary: Option<String>,
    cwd: Option<String>,
    source_path: Option<String>,
    created_at_ms: Option<i64>,
    updated_at_ms: Option<i64>,
    archived: bool,
    resume_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolSessionList {
    tool: ToolId,
    root: String,
    read_only: bool,
    sessions: Vec<ToolSession>,
    warnings: Vec<String>,
}

fn truncate(value: &str, max_chars: usize) -> String {
    let trimmed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.chars().count() <= max_chars {
        return trimmed;
    }
    let mut result = trimmed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    result.push('…');
    result
}

fn extract_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(extract_text)
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .or_else(|| object.get("message"))
            .map(extract_text)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn timestamp_ms(value: &Value) -> Option<i64> {
    if let Some(number) = value.as_i64() {
        return Some(if number.abs() < 10_000_000_000 {
            number.saturating_mul(1_000)
        } else {
            number
        });
    }
    if let Some(number) = value.as_f64() {
        let number = if number.abs() < 10_000_000_000.0 {
            number * 1_000.0
        } else {
            number
        };
        return Some(number as i64);
    }
    let text = value.as_str()?.trim();
    if let Ok(number) = text.parse::<i64>() {
        return Some(if number.abs() < 10_000_000_000 {
            number.saturating_mul(1_000)
        } else {
            number
        });
    }
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|timestamp| timestamp.timestamp_millis())
}

fn collect_named_files(
    root: &Path,
    matcher: impl Fn(&Path) -> bool + Copy,
    files: &mut Vec<PathBuf>,
) {
    if files.len() >= MAX_SESSIONS || !root.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_SESSIONS {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_named_files(&path, matcher, files);
        } else if matcher(&path) {
            files.push(path);
        }
    }
}

fn claude_session(path: &Path) -> Option<ToolSession> {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("agent-"))
    {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut id = None;
    let mut cwd = None;
    let mut created_at_ms = None;
    let mut updated_at_ms = None;
    let mut first_user = None;
    let mut summary = None;
    let mut custom_title = None;
    for line in reader.lines().map_while(std::result::Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if id.is_none() {
            id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        if cwd.is_none() {
            cwd = value
                .get("cwd")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        if let Some(timestamp) = value.get("timestamp").and_then(timestamp_ms) {
            created_at_ms.get_or_insert(timestamp);
            updated_at_ms = Some(timestamp);
        }
        if value.get("type").and_then(Value::as_str) == Some("custom-title") {
            custom_title = value
                .get("customTitle")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(ToString::to_string);
        }
        if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let message = value.get("message");
        let role = message
            .and_then(|message| message.get("role"))
            .and_then(Value::as_str)
            .or_else(|| value.get("type").and_then(Value::as_str));
        let content = message
            .and_then(|message| message.get("content"))
            .map(extract_text)
            .unwrap_or_default();
        let content = content.trim();
        if content.is_empty() {
            continue;
        }
        if first_user.is_none()
            && role == Some("user")
            && !content.contains("<local-command-caveat>")
            && !content.starts_with("<command-name>")
        {
            first_user = Some(content.to_string());
        }
        summary = Some(content.to_string());
    }
    let id = id.or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(ToString::to_string)
    })?;
    let title = custom_title
        .or(first_user)
        .map(|title| truncate(&title, 72))
        .or_else(|| {
            cwd.as_deref().and_then(|cwd| {
                Path::new(cwd)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(ToString::to_string)
            })
        })
        .unwrap_or_else(|| id.clone());
    Some(ToolSession {
        id: id.clone(),
        title,
        summary: summary.map(|summary| truncate(&summary, 160)),
        cwd,
        source_path: Some(path.display().to_string()),
        created_at_ms,
        updated_at_ms,
        archived: false,
        resume_command: Some(format!("claude --resume {id}")),
    })
}

fn claude_sessions() -> Result<ToolSessionList> {
    let root = home_dir()?.join(".claude").join("projects");
    let mut files = Vec::new();
    collect_named_files(
        &root,
        |path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
        },
        &mut files,
    );
    let mut sessions = files
        .iter()
        .filter_map(|path| claude_session(path))
        .collect::<Vec<_>>();
    sort_sessions(&mut sessions);
    Ok(ToolSessionList {
        tool: ToolId::Claude,
        root: root.display().to_string(),
        read_only: true,
        sessions,
        warnings: (files.len() >= MAX_SESSIONS)
            .then(|| format!("会话数量超过 {MAX_SESSIONS}，当前仅展示前 {MAX_SESSIONS} 个文件"))
            .into_iter()
            .collect(),
    })
}

#[derive(Debug, Deserialize)]
struct GrokSessionInfo {
    id: String,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GrokSessionSummary {
    info: GrokSessionInfo,
    #[serde(default)]
    session_summary: Option<String>,
    #[serde(default)]
    generated_title: Option<String>,
    #[serde(default)]
    created_at: Option<Value>,
    #[serde(default)]
    updated_at: Option<Value>,
    #[serde(default)]
    last_active_at: Option<Value>,
}

fn grok_session(path: &Path, archived: bool) -> Option<ToolSession> {
    let text = fs::read_to_string(path).ok()?;
    let summary = serde_json::from_str::<GrokSessionSummary>(&text).ok()?;
    let id = summary.info.id;
    let title = summary
        .generated_title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            summary
                .session_summary
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
        .map(|value| truncate(value, 72))
        .unwrap_or_else(|| id.clone());
    Some(ToolSession {
        id: id.clone(),
        title,
        summary: summary
            .session_summary
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| truncate(value, 160)),
        cwd: summary.info.cwd,
        source_path: Some(path.display().to_string()),
        created_at_ms: summary.created_at.as_ref().and_then(timestamp_ms),
        updated_at_ms: summary
            .last_active_at
            .as_ref()
            .or(summary.updated_at.as_ref())
            .and_then(timestamp_ms),
        archived,
        resume_command: Some(format!("grok --resume {id}")),
    })
}

fn grok_sessions() -> Result<ToolSessionList> {
    let root = home_dir()?.join(".grok");
    let roots = [
        (root.join("sessions"), false),
        (root.join("archived_sessions"), true),
    ];
    let mut sessions = Vec::new();
    let mut truncated = false;
    for (session_root, archived) in roots {
        let mut files = Vec::new();
        collect_named_files(
            &session_root,
            |path| path.file_name().and_then(|name| name.to_str()) == Some("summary.json"),
            &mut files,
        );
        truncated |= files.len() >= MAX_SESSIONS;
        sessions.extend(files.iter().filter_map(|path| grok_session(path, archived)));
    }
    sort_sessions(&mut sessions);
    sessions.truncate(MAX_SESSIONS);
    Ok(ToolSessionList {
        tool: ToolId::Grok,
        root: root.display().to_string(),
        read_only: true,
        sessions,
        warnings: truncated
            .then(|| format!("会话数量超过 {MAX_SESSIONS}，结果已截断"))
            .into_iter()
            .collect(),
    })
}

fn sqlite_time(row: &rusqlite::Row<'_>, index: usize) -> Option<i64> {
    row.get::<_, Option<i64>>(index)
        .ok()
        .flatten()
        .map(|value| {
            if value.abs() < 10_000_000_000 {
                value.saturating_mul(1_000)
            } else {
                value
            }
        })
        .or_else(|| {
            row.get::<_, Option<String>>(index)
                .ok()
                .flatten()
                .and_then(|value| timestamp_ms(&Value::String(value)))
        })
}

fn zcode_sessions() -> Result<ToolSessionList> {
    let root = home_dir()?.join(".zcode");
    let database = root.join("cli").join("db").join("db.sqlite");
    let warning_result = |message: String| ToolSessionList {
        tool: ToolId::Zcode,
        root: root.display().to_string(),
        read_only: true,
        sessions: Vec::new(),
        warnings: vec![message],
    };
    if !database.is_file() {
        return Ok(warning_result(format!(
            "未找到 ZCode 会话数据库: {}",
            database.display()
        )));
    }
    let connection = match Connection::open_with_flags(
        &database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(connection) => connection,
        Err(error) => {
            return Ok(warning_result(format!(
                "无法读取 ZCode 会话数据库 {}: {error}",
                database.display()
            )))
        }
    };
    let mut statement = match connection.prepare(
        "SELECT id, directory, title, time_created, time_updated, time_archived
             FROM session
             ORDER BY time_updated DESC
             LIMIT ?1",
    ) {
        Ok(statement) => statement,
        Err(error) => {
            return Ok(warning_result(format!(
                "当前 ZCode 会话数据库结构暂不兼容，已跳过会话读取: {error}"
            )))
        }
    };
    let rows = match statement.query_map([MAX_SESSIONS as i64], |row| {
        let id = row.get::<_, String>(0)?;
        let title = row
            .get::<_, Option<String>>(2)?
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| id.clone());
        Ok(ToolSession {
            id,
            title,
            summary: None,
            cwd: row.get::<_, Option<String>>(1)?,
            source_path: Some(database.display().to_string()),
            created_at_ms: sqlite_time(row, 3),
            updated_at_ms: sqlite_time(row, 4),
            archived: sqlite_time(row, 5).is_some(),
            resume_command: None,
        })
    }) {
        Ok(rows) => rows,
        Err(error) => {
            return Ok(warning_result(format!(
                "查询 ZCode 会话失败，已返回空列表: {error}"
            )))
        }
    };
    let mut sessions = Vec::new();
    for row in rows {
        match row {
            Ok(session) => sessions.push(session),
            Err(error) => {
                return Ok(warning_result(format!(
                    "解析 ZCode 会话失败，已返回空列表: {error}"
                )))
            }
        }
    }
    Ok(ToolSessionList {
        tool: ToolId::Zcode,
        root: root.display().to_string(),
        read_only: true,
        sessions,
        warnings: Vec::new(),
    })
}

fn codex_sessions(codex_override: Option<String>) -> Result<ToolSessionList> {
    let status = session_sync_status_inner(codex_override, None)?;
    let sessions = status
        .sessions
        .into_iter()
        .map(|session| ToolSession {
            id: session.id,
            title: session.title,
            summary: None,
            cwd: session.cwd,
            source_path: session.rollout_path,
            created_at_ms: None,
            updated_at_ms: session.updated_at_ms,
            archived: session.archived,
            resume_command: None,
        })
        .collect();
    Ok(ToolSessionList {
        tool: ToolId::Codex,
        root: status.codex_dir,
        read_only: false,
        sessions,
        warnings: status.warnings,
    })
}

fn sort_sessions(sessions: &mut [ToolSession]) {
    sessions.sort_by(|left, right| {
        right
            .updated_at_ms
            .or(right.created_at_ms)
            .unwrap_or_default()
            .cmp(
                &left
                    .updated_at_ms
                    .or(left.created_at_ms)
                    .unwrap_or_default(),
            )
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub(crate) fn get_tool_sessions_inner(
    tool: ToolId,
    codex_override: Option<String>,
) -> Result<ToolSessionList> {
    match tool {
        ToolId::Codex => codex_sessions(codex_override),
        ToolId::Claude => claude_sessions(),
        ToolId::Grok => grok_sessions(),
        ToolId::Zcode => zcode_sessions(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_accept_seconds_milliseconds_and_rfc3339() {
        assert_eq!(
            timestamp_ms(&Value::from(1_700_000_000)),
            Some(1_700_000_000_000)
        );
        assert_eq!(
            timestamp_ms(&Value::from(1_700_000_000_123_i64)),
            Some(1_700_000_000_123)
        );
        assert_eq!(
            timestamp_ms(&Value::from("2026-07-16T12:00:00Z")),
            Some(1_784_203_200_000)
        );
    }

    #[test]
    fn text_extraction_handles_content_arrays() {
        let value = serde_json::json!([
            {"type": "text", "text": "hello"},
            {"type": "text", "text": "world"}
        ]);
        assert_eq!(extract_text(&value), "hello\nworld");
    }
}
