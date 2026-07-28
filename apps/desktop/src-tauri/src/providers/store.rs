use super::open_store as open_db;
use crate::error::{CodexxError, Result};
use crate::tools::{redacted_json_text, redacted_toml_text, ToolId};
use crate::{now_rfc3339, sanitize_id};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;
use toml_edit::DocumentMut;

fn default_provider_app_type() -> String {
    "codex".to_string()
}

fn default_provider_available() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedProvider {
    #[serde(default = "default_provider_app_type")]
    pub(crate) app_type: String,
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) native: bool,
    #[serde(default = "default_provider_available")]
    pub(crate) available: bool,
    #[serde(default)]
    pub(crate) status_message: Option<String>,
    #[serde(default)]
    pub(crate) models: Vec<String>,
    pub(crate) provider_name: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) api_key: Option<String>,
    pub(crate) toml_config: Option<String>,
    pub(crate) wire_api: String,
    pub(crate) requires_openai_auth: bool,
}

impl Serialize for SavedProvider {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let has_api_key = self
            .api_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let config_preview = self.toml_config.as_deref().map(|text| {
            if self.app_type == "claude" || text.trim_start().starts_with('{') {
                redacted_json_text(text)
            } else {
                redacted_toml_text(text)
            }
        });
        let mut state = serializer.serialize_struct("SavedProvider", 14)?;
        state.serialize_field("appType", &self.app_type)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("native", &self.native)?;
        state.serialize_field("available", &self.available)?;
        state.serialize_field("statusMessage", &self.status_message)?;
        state.serialize_field("models", &self.models)?;
        state.serialize_field("providerName", &self.provider_name)?;
        state.serialize_field("baseUrl", &self.base_url)?;
        state.serialize_field("model", &self.model)?;
        state.serialize_field("apiKey", &Option::<String>::None)?;
        state.serialize_field("hasApiKey", &has_api_key)?;
        state.serialize_field("tomlConfig", &config_preview)?;
        state.serialize_field("wireApi", &self.wire_api)?;
        state.serialize_field("requiresOpenaiAuth", &self.requires_openai_auth)?;
        state.end()
    }
}

#[derive(Debug, Clone)]
struct StoredProvider {
    provider: SavedProvider,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ProviderIdentity {
    Credential([u8; 32]),
    Unauthenticated { base_url: String, name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderUpsertMode {
    Manual,
    Imported,
    Detected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderUpsertKind {
    Added,
    Updated,
    Merged,
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderUpsertResult {
    pub(crate) provider: SavedProvider,
    pub(crate) kind: ProviderUpsertKind,
}

pub(crate) fn canonical_provider_base_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(parsed) = ureq::get(trimmed).request_url() {
        let url = parsed.as_url();
        if let Some(host) = url.host_str() {
            let mut canonical = format!("{}://", url.scheme().to_ascii_lowercase());
            if !url.username().is_empty() {
                canonical.push_str(url.username());
                if let Some(password) = url.password() {
                    canonical.push(':');
                    canonical.push_str(password);
                }
                canonical.push('@');
            }
            if host.contains(':') && !host.starts_with('[') {
                canonical.push('[');
                canonical.push_str(&host.to_ascii_lowercase());
                canonical.push(']');
            } else {
                canonical.push_str(&host.to_ascii_lowercase());
            }
            if let Some(port) = url.port() {
                canonical.push(':');
                canonical.push_str(&port.to_string());
            }
            let path = url.path().trim_end_matches('/');
            if !path.is_empty() {
                canonical.push_str(path);
            }
            if let Some(query) = url.query() {
                canonical.push('?');
                canonical.push_str(query);
            }
            return canonical;
        }
    }

    trimmed.trim_end_matches('/').to_string()
}

pub(crate) fn provider_identity(provider: &SavedProvider) -> Option<ProviderIdentity> {
    use sha2::{Digest, Sha256};

    let base_url = canonical_provider_base_url(&provider.base_url);
    if base_url.is_empty() {
        return None;
    }
    let explicit_api_key = provider
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let toml_api_key = provider.toml_config.as_deref().and_then(|text| {
        let doc = text.parse::<DocumentMut>().ok()?;
        let provider_id = doc
            .get("model_provider")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        experimental_bearer_token_from_doc(&doc, provider_id)
    });
    if let Some(api_key) = explicit_api_key.or(toml_api_key.as_deref()) {
        // Hash the complete endpoint/credential tuple so neither the key nor a
        // reusable key-only fingerprint is persisted, logged, or sent to the UI.
        let mut hasher = Sha256::new();
        // Keep this namespace stable so the product rename does not change provider identities.
        hasher.update(b"codex-x/provider-identity/v1\0");
        hasher.update(base_url.as_bytes());
        hasher.update(b"\0");
        hasher.update(api_key.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        return Some(ProviderIdentity::Credential(digest));
    }

    let name = provider
        .provider_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (!name.is_empty()).then_some(ProviderIdentity::Unauthenticated { base_url, name })
}

fn saved_provider_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedProvider> {
    Ok(SavedProvider {
        app_type: row.get(0)?,
        id: row.get(1)?,
        native: false,
        available: true,
        status_message: None,
        models: Vec::new(),
        provider_name: row.get(2)?,
        base_url: row.get(3)?,
        model: row.get(4)?,
        api_key: row.get(5)?,
        toml_config: row.get(6)?,
        wire_api: row.get(7)?,
        requires_openai_auth: row.get::<_, i64>(8)? != 0,
    })
}

fn stored_providers_on_connection(
    conn: &Connection,
    app_type: Option<&str>,
) -> Result<Vec<StoredProvider>> {
    let query = if app_type.is_some() {
        "SELECT app_type, id, provider_name, base_url, model, api_key, toml_config, wire_api,
                requires_openai_auth, created_at, updated_at
         FROM providers
         WHERE app_type = ?1
         ORDER BY created_at ASC, updated_at ASC, id ASC"
    } else {
        "SELECT app_type, id, provider_name, base_url, model, api_key, toml_config, wire_api,
                requires_openai_auth, created_at, updated_at
         FROM providers
         ORDER BY app_type ASC, created_at ASC, updated_at ASC, id ASC"
    };
    let mut stmt = conn
        .prepare(query)
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    let collect = |row: &rusqlite::Row<'_>| {
        Ok(StoredProvider {
            provider: saved_provider_from_row(row)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    };
    let rows = if let Some(app_type) = app_type {
        stmt.query_map([app_type], collect)
    } else {
        stmt.query_map([], collect)
    }
    .map_err(|e| CodexxError::Database(e.to_string()))?;

    let mut providers = Vec::new();
    for row in rows {
        providers.push(row.map_err(|e| CodexxError::Database(e.to_string()))?);
    }
    Ok(providers)
}

pub(crate) fn list_saved_providers_on_connection(conn: &Connection) -> Result<Vec<SavedProvider>> {
    list_saved_providers_on_connection_for_app(conn, "codex")
}

pub(crate) fn list_saved_providers_on_connection_for_app(
    conn: &Connection,
    app_type: &str,
) -> Result<Vec<SavedProvider>> {
    Ok(stored_providers_on_connection(conn, Some(app_type))?
        .into_iter()
        .map(|stored| stored.provider)
        .collect())
}

pub(crate) fn list_saved_providers_inner() -> Result<Vec<SavedProvider>> {
    list_saved_providers_for_app_inner("codex")
}

pub(crate) fn list_saved_providers_for_app_inner(app_type: &str) -> Result<Vec<SavedProvider>> {
    let conn = open_db()?;
    list_saved_providers_on_connection_for_app(&conn, app_type)
}

pub(crate) fn provider_by_id_on_connection(
    conn: &Connection,
    id: &str,
) -> Result<Option<SavedProvider>> {
    provider_by_id_on_connection_for_app(conn, "codex", id)
}

pub(crate) fn provider_by_id_on_connection_for_app(
    conn: &Connection,
    app_type: &str,
    id: &str,
) -> Result<Option<SavedProvider>> {
    let mut stmt = conn
        .prepare(
            "SELECT app_type, id, provider_name, base_url, model, api_key, toml_config, wire_api,
                    requires_openai_auth
             FROM providers WHERE app_type = ?1 AND id = ?2 LIMIT 1",
        )
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    stmt.query_row(params![app_type, id], saved_provider_from_row)
        .map(Some)
        .or_else(|error| {
            if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
                Ok(None)
            } else {
                Err(error)
            }
        })
        .map_err(|e| CodexxError::Database(e.to_string()))
}

fn write_provider_on_connection(conn: &Connection, provider: &SavedProvider) -> Result<()> {
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO providers
            (app_type, id, provider_name, base_url, model, api_key, toml_config, wire_api, requires_openai_auth, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
         ON CONFLICT(app_type, id) DO UPDATE SET
            provider_name = excluded.provider_name,
            base_url = excluded.base_url,
            model = excluded.model,
            api_key = excluded.api_key,
            toml_config = excluded.toml_config,
            wire_api = excluded.wire_api,
            requires_openai_auth = excluded.requires_openai_auth,
            updated_at = excluded.updated_at",
        params![
            provider.app_type,
            provider.id,
            provider.provider_name,
            provider.base_url,
            provider.model,
            provider.api_key,
            provider.toml_config,
            provider.wire_api,
            if provider.requires_openai_auth { 1 } else { 0 },
            now,
        ],
    )
    .map_err(|e| CodexxError::Database(e.to_string()))?;
    Ok(())
}

fn unique_provider_id_on_connection(
    conn: &Connection,
    app_type: &str,
    preferred: &str,
) -> Result<String> {
    if provider_by_id_on_connection_for_app(conn, app_type, preferred)?.is_none() {
        return Ok(preferred.to_string());
    }
    let mut index = 2usize;
    loop {
        let candidate = format!("{preferred}-{index}");
        if provider_by_id_on_connection_for_app(conn, app_type, &candidate)?.is_none() {
            return Ok(candidate);
        }
        index += 1;
    }
}

fn preserve_existing_provider_config(
    mut incoming: SavedProvider,
    existing: &SavedProvider,
) -> SavedProvider {
    incoming.id = existing.id.clone();
    if !existing.provider_name.trim().is_empty() {
        incoming.provider_name = existing.provider_name.clone();
    }
    if !existing.base_url.trim().is_empty() {
        incoming.base_url = existing.base_url.clone();
    }
    if !existing.model.trim().is_empty() {
        incoming.model = existing.model.clone();
    }
    if existing
        .api_key
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        incoming.api_key = existing.api_key.clone();
    }
    if existing
        .toml_config
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        incoming.toml_config = existing.toml_config.clone();
    }
    if !existing.wire_api.trim().is_empty() {
        incoming.wire_api = existing.wire_api.clone();
    }
    incoming.requires_openai_auth = existing.requires_openai_auth;
    incoming
}

pub(crate) fn upsert_provider_on_connection(
    conn: &Connection,
    mut provider: SavedProvider,
    mode: ProviderUpsertMode,
) -> Result<ProviderUpsertResult> {
    let requested_id = provider.id.clone();
    let app_type = provider.app_type.clone();
    let identity = provider_identity(&provider);
    let stored = stored_providers_on_connection(conn, Some(&app_type))?;
    let identity_match = identity.as_ref().and_then(|identity| {
        stored
            .iter()
            .find(|candidate| provider_identity(&candidate.provider).as_ref() == Some(identity))
    });
    let exact_id_match = stored
        .iter()
        .find(|candidate| candidate.provider.id == requested_id);

    let target = match mode {
        ProviderUpsertMode::Manual => exact_id_match.or(identity_match),
        ProviderUpsertMode::Imported | ProviderUpsertMode::Detected => identity_match,
    };
    let kind = if let Some(target) = target {
        let existing = &target.provider;
        let same_id = existing.id == requested_id;
        provider.id = existing.id.clone();
        if mode != ProviderUpsertMode::Manual {
            provider = preserve_existing_provider_config(provider, existing);
        }
        if same_id {
            ProviderUpsertKind::Updated
        } else {
            ProviderUpsertKind::Merged
        }
    } else {
        if exact_id_match.is_some() {
            provider.id = unique_provider_id_on_connection(conn, &app_type, &provider.id)?;
        }
        ProviderUpsertKind::Added
    };

    write_provider_on_connection(conn, &provider)?;
    let provider = provider_by_id_on_connection_for_app(conn, &app_type, &provider.id)?
        .ok_or_else(|| CodexxError::Database("provider saved but not found".to_string()))?;
    Ok(ProviderUpsertResult { provider, kind })
}

pub(crate) fn merge_duplicate_provider_identities(conn: &mut Connection) -> Result<usize> {
    let rows = stored_providers_on_connection(conn, None)?;
    let mut groups: HashMap<(String, ProviderIdentity), Vec<StoredProvider>> = HashMap::new();
    for row in rows {
        if let Some(identity @ ProviderIdentity::Credential(_)) = provider_identity(&row.provider) {
            groups
                .entry((row.provider.app_type.clone(), identity))
                .or_default()
                .push(row);
        }
    }
    let duplicate_groups = groups
        .into_values()
        .filter(|group| group.len() > 1)
        .collect::<Vec<_>>();
    if duplicate_groups.is_empty() {
        return Ok(0);
    }

    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    let mut merged = 0usize;
    for mut group in duplicate_groups {
        group.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.provider.id.cmp(&right.provider.id))
        });
        let mut survivor = group[0].clone();
        for duplicate in group.iter().skip(1) {
            if survivor.provider.provider_name.trim().is_empty()
                && !duplicate.provider.provider_name.trim().is_empty()
            {
                survivor.provider.provider_name = duplicate.provider.provider_name.clone();
            }
            if survivor.provider.model.trim().is_empty()
                && !duplicate.provider.model.trim().is_empty()
            {
                survivor.provider.model = duplicate.provider.model.clone();
            }
            if survivor
                .provider
                .toml_config
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
                && duplicate
                    .provider
                    .toml_config
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
            {
                survivor.provider.toml_config = duplicate.provider.toml_config.clone();
            }
            if duplicate.updated_at > survivor.updated_at {
                survivor.updated_at = duplicate.updated_at.clone();
            }
        }
        survivor.provider.base_url = canonical_provider_base_url(&survivor.provider.base_url);
        survivor.provider.api_key = survivor
            .provider
            .api_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        transaction
            .execute(
                "UPDATE providers SET provider_name = ?3, base_url = ?4, model = ?5,
                        api_key = ?6, toml_config = ?7, wire_api = ?8,
                        requires_openai_auth = ?9, updated_at = ?10
                 WHERE app_type = ?1 AND id = ?2",
                params![
                    survivor.provider.app_type,
                    survivor.provider.id,
                    survivor.provider.provider_name,
                    survivor.provider.base_url,
                    survivor.provider.model,
                    survivor.provider.api_key,
                    survivor.provider.toml_config,
                    survivor.provider.wire_api,
                    if survivor.provider.requires_openai_auth {
                        1
                    } else {
                        0
                    },
                    survivor.updated_at,
                ],
            )
            .map_err(|e| CodexxError::Database(e.to_string()))?;
        for duplicate in group.iter().skip(1) {
            transaction
                .execute(
                    "DELETE FROM providers WHERE app_type = ?1 AND id = ?2",
                    params![duplicate.provider.app_type, duplicate.provider.id],
                )
                .map_err(|e| CodexxError::Database(e.to_string()))?;
            merged += 1;
        }
    }
    transaction
        .commit()
        .map_err(|e| CodexxError::Database(e.to_string()))?;
    Ok(merged)
}

pub(crate) fn normalize_saved_provider(provider: SavedProvider) -> Result<SavedProvider> {
    let raw_id = provider.id.trim();
    if raw_id.is_empty() {
        return Err(CodexxError::Config("provider id 不能为空".to_string()));
    }
    let app_type = ToolId::parse(&provider.app_type)?.as_str().to_string();
    let normalized = SavedProvider {
        app_type,
        id: custom_provider_id(raw_id),
        native: false,
        available: true,
        status_message: None,
        models: Vec::new(),
        provider_name: provider.provider_name.trim().to_string(),
        base_url: canonical_provider_base_url(&provider.base_url),
        model: provider.model.trim().to_string(),
        api_key: provider
            .api_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        toml_config: provider
            .toml_config
            .map(|value| value.trim_end().to_string())
            .filter(|value| !value.trim().is_empty()),
        wire_api: if provider.wire_api.trim().is_empty() {
            "responses".to_string()
        } else {
            provider.wire_api.trim().to_string()
        },
        requires_openai_auth: provider.requires_openai_auth,
    };
    if normalized.provider_name.is_empty() {
        return Err(CodexxError::Config("供应商名称不能为空".to_string()));
    }
    if normalized.base_url.is_empty() {
        return Err(CodexxError::Config("base_url 不能为空".to_string()));
    }
    if normalized.model.is_empty() {
        return Err(CodexxError::Config("model 不能为空".to_string()));
    }
    Ok(normalized)
}

pub(crate) fn save_manual_provider_on_connection(
    conn: &Connection,
    provider: SavedProvider,
) -> Result<SavedProvider> {
    let requested_id = provider.id.trim().to_string();
    let mut provider = normalize_saved_provider(provider)?;
    if requested_id != provider.id
        && provider_by_id_on_connection_for_app(conn, &provider.app_type, &provider.id)?.is_some()
    {
        return Err(CodexxError::Config(format!(
            "供应商 ID {} 规范化后与现有供应商冲突，请更换名称或 ID",
            requested_id
        )));
    }
    if let Some(existing) =
        provider_by_id_on_connection_for_app(conn, &provider.app_type, &provider.id)?
    {
        if provider
            .api_key
            .as_deref()
            .is_none_or(|value| value.trim().is_empty() || value == crate::tools::REDACTED_VALUE)
        {
            provider.api_key = existing.api_key;
        }
        if provider
            .toml_config
            .as_deref()
            .is_some_and(|value| value.contains(crate::tools::REDACTED_VALUE))
        {
            provider.toml_config = existing.toml_config;
        }
    }
    Ok(upsert_provider_on_connection(conn, provider, ProviderUpsertMode::Manual)?.provider)
}

pub(crate) fn save_provider_inner(provider: SavedProvider) -> Result<SavedProvider> {
    let conn = open_db()?;
    save_manual_provider_on_connection(&conn, provider)
}

pub(crate) fn save_detected_provider_inner(provider: SavedProvider) -> Result<SavedProvider> {
    let provider = normalize_saved_provider(provider)?;
    let conn = open_db()?;
    Ok(upsert_provider_on_connection(&conn, provider, ProviderUpsertMode::Detected)?.provider)
}

pub(crate) fn delete_provider_inner(id: &str) -> Result<()> {
    delete_provider_for_app_inner("codex", id)
}

pub(crate) fn delete_provider_for_app_inner(app_type: &str, id: &str) -> Result<()> {
    let conn = open_db()?;
    conn.execute(
        "DELETE FROM providers WHERE app_type = ?1 AND id = ?2",
        params![app_type, id],
    )
    .map_err(|e| CodexxError::Database(e.to_string()))?;
    Ok(())
}

pub(crate) fn reserved_codex_provider_id(id: &str) -> bool {
    matches!(
        id.trim().to_ascii_lowercase().as_str(),
        "openai" | "amazon-bedrock" | "ollama" | "lmstudio" | "oss"
    )
}

pub(crate) fn custom_provider_id(input: &str) -> String {
    let id = sanitize_id(input);
    if reserved_codex_provider_id(&id) {
        format!("{id}-custom")
    } else {
        id
    }
}

pub(crate) fn experimental_bearer_token_from_doc(
    doc: &DocumentMut,
    provider_id: Option<&str>,
) -> Option<String> {
    let token_from_table = provider_id.and_then(|id| {
        doc.get("model_providers")
            .and_then(|item| item.as_table())
            .and_then(|providers| providers.get(id))
            .and_then(|item| item.as_table())
            .and_then(|table| table.get("experimental_bearer_token"))
            .and_then(|item| item.as_str())
    });

    token_from_table
        .or_else(|| {
            doc.get("experimental_bearer_token")
                .and_then(|item| item.as_str())
        })
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}
