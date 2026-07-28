use super::mcp::{json_to_toml_item, mcp_summary, sort_managed_mcp_servers, toml_item_to_json};
use super::skills::{
    copy_dir_recursive, move_dir_replace, normalize_legacy_zip_skill_dirs, read_skill_metadata,
    sanitize_dir_name, scan_skill_dir, sort_managed_skills,
};
use super::types::{
    ManagedMcpServer, ManagedSkill, SkillsMcpActionResult, SkillsMcpImportPreview, SkillsMcpState,
};
use crate::ccswitch::default_ccswitch_db_path;
use crate::constants::MAX_SKILL_ZIP_BYTES;
use crate::error::{CodexxError, Result};
use crate::file_io::{
    ensure_directory, io_err, parse_toml_document, read_to_string_if_exists, write_json, write_text,
};
use crate::paths::{app_home, home_dir};
use crate::toml_utils::ensure_table;
use crate::tools::ToolId;
use crate::{now_rfc3339, open_db};
use chrono::Local;
use rusqlite::{params, Connection, OpenFlags};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use toml_edit::Item;

fn disabled_skills_dir(tool: ToolId) -> Result<PathBuf> {
    Ok(app_home()?.join("disabled-skills").join(tool.as_str()))
}

fn mcp_config_path(tool: ToolId, config_dir: Option<String>) -> Result<PathBuf> {
    match tool {
        ToolId::Claude => Ok(home_dir()?.join(".claude.json")),
        _ => tool.config_path(config_dir),
    }
}

fn source_label(tool: ToolId) -> String {
    tool.label().to_string()
}

fn json_mcp_object(value: &Value, tool: ToolId) -> Option<&Map<String, Value>> {
    match tool {
        ToolId::Claude => value.get("mcpServers")?.as_object(),
        ToolId::Zcode => value.get("mcp")?.get("servers")?.as_object(),
        ToolId::Codex | ToolId::Grok => None,
    }
}

fn list_tool_mcp(tool: ToolId, config_dir: Option<String>) -> Result<Vec<ManagedMcpServer>> {
    let config = mcp_config_path(tool, config_dir)?;
    let text = read_to_string_if_exists(&config)?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let configs = match tool {
        ToolId::Codex | ToolId::Grok => {
            let document = parse_toml_document(&config, &text)?;
            let Some(table) = document.get("mcp_servers").and_then(|item| item.as_table()) else {
                return Ok(Vec::new());
            };
            table
                .iter()
                .filter_map(|(id, item)| {
                    item.is_table()
                        .then(|| (id.to_string(), toml_item_to_json(item)))
                })
                .collect::<Vec<_>>()
        }
        ToolId::Claude | ToolId::Zcode => {
            let value = serde_json::from_str::<Value>(&text)
                .map_err(|error| crate::file_io::json_err(&config, error))?;
            json_mcp_object(&value, tool)
                .map(|servers| {
                    servers
                        .iter()
                        .map(|(id, config)| (id.clone(), config.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
    };
    Ok(configs
        .into_iter()
        .map(|(id, config)| {
            let (transport, command, url, summary) = mcp_summary(&config);
            ManagedMcpServer {
                name: id.clone(),
                id,
                transport,
                enabled: true,
                source: config
                    .get("_everythingPatchSource")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| {
                        config
                            .as_object()
                            .map(|_| tool.label())
                            .unwrap_or("native config")
                    })
                    .to_string(),
                summary,
                command,
                url,
                config_json: config,
            }
        })
        .collect())
}

fn mcp_targets(tool: ToolId) -> Result<HashMap<String, bool>> {
    let connection = open_db()?;
    let mut statement = connection
        .prepare(
            "SELECT resource_id, enabled FROM managed_mcp_targets
             WHERE app_type = ?1",
        )
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    let rows = statement
        .query_map([tool.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    let mut targets = HashMap::new();
    for row in rows {
        let (id, enabled) = row.map_err(|error| CodexxError::Database(error.to_string()))?;
        targets.insert(id, enabled);
    }
    Ok(targets)
}

fn set_resource_target(table: &str, tool: ToolId, id: &str, enabled: bool) -> Result<()> {
    let table = match table {
        "managed_mcp_targets" => "managed_mcp_targets",
        "managed_skill_targets" => "managed_skill_targets",
        _ => {
            return Err(CodexxError::Database(
                "invalid managed resource table".to_string(),
            ))
        }
    };
    let connection = open_db()?;
    connection
        .execute(
            &format!(
                "INSERT INTO {table} (app_type, resource_id, enabled, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(app_type, resource_id) DO UPDATE SET
                   enabled = excluded.enabled,
                   updated_at = excluded.updated_at"
            ),
            params![tool.as_str(), id, enabled, now_rfc3339()],
        )
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    Ok(())
}

fn save_mcp_resource(id: &str, name: &str, config: &Value) -> Result<()> {
    let connection = open_db()?;
    connection
        .execute(
            "INSERT INTO managed_mcp_servers (id, name, server_config, enabled, updated_at)
             VALUES (?1, ?2, ?3, 0, ?4)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               server_config = excluded.server_config,
               updated_at = excluded.updated_at",
            params![
                id,
                name,
                serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string()),
                now_rfc3339(),
            ],
        )
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    Ok(())
}

fn db_mcp_for_tool(tool: ToolId) -> Result<Vec<(String, String, Value, bool)>> {
    let connection = open_db()?;
    let mut statement = connection
        .prepare(
            "SELECT server.id, server.name, server.server_config,
                    COALESCE(target.enabled, 0)
             FROM managed_mcp_servers AS server
             LEFT JOIN managed_mcp_targets AS target
               ON target.resource_id = server.id AND target.app_type = ?1
             ORDER BY server.name ASC, server.id ASC",
        )
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    let rows = statement
        .query_map([tool.as_str()], |row| {
            let text = row.get::<_, String>(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                serde_json::from_str::<Value>(&text).unwrap_or_else(|_| Value::Object(Map::new())),
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|error| CodexxError::Database(error.to_string()))?);
    }
    Ok(result)
}

fn save_skill_resource(skill: &ManagedSkill, tool: ToolId) -> Result<()> {
    let connection = open_db()?;
    connection
        .execute(
            "INSERT INTO managed_skills
               (id, name, description, directory, source_path, content_hash, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               description = excluded.description,
               directory = excluded.directory,
               source_path = excluded.source_path,
               content_hash = excluded.content_hash,
               updated_at = excluded.updated_at",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.directory,
                skill.path,
                skill.content_hash,
                now_rfc3339(),
            ],
        )
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    set_resource_target("managed_skill_targets", tool, &skill.id, skill.enabled)
}

fn scan_shared_skills(
    selected: ToolId,
    config_dir: Option<String>,
    selected_skills_dir: &Path,
    selected_disabled_dir: &Path,
    warnings: &mut Vec<String>,
) -> Result<Vec<ManagedSkill>> {
    let mut skills = Vec::new();
    let mut seen = HashSet::new();
    for (directory, enabled, source) in [
        (
            selected_skills_dir.to_path_buf(),
            true,
            source_label(selected),
        ),
        (
            selected_disabled_dir.to_path_buf(),
            false,
            "Everything Patch 已禁用".to_string(),
        ),
    ] {
        if let Err(error) = scan_skill_dir(&directory, enabled, &source, &mut skills, &mut seen) {
            warnings.push(error.to_string());
        }
    }
    for tool in ToolId::ALL {
        if tool == selected {
            continue;
        }
        let active = tool.skills_dir(config_dir.clone())?;
        let disabled = disabled_skills_dir(tool)?;
        for (directory, source) in [
            (active, format!("来自 {}", tool.label())),
            (disabled, format!("{} 已禁用", tool.label())),
        ] {
            if let Err(error) = scan_skill_dir(&directory, false, &source, &mut skills, &mut seen) {
                warnings.push(error.to_string());
            }
        }
    }
    Ok(skills)
}

pub(crate) fn build_tool_state_inner(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<SkillsMcpState> {
    let tool_dir = tool.home_dir(config_dir.clone())?;
    let skills_dir = tool.skills_dir(config_dir.clone())?;
    let disabled_dir = disabled_skills_dir(tool)?;
    let config_path = mcp_config_path(tool, config_dir.clone())?;
    let mut warnings = Vec::new();
    for directory in [&skills_dir, &disabled_dir] {
        if let Err(error) = normalize_legacy_zip_skill_dirs(directory) {
            warnings.push(format!("修正 ZIP Skill 目录名失败: {error}"));
        }
    }
    let mut skills = scan_shared_skills(
        tool,
        config_dir.clone(),
        &skills_dir,
        &disabled_dir,
        &mut warnings,
    )?;
    for skill in &skills {
        if let Err(error) = save_skill_resource(skill, tool) {
            warnings.push(error.to_string());
        }
    }

    let mut mcp_servers = list_tool_mcp(tool, config_dir)?;
    let live_ids = mcp_servers
        .iter()
        .map(|server| server.id.clone())
        .collect::<HashSet<_>>();
    let target_state = mcp_targets(tool)?;
    for (id, name, config, enabled) in db_mcp_for_tool(tool)? {
        if live_ids.contains(&id) {
            continue;
        }
        let (transport, command, url, summary) = mcp_summary(&config);
        mcp_servers.push(ManagedMcpServer {
            id: id.clone(),
            name,
            transport,
            enabled: target_state.get(&id).copied().unwrap_or(enabled),
            source: "Everything Patch".to_string(),
            summary,
            command,
            url,
            config_json: config,
        });
    }
    sort_managed_skills(&mut skills);
    sort_managed_mcp_servers(&mut mcp_servers);
    Ok(SkillsMcpState {
        tool,
        tool_label: tool.label().to_string(),
        tool_dir: tool_dir.display().to_string(),
        skills_dir: skills_dir.display().to_string(),
        config_path: config_path.display().to_string(),
        codex_dir: tool_dir.display().to_string(),
        codex_skills_dir: skills_dir.display().to_string(),
        disabled_skills_dir: disabled_dir.display().to_string(),
        skills,
        mcp_servers,
        warnings,
    })
}

fn set_json_mcp(root: &mut Map<String, Value>, tool: ToolId, id: &str, config: Option<Value>) {
    match tool {
        ToolId::Claude => {
            let servers = root
                .entry("mcpServers".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !servers.is_object() {
                *servers = Value::Object(Map::new());
            }
            let servers = servers.as_object_mut().expect("mcpServers is an object");
            if let Some(config) = config {
                servers.insert(id.to_string(), config);
            } else {
                servers.remove(id);
            }
        }
        ToolId::Zcode => {
            let mcp = root
                .entry("mcp".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !mcp.is_object() {
                *mcp = Value::Object(Map::new());
            }
            let mcp = mcp.as_object_mut().expect("mcp is an object");
            let servers = mcp
                .entry("servers".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !servers.is_object() {
                *servers = Value::Object(Map::new());
            }
            let servers = servers.as_object_mut().expect("servers is an object");
            if let Some(config) = config {
                servers.insert(id.to_string(), config);
            } else {
                servers.remove(id);
            }
        }
        ToolId::Codex | ToolId::Grok => {}
    }
}

fn toml_mcp_item_for_tool(tool: ToolId, config: &Value) -> Item {
    let mut normalized = config.clone();
    if let Some(server) = normalized.as_object_mut() {
        match tool {
            ToolId::Codex => {
                if !server.contains_key("http_headers") {
                    if let Some(headers) = server.remove("headers") {
                        server.insert("http_headers".to_string(), headers);
                    }
                } else {
                    server.remove("headers");
                }
            }
            ToolId::Grok => {
                server.remove("type");
                if !server.contains_key("headers") {
                    if let Some(headers) = server.remove("http_headers") {
                        server.insert("headers".to_string(), headers);
                    }
                } else {
                    server.remove("http_headers");
                }
            }
            ToolId::Claude | ToolId::Zcode => {}
        }
    }
    json_to_toml_item(&normalized)
}

fn write_tool_mcp(
    tool: ToolId,
    config_dir: Option<String>,
    id: &str,
    config: Option<Value>,
) -> Result<()> {
    let path = mcp_config_path(tool, config_dir)?;
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }
    let text = read_to_string_if_exists(&path)?;
    match tool {
        ToolId::Codex | ToolId::Grok => {
            let mut document = parse_toml_document(&path, &text)?;
            if let Some(config) = config {
                ensure_table(document.as_table_mut(), "mcp_servers")?
                    .insert(id, toml_mcp_item_for_tool(tool, &config));
            } else if let Some(table) = document
                .get_mut("mcp_servers")
                .and_then(|item| item.as_table_mut())
            {
                table.remove(id);
            }
            write_text(&path, &document.to_string())
        }
        ToolId::Claude | ToolId::Zcode => {
            let mut root = if text.trim().is_empty() {
                Map::new()
            } else {
                serde_json::from_str::<Value>(&text)
                    .map_err(|error| crate::file_io::json_err(&path, error))?
                    .as_object()
                    .cloned()
                    .ok_or_else(|| {
                        CodexxError::Config(format!(
                            "MCP 配置必须是 JSON object: {}",
                            path.display()
                        ))
                    })?
            };
            set_json_mcp(&mut root, tool, id, config);
            write_json(&path, &Value::Object(root))
        }
    }
}

pub(crate) fn toggle_tool_mcp_inner(
    tool: ToolId,
    config_dir: Option<String>,
    id: String,
    enabled: bool,
) -> Result<SkillsMcpState> {
    let live = list_tool_mcp(tool, config_dir.clone())?;
    let existing_live = live.iter().find(|server| server.id == id);
    let config = if enabled {
        db_mcp_for_tool(tool)?
            .into_iter()
            .find(|(server_id, _, _, _)| server_id == &id)
            .map(|(_, _, config, _)| config)
            .or_else(|| existing_live.map(|server| server.config_json.clone()))
            .ok_or_else(|| CodexxError::Config(format!("未找到 MCP: {id}")))?
    } else {
        if let Some(server) = existing_live {
            save_mcp_resource(&server.id, &server.name, &server.config_json)?;
        }
        Value::Null
    };
    write_tool_mcp(tool, config_dir.clone(), &id, enabled.then_some(config))?;
    set_resource_target("managed_mcp_targets", tool, &id, enabled)?;
    build_tool_state_inner(tool, config_dir)
}

pub(crate) fn toggle_tool_skill_inner(
    tool: ToolId,
    config_dir: Option<String>,
    id: String,
    enabled: bool,
) -> Result<SkillsMcpState> {
    let state = build_tool_state_inner(tool, config_dir.clone())?;
    let skill = state
        .skills
        .iter()
        .find(|skill| skill.id == id)
        .cloned()
        .ok_or_else(|| CodexxError::Config(format!("未找到 Skill: {id}")))?;
    let skills_dir = tool.skills_dir(config_dir.clone())?;
    let disabled_dir = disabled_skills_dir(tool)?;
    ensure_directory(&skills_dir)?;
    ensure_directory(&disabled_dir)?;
    let enabled_path = skills_dir.join(&skill.directory);
    let disabled_path = disabled_dir.join(&skill.directory);
    if enabled {
        if disabled_path.is_dir() {
            move_dir_replace(&disabled_path, &enabled_path)?;
        } else if !enabled_path.is_dir() {
            let source = PathBuf::from(&skill.path);
            if !source.is_dir() {
                return Err(CodexxError::Config(format!(
                    "Skill 源目录不存在: {}",
                    source.display()
                )));
            }
            copy_dir_recursive(&source, &enabled_path)?;
        }
    } else if enabled_path.is_dir() {
        move_dir_replace(&enabled_path, &disabled_path)?;
    }
    set_resource_target("managed_skill_targets", tool, &id, enabled)?;
    build_tool_state_inner(tool, config_dir)
}

fn ccswitch_mcp_candidates(tool: ToolId) -> Result<Vec<ManagedMcpServer>> {
    let database = default_ccswitch_db_path()?;
    if !database.is_file() {
        return Ok(Vec::new());
    }
    let connection = Connection::open_with_flags(
        &database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| CodexxError::Database(error.to_string()))?;
    let columns = crate::sqlite_utils::table_column_set(&connection, "mcp_servers")?;
    if columns.is_empty() {
        return Ok(Vec::new());
    }
    let enabled_column = match tool {
        ToolId::Codex => "enabled_codex",
        ToolId::Claude => "enabled_claude",
        ToolId::Grok => "enabled_grokbuild",
        ToolId::Zcode => "",
    };
    let enabled_expression = if columns.contains(enabled_column) {
        enabled_column
    } else {
        "0"
    };
    let query = format!(
        "SELECT id, name, server_config, {enabled_expression}
         FROM mcp_servers ORDER BY name ASC, id ASC"
    );
    let mut statement = connection
        .prepare(&query)
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            let id = row.get::<_, String>(0)?;
            let name = row.get::<_, String>(1)?;
            let config_text = row.get::<_, String>(2)?;
            let config = serde_json::from_str::<Value>(&config_text)
                .unwrap_or_else(|_| Value::Object(Map::new()));
            let (transport, command, url, summary) = mcp_summary(&config);
            Ok(ManagedMcpServer {
                id,
                name,
                transport,
                enabled: row.get::<_, bool>(3)?,
                source: "cc-switch".to_string(),
                summary,
                command,
                url,
                config_json: config,
            })
        })
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|error| CodexxError::Database(error.to_string()))?);
    }
    Ok(result)
}

fn import_skill_candidates(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<Vec<(PathBuf, String)>> {
    let mut candidates = vec![
        (
            home_dir()?.join(".agents").join("skills"),
            ".agents".to_string(),
        ),
        (
            home_dir()?.join(".cc-switch").join("skills"),
            "cc-switch".to_string(),
        ),
    ];
    for source_tool in ToolId::ALL {
        if source_tool != tool {
            candidates.push((
                source_tool.skills_dir(config_dir.clone())?,
                source_tool.label().to_string(),
            ));
        }
    }
    Ok(candidates)
}

pub(crate) fn preview_tool_import_inner(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<SkillsMcpImportPreview> {
    let destination = tool.skills_dir(config_dir.clone())?;
    let mut skills = Vec::new();
    let mut seen = HashSet::new();
    let mut warnings = Vec::new();
    for (candidate, source) in import_skill_candidates(tool, config_dir.clone())? {
        let before = skills.len();
        if let Err(error) = scan_skill_dir(&candidate, false, &source, &mut skills, &mut seen) {
            warnings.push(error.to_string());
            continue;
        }
        for skill in &mut skills[before..] {
            skill.update_status = if destination.join(&skill.directory).is_dir() {
                "已存在，将跳过".to_string()
            } else {
                "可导入".to_string()
            };
        }
    }
    skills.retain(|skill| skill.update_status == "可导入");
    let managed_ids = db_mcp_for_tool(tool)?
        .into_iter()
        .map(|(id, _, _, _)| id)
        .collect::<HashSet<_>>();
    let mut mcp_servers = list_tool_mcp(tool, config_dir)?
        .into_iter()
        .chain(ccswitch_mcp_candidates(tool)?)
        .filter(|server| !managed_ids.contains(&server.id))
        .collect::<Vec<_>>();
    let mut mcp_seen = HashSet::new();
    mcp_servers.retain(|server| mcp_seen.insert(server.id.clone()));
    sort_managed_skills(&mut skills);
    sort_managed_mcp_servers(&mut mcp_servers);
    Ok(SkillsMcpImportPreview {
        skills,
        mcp_servers,
        warnings,
    })
}

pub(crate) fn import_tool_resources_inner(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<SkillsMcpActionResult> {
    let skills_dir = tool.skills_dir(config_dir.clone())?;
    ensure_directory(&skills_dir)?;
    let mut imported_skills = 0usize;
    for (candidate, _) in import_skill_candidates(tool, config_dir.clone())? {
        if !candidate.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&candidate).map_err(|error| io_err(&candidate, error))? {
            let entry = entry.map_err(|error| io_err(&candidate, error))?;
            let source = entry.path();
            if !source.is_dir() || !source.join("SKILL.md").is_file() {
                continue;
            }
            let directory = sanitize_dir_name(&entry.file_name().to_string_lossy(), "skill");
            let destination = skills_dir.join(directory);
            if destination.exists() {
                continue;
            }
            copy_dir_recursive(&source, &destination)?;
            imported_skills += 1;
        }
    }
    let mut imported_mcp = 0usize;
    let live_ids = list_tool_mcp(tool, config_dir.clone())?
        .into_iter()
        .map(|server| server.id)
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    for server in list_tool_mcp(tool, config_dir.clone())?
        .into_iter()
        .chain(ccswitch_mcp_candidates(tool)?)
    {
        if !seen.insert(server.id.clone()) {
            continue;
        }
        save_mcp_resource(&server.id, &server.name, &server.config_json)?;
        let enabled = live_ids.contains(&server.id) || server.enabled;
        set_resource_target("managed_mcp_targets", tool, &server.id, enabled)?;
        if enabled && !live_ids.contains(&server.id) {
            write_tool_mcp(
                tool,
                config_dir.clone(),
                &server.id,
                Some(server.config_json.clone()),
            )?;
        }
        imported_mcp += 1;
    }
    let state = build_tool_state_inner(tool, config_dir)?;
    Ok(SkillsMcpActionResult {
        imported_skills,
        imported_mcp,
        message: format!(
            "已为 {} 导入 {imported_skills} 个 Skills，纳管 {imported_mcp} 个 MCP",
            tool.label()
        ),
        state,
    })
}

pub(crate) fn install_tool_skill_zip_inner(
    tool: ToolId,
    config_dir: Option<String>,
    file_name: String,
    bytes: Vec<u8>,
) -> Result<SkillsMcpActionResult> {
    let skills_dir = tool.skills_dir(config_dir.clone())?;
    ensure_directory(&skills_dir)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| CodexxError::Config(format!("读取 ZIP 失败: {error}")))?;
    let temporary = app_home()?
        .join("tmp")
        .join(format!("skill-zip-{}", Local::now().timestamp_millis()));
    ensure_directory(&temporary)?;
    let install_result = (|| -> Result<usize> {
        let mut total_size = 0u64;
        for index in 0..archive.len() {
            let mut file = archive
                .by_index(index)
                .map_err(|error| CodexxError::Config(format!("读取 ZIP 条目失败: {error}")))?;
            let Some(relative) = file.enclosed_name() else {
                continue;
            };
            total_size = total_size.saturating_add(file.size());
            if total_size > MAX_SKILL_ZIP_BYTES {
                return Err(CodexxError::Config("ZIP 解压后超过 20MB".to_string()));
            }
            let output = temporary.join(relative);
            if file.is_dir() {
                ensure_directory(&output)?;
            } else {
                if let Some(parent) = output.parent() {
                    ensure_directory(parent)?;
                }
                let mut destination =
                    fs::File::create(&output).map_err(|error| io_err(&output, error))?;
                std::io::copy(&mut file, &mut destination)
                    .map_err(|error| io_err(&output, error))?;
            }
        }
        fn find_skills(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
            if directory.join("SKILL.md").is_file() {
                output.push(directory.to_path_buf());
                return Ok(());
            }
            for entry in fs::read_dir(directory).map_err(|error| io_err(directory, error))? {
                let path = entry.map_err(|error| io_err(directory, error))?.path();
                if path.is_dir() {
                    find_skills(&path, output)?;
                }
            }
            Ok(())
        }
        let mut found = Vec::new();
        find_skills(&temporary, &mut found)?;
        if found.is_empty() {
            return Err(CodexxError::Config("ZIP 中没有找到 SKILL.md".to_string()));
        }
        for source in &found {
            let fallback = file_name.trim_end_matches(".zip");
            let directory = source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(fallback);
            let (name, _) = read_skill_metadata(source, directory);
            let destination = skills_dir.join(sanitize_dir_name(&name, "skill"));
            if destination.exists() {
                fs::remove_dir_all(&destination).map_err(|error| io_err(&destination, error))?;
            }
            copy_dir_recursive(source, &destination)?;
        }
        Ok(found.len())
    })();
    let _ = fs::remove_dir_all(&temporary);
    let imported_skills = install_result?;
    let state = build_tool_state_inner(tool, config_dir)?;
    Ok(SkillsMcpActionResult {
        imported_skills,
        imported_mcp: 0,
        message: format!(
            "已为 {} 从 ZIP 安装 {imported_skills} 个 Skill",
            tool.label()
        ),
        state,
    })
}

pub(crate) fn check_tool_skill_updates_inner(
    tool: ToolId,
    config_dir: Option<String>,
) -> Result<SkillsMcpState> {
    if tool == ToolId::Codex {
        return super::skills::check_skill_updates_inner(config_dir);
    }
    build_tool_state_inner(tool, config_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn grok_mcp_omits_type_and_uses_headers() {
        let item = toml_mcp_item_for_tool(
            ToolId::Grok,
            &json!({
                "type": "http",
                "url": "https://example.com/mcp",
                "http_headers": { "Authorization": "Bearer token" }
            }),
        );
        let table = item.as_table().expect("Grok MCP should be a TOML table");
        assert!(!table.contains_key("type"));
        assert!(!table.contains_key("http_headers"));
        assert_eq!(
            table
                .get("headers")
                .and_then(Item::as_table)
                .and_then(|headers| headers.get("Authorization"))
                .and_then(Item::as_str),
            Some("Bearer token")
        );
    }

    #[test]
    fn codex_mcp_uses_http_headers() {
        let item = toml_mcp_item_for_tool(
            ToolId::Codex,
            &json!({
                "type": "http",
                "url": "https://example.com/mcp",
                "headers": { "Authorization": "Bearer token" }
            }),
        );
        let table = item.as_table().expect("Codex MCP should be a TOML table");
        assert!(table.contains_key("type"));
        assert!(!table.contains_key("headers"));
        assert!(table.contains_key("http_headers"));
    }
}
