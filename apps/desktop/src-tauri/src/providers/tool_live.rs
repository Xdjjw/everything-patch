use super::{
    activate_zcode_provider_inner, provider_by_id_on_connection_for_app,
    save_provider_toml_config_inner, switch_provider_inner, ProviderInput, ProviderTomlInput,
    SavedProvider,
};
use crate::error::{CodexxError, Result};
use crate::file_io::{
    ensure_directory, parse_toml_document, read_to_string_if_exists, write_json, write_text,
};
use crate::paths::app_home;
use crate::toml_utils::ensure_table;
use crate::tools::{redacted_json_text, redacted_toml_text, ToolId};
use crate::{now_rfc3339, open_db};
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{value, Item, Table};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolProviderActionResult {
    ok: bool,
    message: String,
    app_type: String,
    provider_id: String,
    backup_path: Option<String>,
}

fn provider_backup(path: &Path, tool: ToolId, provider_id: &str) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let safe_time = now_rfc3339()
        .replace(':', "-")
        .replace('+', "_")
        .replace(' ', "_");
    let directory = app_home()?
        .join("backups")
        .join("providers")
        .join(tool.as_str())
        .join(format!("{safe_time}-{provider_id}"));
    ensure_directory(&directory)?;
    let file_name = path
        .file_name()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "config".into());
    let destination = directory.join(file_name);
    fs::copy(path, &destination).map_err(|error| crate::file_io::io_err(&destination, error))?;
    Ok(Some(directory.display().to_string()))
}

fn json_object_from_file(path: &Path) -> Result<Map<String, Value>> {
    let text = read_to_string_if_exists(path)?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    let value = serde_json::from_str::<Value>(&text)
        .map_err(|error| crate::file_io::json_err(path, error))?;
    value.as_object().cloned().ok_or_else(|| {
        CodexxError::Config(format!("配置文件必须是 JSON object: {}", path.display()))
    })
}

fn merge_json_value(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, source_value) in source {
                if let Some(target_value) = target.get_mut(key) {
                    merge_json_value(target_value, source_value);
                } else {
                    target.insert(key.clone(), source_value.clone());
                }
            }
        }
        (target, source) => *target = source.clone(),
    }
}

fn merge_json_object(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    for (key, source_value) in source {
        if let Some(target_value) = target.get_mut(key) {
            merge_json_value(target_value, source_value);
        } else {
            target.insert(key.clone(), source_value.clone());
        }
    }
}

fn claude_provider_template(provider: &SavedProvider) -> Result<Option<Map<String, Value>>> {
    let Some(text) = provider
        .toml_config
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    else {
        return Ok(None);
    };
    let mut template = serde_json::from_str::<Value>(text)
        .map_err(|error| CodexxError::Config(format!("Claude 供应商模板不是合法 JSON: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| CodexxError::Config("Claude 供应商模板必须是 JSON object".to_string()))?;
    for private_key in [
        "api_format",
        "apiFormat",
        "openrouter_compat_mode",
        "openrouterCompatMode",
        "modelCatalog",
    ] {
        template.remove(private_key);
    }
    Ok(Some(template))
}

fn activate_claude_provider(provider: &SavedProvider) -> Result<ToolProviderActionResult> {
    let config = ToolId::Claude.config_path(None)?;
    let backup_path = provider_backup(&config, ToolId::Claude, &provider.id)?;
    let mut root = json_object_from_file(&config)?;
    if let Some(template) = claude_provider_template(provider)? {
        merge_json_object(&mut root, &template);
    }
    for private_key in [
        "api_format",
        "apiFormat",
        "openrouter_compat_mode",
        "openrouterCompatMode",
        "modelCatalog",
    ] {
        root.remove(private_key);
    }
    let env = root
        .entry("env".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !env.is_object() {
        *env = Value::Object(Map::new());
    }
    let env = env
        .as_object_mut()
        .expect("env was replaced with an object");
    env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        Value::String(provider.base_url.clone()),
    );
    env.insert(
        "ANTHROPIC_MODEL".to_string(),
        Value::String(provider.model.clone()),
    );
    if let Some(api_key) = provider
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            Value::String(api_key.to_string()),
        );
    }
    write_json(&config, &Value::Object(root))?;
    Ok(ToolProviderActionResult {
        ok: true,
        message: format!(
            "已切换 Claude Code 到 {} / {}",
            provider.provider_name, provider.model
        ),
        app_type: provider.app_type.clone(),
        provider_id: provider.id.clone(),
        backup_path,
    })
}

fn merge_toml_table(target: &mut Table, source: &Table) {
    for (key, source_item) in source.iter() {
        let merged_table = match (
            target.get_mut(key).and_then(Item::as_table_mut),
            source_item.as_table(),
        ) {
            (Some(target_table), Some(source_table)) => {
                merge_toml_table(target_table, source_table);
                true
            }
            _ => false,
        };
        if !merged_table {
            target.insert(key, source_item.clone());
        }
    }
}

fn grok_model_table<'a>(root: &'a mut Table, model_id: &str) -> Result<&'a mut Table> {
    let models = ensure_table(root, "model")?;
    if !models.contains_key(model_id) {
        models.insert(model_id, Item::Table(Table::new()));
    }
    models
        .get_mut(model_id)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| CodexxError::Config(format!("Grok model.{model_id} 不是合法 TOML table")))
}

fn activate_grok_provider(provider: &SavedProvider) -> Result<ToolProviderActionResult> {
    let config = ToolId::Grok.config_path(None)?;
    let backup_path = provider_backup(&config, ToolId::Grok, &provider.id)?;
    if let Some(parent) = config.parent() {
        ensure_directory(parent)?;
    }
    let text = read_to_string_if_exists(&config)?;
    let mut document = parse_toml_document(&config, &text)?;
    let mut template_has_upstream_model = false;
    if let Some(template_text) = provider
        .toml_config
        .as_deref()
        .filter(|template| !template.trim().is_empty())
    {
        let template = template_text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|error| {
                CodexxError::Config(format!("Grok Build 供应商模板不是合法 TOML: {error}"))
            })?;
        template_has_upstream_model = template
            .get("model")
            .and_then(Item::as_table)
            .and_then(|models| models.get(&provider.model))
            .and_then(Item::as_table)
            .is_some_and(|model| model.get("model").is_some());
        merge_toml_table(document.as_table_mut(), template.as_table());
    }
    ensure_table(document.as_table_mut(), "models")?["default"] = value(&provider.model);
    let model_table = grok_model_table(document.as_table_mut(), &provider.model)?;
    if !template_has_upstream_model {
        model_table["model"] = value(&provider.model);
    }
    model_table["base_url"] = value(&provider.base_url);
    model_table["name"] = value(&provider.provider_name);
    model_table["api_backend"] = value(if provider.wire_api.trim().is_empty() {
        "responses"
    } else {
        provider.wire_api.as_str()
    });
    if model_table.get("context_window").is_none() {
        model_table["context_window"] = value(500_000);
    }
    if let Some(api_key) = provider
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        model_table["api_key"] = value(api_key);
    }
    write_text(
        &config,
        &(document.to_string().trim_end().to_string() + "\n"),
    )?;
    Ok(ToolProviderActionResult {
        ok: true,
        message: format!(
            "已切换 Grok Build 到 {} / {}",
            provider.provider_name, provider.model
        ),
        app_type: provider.app_type.clone(),
        provider_id: provider.id.clone(),
        backup_path,
    })
}

fn activate_codex_provider(
    provider: &SavedProvider,
    config_dir: Option<String>,
) -> Result<ToolProviderActionResult> {
    let result = if let Some(config_text) = provider
        .toml_config
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        save_provider_toml_config_inner(ProviderTomlInput {
            config_dir,
            config_text: config_text.to_string(),
            api_key: provider.api_key.clone(),
        })?
    } else {
        switch_provider_inner(ProviderInput {
            config_dir,
            _provider_id: Some(provider.id.clone()),
            provider_name: provider.provider_name.clone(),
            base_url: provider.base_url.clone(),
            model: provider.model.clone(),
            api_key: provider.api_key.clone(),
            wire_api: Some(provider.wire_api.clone()),
            requires_openai_auth: Some(provider.requires_openai_auth),
        })?
    };
    Ok(ToolProviderActionResult {
        ok: result.ok,
        message: result.message,
        app_type: provider.app_type.clone(),
        provider_id: provider.id.clone(),
        backup_path: result.backup_id,
    })
}

pub(crate) fn activate_saved_provider_inner(
    tool: ToolId,
    id: &str,
    model: Option<String>,
    config_dir: Option<String>,
) -> Result<ToolProviderActionResult> {
    if tool == ToolId::Kilo {
        return Err(CodexxError::Config(
            "Kilo 供应商请使用 Kilo 的 /connect 或原生设置界面管理".to_string(),
        ));
    }
    if tool == ToolId::Zcode {
        let result = activate_zcode_provider_inner(id, model.as_deref())?;
        return Ok(ToolProviderActionResult {
            ok: true,
            message: format!(
                "已切换 ZCode 到 {} / {}。重新打开 ZCode 后生效",
                result.provider_name, result.model
            ),
            app_type: tool.as_str().to_string(),
            provider_id: id.to_string(),
            backup_path: Some(result.backup_path),
        });
    }
    let connection = open_db()?;
    let provider = provider_by_id_on_connection_for_app(&connection, tool.as_str(), id)?
        .ok_or_else(|| CodexxError::Config(format!("未找到 {} 供应商: {id}", tool.label())))?;
    match tool {
        ToolId::Codex => activate_codex_provider(&provider, config_dir),
        ToolId::Claude => activate_claude_provider(&provider),
        ToolId::Grok => activate_grok_provider(&provider),
        ToolId::Zcode => unreachable!("ZCode is handled before the local provider lookup"),
        ToolId::Kilo => unreachable!("Kilo is rejected before provider lookup"),
    }
}

pub(crate) fn provider_template_text(provider: &SavedProvider) -> Result<String> {
    let tool = ToolId::parse(&provider.app_type)?;
    if let Some(template) = provider
        .toml_config
        .as_deref()
        .filter(|template| !template.trim().is_empty())
    {
        return Ok(match tool {
            ToolId::Claude => redacted_json_text(template),
            ToolId::Codex | ToolId::Grok => redacted_toml_text(template),
            ToolId::Zcode | ToolId::Kilo => String::new(),
        });
    }
    match tool {
        ToolId::Codex => Ok(String::new()),
        ToolId::Claude => {
            let mut env = Map::new();
            env.insert(
                "ANTHROPIC_BASE_URL".to_string(),
                Value::String(provider.base_url.clone()),
            );
            env.insert(
                "ANTHROPIC_MODEL".to_string(),
                Value::String(provider.model.clone()),
            );
            if provider.api_key.is_some() {
                env.insert(
                    "ANTHROPIC_AUTH_TOKEN".to_string(),
                    Value::String(crate::tools::REDACTED_VALUE.to_string()),
                );
            }
            serde_json::to_string_pretty(&serde_json::json!({ "env": env }))
                .map_err(|error| CodexxError::Config(error.to_string()))
        }
        ToolId::Grok => {
            let api_key = provider
                .api_key
                .as_ref()
                .map(|_| format!("api_key = \"{}\"\n", crate::tools::REDACTED_VALUE))
                .unwrap_or_default();
            Ok(format!(
                "[models]\ndefault = {model:?}\n\n[model.{model:?}]\nmodel = {model:?}\nbase_url = {base_url:?}\nname = {name:?}\n{api_key}api_backend = {wire_api:?}\ncontext_window = 500000\n",
                model = provider.model,
                base_url = provider.base_url,
                name = provider.provider_name,
                wire_api = provider.wire_api,
            ))
        }
        ToolId::Zcode | ToolId::Kilo => Ok(String::new()),
    }
}

#[allow(dead_code)]
fn _config_parent(path: &Path) -> PathBuf {
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_template_merge_preserves_unrelated_env_fields() {
        let mut live = json!({
            "env": {
                "KEEP_FROM_LIVE": "yes",
                "ANTHROPIC_MODEL": "old"
            },
            "theme": "dark"
        });
        let template = json!({
            "env": {
                "KEEP_FROM_TEMPLATE": "yes",
                "ANTHROPIC_MODEL": "new"
            },
            "permissions": { "allow": ["Read"] }
        });
        merge_json_value(&mut live, &template);
        assert_eq!(live.pointer("/env/KEEP_FROM_LIVE"), Some(&json!("yes")));
        assert_eq!(live.pointer("/env/KEEP_FROM_TEMPLATE"), Some(&json!("yes")));
        assert_eq!(live.pointer("/env/ANTHROPIC_MODEL"), Some(&json!("new")));
        assert_eq!(live.get("theme"), Some(&json!("dark")));
    }

    #[test]
    fn grok_template_merge_preserves_extended_model_fields() {
        let mut live = "theme = \"dark\"\n"
            .parse::<toml_edit::DocumentMut>()
            .expect("live TOML");
        let template = r#"
[models]
default = "alias"

[model.alias]
model = "upstream-model"
base_url = "https://example.com"
env_key = "EXAMPLE_API_KEY"
context_window = 131072
"#
        .parse::<toml_edit::DocumentMut>()
        .expect("template TOML");
        merge_toml_table(live.as_table_mut(), template.as_table());
        let model = live["model"]["alias"]
            .as_table()
            .expect("merged model table");
        assert_eq!(
            model.get("model").and_then(Item::as_str),
            Some("upstream-model")
        );
        assert_eq!(
            model.get("env_key").and_then(Item::as_str),
            Some("EXAMPLE_API_KEY")
        );
        assert_eq!(
            model.get("context_window").and_then(Item::as_integer),
            Some(131072)
        );
        assert_eq!(live.get("theme").and_then(Item::as_str), Some("dark"));
    }
}
