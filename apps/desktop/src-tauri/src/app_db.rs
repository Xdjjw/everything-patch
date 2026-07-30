use crate::error::{CodexxError, Result};
use crate::file_io::ensure_directory;
use crate::paths::app_home;
use crate::sqlite_utils::table_column_set;
use rusqlite::Connection;
use std::path::PathBuf;

const PROVIDERS_TABLE_SQL: &str = "CREATE TABLE providers (
    app_type TEXT NOT NULL DEFAULT 'codex',
    id TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    api_key TEXT,
    toml_config TEXT,
    wire_api TEXT NOT NULL DEFAULT 'responses',
    requires_openai_auth INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (app_type, id)
)";

fn db_path() -> Result<PathBuf> {
    let home = app_home()?;
    let current = home.join("everything-patch.db");
    let legacy = home.join("codexx.db");
    if !current.exists() && legacy.is_file() {
        return Ok(legacy);
    }
    Ok(current)
}

fn ensure_sqlite_column(
    conn: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<()> {
    let cols = table_column_set(conn, table)?;
    if cols.contains(column) {
        return Ok(());
    }
    match conn.execute(alter_sql, []) {
        Ok(_) => Ok(()),
        Err(e) => {
            let message = e.to_string().to_ascii_lowercase();
            if message.contains("duplicate column") || message.contains("duplicate column name") {
                // Another running DevConduit process may have applied the same
                // lightweight migration between our PRAGMA check and ALTER.
                Ok(())
            } else {
                Err(CodexxError::Database(e.to_string()))
            }
        }
    }
}

fn providers_primary_key(conn: &Connection) -> Result<Vec<String>> {
    let mut statement = conn
        .prepare("PRAGMA table_info(providers)")
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    let mut columns = Vec::new();
    for row in rows {
        let (name, order) = row.map_err(|error| CodexxError::Database(error.to_string()))?;
        if order > 0 {
            columns.push((order, name));
        }
    }
    columns.sort_by_key(|(order, _)| *order);
    Ok(columns.into_iter().map(|(_, name)| name).collect())
}

fn migrate_providers_schema(conn: &mut Connection) -> Result<()> {
    let columns = table_column_set(conn, "providers")?;
    if columns.is_empty() {
        return Ok(());
    }
    let primary_key = providers_primary_key(conn)?;
    if columns.contains("app_type") && primary_key == vec!["app_type".to_string(), "id".to_string()]
    {
        return Ok(());
    }

    let app_type = if columns.contains("app_type") {
        "COALESCE(NULLIF(trim(app_type), ''), 'codex')"
    } else {
        "'codex'"
    };
    let toml_config = if columns.contains("toml_config") {
        "toml_config"
    } else {
        "NULL"
    };
    let transaction = conn
        .transaction()
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    transaction
        .execute_batch(
            "ALTER TABLE providers RENAME TO providers_everything_patch_legacy;
             DROP INDEX IF EXISTS idx_providers_updated_at;",
        )
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    transaction
        .execute(PROVIDERS_TABLE_SQL, [])
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    transaction
        .execute(
            &format!(
                "INSERT INTO providers (
                    app_type, id, provider_name, base_url, model, api_key, toml_config,
                    wire_api, requires_openai_auth, created_at, updated_at
                 )
                 SELECT {app_type}, id, provider_name, base_url, model, api_key, {toml_config},
                        wire_api, requires_openai_auth, created_at, updated_at
                 FROM providers_everything_patch_legacy"
            ),
            [],
        )
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    transaction
        .execute_batch(
            "DROP TABLE providers_everything_patch_legacy;
             CREATE INDEX idx_providers_updated_at
             ON providers(app_type, updated_at DESC);",
        )
        .map_err(|error| CodexxError::Database(error.to_string()))?;
    transaction
        .commit()
        .map_err(|error| CodexxError::Database(error.to_string()))
}

pub(crate) fn open() -> Result<Connection> {
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }
    let mut conn = Connection::open(&path).map_err(|e| CodexxError::Database(e.to_string()))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS providers (
            app_type TEXT NOT NULL DEFAULT 'codex',
            id TEXT NOT NULL,
            provider_name TEXT NOT NULL,
            base_url TEXT NOT NULL,
            model TEXT NOT NULL,
            api_key TEXT,
            toml_config TEXT,
            wire_api TEXT NOT NULL DEFAULT 'responses',
            requires_openai_auth INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (app_type, id)
        );
        CREATE TABLE IF NOT EXISTS prompts (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            filename TEXT NOT NULL,
            content TEXT NOT NULL,
            engine TEXT NOT NULL DEFAULT 'codex',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_prompts_updated_at ON prompts(updated_at DESC);
        CREATE TABLE IF NOT EXISTS builtin_prompt_cache (
            id TEXT PRIMARY KEY,
            filename TEXT NOT NULL,
            source_url TEXT NOT NULL,
            content TEXT NOT NULL,
            checked_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS managed_mcp_servers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            server_config TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS managed_skills (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            directory TEXT NOT NULL,
            source_path TEXT,
            content_hash TEXT,
            enabled INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS managed_mcp_targets (
            app_type TEXT NOT NULL,
            resource_id TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (app_type, resource_id),
            FOREIGN KEY (resource_id) REFERENCES managed_mcp_servers(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS managed_skill_targets (
            app_type TEXT NOT NULL,
            resource_id TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (app_type, resource_id),
            FOREIGN KEY (resource_id) REFERENCES managed_skills(id) ON DELETE CASCADE
        );",
    )
    .map_err(|e| CodexxError::Database(e.to_string()))?;
    ensure_sqlite_column(
        &conn,
        "providers",
        "toml_config",
        "ALTER TABLE providers ADD COLUMN toml_config TEXT",
    )?;
    migrate_providers_schema(&mut conn)?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_providers_updated_at
         ON providers(app_type, updated_at DESC)",
        [],
    )
    .map_err(|error| CodexxError::Database(error.to_string()))?;
    conn.execute(
        "INSERT OR IGNORE INTO managed_mcp_targets (app_type, resource_id, enabled, updated_at)
         SELECT 'codex', id, enabled, updated_at FROM managed_mcp_servers",
        [],
    )
    .map_err(|error| CodexxError::Database(error.to_string()))?;
    conn.execute(
        "INSERT OR IGNORE INTO managed_skill_targets (app_type, resource_id, enabled, updated_at)
         SELECT 'codex', id, enabled, updated_at FROM managed_skills",
        [],
    )
    .map_err(|error| CodexxError::Database(error.to_string()))?;
    // prompts.engine：旧库没有该列，幂等添加并回填为 'codex'。
    ensure_sqlite_column(
        &conn,
        "prompts",
        "engine",
        "ALTER TABLE prompts ADD COLUMN engine TEXT NOT NULL DEFAULT 'codex'",
    )?;
    conn.execute(
        "DELETE FROM prompts
         WHERE id LIKE 'external-%'
           AND EXISTS (
             SELECT 1 FROM prompts AS kept
             WHERE lower(kept.filename) = lower(prompts.filename)
               AND kept.id NOT LIKE 'external-%'
               AND kept.engine = prompts.engine
           )",
        [],
    )
    .map_err(|e| CodexxError::Database(e.to_string()))?;
    conn.execute(
        "DELETE FROM prompts
         WHERE id LIKE 'external-%'
           AND EXISTS (
             SELECT 1 FROM prompts AS kept
             WHERE kept.content = prompts.content
               AND kept.id NOT LIKE 'external-%'
               AND kept.engine = prompts.engine
           )",
        [],
    )
    .map_err(|e| CodexxError::Database(e.to_string()))?;
    conn.execute(
        "DELETE FROM prompts
         WHERE id LIKE 'external-%'
           AND EXISTS (
             SELECT 1 FROM prompts AS kept
             WHERE kept.content = prompts.content
               AND kept.id LIKE 'external-%'
               AND kept.engine = prompts.engine
               AND kept.rowid <> prompts.rowid
               AND (kept.updated_at > prompts.updated_at OR (kept.updated_at = prompts.updated_at AND kept.rowid > prompts.rowid))
           )",
        [],
    )
    .map_err(|e| CodexxError::Database(e.to_string()))?;
    Ok(conn)
}
