mod ccswitch;
mod connection;
mod live;
mod store;
mod tool_live;
mod zcode;

use crate::error::Result;
use rusqlite::Connection;

#[cfg(test)]
pub(crate) use ccswitch::{
    build_ccswitch_codex_provider, codex_sections_from_config, is_official_ccswitch_row,
    read_ccswitch_codex_rows, CcSwitchCodexRow,
};
pub(crate) use ccswitch::{
    import_ccswitch_codex_providers_inner, import_ccswitch_providers_inner,
    read_ccswitch_official_auth_inner, ImportResult, OfficialAuthCandidate,
};
#[cfg(test)]
pub(crate) use connection::provider_status_result;
pub(crate) use connection::{
    fetch_provider_models_inner, test_provider_connection_inner, ProviderConnectionResult,
    ProviderModelsResult,
};
#[cfg(test)]
pub(crate) use live::{
    detected_live_custom_provider, save_provider_toml_config_with_pre_persist,
    switch_official_provider_with_pre_persist, switch_provider_with_pre_persist,
};
pub(crate) use live::{
    save_official_config_inner, save_provider_toml_config_inner, switch_official_provider_inner,
    switch_provider_inner, OfficialConfigInput, ProviderInput, ProviderTomlInput,
};
#[cfg(test)]
pub(crate) use store::{
    canonical_provider_base_url, merge_duplicate_provider_identities, provider_by_id_on_connection,
    provider_identity, save_manual_provider_on_connection,
};
pub(crate) use store::{
    custom_provider_id, delete_provider_for_app_inner, experimental_bearer_token_from_doc,
    list_saved_providers_for_app_inner, list_saved_providers_inner,
    list_saved_providers_on_connection, list_saved_providers_on_connection_for_app,
    normalize_saved_provider, provider_by_id_on_connection_for_app, reserved_codex_provider_id,
    save_detected_provider_inner, save_provider_inner, upsert_provider_on_connection,
    ProviderUpsertKind, ProviderUpsertMode, SavedProvider,
};
pub(crate) use tool_live::{activate_saved_provider_inner, ToolProviderActionResult};
pub(crate) use zcode::{
    activate_zcode_provider_inner, current_zcode_provider_inner, list_zcode_providers_inner,
};

pub(crate) fn open_store() -> Result<Connection> {
    let mut conn = crate::app_db::open()?;
    store::merge_duplicate_provider_identities(&mut conn)?;
    Ok(conn)
}
