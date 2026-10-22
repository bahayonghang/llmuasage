use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Result, anyhow};
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    query::{
        ContextPressurePayload, DailyTrendPoint, Dashboard, HourlyTrendPoint, ModelBreakdown,
        MonthlyTrendPoint, PeriodDetailRow, QueryFilter, SyncCommandCenterPayload,
        reports::BlockReportRow,
    },
    store::Store,
};

use super::app::{
    BehaviorPanelPayload, OverviewPanelPayload, Panel, PeriodDetailKind, StatsPanelPayload,
    TimeWindow,
};

const TUI_DASHBOARD_QUERY_PERMITS: usize = 5;
const TUI_RESULT_CHANNEL_CAPACITY: usize = 32;

#[derive(Debug, thiserror::Error)]
pub(super) enum LoadError {
    #[error("dashboard query cancelled")]
    Cancelled,
    #[error("dashboard query semaphore closed")]
    SemaphoreClosed,
    #[error("dashboard query task failed: {0}")]
    TaskFailed(String),
    #[error("{0}")]
    Query(#[from] crate::LlmusageError),
    #[error("{0}")]
    External(String),
}

impl From<anyhow::Error> for LoadError {
    fn from(error: anyhow::Error) -> Self {
        match error.downcast::<crate::LlmusageError>() {
            Ok(error) => Self::Query(error),
            Err(error) => Self::External(format!("{error:#}")),
        }
    }
}

impl LoadError {
    pub(super) fn render(&self) -> String {
        use crate::common::i18n::{commands::DomainDiagnostic, current, text};
        match self {
            Self::Cancelled => text("Dashboard query cancelled", "看板查询已取消").to_string(),
            Self::SemaphoreClosed => {
                text("Dashboard query semaphore closed", "看板查询调度已关闭").to_string()
            }
            Self::TaskFailed(detail) => format!(
                "{} {}",
                text("Dashboard query task failed.", "看板查询任务失败。"),
                crate::common::i18n::tui::original(detail)
            ),
            Self::Query(error) => crate::common::i18n::commands::DiagnosticFacts::Domain(
                DomainDiagnostic::from_error(error),
            )
            .render(current())
            .unwrap_or_else(|| crate::common::i18n::tui::original(&error.to_string())),
            Self::External(detail) => crate::common::i18n::tui::original(detail),
        }
    }
}

#[cfg(test)]
static RUN_QUERY_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(Clone)]
pub(super) struct PanelRequest {
    pub panel: Panel,
    pub filter: QueryFilter,
    pub time_window: TimeWindow,
    pub generation: u64,
    pub refreshing: bool,
    pub detail: Option<PeriodDetailKind>,
}

pub(super) struct PanelResult {
    pub panel: Panel,
    pub filter: QueryFilter,
    pub time_window: TimeWindow,
    pub generation: u64,
    pub refreshing: bool,
    pub payload: PanelPayload,
}

#[allow(clippy::large_enum_variant)]
pub(super) enum PanelPayload {
    Overview(Result<OverviewPanelPayload, LoadError>),
    SyncCenter(Result<SyncCommandCenterPayload, LoadError>),
    Models(Result<Vec<ModelBreakdown>, LoadError>),
    Daily(Result<Vec<DailyTrendPoint>, LoadError>),
    Hourly(Result<Vec<HourlyTrendPoint>, LoadError>),
    Monthly(Result<Vec<MonthlyTrendPoint>, LoadError>),
    DailyDetail(Result<Vec<PeriodDetailRow>, LoadError>),
    MonthlyDetail(Result<Vec<DailyTrendPoint>, LoadError>),
    Stats(Result<StatsPanelPayload, LoadError>),
    Behavior(Box<Result<(BehaviorPanelPayload, crate::query::OptimizePresentation), LoadError>>),
    Blocks(Result<Vec<BlockReportRow>, LoadError>),
}

pub(super) struct PanelDataLoader {
    runtime: tokio::runtime::Handle,
    store: Store,
    semaphore: Arc<Semaphore>,
    tx: mpsc::Sender<PanelResult>,
    rx: mpsc::Receiver<PanelResult>,
    active_cancel: Option<CancellationToken>,
    pub(super) behavior_presentation: Option<crate::query::OptimizePresentation>,
}

impl PanelDataLoader {
    pub(super) fn new(store: &Store) -> Result<Self> {
        let (tx, rx) = mpsc::channel(TUI_RESULT_CHANNEL_CAPACITY);
        Ok(Self {
            runtime: tokio::runtime::Handle::try_current().map_err(|err| {
                anyhow!(
                    "{} {err}",
                    crate::common::i18n::text(
                        "TUI requires a Tokio runtime:",
                        "TUI 需要 Tokio 运行时："
                    )
                )
            })?,
            store: store.clone(),
            semaphore: Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
            tx,
            rx,
            active_cancel: None,
            behavior_presentation: None,
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
    let window_filter = request.time_window.query_filter(&filter);
    let payload = match (&request.detail, request.panel) {
        (Some(PeriodDetailKind::Daily { date }), _) => {
            let mut detail_filter = window_filter;
            if let Ok(day) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                detail_filter.since = Some(day);
                detail_filter.until = Some(day);
            }
            PanelPayload::DailyDetail(
                run_query(store, semaphore, cancel, move |dashboard| {
                    dashboard
                        .period_model_breakdown(&detail_filter)
                        .map_err(LoadError::from)
                })
                .await,
            )
        }
        (Some(PeriodDetailKind::Monthly { month }), _) => {
            let mut detail_filter = window_filter;
            if let Some((start, end)) = crate::query::month_date_bounds(month) {
                detail_filter.since = Some(start);
                detail_filter.until = Some(end);
            }
            PanelPayload::MonthlyDetail(
                run_query(store, semaphore, cancel, move |dashboard| {
                    dashboard
                        .trends_daily(&detail_filter)
                        .map_err(LoadError::from)
                })
                .await,
            )
        }
        (None, Panel::Overview) => PanelPayload::Overview(
            load_overview_panel_data(store, semaphore, cancel, filter, window_filter).await,
        ),
        (None, Panel::Trends) => PanelPayload::SyncCenter(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .sync_command_center(&filter)
                    .map_err(LoadError::from)
            })
            .await,
        ),
        (None, Panel::Models) => PanelPayload::Models(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .model_breakdown(&window_filter)
                    .map_err(LoadError::from)
            })
            .await,
        ),
        (None, Panel::Sources) => PanelPayload::Daily(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .trends_daily(&window_filter)
                    .map_err(LoadError::from)
            })
            .await,
        ),
        (None, Panel::Projects) => PanelPayload::Hourly(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .trends_hourly(&window_filter)
                    .map_err(LoadError::from)
            })
            .await,
        ),
        (None, Panel::Monthly) => PanelPayload::Monthly(
            run_query(store, semaphore, cancel, move |dashboard| {
                dashboard
                    .trends_monthly(&window_filter)
                    .map_err(LoadError::from)
            })
            .await,
        ),
        (None, Panel::Health) => PanelPayload::Stats(
            load_stats_panel_data(store, semaphore, cancel, filter, window_filter).await,
        ),
        (None, Panel::Behavior) => PanelPayload::Behavior(Box::new(
            load_behavior_panel_data(store, semaphore, cancel, window_filter).await,
        )),
        (None, Panel::Blocks) => PanelPayload::Blocks(
            run_query(store, semaphore, cancel, |dashboard| {
                dashboard.blocks_report().map_err(LoadError::from)
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

async fn load_overview_panel_data(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    base_filter: QueryFilter,
    window_filter: QueryFilter,
) -> Result<OverviewPanelPayload, LoadError> {
    let totals = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = base_filter;
        move |dashboard| dashboard.overview(&filter).map_err(LoadError::from)
    });
    let daily_models = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = window_filter.clone();
        move |dashboard| {
            dashboard
                .trends_daily_by_model(&filter)
                .map_err(LoadError::from)
        }
    });
    let models = run_query(store, semaphore, cancel, move |dashboard| {
        dashboard
            .model_breakdown(&window_filter)
            .map_err(LoadError::from)
    });
    let (totals, daily_models, models) = tokio::join!(totals, daily_models, models);
    Ok(OverviewPanelPayload {
        totals: totals?,
        daily_models: daily_models?,
        models: models?,
    })
}

async fn load_stats_panel_data(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    base_filter: QueryFilter,
    window_filter: QueryFilter,
) -> Result<StatsPanelPayload, LoadError> {
    let overview = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = base_filter.clone();
        move |dashboard| dashboard.overview(&filter).map_err(LoadError::from)
    });
    let heatmap = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = base_filter.clone();
        move |dashboard| dashboard.heatmap(&filter, 365).map_err(LoadError::from)
    });
    let models = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = base_filter;
        move |dashboard| dashboard.model_breakdown(&filter).map_err(LoadError::from)
    });
    let context_pressure = load_context_pressure(store, semaphore, cancel, window_filter);
    let (overview, heatmap, models, context_pressure) =
        tokio::join!(overview, heatmap, models, context_pressure);

    Ok(StatsPanelPayload {
        overview: overview?,
        heatmap: heatmap?,
        models: models?,
        context_pressure: context_pressure?,
    })
}

async fn load_context_pressure(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    filter: QueryFilter,
) -> Result<ContextPressurePayload, LoadError> {
    run_query(store, semaphore, cancel, move |dashboard| {
        dashboard.context_pressure(&filter).map_err(LoadError::from)
    })
    .await
}

#[cfg(test)]
fn merge_context_pressure(
    parts: impl IntoIterator<Item = ContextPressurePayload>,
) -> ContextPressurePayload {
    let mut merged = ContextPressurePayload {
        peak_percent: 0.0,
        avg_percent: 0.0,
        peak_model: None,
        priced_events: 0,
        unpriced_events: 0,
    };
    let mut weighted_ratio_sum = 0.0;
    for part in parts {
        if part.peak_percent > merged.peak_percent {
            merged.peak_percent = part.peak_percent;
            merged.peak_model = part.peak_model;
        }
        weighted_ratio_sum += part.avg_percent * part.priced_events as f64;
        merged.priced_events += part.priced_events;
        merged.unpriced_events += part.unpriced_events;
    }
    if merged.priced_events > 0 {
        merged.avg_percent = weighted_ratio_sum / merged.priced_events as f64;
    }
    merged
}

async fn load_behavior_panel_data(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    filter: QueryFilter,
) -> Result<(BehaviorPanelPayload, crate::query::OptimizePresentation), LoadError> {
    let activity = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| {
            dashboard
                .activity_breakdown(&filter)
                .map_err(LoadError::from)
        }
    });
    let tools = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| dashboard.tool_breakdown(&filter).map_err(LoadError::from)
    });
    let optimize = run_query(store.clone(), Arc::clone(&semaphore), cancel.clone(), {
        let filter = filter.clone();
        move |dashboard| {
            dashboard
                .optimize_with_presentation(&filter)
                .map_err(LoadError::from)
        }
    });
    let zombie = run_query(
        store.clone(),
        Arc::clone(&semaphore),
        cancel.clone(),
        |dashboard| {
            dashboard
                .zombie_report(&crate::query::InventoryRoots::discover())
                .map_err(LoadError::from)
        },
    );
    let compare = run_query(store, semaphore, cancel, move |dashboard| {
        dashboard
            .model_compare(&filter, None, None)
            .map_err(LoadError::from)
    });
    let (activity, tools, optimize, zombie, compare) =
        tokio::join!(activity, tools, optimize, zombie, compare);

    let (optimize, presentation) = optimize?;
    Ok((
        BehaviorPanelPayload {
            activity: activity?,
            tools: tools?,
            optimize,
            zombie: zombie?,
            compare: compare?,
        },
        presentation,
    ))
}

async fn run_query<T, F>(
    store: Store,
    semaphore: Arc<Semaphore>,
    cancel: CancellationToken,
    query: F,
) -> Result<T, LoadError>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> Result<T, LoadError> + Send + 'static,
{
    #[cfg(test)]
    RUN_QUERY_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let permit = tokio::select! {
        permit = semaphore.acquire_owned() => permit.map_err(|_| LoadError::SemaphoreClosed)?,
        _ = cancel.cancelled() => return Err(LoadError::Cancelled),
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let blocking_cancelled = Arc::clone(&cancelled);
    let interrupt = Arc::new(Mutex::new(None));
    let blocking_interrupt = Arc::clone(&interrupt);
    let mut task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let dashboard = Dashboard::open(&store).map_err(LoadError::from)?;
        if blocking_cancelled.load(Ordering::SeqCst) {
            return Err(LoadError::Cancelled);
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
            return Err(LoadError::Cancelled);
        }
        query(&dashboard)
    });

    tokio::select! {
        joined = &mut task => joined.map_err(|err| LoadError::TaskFailed(err.to_string()))?,
        _ = cancel.cancelled() => {
            cancelled.store(true, Ordering::SeqCst);
            if let Ok(slot) = interrupt.lock()
                && let Some(interrupt) = slot.as_ref()
            {
                interrupt.interrupt();
            }
            let _ = task.await;
            Err(LoadError::Cancelled)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use std::time::Instant;
    use tempfile::TempDir;

    #[test]
    fn loader_owned_failures_are_typed_before_presentation_and_external_detail_is_marked() {
        use crate::common::i18n::{Language, with_language};
        for (language, cancelled, closed, original) in [
            (
                Language::English,
                "cancelled",
                "semaphore closed",
                "Original detail",
            ),
            (Language::Chinese, "已取消", "已关闭", "原始详情"),
        ] {
            with_language(language, || {
                assert!(LoadError::Cancelled.render().contains(cancelled));
                assert!(LoadError::SemaphoreClosed.render().contains(closed));
                let detail = LoadError::External("SQLITE_SENTINEL".into()).render();
                assert!(detail.contains(original) && detail.contains("SQLITE_SENTINEL"));
                let typed = LoadError::from(crate::LlmusageError::NotInitialized).render();
                assert!(typed.contains("llmusage init"));
                assert!(!typed.contains(original));
            });
        }
    }

    fn temp_store() -> Result<(TempDir, Store)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().to_path_buf())?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok((temp, store))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unset_source_context_pressure_opens_one_dashboard() -> Result<()> {
        let (_temp, store) = temp_store()?;
        RUN_QUERY_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
        let payload = load_context_pressure(
            store.clone(),
            Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
            CancellationToken::new(),
            QueryFilter::default(),
        )
        .await
        .map_err(anyhow::Error::new)?;
        assert_eq!(
            RUN_QUERY_CALLS.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "unset-source context_pressure must call Dashboard::context_pressure once"
        );
        let serial = Dashboard::open(&store)?.context_pressure(&QueryFilter::default())?;
        assert_eq!(
            serde_json::to_value(&payload)?,
            serde_json::to_value(&serial)?
        );
        Ok(())
    }

    #[test]
    fn context_pressure_parts_merge_with_weighted_average() {
        let merged = merge_context_pressure([
            ContextPressurePayload {
                peak_percent: 0.5,
                avg_percent: 0.25,
                peak_model: Some("codex:gpt-5".to_string()),
                priced_events: 2,
                unpriced_events: 1,
            },
            ContextPressurePayload {
                peak_percent: 0.4,
                avg_percent: 0.5,
                peak_model: Some("claude:claude-fable-5".to_string()),
                priced_events: 1,
                unpriced_events: 2,
            },
        ]);

        assert_eq!(merged.peak_percent, 0.5);
        assert_eq!(merged.peak_model.as_deref(), Some("codex:gpt-5"));
        assert!((merged.avg_percent - (1.0 / 3.0)).abs() < 1e-12);
        assert_eq!(merged.priced_events, 3);
        assert_eq!(merged.unpriced_events, 3);
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
            filter.clone(),
        )
        .await
        .map_err(anyhow::Error::new)?;
        let parallel_behavior = load_behavior_panel_data(
            store.clone(),
            Arc::clone(&semaphore),
            CancellationToken::new(),
            filter.clone(),
        )
        .await
        .map_err(anyhow::Error::new)?;

        let dashboard = Dashboard::open(&store)?;
        let serial_stats = StatsPanelPayload {
            overview: dashboard.overview(&filter)?,
            heatmap: dashboard.heatmap(&filter, 365)?,
            models: dashboard.model_breakdown(&filter)?,
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
            serde_json::to_value(&parallel_stats.models)?,
            serde_json::to_value(&serial_stats.models)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_stats.context_pressure)?,
            serde_json::to_value(&serial_stats.context_pressure)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.0.activity)?,
            serde_json::to_value(&serial_behavior.activity)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.0.tools)?,
            serde_json::to_value(&serial_behavior.tools)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.0.optimize)?,
            serde_json::to_value(&serial_behavior.optimize)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.0.zombie)?,
            serde_json::to_value(&serial_behavior.zombie)?
        );
        assert_eq!(
            serde_json::to_value(&parallel_behavior.0.compare)?,
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
            |dashboard| dashboard.test_slow_query().map_err(LoadError::from),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        cancel.cancel();

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), task).await??;
        assert_eq!(result.unwrap_err().to_string(), "dashboard query cancelled");
        Ok(())
    }

    #[ignore = "reads the local usage database for release-mode performance evidence"]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn measure_local_parallel_panel_loading() -> Result<()> {
        let paths = AppPaths::discover()?;
        let database_bytes = std::fs::metadata(&paths.db_path)?.len();
        let store = Store::new(&paths)?;
        let base_filter = QueryFilter::default();
        let window_filter = TimeWindow::Month30d.query_filter(&base_filter);

        let _ = serial_stats(&store, &base_filter, &window_filter)?;
        let _ = serial_behavior(&store, &window_filter)?;
        let _ = load_stats_panel_data(
            store.clone(),
            Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
            CancellationToken::new(),
            base_filter.clone(),
            window_filter.clone(),
        )
        .await
        .map_err(anyhow::Error::new)?;
        let _ = load_behavior_panel_data(
            store.clone(),
            Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
            CancellationToken::new(),
            window_filter.clone(),
        )
        .await
        .map_err(anyhow::Error::new)?;

        let mut serial_stats_ms = Vec::new();
        let mut parallel_stats_ms = Vec::new();
        let mut serial_behavior_ms = Vec::new();
        let mut parallel_behavior_ms = Vec::new();
        let dashboard = Dashboard::open(&store)?;
        let started = Instant::now();
        let _ = dashboard.overview(&base_filter)?;
        let overview_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let started = Instant::now();
        let _ = dashboard.heatmap(&base_filter, 365)?;
        let heatmap_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let started = Instant::now();
        let _ = dashboard.model_breakdown(&base_filter)?;
        let models_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let started = Instant::now();
        let _ = dashboard.context_pressure(&window_filter)?;
        let context_pressure_ms = started.elapsed().as_secs_f64() * 1_000.0;
        for _ in 0..3 {
            let started = Instant::now();
            let _ = serial_stats(&store, &base_filter, &window_filter)?;
            serial_stats_ms.push(started.elapsed().as_secs_f64() * 1_000.0);

            let started = Instant::now();
            let _ = load_stats_panel_data(
                store.clone(),
                Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
                CancellationToken::new(),
                base_filter.clone(),
                window_filter.clone(),
            )
            .await
            .map_err(anyhow::Error::new)?;
            parallel_stats_ms.push(started.elapsed().as_secs_f64() * 1_000.0);

            let started = Instant::now();
            let _ = serial_behavior(&store, &window_filter)?;
            serial_behavior_ms.push(started.elapsed().as_secs_f64() * 1_000.0);

            let started = Instant::now();
            let _ = load_behavior_panel_data(
                store.clone(),
                Arc::new(Semaphore::new(TUI_DASHBOARD_QUERY_PERMITS)),
                CancellationToken::new(),
                window_filter.clone(),
            )
            .await
            .map_err(anyhow::Error::new)?;
            parallel_behavior_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
        }

        let stats_serial = median(&mut serial_stats_ms);
        let stats_parallel = median(&mut parallel_stats_ms);
        let behavior_serial = median(&mut serial_behavior_ms);
        let behavior_parallel = median(&mut parallel_behavior_ms);
        eprintln!(
            "database_bytes={database_bytes} window=30d since={:?} until={:?} stats_parts_ms={{overview:{overview_ms:.1},heatmap:{heatmap_ms:.1},models:{models_ms:.1},context_pressure:{context_pressure_ms:.1}}} stats_serial_ms={serial_stats_ms:?} stats_parallel_ms={parallel_stats_ms:?} stats_improvement_pct={:.1} behavior_serial_ms={serial_behavior_ms:?} behavior_parallel_ms={parallel_behavior_ms:?} behavior_improvement_pct={:.1}",
            window_filter.since,
            window_filter.until,
            improvement(stats_serial, stats_parallel),
            improvement(behavior_serial, behavior_parallel),
        );
        Ok(())
    }

    fn serial_stats(
        store: &Store,
        base_filter: &QueryFilter,
        window_filter: &QueryFilter,
    ) -> Result<StatsPanelPayload> {
        let dashboard = Dashboard::open(store)?;
        Ok(StatsPanelPayload {
            overview: dashboard.overview(base_filter)?,
            heatmap: dashboard.heatmap(base_filter, 365)?,
            models: dashboard.model_breakdown(base_filter)?,
            context_pressure: dashboard.context_pressure(window_filter)?,
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
