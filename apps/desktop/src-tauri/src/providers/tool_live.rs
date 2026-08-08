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
use jsonc_parser::cst::{CstInputValue, CstRootNode};
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

#[derive(Debug)]
struct OptionalFileSnapshot {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

fn capture_optional_file(path: PathBuf) -> Result<OptionalFileSnapshot> {
    let bytes = match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(CodexxError::Config(format!(
                "Pi Provider 目标不是普通文件: {}",
                path.display()
            )));
        }
        Ok(_) => Some(fs::read(&path).map_err(|error| crate::file_io::io_err(&path, error))?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(crate::file_io::io_err(&path, error)),
    };
    Ok(OptionalFileSnapshot { path, bytes })
}

fn restore_optional_files(snapshots: &[OptionalFileSnapshot]) -> Result<()> {
    let mut failures = Vec::new();
    for snapshot in snapshots.iter().rev() {
        let result = match &snapshot.bytes {
            Some(bytes) => crate::file_io::atomic_write(&snapshot.path, bytes),
            None => match fs::symlink_metadata(&snapshot.path) {
                Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
                    fs::remove_file(&snapshot.path)
                        .map_err(|error| crate::file_io::io_err(&snapshot.path, error))
                }
                Ok(_) => Err(CodexxError::Config(format!(
                    "Pi Provider 回滚目标被目录占用: {}",
                    snapshot.path.display()
                ))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(crate::file_io::io_err(&snapshot.path, error)),
            },
        };
        if let Err(error) = result {
            failures.push(error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(CodexxError::Config(format!(
            "Pi Provider 回滚不完整: {}",
            failures.join("；")
        )))
    }
}

fn pi_api_type(wire_api: &str) -> &'static str {
    let normalized = wire_api.trim().to_ascii_lowercase();
    if normalized.contains("anthropic") {
        "anthropic-messages"
    } else if normalized.contains("google") || normalized.contains("gemini") {
        "google-generative-ai"
    } else if normalized.contains("response") {
        "openai-responses"
    } else {
        "openai-completions"
    }
}

fn json_to_cst_input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(value) => CstInputValue::Bool(*value),
        Value::Number(value) => CstInputValue::Number(value.to_string()),
        Value::String(value) => CstInputValue::String(value.clone()),
        Value::Array(values) => {
            CstInputValue::Array(values.iter().map(json_to_cst_input).collect())
        }
        Value::Object(values) => CstInputValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), json_to_cst_input(value)))
                .collect(),
        ),
    }
}

fn update_pi_models_jsonc(text: &str, provider: &SavedProvider) -> Result<String> {
    let source = if text.trim().is_empty() { "{}\n" } else { text };
    let root = CstRootNode::parse(source, &Default::default())
        .map_err(|error| CodexxError::Config(format!("Pi models.json 解析失败: {error}")))?;
    let root_object = root
        .object_value()
        .ok_or_else(|| CodexxError::Config("Pi models.json 根节点必须是 object".to_string()))?;
    let providers = match root_object.get("providers") {
        Some(property) => property.object_value().ok_or_else(|| {
            CodexxError::Config("Pi models.json 的 providers 字段必须是 object".to_string())
        })?,
        None => root_object
            .append("providers", CstInputValue::Object(Vec::new()))
            .object_value()
            .expect("new providers value is an object"),
    };
    let definition = match providers.get(&provider.id) {
        Some(property) => property.object_value().ok_or_else(|| {
            CodexxError::Config(format!(
                "Pi models.json 的 Provider {} 必须是 object",
                provider.id
            ))
        })?,
        None => providers
            .append(&provider.id, CstInputValue::Object(Vec::new()))
            .object_value()
            .expect("new provider value is an object"),
    };

    let values = [
        (
            "baseUrl",
            Value::String(provider.base_url.trim_end_matches('/').to_string()),
        ),
        (
            "api",
            Value::String(pi_api_type(&provider.wire_api).to_string()),
        ),
        (
            "models",
            serde_json::json!([{
                "id": provider.model,
                "name": provider.model,
            }]),
        ),
    ];
    for (key, value) in values {
        let input = json_to_cst_input(&value);
        if let Some(property) = definition.get(key) {
            property.set_value(input);
        } else {
            definition.append(key, input);
        }
    }
    if let Some(api_key) = provider
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let input = CstInputValue::String(api_key.to_string());
        if let Some(property) = definition.get("apiKey") {
            property.set_value(input);
        } else {
            definition.append("apiKey", input);
        }
    }

    let mut output = root.to_string();
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

fn pi_provider_backup(
    settings_path: &Path,
    models_path: &Path,
    provider_id: &str,
) -> Result<Option<String>> {
    if !settings_path.is_file() && !models_path.is_file() {
        return Ok(None);
    }
    let safe_time = now_rfc3339()
        .replace(':', "-")
        .replace('+', "_")
        .replace(' ', "_");
    let directory = app_home()?
        .join("backups")
        .join("providers")
        .join(ToolId::Pi.as_str())
        .join(format!("{safe_time}-{provider_id}"));
    ensure_directory(&directory)?;
    for path in [settings_path, models_path] {
        if path.is_file() {
            let destination = directory.join(
                path.file_name()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| "config.json".into()),
            );
            fs::copy(path, &destination)
                .map_err(|error| crate::file_io::io_err(&destination, error))?;
        }
    }
    Ok(Some(directory.display().to_string()))
}

fn activate_pi_provider_at(
    provider: &SavedProvider,
    pi_dir: &Path,
) -> Result<ToolProviderActionResult> {
    if provider.base_url.trim().is_empty() || provider.model.trim().is_empty() {
        return Err(CodexxError::Config(
            "Pi Provider 需要 base URL 与模型 ID".to_string(),
        ));
    }
    ensure_directory(pi_dir)?;
    let settings_path = pi_dir.join("settings.json");
    let models_path = pi_dir.join("models.json");
    let snapshots = [
        capture_optional_file(settings_path.clone())?,
        capture_optional_file(models_path.clone())?,
    ];
    let backup_path = pi_provider_backup(&settings_path, &models_path, &provider.id)?;

    let result = (|| {
        let models_text = read_to_string_if_exists(&models_path)?;
        write_text(
            &models_path,
            &update_pi_models_jsonc(&models_text, provider)?,
        )?;

        let mut settings = json_object_from_file(&settings_path)?;
        settings.insert(
            "defaultProvider".to_string(),
            Value::String(provider.id.clone()),
        );
        settings.insert(
            "defaultModel".to_string(),
            Value::String(provider.model.clone()),
        );
        write_json(&settings_path, &Value::Object(settings))
    })();

    if let Err(error) = result {
        return match restore_optional_files(&snapshots) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CodexxError::Config(format!("{error}；{rollback_error}"))),
        };
    }
    Ok(ToolProviderActionResult {
        ok: true,
        message: format!(
            "已切换 Pi 到 {} / {}，可在 Pi 中执行 /model 立即查看",
            provider.provider_name, provider.model
        ),
        app_type: provider.app_type.clone(),
        provider_id: provider.id.clone(),
        backup_path,
    })
}

fn activate_pi_provider(provider: &SavedProvider) -> Result<ToolProviderActionResult> {
    activate_pi_provider_at(provider, &ToolId::Pi.home_dir(None)?)
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
        ToolId::Pi => activate_pi_provider(&provider),
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
            ToolId::Pi => redacted_json_text(template),
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
        ToolId::Pi => {
            let mut definition = Map::new();
            definition.insert(
                "baseUrl".to_string(),
                Value::String(provider.base_url.clone()),
            );
            definition.insert(
                "api".to_string(),
                Value::String(pi_api_type(&provider.wire_api).to_string()),
            );
            if provider.api_key.is_some() {
                definition.insert(
                    "apiKey".to_string(),
                    Value::String(crate::tools::REDACTED_VALUE.to_string()),
                );
            }
            definition.insert(
                "models".to_string(),
                Value::Array(vec![serde_json::json!({
                    "id": provider.model,
                    "name": provider.model,
                })]),
            );
            serde_json::to_string_pretty(&serde_json::json!({
                "providers": { provider.id.clone(): definition },
            }))
            .map_err(|error| CodexxError::Config(error.to_string()))
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

    #[test]
    fn pi_provider_write_preserves_unrelated_settings_and_models() {
        let root = std::env::temp_dir().join(format!(
            "devconduit-pi-provider-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).expect("create Pi provider fixture");
        fs::write(
            root.join("settings.json"),
            r#"{"theme":"dark","defaultProvider":"old","defaultModel":"old-model"}"#,
        )
        .expect("write Pi settings fixture");
        fs::write(
            root.join("models.json"),
            r#"{
  // Keep this provider and comment.
  "providers": {
    "existing": {"baseUrl":"http://localhost","api":"openai-completions","apiKey":"local","models":[{"id":"existing"}]},
    "devconduit-test": {
      "baseUrl":"https://old.example/v1",
      "api":"openai-completions",
      "apiKey":"keep-existing",
      "customField":{"keep":true},
      "models":[{"id":"old-model"}],
    },
  },
}"#,
        )
        .expect("write Pi models fixture");
        let provider = SavedProvider {
            app_type: "pi".to_string(),
            id: "devconduit-test".to_string(),
            native: false,
            available: true,
            status_message: None,
            models: Vec::new(),
            provider_name: "Test".to_string(),
            base_url: "https://example.com/v1".to_string(),
            model: "test-model".to_string(),
            api_key: None,
            toml_config: None,
            wire_api: "responses".to_string(),
            requires_openai_auth: false,
        };

        activate_pi_provider_at(&provider, &root).expect("activate Pi provider");
        let settings: Value = serde_json::from_str(
            &fs::read_to_string(root.join("settings.json")).expect("read Pi settings"),
        )
        .expect("parse Pi settings");
        assert_eq!(settings.get("theme"), Some(&json!("dark")));
        assert_eq!(
            settings.get("defaultProvider"),
            Some(&json!("devconduit-test"))
        );
        assert_eq!(settings.get("defaultModel"), Some(&json!("test-model")));

        let models_text = fs::read_to_string(root.join("models.json")).expect("read Pi models");
        assert!(models_text.contains("// Keep this provider and comment."));
        let models: Value = CstRootNode::parse(&models_text, &Default::default())
            .expect("parse Pi models JSONC")
            .to_serde_value()
            .expect("convert Pi models JSONC");
        assert!(models.pointer("/providers/existing").is_some());
        assert_eq!(
            models.pointer("/providers/devconduit-test/api"),
            Some(&json!("openai-responses"))
        );
        assert_eq!(
            models.pointer("/providers/devconduit-test/models/0/id"),
            Some(&json!("test-model"))
        );
        assert_eq!(
            models.pointer("/providers/devconduit-test/apiKey"),
            Some(&json!("keep-existing"))
        );
        assert_eq!(
            models.pointer("/providers/devconduit-test/customField/keep"),
            Some(&json!(true))
        );

        fs::remove_dir_all(root).expect("remove Pi provider fixture");
    }
}
