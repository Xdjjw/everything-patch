use super::SavedProvider;
use crate::error::{CodexxError, Result};
use crate::file_io::{atomic_write, ensure_directory, io_err, json_err};
use crate::now_rfc3339;
use crate::paths::{app_home, home_dir};
use crate::tools::REDACTED_VALUE;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const REGISTRY_RELATIVE_PATH: &str = ".zcode/v2/config.json";
const SETTINGS_RELATIVE_PATH: &str = ".zcode/v2/setting.json";
const CLI_RELATIVE_PATH: &str = ".zcode/cli/config.json";

#[derive(Debug, Clone)]
struct ZcodeProviderPaths {
    registry: PathBuf,
    settings: PathBuf,
    cli: PathBuf,
}

impl ZcodeProviderPaths {
    fn from_home(home: &Path) -> Self {
        Self {
            registry: home.join(REGISTRY_RELATIVE_PATH),
            settings: home.join(SETTINGS_RELATIVE_PATH),
            cli: home.join(CLI_RELATIVE_PATH),
        }
    }

    fn all(&self) -> [(&Path, &'static str); 3] {
        [
            (&self.registry, "v2-config.json"),
            (&self.settings, "v2-setting.json"),
            (&self.cli, "cli-config.json"),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZcodeProviderSelection {
    pub(crate) provider_id: String,
    pub(crate) provider_name: String,
    pub(crate) model: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ZcodeProviderSwitchResult {
    pub(crate) provider_name: String,
    pub(crate) model: String,
    pub(crate) backup_path: String,
}

#[derive(Debug, Clone)]
struct FileSnapshot {
    path: PathBuf,
    backup_name: &'static str,
    bytes: Option<Vec<u8>>,
}

fn read_json_value(path: &Path, required: bool) -> Result<Value> {
    if !path.is_file() {
        if required {
            return Err(CodexxError::Config(format!(
                "未找到 ZCode 原生配置：{}",
                path.display()
            )));
        }
        return Ok(Value::Object(Map::new()));
    }
    let bytes = fs::read(path).map_err(|error| io_err(path, error))?;
    serde_json::from_slice(&bytes).map_err(|error| json_err(path, error))
}

fn object_or_error<'a>(value: &'a Value, path: &Path) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| {
        CodexxError::Config(format!("ZCode 配置必须是 JSON object：{}", path.display()))
    })
}

fn object_or_error_mut<'a>(
    value: &'a mut Value,
    path: &Path,
) -> Result<&'a mut Map<String, Value>> {
    value.as_object_mut().ok_or_else(|| {
        CodexxError::Config(format!("ZCode 配置必须是 JSON object：{}", path.display()))
    })
}

fn string_at<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn provider_models(provider: &Value) -> Vec<String> {
    let Some(models) = provider.get("models") else {
        return Vec::new();
    };
    let mut result = match models {
        Value::Object(models) => models.keys().cloned().collect(),
        Value::Array(models) => models
            .iter()
            .filter_map(|model| match model {
                Value::String(model) => Some(model.trim().to_string()),
                Value::Object(model) => ["id", "model", "name"].iter().find_map(|key| {
                    model
                        .get(*key)
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                }),
                _ => None,
            })
            .collect(),
        Value::String(model) => vec![model.trim().to_string()],
        _ => Vec::new(),
    };
    result.retain(|model| !model.is_empty());
    result.dedup();
    let preferred = string_at(provider, &["defaultModel", "default_model", "model"])
        .or_else(|| {
            result
                .iter()
                .find(|model| model.eq_ignore_ascii_case("glm-5.2"))
                .map(String::as_str)
        })
        .map(ToString::to_string);
    if let Some(index) = preferred
        .as_deref()
        .and_then(|preferred| result.iter().position(|model| model == preferred))
        .filter(|index| *index > 0)
    {
        let preferred = result.remove(index);
        result.insert(0, preferred);
    }
    result
}

fn provider_base_url(provider: &Value) -> String {
    provider
        .get("options")
        .and_then(|options| string_at(options, &["baseURL", "baseUrl", "base_url"]))
        .or_else(|| string_at(provider, &["baseURL", "baseUrl", "base_url"]))
        .unwrap_or_default()
        .to_string()
}

fn provider_has_api_key(provider: &Value) -> bool {
    provider
        .get("options")
        .and_then(|options| string_at(options, &["apiKey", "api_key", "token"]))
        .is_some()
}

fn provider_default_model(provider: &Value, models: &[String]) -> String {
    string_at(provider, &["defaultModel", "default_model", "model"])
        .filter(|model| models.is_empty() || models.iter().any(|item| item == model))
        .map(ToString::to_string)
        .or_else(|| {
            models
                .iter()
                .find(|model| model.eq_ignore_ascii_case("glm-5.2"))
                .cloned()
        })
        .or_else(|| models.first().cloned())
        .unwrap_or_default()
}

fn registry_providers<'a>(registry: &'a Value, path: &Path) -> Result<&'a Map<String, Value>> {
    let root = object_or_error(registry, path)?;
    match root.get("provider") {
        None | Some(Value::Null) => Ok(empty_json_object()),
        Some(Value::Object(providers)) => Ok(providers),
        Some(_) => Err(CodexxError::Config(format!(
            "ZCode provider 字段必须是 JSON object：{}",
            path.display()
        ))),
    }
}

fn empty_json_object() -> &'static Map<String, Value> {
    static EMPTY: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Map::new)
}

fn model_spec_selection(
    model_spec: &str,
    providers: &Map<String, Value>,
) -> Option<(String, String)> {
    let trimmed = model_spec.trim();
    let mut ids = providers.keys().collect::<Vec<_>>();
    ids.sort_by_key(|id| std::cmp::Reverse(id.len()));
    ids.into_iter().find_map(|id| {
        let prefix = format!("{id}/");
        trimmed.strip_prefix(&prefix).and_then(|model| {
            let model = model.trim();
            (!model.is_empty()).then(|| (id.clone(), model.to_string()))
        })
    })
}

fn provider_id_from_channel(selected: &str, providers: &Map<String, Value>) -> Option<String> {
    let mut remainder = selected.trim();
    for prefix in ["preset:", "coding-plan:", "team-plan:", "custom:"] {
        if let Some(value) = remainder.strip_prefix(prefix) {
            remainder = value;
            break;
        }
    }
    let mut ids = providers.keys().collect::<Vec<_>>();
    ids.sort_by_key(|id| std::cmp::Reverse(id.len()));
    ids.into_iter().find_map(|id| {
        (remainder == id || remainder.starts_with(&format!("{id}:"))).then(|| id.clone())
    })
}

fn active_provider_from_settings(
    settings: &Value,
    providers: &Map<String, Value>,
) -> Option<String> {
    let family = settings
        .get("providerFamilyDomain")
        .and_then(Value::as_str)?
        .trim();
    if family.is_empty() {
        return None;
    }
    let selected = settings
        .get("modelProviderFamilySelectedKeys")
        .and_then(Value::as_object)
        .and_then(|selected| selected.get(family))
        .and_then(Value::as_str)
        .and_then(|selected| provider_id_from_channel(selected, providers));
    if selected.is_some() {
        return selected;
    }
    let family = family.to_ascii_lowercase().replace(['.', '-'], "");
    providers
        .keys()
        .find(|id| {
            let normalized = id.to_ascii_lowercase().replace(['.', '-'], "");
            normalized == format!("builtin:{family}")
        })
        .cloned()
}

fn current_selection_from_values(
    registry: &Value,
    settings: &Value,
    cli: &Value,
    registry_path: &Path,
) -> Result<Option<ZcodeProviderSelection>> {
    let providers = registry_providers(registry, registry_path)?;
    let cli_selection = cli
        .get("model")
        .and_then(Value::as_str)
        .and_then(|model| model_spec_selection(model, providers));
    let (provider_id, cli_model) = if let Some((provider_id, model)) = cli_selection {
        (provider_id, Some(model))
    } else if let Some(provider_id) = active_provider_from_settings(settings, providers) {
        (provider_id, None)
    } else {
        return Ok(None);
    };
    let Some(provider) = providers.get(&provider_id) else {
        return Ok(None);
    };
    let models = provider_models(provider);
    let model = cli_model
        .filter(|model| models.is_empty() || models.iter().any(|item| item == model))
        .unwrap_or_else(|| provider_default_model(provider, &models));
    let provider_name = string_at(provider, &["name", "label"])
        .unwrap_or(&provider_id)
        .to_string();
    Ok(Some(ZcodeProviderSelection {
        provider_id,
        provider_name,
        model,
    }))
}

fn provider_status(id: &str, provider: &Value, models: &[String]) -> (bool, Option<String>) {
    if models.is_empty() {
        return (false, Some("该供应商没有可用模型".to_string()));
    }
    let has_api_key = provider_has_api_key(provider);
    let api_key_required = provider
        .get("options")
        .and_then(|options| options.get("apiKeyRequired"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let builtin_api_key = matches!(id, "builtin:zai" | "builtin:bigmodel");
    if (api_key_required || builtin_api_key) && !has_api_key {
        return (
            false,
            Some("请先在 ZCode 中为此供应商配置 API Key".to_string()),
        );
    }
    let reason = provider
        .get("systemDisabledReason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty());
    match reason {
        Some(reason) if reason.contains("not_entitled") => {
            (false, Some("当前 ZCode 账号尚未开通此套餐".to_string()))
        }
        Some("oauth_provider_inactive") => (
            true,
            Some("当前不是活动登录通道，切换时会将其激活".to_string()),
        ),
        Some(reason) => (true, Some(format!("ZCode 状态：{reason}"))),
        None => (true, None),
    }
}

fn list_zcode_providers_at(paths: &ZcodeProviderPaths) -> Result<Vec<SavedProvider>> {
    if !paths.registry.is_file() {
        return Ok(Vec::new());
    }
    let registry = read_json_value(&paths.registry, true)?;
    let settings = read_json_value(&paths.settings, false)?;
    let cli = read_json_value(&paths.cli, false)?;
    let current = current_selection_from_values(&registry, &settings, &cli, &paths.registry)?;
    let providers = registry_providers(&registry, &paths.registry)?;
    Ok(providers
        .iter()
        .map(|(id, provider)| {
            let models = provider_models(provider);
            let (available, status_message) = provider_status(id, provider, &models);
            let model = current
                .as_ref()
                .filter(|current| current.provider_id == *id && !current.model.is_empty())
                .map(|current| current.model.clone())
                .unwrap_or_else(|| provider_default_model(provider, &models));
            SavedProvider {
                app_type: "zcode".to_string(),
                id: id.clone(),
                native: true,
                available,
                status_message,
                models,
                provider_name: string_at(provider, &["name", "label"])
                    .unwrap_or(id)
                    .to_string(),
                base_url: provider_base_url(provider),
                model,
                api_key: provider_has_api_key(provider).then(|| REDACTED_VALUE.to_string()),
                toml_config: None,
                wire_api: string_at(provider, &["kind", "apiFormat", "api_format"])
                    .unwrap_or("anthropic")
                    .to_string(),
                requires_openai_auth: false,
            }
        })
        .collect())
}

pub(crate) fn list_zcode_providers_inner() -> Result<Vec<SavedProvider>> {
    let home = home_dir()?;
    list_zcode_providers_at(&ZcodeProviderPaths::from_home(&home))
}

pub(crate) fn current_zcode_provider_inner() -> Result<Option<ZcodeProviderSelection>> {
    let home = home_dir()?;
    let paths = ZcodeProviderPaths::from_home(&home);
    if !paths.registry.is_file() {
        return Ok(None);
    }
    let registry = read_json_value(&paths.registry, true)?;
    let settings = read_json_value(&paths.settings, false)?;
    let cli = read_json_value(&paths.cli, false)?;
    current_selection_from_values(&registry, &settings, &cli, &paths.registry)
}

fn object_entry_mut<'a>(root: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let entry = root
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry
        .as_object_mut()
        .expect("entry was replaced with an object")
}

fn builtin_channel(id: &str) -> Option<(&'static str, &'static str, String)> {
    let family = if id == "builtin:zai" || id.starts_with("builtin:zai-") {
        "zai"
    } else if id == "builtin:bigmodel" || id.starts_with("builtin:bigmodel-") {
        "bigmodel"
    } else {
        return None;
    };
    if matches!(id, "builtin:zai" | "builtin:bigmodel") {
        return Some((family, "apiKey", format!("preset:{id}")));
    }
    if id.contains("team-plan") {
        return Some((family, "oauth", format!("team-plan:{id}")));
    }
    Some((family, "oauth", format!("coding-plan:{id}")))
}

fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn sanitize_backup_component(id: &str) -> String {
    let mut safe = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    safe.truncate(80);
    let safe = safe.trim_matches(['.', ' ', '_']).to_string();
    if safe.is_empty() {
        "provider".to_string()
    } else {
        safe
    }
}

fn snapshots(paths: &ZcodeProviderPaths) -> Result<Vec<FileSnapshot>> {
    paths
        .all()
        .into_iter()
        .map(|(path, backup_name)| {
            let bytes = if path.is_file() {
                Some(fs::read(path).map_err(|error| io_err(path, error))?)
            } else {
                None
            };
            Ok(FileSnapshot {
                path: path.to_path_buf(),
                backup_name,
                bytes,
            })
        })
        .collect()
}

fn create_backup(
    backup_root: &Path,
    provider_id: &str,
    snapshots: &[FileSnapshot],
) -> Result<PathBuf> {
    let timestamp = now_rfc3339()
        .replace(':', "-")
        .replace('+', "_")
        .replace(' ', "_");
    let directory = backup_root.join(format!(
        "{timestamp}-{}",
        sanitize_backup_component(provider_id)
    ));
    ensure_directory(&directory)?;
    for snapshot in snapshots {
        if let Some(bytes) = &snapshot.bytes {
            atomic_write(&directory.join(snapshot.backup_name), bytes)?;
        }
    }
    Ok(directory)
}

fn json_bytes(path: &Path, value: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| json_err(path, error))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn rollback_snapshots(snapshots: &[FileSnapshot]) -> std::result::Result<(), String> {
    let mut errors = Vec::new();
    for snapshot in snapshots.iter().rev() {
        let restored = match &snapshot.bytes {
            Some(bytes) => atomic_write(&snapshot.path, bytes).map_err(|error| error.to_string()),
            None if snapshot.path.exists() => fs::remove_file(&snapshot.path)
                .map_err(|error| io_err(&snapshot.path, error).to_string()),
            None => Ok(()),
        };
        if let Err(error) = restored {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

fn write_transaction_with<F>(
    snapshots: &[FileSnapshot],
    writes: Vec<(&Path, Vec<u8>)>,
    mut write: F,
) -> Result<()>
where
    F: FnMut(&Path, &[u8]) -> Result<()>,
{
    for (path, bytes) in writes {
        if let Err(error) = write(path, &bytes) {
            return match rollback_snapshots(snapshots) {
                Ok(()) => Err(CodexxError::Config(format!(
                    "写入 ZCode 供应商配置失败，已还原原文件：{error}"
                ))),
                Err(rollback_error) => Err(CodexxError::Config(format!(
                    "写入 ZCode 供应商配置失败，且自动还原未完整成功：{error}；{rollback_error}"
                ))),
            };
        }
    }
    Ok(())
}

fn write_transaction(snapshots: &[FileSnapshot], writes: Vec<(&Path, Vec<u8>)>) -> Result<()> {
    write_transaction_with(snapshots, writes, atomic_write)
}

fn activate_zcode_provider_at(
    paths: &ZcodeProviderPaths,
    backup_root: &Path,
    id: &str,
    requested_model: Option<&str>,
) -> Result<ZcodeProviderSwitchResult> {
    let mut registry = read_json_value(&paths.registry, true)?;
    let mut settings = read_json_value(&paths.settings, false)?;
    let mut cli = read_json_value(&paths.cli, false)?;

    let providers = registry_providers(&registry, &paths.registry)?;
    let provider = providers
        .get(id)
        .cloned()
        .ok_or_else(|| CodexxError::Config(format!("ZCode 原生配置中不存在供应商：{id}")))?;
    let models = provider_models(&provider);
    let (available, status_message) = provider_status(id, &provider, &models);
    if !available {
        return Err(CodexxError::Config(
            status_message.unwrap_or_else(|| format!("ZCode 供应商当前不可用：{id}")),
        ));
    }
    let model = requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| provider_default_model(&provider, &models));
    if model.is_empty() || !models.iter().any(|candidate| candidate == &model) {
        return Err(CodexxError::Config(format!(
            "模型 {model:?} 不属于 ZCode 供应商 {id}"
        )));
    }
    let provider_name = string_at(&provider, &["name", "label"])
        .unwrap_or(id)
        .to_string();

    let registry_root = object_or_error_mut(&mut registry, &paths.registry)?;
    let registry_providers = object_entry_mut(registry_root, "provider");
    let selected_provider = registry_providers
        .get_mut(id)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| CodexxError::Config(format!("ZCode 供应商配置无效：{id}")))?;
    if !id.starts_with("builtin:") {
        selected_provider.insert("enabled".to_string(), Value::Bool(true));
    }
    let provider_for_cli = Value::Object(selected_provider.clone());

    let cli_root = object_or_error_mut(&mut cli, &paths.cli)?;
    cli_root.insert("model".to_string(), Value::String(format!("{id}/{model}")));
    object_entry_mut(cli_root, "provider").insert(id.to_string(), provider_for_cli);

    let builtin = builtin_channel(id);
    if let Some((family, mode, selected_key)) = &builtin {
        let settings_root = object_or_error_mut(&mut settings, &paths.settings)?;
        object_entry_mut(settings_root, "modelProviderFamilyModes")
            .insert((*family).to_string(), Value::String((*mode).to_string()));
        object_entry_mut(settings_root, "modelProviderFamilySelectedKeys")
            .insert((*family).to_string(), Value::String(selected_key.clone()));
        settings_root.insert(
            "providerFamilyDomain".to_string(),
            Value::String((*family).to_string()),
        );
        settings_root.insert(
            "providerFamilyDomainUpdatedAt".to_string(),
            Value::Number(unix_time_millis().into()),
        );
    }

    let snapshots = snapshots(paths)?;
    let backup_path = create_backup(backup_root, id, &snapshots)?;
    let registry_bytes = json_bytes(&paths.registry, &registry)?;
    let cli_bytes = json_bytes(&paths.cli, &cli)?;
    let mut writes = vec![
        (paths.registry.as_path(), registry_bytes),
        (paths.cli.as_path(), cli_bytes),
    ];
    if builtin.is_some() {
        let settings_bytes = json_bytes(&paths.settings, &settings)?;
        writes.insert(1, (paths.settings.as_path(), settings_bytes));
    }
    write_transaction(&snapshots, writes)?;

    Ok(ZcodeProviderSwitchResult {
        provider_name,
        model,
        backup_path: backup_path.display().to_string(),
    })
}

pub(crate) fn activate_zcode_provider_inner(
    id: &str,
    requested_model: Option<&str>,
) -> Result<ZcodeProviderSwitchResult> {
    if crate::zcode::is_zcode_running() {
        return Err(CodexxError::Config(
            "ZCode 正在运行。请先完全退出 ZCode（包括系统托盘），再切换供应商，以免配置被运行中的进程覆盖。"
                .to_string(),
        ));
    }
    let home = home_dir()?;
    let backup_root = app_home()?.join("backups").join("providers").join("zcode");
    activate_zcode_provider_at(
        &ZcodeProviderPaths::from_home(&home),
        &backup_root,
        id,
        requested_model,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn test_root(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "everything-patch-zcode-provider-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test root");
        path
    }

    fn write_fixture(path: &Path, value: &Value) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).expect("write fixture");
    }

    fn fixture(root: &Path) -> ZcodeProviderPaths {
        let paths = ZcodeProviderPaths::from_home(root);
        write_fixture(
            &paths.registry,
            &json!({
                "keepRegistry": true,
                "provider": {
                    "builtin:zai": {
                        "name": "Z.ai - API Key",
                        "kind": "anthropic",
                        "options": { "apiKey": "builtin-secret", "baseURL": "https://api.z.ai/api/anthropic" },
                        "models": { "GLM-5.2": {}, "GLM-5-Turbo": {} }
                    },
                    "custom-uuid": {
                        "name": "Router",
                        "kind": "anthropic",
                        "options": { "apiKey": "custom-secret", "baseURL": "https://router.example/v1", "apiKeyRequired": true },
                        "source": "custom",
                        "models": { "glm-5.2": {}, "glm-4.7": {} }
                    }
                }
            }),
        );
        write_fixture(
            &paths.settings,
            &json!({
                "keepSetting": { "nested": true },
                "modelProviderFamilyModes": { "zai": "apiKey" },
                "modelProviderFamilySelectedKeys": { "zai": "preset:builtin:zai" },
                "providerFamilyDomain": "zai"
            }),
        );
        write_fixture(&paths.cli, &json!({ "mcp": { "keep": true } }));
        paths
    }

    #[test]
    fn native_list_reads_object_models_and_never_exposes_secrets() {
        let root = test_root("list");
        let paths = fixture(&root);

        let providers = list_zcode_providers_at(&paths).expect("list native providers");
        let builtin = providers
            .iter()
            .find(|provider| provider.id == "builtin:zai")
            .expect("builtin provider");
        assert!(builtin.native);
        assert_eq!(builtin.model, "GLM-5.2");
        assert_eq!(builtin.models, vec!["GLM-5.2", "GLM-5-Turbo"]);
        assert_eq!(builtin.api_key.as_deref(), Some(REDACTED_VALUE));
        assert!(!format!("{builtin:?}").contains("builtin-secret"));

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn custom_switch_preserves_unknown_fields_and_writes_cli_default() {
        let root = test_root("custom-switch");
        let paths = fixture(&root);
        let backup_root = root.join("backups");
        let before_settings = fs::read(&paths.settings).expect("read settings before");

        let result =
            activate_zcode_provider_at(&paths, &backup_root, "custom-uuid", Some("glm-4.7"))
                .expect("switch custom provider");

        let registry = read_json_value(&paths.registry, true).unwrap();
        let cli = read_json_value(&paths.cli, true).unwrap();
        assert_eq!(registry.get("keepRegistry"), Some(&json!(true)));
        assert_eq!(
            registry.pointer("/provider/custom-uuid/enabled"),
            Some(&json!(true))
        );
        assert_eq!(cli.get("model"), Some(&json!("custom-uuid/glm-4.7")));
        assert_eq!(cli.pointer("/mcp/keep"), Some(&json!(true)));
        assert_eq!(
            cli.pointer("/provider/custom-uuid/options/baseURL"),
            Some(&json!("https://router.example/v1"))
        );
        assert_eq!(fs::read(&paths.settings).unwrap(), before_settings);
        assert!(Path::new(&result.backup_path)
            .join("v2-config.json")
            .is_file());
        assert!(Path::new(&result.backup_path)
            .join("v2-setting.json")
            .is_file());
        assert!(Path::new(&result.backup_path)
            .join("cli-config.json")
            .is_file());

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn builtin_switch_updates_native_channel_and_uses_windows_safe_backup_name() {
        let root = test_root("builtin-switch");
        let paths = fixture(&root);
        let result = activate_zcode_provider_at(
            &paths,
            &root.join("backups"),
            "builtin:zai",
            Some("GLM-5-Turbo"),
        )
        .expect("switch builtin provider");

        let settings = read_json_value(&paths.settings, true).unwrap();
        assert_eq!(
            settings.pointer("/modelProviderFamilyModes/zai"),
            Some(&json!("apiKey"))
        );
        assert_eq!(
            settings.pointer("/modelProviderFamilySelectedKeys/zai"),
            Some(&json!("preset:builtin:zai"))
        );
        assert_eq!(settings.pointer("/keepSetting/nested"), Some(&json!(true)));
        let backup_name = Path::new(&result.backup_path)
            .file_name()
            .unwrap()
            .to_string_lossy();
        assert!(!backup_name.contains(':'));

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn invalid_model_does_not_modify_any_native_file() {
        let root = test_root("invalid-model");
        let paths = fixture(&root);
        let before = paths
            .all()
            .into_iter()
            .map(|(path, _)| fs::read(path).unwrap())
            .collect::<Vec<_>>();

        activate_zcode_provider_at(
            &paths,
            &root.join("backups"),
            "custom-uuid",
            Some("missing-model"),
        )
        .expect_err("unknown model must fail");

        let after = paths
            .all()
            .into_iter()
            .map(|(path, _)| fs::read(path).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(after, before);
        assert!(!root.join("backups").exists());

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn partial_write_failure_restores_every_native_file() {
        let root = test_root("rollback");
        let paths = fixture(&root);
        let snapshots = snapshots(&paths).expect("capture snapshots");
        let original_registry = fs::read(&paths.registry).unwrap();
        let original_settings = fs::read(&paths.settings).unwrap();
        let original_cli = fs::read(&paths.cli).unwrap();
        let mut write_count = 0usize;

        write_transaction_with(
            &snapshots,
            vec![
                (paths.registry.as_path(), b"changed registry".to_vec()),
                (paths.settings.as_path(), b"changed settings".to_vec()),
                (paths.cli.as_path(), b"changed cli".to_vec()),
            ],
            |path, bytes| {
                write_count += 1;
                if write_count == 2 {
                    return Err(CodexxError::Config("injected write failure".to_string()));
                }
                atomic_write(path, bytes)
            },
        )
        .expect_err("second write must fail");

        assert_eq!(fs::read(&paths.registry).unwrap(), original_registry);
        assert_eq!(fs::read(&paths.settings).unwrap(), original_settings);
        assert_eq!(fs::read(&paths.cli).unwrap(), original_cli);

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn team_plan_channel_parser_matches_ids_that_contain_colons() {
        let providers = json!({
            "builtin:zai-team-plan": {},
            "other": {}
        });
        let providers = providers.as_object().unwrap();
        assert_eq!(
            provider_id_from_channel("team-plan:builtin:zai-team-plan:organization-id", providers)
                .as_deref(),
            Some("builtin:zai-team-plan")
        );
    }
}
