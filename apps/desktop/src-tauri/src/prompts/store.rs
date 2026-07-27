use super::types::SavedPrompt;
use crate::error::{CodexxError, Result};
use crate::{now_rfc3339, open_db};
use rusqlite::params;

/// Codex engine 标识，用于 prompts 表的 engine 列过滤。
pub(crate) const ENGINE_CODEX: &str = "codex";
/// Claude engine 标识。
pub(crate) const ENGINE_CLAUDE: &str = "claude";
/// ZCode engine 标识。
pub(crate) const ENGINE_ZCODE: &str = "zcode";
/// Grok engine 标识。
pub(crate) const ENGINE_GROK: &str = "grok";

pub(crate) fn normalize_prompt_filename(input: &str, fallback: &str) -> String {
    let raw = input.trim().trim_end_matches(".md");
    let base = if raw.is_empty() { fallback } else { raw };
    let mut out = String::new();
    let mut last_dash = false;
    for ch in base.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-');
    format!("{}.md", if out.is_empty() { "custom-prompt" } else { out })
}

fn canonical_prompt_content(input: &str) -> String {
    input
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

pub(crate) fn list_saved_prompts_inner(engine: &str) -> Result<Vec<SavedPrompt>> {
    let conn = open_db()?;
    let mut stmt = conn
        .prepare("SELECT id, title, filename, content FROM prompts WHERE engine = ?1 ORDER BY updated_at DESC, created_at DESC")
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![engine], |row| {
            Ok(SavedPrompt {
                id: row.get(0)?,
                title: row.get(1)?,
                filename: row.get(2)?,
                content: row.get(3)?,
            })
        })
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    let mut prompts = Vec::new();
    for row in rows {
        let prompt = row.map_err(|e| CodexxError::Database(e.to_string()))?;
        let filename_key = prompt.filename.to_ascii_lowercase();
        let content_key = canonical_prompt_content(&prompt.content);
        let duplicate_index = prompts.iter().position(|existing: &SavedPrompt| {
            existing.filename.to_ascii_lowercase() == filename_key
                || (canonical_prompt_content(&existing.content) == content_key
                    && (existing.id.starts_with("external-") || prompt.id.starts_with("external-")))
        });
        if let Some(index) = duplicate_index {
            let existing_is_external = prompts[index].id.starts_with("external-");
            let prompt_is_external = prompt.id.starts_with("external-");
            if existing_is_external && !prompt_is_external {
                prompts[index] = prompt;
            }
            continue;
        }
        prompts.push(prompt);
    }
    Ok(prompts)
}

pub(crate) fn save_prompt_inner(prompt: SavedPrompt, engine: &str) -> Result<SavedPrompt> {
    let conn = open_db()?;
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO prompts (id, title, filename, content, engine, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            filename = excluded.filename,
            content = excluded.content,
            engine = excluded.engine,
            updated_at = excluded.updated_at",
        params![prompt.id, prompt.title, prompt.filename, prompt.content, engine, now],
    )
    .map_err(|e| CodexxError::Database(e.to_string()))?;
    list_saved_prompts_inner(engine)?
        .into_iter()
        .find(|p| p.id == prompt.id)
        .ok_or_else(|| CodexxError::Database("prompt saved but not found".to_string()))
}

pub(crate) fn get_saved_prompt_inner(id: &str, engine: &str) -> Result<SavedPrompt> {
    list_saved_prompts_inner(engine)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| CodexxError::Config(format!("提示词不存在: {id}")))
}

pub(crate) fn delete_prompt_inner(id: &str, engine: &str) -> Result<()> {
    let conn = open_db()?;
    conn.execute(
        "DELETE FROM prompts WHERE id = ?1 AND engine = ?2",
        params![id, engine],
    )
    .map_err(|e| CodexxError::Database(e.to_string()))?;
    Ok(())
}

fn find_saved_prompt_by_content(content: &str, engine: &str) -> Result<Option<SavedPrompt>> {
    let conn = open_db()?;
    let mut stmt = conn
        .prepare("SELECT id, title, filename, content FROM prompts WHERE engine = ?1 ORDER BY CASE WHEN id LIKE 'external-%' THEN 1 ELSE 0 END, updated_at DESC, created_at DESC")
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params![engine], |row| {
            Ok(SavedPrompt {
                id: row.get(0)?,
                title: row.get(1)?,
                filename: row.get(2)?,
                content: row.get(3)?,
            })
        })
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    let target = canonical_prompt_content(content);
    for row in rows {
        let prompt = row.map_err(|e| CodexxError::Database(e.to_string()))?;
        if canonical_prompt_content(&prompt.content) == target {
            return Ok(Some(prompt));
        }
    }
    Ok(None)
}

pub(super) fn find_saved_prompt_by_current_file(
    filename: &str,
    content: &str,
    engine: &str,
) -> Result<Option<SavedPrompt>> {
    if let Some(prompt) = find_saved_prompt_by_content(content, engine)? {
        return Ok(Some(prompt));
    }
    let normalized_filename = normalize_prompt_filename(filename, "external-prompt");
    let conn = open_db()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, title, filename, content FROM prompts
             WHERE engine = ?1 AND lower(filename) = lower(?2)
             ORDER BY CASE WHEN id LIKE 'external-%' THEN 1 ELSE 0 END, updated_at DESC, created_at DESC
             LIMIT 1",
        )
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    match stmt.query_row(params![engine, normalized_filename], |row| {
        Ok(SavedPrompt {
            id: row.get(0)?,
            title: row.get(1)?,
            filename: row.get(2)?,
            content: row.get(3)?,
        })
    }) {
        Ok(mut prompt) => {
            if canonical_prompt_content(&prompt.content) != canonical_prompt_content(content) {
                prompt.content = content.to_string();
            }
            Ok(Some(prompt))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(CodexxError::Database(e.to_string())),
    }
}
