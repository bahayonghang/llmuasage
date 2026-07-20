use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Result, anyhow};
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    query::{
        CostLine, DailyTrendPoint, Dashboard, ModelBreakdown, OverviewPayload, QueryFilter,
        SyncCommandCenterPayload, TrendPoint, reports::BlockReportRow,
    },
    store::Store,
};

use super::app::{BehaviorPanelPayload, Panel, StatsPanelPayload, TimeWindow};

const TUI_DASHBOARD_QUERY_PERMITS: usize = 5;
const TUI_RESULT_CHANNEL_CAPACITY: usize = 32;

#[derive(Clone)]
pub(super) struct PanelRequest {
    pub panel: Panel,
    pub filter: QueryFilter,
    pub time_window: TimeWindow,
    pub generation: u64,
    pub refreshing: bool,
}

pub(super) struct PanelResult {
    pub panel: Panel,
    pub filter: QueryFilter,
    pub time_window: TimeWindow,
    pub generation: u64,
    pub refreshing: bool,
    pub payload: PanelPayload,
}

pub(super) enum PanelPayload {
    Overview(Result<OverviewPayload, String>),
    SyncCenter(Result<SyncCommandCenterPayload, String>),
    Models(Result<Vec<ModelBreakdown>, String>),
    Daily(Result<Vec<DailyTrendPoint>, String>),
    Hourly(Result<Vec<TrendPoint>, String>),
    Costs(Result<Vec<CostLine>, String>),
    Stats(Result<StatsPanelPayload, String>),
    Behavior(Box<Result<BehaviorPanelPayload, String>>),
    Blocks(Result<Vec<BlockReportRow>, String>),
}

pub(super) struct PanelDataLoader {
    runtime: tokio::runtime::Handle,
    store: Store,
    semaphore: Arc<Semaphore>,
    tx: mpsc::Sender<PanelResult>,
    rx: mpsc::Receiver<PanelResult>,
    active_cancel: Option<CancellationToken>,
}

impl PanelDataLoader {
    pub(super) fn new(store: &Store) -> Result<Self> {
        let (tx, rx) = mpsc::channel(TUI_RESULT_CHANNEL_CAPACITY);
        Ok(Self {
            runtime: tokio::runtime::Handle::try_current()
                .map_err(|err| anyhow!("TUI requires a Tokio runtime: {err}"))?,
            store: store.clone(),
            semaphore: Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
            tx,
            rx,
            active_cancel: None,
        })
    }

    pub(super) fn request(&mut self, request: PanelRequest) {
        self.cancel_active();
        let cancel = CancellationToken::new();
        self.active_cancel = Some(cancel.clone());
        let store = self.store.clone();
        let semaphore = Arc::clone(&self.semaphore);
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = load_panel_request(store, semaphore, cancel, request).await;
            let _ = tx.send(result).await;
        });
    }

    pub(super) fn try_recv(&mut self) -> Option<PanelResult> {
        self.rx.try_recv().ok()
    }

    pub(super) fn cancel_active(&mut self) {
        if let Some(cancel) = self.active_cancel.take() {
            cancel.cancel();
        }
    }
}

impl Drop for PanelDataLoader {
    fn drop(&mut self) {
        self.cancel_active();
    }
}

async fn load_panel_request(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    request: PanelRequest,
) -> PanelResult {
    let filter = request.filter.clone();
    let payload = match request.panel {
        Panel::Overview => PanelPayload::Overview(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard.overview(&filter).map_err(|err| err.to_string())
            })
            .await,
        ),
        Panel::Trends => PanelPayload::SyncCenter(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .sync_command_center(&filter)
                    .map_err(|err| err.to_string())
            })
            .await,
        ),
        Panel::Models => PanelPayload::Models(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .model_breakdown(&filter)
                    .map_err(|err| err.to_string())
            })
            .await,
        ),
        Panel::Sources => PanelPayload::Daily(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .trends_daily(&filter)
                    .map_err(|err| err.to_string())
            })
            .await,
        ),
        Panel::Projects => PanelPayload::Hourly(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .trends("day", &filter)
                    .map_err(|err| err.to_string())
            })
            .await,
        ),
        Panel::Cost => PanelPayload::Costs(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .cost_breakdown(&filter)
                    .map_err(|err| err.to_string())
            })
            .await,
        ),
        Panel::Health => {
            PanelPayload::Stats(load_stats_panel_data(store, semaphore, cancel, filter).await)
        }
        Panel::Behavior => PanelPayload::Behavior(Box::new(
            load_behavior_panel_data(store, semaphore, cancel, filter).await,
        )),
        Panel::Blocks => PanelPayload::Blocks(
            run_query(store, semaphore, cancel, |dashboard| {
                dashboard.blocks_report().map_err(|err| err.to_string())
            })
            .await,
        ),
    };

    PanelResult {
        panel: request.panel,
        filter: request.filter,
        time_window: request.time_window,
        generation: request.generation,
        refreshing: request.refreshing,
        payload,
    }
}

async fn load_stats_panel_data(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    filter: QueryFilter,
) -> Result<StatsPanelPayload, String> {
    let overview = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| dashboard.overview(&filter).map_err(|err| err.to_string())
    });
    let heatmap = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| {
            dashboard
                .heatmap(&filter, 365)
                .map_err(|err| err.to_string())
        }
    });
    let sources = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| {
            dashboard
                .source_breakdown(&filter)
                .map_err(|err| err.to_string())
        }
    });
    let health = run_query(
        store.clone(),
        Arc::clone(&semaphore),
        cancel.clone(),
        |dashboard| dashboard.health().map_err(|err| err.to_string()),
    );
    let context_pressure = run_query(store, semaphore, cancel, move |dashboard| {
        dashboard
            .context_pressure(&filter)
            .map_err(|err| err.to_string())
    });
    let (overview, heatmap, sources, health, context_pressure) =
        tokio::join!(overview, heatmap, sources, health, context_pressure);

    Ok(StatsPanelPayload {
        overview: overview?,
        heatmap: heatmap?,
        sources: sources?,
        health: health?,
        context_pressure: context_pressure?,
    })
}

async fn load_behavior_panel_data(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    filter: QueryFilter,
) -> Result<BehaviorPanelPayload, String> {
    let activity = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| {
            dashboard
                .activity_breakdown(&filter)
                .map_err(|err| err.to_string())
        }
    });
    let tools = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| {
            dashboard
                .tool_breakdown(&filter)
                .map_err(|err| err.to_string())
        }
    });
    let optimize = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| dashboard.optimize(&filter).map_err(|err| err.to_string())
    });
    let zombie = run_query(
        store.clone(),
        Arc::clone(&semaphore),
        cancel.clone(),
        |dashboard| {
            dashboard
                .zombie_report(&crate::query::InventoryRoots::discover())
                .map_err(|err| err.to_string())
        },
    );
    let compare = run_query(store, semaphore, cancel, move |dashboard| {
        dashboard
            .model_compare(&filter, None, None)
            .map_err(|err| err.to_string())
    });
    let (activity, tools, optimize, zombie, compare) =
        tokio::join!(activity, tools, optimize, zombie, compare);

    Ok(BehaviorPanelPayload {
        activity: activity?,
        tools: tools?,
        optimize: optimize?,
        zombie: zombie?,
        compare: compare?,
    })
}

async fn run_query<T, F>(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    query: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> Result<T, String> + Send + 'static,
{
    let permit = tokio::select! {
        permit = semaphore.acquire_owned() => permit.map_err(|_| "dashboard query semaphore closed".to_string())?,
        _ = cancel.cancelled() => return Err("dashboard query cancelled".to_string()),
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let blocking_cancelled = Arc::clone(&cancelled);
    let interrupt = Arc::new(Mutex::new(None));
    let blocking_interrupt = Arc::clone(&interrupt);
    let mut task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let dashboard = Dashboard::open(&store).map_err(|err| err.to_string())?;
        if blocking_cancelled.load(Ordering::SeqCst) {
            return Err("dashboard query cancelled".to_string());
        }
        if let Ok(mut slot) = blocking_interrupt.lock() {
            *slot = Some(dashboard.interrupt_handle());
        }
        if blocking_cancelled.load(Ordering::SeqCst) {
            if let Ok(slot) = blocking_interrupt.lock()
                && let Some(interrupt) = slot.as_ref()
            {
                interrupt.interrupt();
            }
            return Err("dashboard query cancelled".to_string());
        }
        query(&dashboard)
    });

    tokio::select! {
        joined = &mut task => joined.map_err(|err| format!("dashboard query task failed: {err}"))?,
        _ = cancel.cancelled() => {
            cancelled.store(true, Ordering::SeqCst);
            if let Ok(slot) = interrupt.lock()
                && let Some(interrupt) = slot.as_ref()
            {
                interrupt.interrupt();
            }
            let _ = task.await;
            Err("dashboard query cancelled".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use std::time::Instant;
    use tempfile::TempDir;

    fn temp_store() -> Result<(TempDir, Store)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok((temp, store))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_stats_and_behavior_match_serial_queries() -> Result<()> {
        let (_temp, store) = temp_store()?;
        let filter = QueryFilter::default();
        let semaphore = Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS));

        let parallel_stats = load_stats_panel_data(
            store.clone(),
            Arc::clone(&semaphore),
            CancellationToken::new(),
            filter.clone(),
        )
        .await
        .map_err(anyhow::Error::msg)?;
        let parallel_behavior = load_behavior_panel_data(
            store.clone(),
            Arc::clone(&semaphore),
            CancellationToken::new(),
            filter.clone(),
        )
        .await
        .map_err(anyhow::Error::msg)?;

        let dashboard = Dashboard::open(&store)?;
        let serial_stats = StatsPanelPayload {
            overview: dashboard.overview(&filter)?,
            heatmap: dashboard.heatmap(&filter, 365)?,
            sources: dashboard.source_breakdown(&filter)?,
            health: dashboard.health()?,
            context_pressure: dashboard.context_pressure(&filter)?,
        };
        let serial_behavior = BehaviorPanelPayload {
            activity: dashboard.activity_breakdown(&filter)?,
            tools: dashboard.tool_breakdown(&filter)?,
            optimize: dashboard.optimize(&filter)?,
            zombie: dashboard.zombie_report(&crate::query::InventoryRoots::discover())?,
            compare: dashboard.model_compare(&filter, None, None)?,
        };

        assert_eq!(
            serde_json::to_value(&parallel_stats.overview.total)?,
            serde_json::to_value(&serial_stats.overview.total)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_stats.heatmap)?,
            serde_json::to_value(&serial_stats.heatmap)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_stats.sources)?,
            serde_json::to_value(&serial_stats.sources)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_stats.health)?,
            serde_json::to_value(&serial_stats.health)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_stats.context_pressure)?,
            serde_json::to_value(&serial_stats.context_pressure)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.activity)?,
            serde_json::to_value(&serial_behavior.activity)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.tools)?,
            serde_json::to_value(&serial_behavior.tools)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.optimize)?,
            serde_json::to_value(&serial_behavior.optimize)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.zombie)?,
            serde_json::to_value(&serial_behavior.zombie)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.compare)?,
            serde_json::to_value(&serial_behavior.compare)?
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelling_slow_query_interrupts_without_hanging() -> Result<()> {
        let (_temp, store) = temp_store()?;
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run_query(
            store,
            Arc::new(Semaphore::new(1)),
            cancel.clone(),
            |dashboard| dashboard.test_slow_query().map_err(|err| err.to_string()),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        cancel.cancel();

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), task).await??;
        assert_eq!(result.unwrap_err(), "dashboard query cancelled");
        Ok(())
    }

    #[ignore = "reads the local usage database for release-mode performance evidence"]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn measure_local_parallel_panel_loading() -> Result<()> {
        let paths = AppPaths::discover()?;
        let database_bytes = std::fs::metadata(&paths.db_path)?.len();
        let store = Store::new(&paths)?;
        let filter = QueryFilter::default();

        let _ = serial_stats(&store, &filter)?;
        let _ = serial_behavior(&store, &filter)?;
        let _ = load_stats_panel_data(
            store.clone(),
            Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
            CancellationToken::new(),
            filter.clone(),
        )
        .await
        .map_err(anyhow::Error::msg)?;
        let _ = load_behavior_panel_data(
            store.clone(),
            Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
            CancellationToken::new(),
            filter.clone(),
        )
        .await
        .map_err(anyhow::Error::msg)?;

        let mut serial_stats_ms = Vec::new();
        let mut parallel_stats_ms = Vec::new();
        let mut serial_behavior_ms = Vec::new();
        let mut parallel_behavior_ms = Vec::new();
        let dashboard = Dashboard::open(&store)?;
        let started = Instant::now();
        let _ = dashboard.overview(&filter)?;
        let overview_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let started = Instant::now();
        let _ = dashboard.heatmap(&filter, 365)?;
        let heatmap_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let started = Instant::now();
        let _ = dashboard.source_breakdown(&filter)?;
        let sources_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let started = Instant::now();
        let _ = dashboard.health()?;
        let health_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let started = Instant::now();
        let _ = dashboard.context_pressure(&filter)?;
        let context_pressure_ms = started.elapsed().as_secs_f64() * 1_000.0;
        for _ in 0..3 {
            let started = Instant::now();
            let _ = serial_stats(&store, &filter)?;
            serial_stats_ms.push(started.elapsed().as_secs_f64() * 1_000.0);

            let started = Instant::now();
            let _ = load_stats_panel_data(
                store.clone(),
                Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
                CancellationToken::new(),
                filter.clone(),
            )
            .await
            .map_err(anyhow::Error::msg)?;
            parallel_stats_ms.push(started.elapsed().as_secs_f64() * 1_000.0);

            let started = Instant::now();
            let _ = serial_behavior(&store, &filter)?;
            serial_behavior_ms.push(started.elapsed().as_secs_f64() * 1_000.0);

            let started = Instant::now();
            let _ = load_behavior_panel_data(
                store.clone(),
                Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
                CancellationToken::new(),
                filter.clone(),
            )
            .await
            .map_err(anyhow::Error::msg)?;
            parallel_behavior_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
        }

        let stats_serial = median(&mut serial_stats_ms);
        let stats_parallel = median(&mut parallel_stats_ms);
        let behavior_serial = median(&mut serial_behavior_ms);
        let behavior_parallel = median(&mut parallel_behavior_ms);
        eprintln!(
            "database_bytes={database_bytes} stats_parts_ms={{overview:{overview_ms:.1},heatmap:{heatmap_ms:.1},sources:{sources_ms:.1},health:{health_ms:.1},context_pressure:{context_pressure_ms:.1}}} stats_serial_ms={serial_stats_ms:?} stats_parallel_ms={parallel_stats_ms:?} stats_improvement_pct={:.1} behavior_serial_ms={serial_behavior_ms:?} behavior_parallel_ms={parallel_behavior_ms:?} behavior_improvement_pct={:.1}",
            improvement(stats_serial, stats_parallel),
            improvement(behavior_serial, behavior_parallel),
        );
        Ok(())
    }

    fn serial_stats(store: &Store, filter: &QueryFilter) -> Result<StatsPanelPayload> {
        let dashboard = Dashboard::open(store)?;
        Ok(StatsPanelPayload {
            overview: dashboard.overview(filter)?,
            heatmap: dashboard.heatmap(filter, 365)?,
            sources: dashboard.source_breakdown(filter)?,
            health: dashboard.health()?,
            context_pressure: dashboard.context_pressure(filter)?,
        })
    }

    fn serial_behavior(store: &Store, filter: &QueryFilter) -> Result<BehaviorPanelPayload> {
        let dashboard = Dashboard::open(store)?;
        Ok(BehaviorPanelPayload {
            activity: dashboard.activity_breakdown(filter)?,
            tools: dashboard.tool_breakdown(filter)?,
            optimize: dashboard.optimize(filter)?,
            zombie: dashboard.zombie_report(&crate::query::InventoryRoots::discover())?,
            compare: dashboard.model_compare(filter, None, None)?,
        })
    }

    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    }

    fn improvement(before: f64, after: f64) -> f64 {
        (before - after) * 100.0 / before
    }
}
