use std::{
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use llmusage::{
    AppPaths, DiagnosticsPayload, JobRegistry, Store, app::AppContext,
    commands::serve::repair_legacy_token_accounting,
};
use tokio::sync::Semaphore;

use crate::{
    error::{DesktopError, map_llmusage_error},
    supervisor::DesktopQuerySupervisor,
};

pub const QUERY_PERMITS: usize = 4;
const DIAGNOSTICS_CACHE_TTL: Duration = Duration::from_secs(30);

pub struct DiagnosticsCache {
    ttl: Duration,
    entry: RwLock<Option<DiagnosticsCacheEntry>>,
    compute: tokio::sync::Mutex<()>,
    generation: AtomicU64,
}

struct DiagnosticsCacheEntry {
    payload: DiagnosticsPayload,
    computed_at: Instant,
}

impl DiagnosticsCache {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entry: RwLock::new(None),
            compute: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
        }
    }

    pub fn get_fresh(&self) -> Option<DiagnosticsPayload> {
        let guard = self.entry.read().ok()?;
        let entry = guard.as_ref()?;
        (entry.computed_at.elapsed() < self.ttl).then(|| entry.payload.clone())
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn store_if_generation(&self, payload: DiagnosticsPayload, expected_generation: u64) -> bool {
        if let Ok(mut guard) = self.entry.write() {
            if self.generation.load(Ordering::Acquire) != expected_generation {
                return false;
            }
            *guard = Some(DiagnosticsCacheEntry {
                payload,
                computed_at: Instant::now(),
            });
            return true;
        }
        false
    }

    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.entry.write() {
            self.generation.fetch_add(1, Ordering::AcqRel);
            *guard = None;
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub paths: AppPaths,
    pub store: Store,
    pub jobs: JobRegistry,
    pub supervisor: Arc<DesktopQuerySupervisor>,
    pub diagnostics_cache: Arc<DiagnosticsCache>,
    pub query_semaphore: Arc<Semaphore>,
}

pub async fn startup_from_root(root: Option<PathBuf>) -> Result<AppState, DesktopError> {
    let app = match root {
        Some(root) => AppContext::with_cli_home(Some(root)).map_err(DesktopError::from_anyhow)?,
        None => AppContext::discover().map_err(DesktopError::from_anyhow)?,
    };
    startup(app).await
}

pub async fn startup(app: AppContext) -> Result<AppState, DesktopError> {
    let store = Store::new(&app.paths).map_err(map_llmusage_error)?;
    store.bootstrap().map_err(map_llmusage_error)?;
    repair_legacy_token_accounting(&app, &store)
        .await
        .map_err(DesktopError::from_anyhow)?;
    let diagnostics_cache = Arc::new(DiagnosticsCache::new(DIAGNOSTICS_CACHE_TTL));
    let jobs = JobRegistry::default();
    jobs.register_terminal_hook({
        let cache = Arc::downgrade(&diagnostics_cache);
        move || {
            if let Some(cache) = cache.upgrade() {
                cache.invalidate();
            }
        }
    });
    Ok(AppState {
        paths: app.paths,
        store,
        jobs,
        supervisor: Arc::new(DesktopQuerySupervisor::new()),
        diagnostics_cache,
        query_semaphore: Arc::new(Semaphore::new(QUERY_PERMITS)),
    })
}

impl AppState {
    pub async fn load_diagnostics_cached(&self) -> Result<DiagnosticsPayload, DesktopError> {
        if let Some(payload) = self.diagnostics_cache.get_fresh() {
            return Ok(payload);
        }
        let _compute = self.diagnostics_cache.compute.lock().await;
        loop {
            if let Some(payload) = self.diagnostics_cache.get_fresh() {
                return Ok(payload);
            }
            let generation = self.diagnostics_cache.generation();
            let payload =
                crate::supervisor::run_query(self, 0, Duration::from_secs(5), |dashboard| {
                    dashboard.diagnostics()
                })
                .await?;
            if self
                .diagnostics_cache
                .store_if_generation(payload.clone(), generation)
            {
                return Ok(payload);
            }
        }
    }
}
