import { useEffect, useMemo, useRef, useState } from "react";
import { BehaviorPanel } from "../features/behavior/BehaviorPanel";
import { CostsPanel } from "../features/costs/CostsPanel";
import { buildAnalyticsCsv } from "../features/export/csv";
import { saveAnalyticsCsv } from "../features/export/save";
import { ExplorerPanel } from "../features/explorer/ExplorerPanel";
import { HeatmapPanel } from "../features/heatmap/HeatmapPanel";
import { HourOfWeekPanel } from "../features/heatmap/HourOfWeekPanel";
import { HostsPanel } from "../features/hosts/HostsPanel";
import { LogsPage } from "../features/logs/LogsPage";
import { ModelsPanel } from "../features/models/ModelsPanel";
import { OverviewPanel } from "../features/overview/OverviewPanel";
import { ProjectsPanel } from "../features/projects/ProjectsPanel";
import { QuotaPage } from "../features/quota/QuotaPage";
import { TopSessionsPanel } from "../features/sessions/TopSessionsPanel";
import { SourcesPanel } from "../features/sources/SourcesPanel";
import { deriveRuntimeStatus, StatusPanel } from "../features/status/StatusPanel";
import { SyncCenter } from "../features/sync/SyncCenter";
import { TrendsDailyPanel } from "../features/trends/TrendsDailyPanel";
import { TrendsPanel } from "../features/trends/TrendsPanel";
import {
  allocateRequestId,
  invokeCommand,
  normalizeInvokeError,
  type DesktopCommandError,
} from "../runtime/invoke";
import { COPY, NAV_ITEMS, type Copy } from "./i18n";
import { applyRange, rangeToFilterDto, syncOptionsFromState } from "./filters";
import { createLoadController } from "./load-state";
import {
  AUTO_REFRESH_OPTIONS,
  createAutoRefreshController,
  filterFromPrefs,
  loadPrefs,
  savePrefs,
  toPrefsDto,
} from "./prefs";
import {
  DEFAULT_EXPLORER_QUERY,
  SECONDARY_SECTIONS,
  applyHeatmapDateClick,
  emitLogsNavigationIntent,
  loadSecondarySections,
  toExplorerDto,
  type HeatmapDrill,
} from "./secondary";
import type {
  ActivityPayload,
  DailyTrendPoint,
  ExplorerPayload,
  ExplorerQueryState,
  FilterState,
  HeatmapPoint,
  HomeOverviewPayload,
  HourOfWeekCell,
  InteractiveSnapshot,
  JobSnapshot,
  Locale,
  ModelComparePayload,
  OptimizePayload,
  RangePreset,
  RuntimeInfoDto,
  SecondarySectionState,
  ThemeName,
  ToolsPayload,
  TopSessionRow,
  AutoRefreshMs,
} from "./types";

const DEFAULT_FILTER: FilterState = { range: "all", window: "all" };
const RANGES: RangePreset[] = ["1d", "7d", "30d", "all", "custom"];

function loadingSection<T>(): SecondarySectionState<T> {
  return { status: "loading", payload: null };
}

function readSection<T>(
  store: Record<string, SecondarySectionState<unknown>>,
  key: string,
): SecondarySectionState<T> {
  const value = store[key];
  if (!value) {
    return loadingSection<T>();
  }
  return value as SecondarySectionState<T>;
}

function applyTheme(theme: ThemeName): void {
  document.documentElement.setAttribute("data-theme", theme);
}

function lockIdentity(lock?: RuntimeInfoDto["lock"] | null): string | undefined {
  if (!lock) {
    return undefined;
  }
  return `${lock.holder_kind}:${lock.holder_pid}@${lock.acquired_at}`;
}

function lockHolderText(info: RuntimeInfoDto | null, copy: Copy): string {
  return lockIdentity(info?.lock) ?? copy.noLock;
}

export function Shell() {
  const [locale, setLocale] = useState<Locale>("zh");
  const [theme, setTheme] = useState<ThemeName>("dark");
  const [autoRefreshMs, setAutoRefreshMs] = useState<AutoRefreshMs>(0);
  const [prefsReady, setPrefsReady] = useState(false);
  const [filter, setFilter] = useState<FilterState>(DEFAULT_FILTER);
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeInfoDto | null>(null);
  const [snapshot, setSnapshot] = useState<InteractiveSnapshot | null>(null);
  const [slow, setSlow] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [job, setJob] = useState<JobSnapshot | null>(null);
  const [commandError, setCommandError] = useState<DesktopCommandError | null>(null);
  const [explorerQuery, setExplorerQuery] = useState<ExplorerQueryState>(DEFAULT_EXPLORER_QUERY);
  const [sessionsSort, setSessionsSort] = useState<"tokens" | "duration" | "cost">("tokens");
  const [heatmapDrill, setHeatmapDrill] = useState<HeatmapDrill>({ date: null, previous: null });
  const [secondary, setSecondary] = useState<Record<string, SecondarySectionState<unknown>>>({});
  const copy = COPY[locale];

  const filterRef = useRef(filter);
  filterRef.current = filter;
  const explorerQueryRef = useRef(explorerQuery);
  explorerQueryRef.current = explorerQuery;
  const sessionsSortRef = useRef(sessionsSort);
  sessionsSortRef.current = sessionsSort;
  const explorerSeqRef = useRef(0);
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;

  const applySection = (section: string, payload: unknown, error: unknown) => {
    setSecondary((current) => ({
      ...current,
      [section]: error
        ? {
            status: "degraded",
            payload: payload ?? null,
            error: normalizeInvokeError(error).message,
          }
        : { status: "ready", payload },
    }));
  };

  const trackRequestId = (id: number) => {
    controllerRef.current.inflightIds.push(id);
  };

  const loadSecondaryRef = useRef<(generation: number) => Promise<void>>(async () => {});
  loadSecondaryRef.current = async (generation: number) => {
    const explorerSeq = explorerSeqRef.current;
    setSecondary(
      Object.fromEntries(SECONDARY_SECTIONS.map((section) => [section, loadingSection()])),
    );
    await loadSecondarySections({
      generation,
      isCurrent: (value) => value === controllerRef.current.generation,
      filter: filterRef.current,
      explorer: explorerQueryRef.current,
      sessionsSort: sessionsSortRef.current,
      onRequestId: trackRequestId,
      onResult: (section, payload, error) => {
        if (section === "explorer" && explorerSeq !== explorerSeqRef.current) {
          return;
        }
        applySection(section, payload, error);
      },
    });
  };

  const controllerRef = useRef(
    createLoadController({
      onSlow: () => setSlow(true),
      onFail: (_generation, error) => {
        const parsed = normalizeInvokeError(error);
        setCommandError(parsed);
        setLoadError(parsed.message);
        setSlow(false);
      },
      onSnapshot: (generation, next) => {
        setSnapshot(next);
        setSlow(false);
        setLoadError(null);
        void loadSecondaryRef.current(generation);
      },
    }),
  );

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    void invokeCommand<RuntimeInfoDto>("runtime_info")
      .then(setRuntimeInfo)
      .catch((error) => setCommandError(normalizeInvokeError(error)));
  }, []);

  useEffect(() => {
    let cancelled = false;
    void loadPrefs()
      .then((prefs) => {
        if (cancelled) {
          return;
        }
        setTheme(prefs.theme);
        setLocale(prefs.locale);
        setAutoRefreshMs(prefs.auto_refresh_ms);
        setFilter(filterFromPrefs(prefs));
      })
      .finally(() => {
        if (!cancelled) {
          setPrefsReady(true);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!prefsReady) {
      return;
    }
    void savePrefs(toPrefsDto(theme, locale, autoRefreshMs, filter)).catch((error) => {
      setCommandError(normalizeInvokeError(error));
    });
  }, [prefsReady, theme, locale, autoRefreshMs, filter]);

  useEffect(() => {
    if (!prefsReady) {
      return;
    }
    const refresh = createAutoRefreshController(autoRefreshMs, () => {
      void controllerRef.current.loadDashboardProgressive(filterRef.current);
    });
    return () => refresh.stop();
  }, [prefsReady, autoRefreshMs]);

  useEffect(() => {
    if (!prefsReady) {
      return;
    }
    setSlow(false);
    void controllerRef.current.loadDashboardProgressive(filter);
  }, [filter, prefsReady]);

  useEffect(() => {
    if (!job || (job.status !== "running" && job.status !== "cancelling")) {
      return;
    }
    const timer = window.setInterval(() => {
      void invokeCommand<JobSnapshot | null>("job_snapshot", { id: job.job_id }).then((next) => {
        if (!next) {
          return;
        }
        setJob(next);
        if (next.status !== "running" && next.status !== "cancelling") {
          void controllerRef.current.loadDashboardProgressive(filter);
        }
      });
    }, 800);
    return () => window.clearInterval(timer);
  }, [job, filter]);

  const status = deriveRuntimeStatus({
    lastErrorCode: commandError?.code,
    jobStatus: job?.status,
    lockPresent: Boolean(runtimeInfo?.lock),
    lastRunStatus: snapshot?.sync_command_center.last_run?.status,
  });
  const writesBlocked = status === "lock_lost" || status === "lock_busy";
  const running = job?.status === "running" || job?.status === "cancelling";
  const statusHolder =
    commandError?.holder ??
    lockIdentity(runtimeInfo?.lock) ??
    snapshot?.sync_command_center.safety.worker_lock_holder;

  const rangeLabel: Record<RangePreset, string> = {
    "1d": copy.range1d,
    "7d": copy.range7d,
    "30d": copy.range30d,
    all: copy.rangeAll,
    custom: copy.rangeCustom,
  };

  const sourceOptions = useMemo(() => {
    const ids = new Set(snapshot?.sources.map((row) => row.source) ?? []);
    if (filter.source) {
      ids.add(filter.source);
    }
    return [...ids];
  }, [snapshot, filter.source]);

  async function handleSync(): Promise<void> {
    if (writesBlocked) {
      return;
    }
    try {
      const next = await invokeCommand<JobSnapshot>("start_sync", {
        request: syncOptionsFromState(filter),
      });
      setJob(next);
      setCommandError(null);
    } catch (error) {
      setCommandError(normalizeInvokeError(error));
    }
  }

  async function handleCancel(): Promise<void> {
    if (!job) {
      return;
    }
    try {
      await invokeCommand<boolean>("cancel_job", { id: job.job_id });
    } catch (error) {
      setCommandError(normalizeInvokeError(error));
    }
  }

  async function handleExplorerChange(next: ExplorerQueryState): Promise<void> {
    setExplorerQuery(next);
    explorerQueryRef.current = next;
    if (!snapshotRef.current) {
      return;
    }
    const generation = controllerRef.current.generation;
    const seq = (explorerSeqRef.current += 1);
    const requestId = allocateRequestId();
    trackRequestId(requestId);
    try {
      const payload = await invokeCommand<ExplorerPayload>("explorer", {
        request: toExplorerDto(rangeToFilterDto(filterRef.current), requestId, next),
      });
      if (generation !== controllerRef.current.generation || seq !== explorerSeqRef.current) {
        return;
      }
      applySection("explorer", payload, null);
    } catch (error) {
      if (generation !== controllerRef.current.generation || seq !== explorerSeqRef.current) {
        return;
      }
      applySection("explorer", null, error);
    }
  }

  async function handleSessionsSort(sort: "tokens" | "duration" | "cost"): Promise<void> {
    setSessionsSort(sort);
    sessionsSortRef.current = sort;
    if (!snapshotRef.current) {
      return;
    }
    const generation = controllerRef.current.generation;
    const requestId = allocateRequestId();
    trackRequestId(requestId);
    try {
      const payload = await invokeCommand<TopSessionRow[]>("top_sessions", {
        request: {
          request_id: requestId,
          filter: rangeToFilterDto(filterRef.current),
          sort,
          limit: 10,
        },
      });
      if (generation !== controllerRef.current.generation) {
        return;
      }
      applySection("top_sessions", payload, null);
    } catch (error) {
      if (generation !== controllerRef.current.generation) {
        return;
      }
      applySection("top_sessions", null, error);
    }
  }

  function handleHeatmapDate(date: string): void {
    const next = applyHeatmapDateClick(filter, heatmapDrill, date);
    setHeatmapDrill(next.drill);
    setFilter(next.filter);
  }

  async function handleExportCsv(): Promise<void> {
    if (!snapshot) {
      return;
    }
    const csv = buildAnalyticsCsv(
      {
        home_overview: readSection<HomeOverviewPayload>(secondary, "home_overview").payload ?? undefined,
        trends_daily: readSection<DailyTrendPoint[]>(secondary, "trends_daily").payload ?? [],
        projects: snapshot.projects,
        models: snapshot.models,
        sources: snapshot.sources,
        top_sessions: readSection<TopSessionRow[]>(secondary, "top_sessions").payload ?? [],
      },
      locale,
    );
    try {
      await saveAnalyticsCsv(csv);
    } catch (error) {
      setCommandError(normalizeInvokeError(error));
    }
  }

  const refreshLabel: Record<AutoRefreshMs, string> = {
    0: copy.autoRefreshOff,
    30000: copy.autoRefresh30,
    60000: copy.autoRefresh60,
  };

  return (
    <div className="app" data-locale={locale}>
      <aside className="sidebar" data-testid="sidebar">
        <div className="brand">
          <div>
            <div className="brand-name">llmusage</div>
            <div className="brand-sub">
              v{runtimeInfo?.version ?? "…"} · {copy.brandSub}
            </div>
          </div>
        </div>
        {(["overview", "distribution", "ops"] as const).map((group) => (
          <div key={group}>
            <div className="nav-label">
              {group === "overview"
                ? copy.navGroupOverview
                : group === "distribution"
                  ? copy.navGroupDistribution
                  : copy.navGroupOps}
            </div>
            <nav>
              {NAV_ITEMS.filter((item) => item.group === group).map((item) => (
                <a key={item.id} href={`#${item.id}`} data-testid={`nav-${item.id}`}>
                  {copy[item.labelKey]}
                </a>
              ))}
            </nav>
          </div>
        ))}
        <div className="sidebar-footer">
          <div className="sidebar-meta" data-testid="root-dir">
            <strong>{copy.rootDir}</strong>
            {runtimeInfo?.root_dir ?? "…"}
          </div>
          <div className="sidebar-meta" data-testid="lock-holder">
            <strong>{copy.lockHolder}</strong>
            {lockHolderText(runtimeInfo, copy)}
            {commandError?.holder ? ` · ${commandError.holder}` : ""}
          </div>
          <div className="sidebar-toggles">
            <button
              type="button"
              className="toggle-btn"
              data-testid="theme-toggle"
              onClick={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
            >
              {theme === "dark" ? copy.themeToLight : copy.themeToDark}
            </button>
            <button
              type="button"
              className="toggle-btn"
              data-testid="locale-toggle"
              onClick={() => setLocale((current) => (current === "zh" ? "en" : "zh"))}
            >
              {copy.localeToggle}
            </button>
          </div>
        </div>
      </aside>
      <main>
        <div className="topbar">
          <div>{copy.heroTitle}</div>
          <div className="topbar-actions">
            <div className="seg" role="group" data-testid="auto-refresh">
              {AUTO_REFRESH_OPTIONS.map((interval) => (
                <button
                  key={interval}
                  type="button"
                  className={autoRefreshMs === interval ? "active" : ""}
                  data-testid={`auto-refresh-${interval}`}
                  aria-pressed={autoRefreshMs === interval}
                  onClick={() => setAutoRefreshMs(interval)}
                >
                  {refreshLabel[interval]}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="btn"
              data-testid="export-csv"
              disabled={!snapshot}
              onClick={() => void handleExportCsv()}
            >
              {copy.exportCsv}
            </button>
            {running ? (
              <button
                type="button"
                className="btn"
                data-testid="cancel-sync-button"
                onClick={() => void handleCancel()}
              >
                {copy.cancelSync}
              </button>
            ) : (
              <button
                type="button"
                className="btn primary"
                data-testid="sync-button"
                disabled={writesBlocked}
                onClick={() => void handleSync()}
              >
                {copy.sync}
              </button>
            )}
          </div>
        </div>
        <form
          className="filter-rail"
          onSubmit={(event) => {
            event.preventDefault();
            setFilter((current) => ({ ...current }));
          }}
        >
          <div className="filter-group">
            <label htmlFor="source-filter">{copy.source}</label>
            <select
              id="source-filter"
              value={filter.source ?? ""}
              onChange={(event) =>
                setFilter((current) => ({
                  ...current,
                  source: event.target.value || undefined,
                }))
              }
            >
              <option value="">{copy.allSources}</option>
              {sourceOptions.map((source) => (
                <option key={source} value={source}>
                  {source}
                </option>
              ))}
            </select>
          </div>
          <div className="filter-group">
            <span id="range-label">{copy.range}</span>
            <div className="range-presets" role="group" aria-labelledby="range-label">
              {RANGES.map((range) => (
                <button
                  key={range}
                  type="button"
                  className={filter.range === range ? "active" : ""}
                  data-testid={`range-${range}`}
                  onClick={() => {
                    setHeatmapDrill({ date: null, previous: null });
                    setFilter((current) => applyRange(current, range));
                  }}
                >
                  {rangeLabel[range]}
                </button>
              ))}
            </div>
          </div>
          {filter.range === "custom" ? (
            <div className="custom-dates">
              <div className="filter-group">
                <label htmlFor="since-input">{copy.since}</label>
                <input
                  id="since-input"
                  value={filter.since ?? ""}
                  placeholder="YYYY-MM-DD"
                  onChange={(event) =>
                    setFilter((current) => ({ ...current, since: event.target.value }))
                  }
                />
              </div>
              <div className="filter-group">
                <label htmlFor="until-input">{copy.until}</label>
                <input
                  id="until-input"
                  value={filter.until ?? ""}
                  placeholder="YYYY-MM-DD"
                  onChange={(event) =>
                    setFilter((current) => ({ ...current, until: event.target.value }))
                  }
                />
              </div>
            </div>
          ) : (
            <div />
          )}
        </form>
        {slow ? (
          <p className="load-banner" data-testid="core-slow">
            {copy.loadSlow}
          </p>
        ) : null}
        {loadError ? (
          <p className="alert" data-testid="core-failed">
            {copy.loadFailed}: {loadError}
          </p>
        ) : null}
        {snapshot ? (
          <div data-testid="core-blocks">
            <OverviewPanel
              overview={snapshot.overview}
              copy={copy}
              summaryStatus={readSection<HomeOverviewPayload>(secondary, "home_overview").status}
              summary={readSection<HomeOverviewPayload>(secondary, "home_overview").payload?.summary ?? null}
              summaryReason={readSection<HomeOverviewPayload>(secondary, "home_overview").error}
            />
            <HeatmapPanel
              status={readSection<HeatmapPoint[]>(secondary, "heatmap").status}
              rows={readSection<HeatmapPoint[]>(secondary, "heatmap").payload}
              reason={readSection<HeatmapPoint[]>(secondary, "heatmap").error}
              selectedDate={heatmapDrill.date}
              copy={copy}
              onDateClick={handleHeatmapDate}
            />
            <HourOfWeekPanel
              status={readSection<HourOfWeekCell[]>(secondary, "hour_of_week").status}
              cells={readSection<HourOfWeekCell[]>(secondary, "hour_of_week").payload}
              reason={readSection<HourOfWeekCell[]>(secondary, "hour_of_week").error}
              copy={copy}
            />
            <TrendsDailyPanel
              status={readSection<DailyTrendPoint[]>(secondary, "trends_daily").status}
              rows={readSection<DailyTrendPoint[]>(secondary, "trends_daily").payload}
              reason={readSection<DailyTrendPoint[]>(secondary, "trends_daily").error}
              copy={copy}
            />
            <TopSessionsPanel
              status={readSection<TopSessionRow[]>(secondary, "top_sessions").status}
              rows={readSection<TopSessionRow[]>(secondary, "top_sessions").payload}
              sort={sessionsSort}
              reason={readSection<TopSessionRow[]>(secondary, "top_sessions").error}
              copy={copy}
              onSortChange={(sort) => void handleSessionsSort(sort)}
              onSessionClick={(session) => emitLogsNavigationIntent(session)}
            />
            <TrendsPanel trends={snapshot.trends} copy={copy} />
            <ModelsPanel models={snapshot.models} copy={copy} />
            <SourcesPanel sources={snapshot.sources} copy={copy} />
            <HostsPanel
              hosts={snapshot.hosts}
              copy={copy}
              onHostClick={(hostId) => setFilter((current) => ({ ...current, host_id: hostId }))}
            />
            <ProjectsPanel
              projects={snapshot.projects}
              copy={copy}
              onProjectClick={(projectHash) =>
                setFilter((current) => ({ ...current, project_hash: projectHash }))
              }
            />
            <BehaviorPanel
              activity={readSection<ActivityPayload>(secondary, "activity")}
              tools={readSection<ToolsPayload>(secondary, "tools")}
              optimize={readSection<OptimizePayload>(secondary, "optimize")}
              compare={readSection<ModelComparePayload>(secondary, "compare")}
              copy={copy}
            />
            <ExplorerPanel
              query={explorerQuery}
              status={readSection<ExplorerPayload>(secondary, "explorer").status}
              payload={readSection<ExplorerPayload>(secondary, "explorer").payload}
              reason={readSection<ExplorerPayload>(secondary, "explorer").error}
              copy={copy}
              onChange={(next) => void handleExplorerChange(next)}
            />
            <CostsPanel costs={snapshot.costs} copy={copy} />
            <SyncCenter
              center={snapshot.sync_command_center}
              job={job}
              copy={copy}
              lockBusy={status === "lock_busy"}
              holder={statusHolder}
            />
            <StatusPanel
              status={status}
              holder={statusHolder}
              diagnostics={snapshot.diagnostics}
              copy={copy}
            />
            <LogsPage filter={filter} copy={copy} />
            <QuotaPage copy={copy} />
          </div>
        ) : null}
      </main>
    </div>
  );
}
