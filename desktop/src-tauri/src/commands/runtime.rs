use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use llmusage::{
    subscription::{FetchContext, UsageEndpoints, fetch_all},
    util::resolve_home_dir,
};

use crate::{
    dto::{CancelQueriesDto, PrefsDto, QuotaResponse, RuntimeInfoDto},
    error::{DesktopError, map_llmusage_error},
    state::AppState,
};

const QUOTA_CACHE_TTL: Duration = Duration::from_secs(300);
const QUOTA_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Default)]
pub struct QuotaInject {
    pub user_home: Option<PathBuf>,
    pub endpoints: Option<UsageEndpoints>,
}

pub fn runtime_info(state: &AppState) -> Result<RuntimeInfoDto, DesktopError> {
    let schema_version = state
        .store
        .meta_value("schema_version")
        .map_err(map_llmusage_error)?
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(llmusage::store::latest_schema_version);
    Ok(RuntimeInfoDto {
        version: env!("CARGO_PKG_VERSION").to_string(),
        root_dir: state.paths.root_dir.clone(),
        db_path: state.paths.db_path.clone(),
        schema_version,
        lock: state
            .store
            .current_worker_lock()
            .map_err(map_llmusage_error)?,
    })
}

pub fn cancel_queries(state: &AppState, dto: CancelQueriesDto) {
    if dto.request_ids.is_empty() {
        state.supervisor.cancel_all();
    } else {
        state.supervisor.cancel(&dto.request_ids);
    }
}

pub async fn fetch_quota(
    state: &AppState,
    bypass_cache: bool,
    inject: QuotaInject,
) -> Result<QuotaResponse, DesktopError> {
    let user_home = inject.user_home.unwrap_or_else(resolve_home_dir);
    let endpoints = inject.endpoints.unwrap_or_else(UsageEndpoints::production);
    let cache_path = state.paths.subscription_cache_path();
    let cache_hit = !bypass_cache && cache_file_fresh(&cache_path, QUOTA_CACHE_TTL);
    let ctx = FetchContext {
        endpoints,
        user_home,
        cache_path: Some(cache_path),
        timeout: QUOTA_TIMEOUT,
    };
    let report = fetch_all(&ctx, bypass_cache).await;
    Ok(QuotaResponse { cache_hit, report })
}

pub fn load_prefs(state: &AppState) -> Result<PrefsDto, DesktopError> {
    let path = prefs_path(state);
    if !path.is_file() {
        return Ok(PrefsDto::default());
    }
    let raw = fs::read_to_string(&path).map_err(|error| {
        DesktopError::invalid_request(format!("failed to read desktop.json: {error}"))
    })?;
    let prefs: PrefsDto = serde_json::from_str(&raw)
        .map_err(|error| DesktopError::invalid_request(format!("invalid desktop.json: {error}")))?;
    crate::dto::validate_prefs(&prefs)?;
    Ok(prefs)
}

pub fn save_prefs(state: &AppState, prefs: PrefsDto) -> Result<PrefsDto, DesktopError> {
    crate::dto::validate_prefs(&prefs)?;
    let path = prefs_path(state);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            DesktopError::invalid_request(format!("failed to create prefs dir: {error}"))
        })?;
    }
    let encoded = serde_json::to_string_pretty(&prefs).map_err(|error| {
        DesktopError::invalid_request(format!("failed to encode prefs: {error}"))
    })?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, encoded).map_err(|error| {
        DesktopError::invalid_request(format!("failed to write desktop.json: {error}"))
    })?;
    fs::rename(&tmp, &path).map_err(|error| {
        DesktopError::invalid_request(format!("failed to replace desktop.json: {error}"))
    })?;
    Ok(prefs)
}

fn prefs_path(state: &AppState) -> PathBuf {
    state.paths.root_dir.join("desktop.json")
}

fn cache_file_fresh(path: &std::path::Path, ttl: Duration) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    match SystemTime::now().duration_since(modified) {
        Ok(age) => age <= ttl,
        Err(_) => false,
    }
}
