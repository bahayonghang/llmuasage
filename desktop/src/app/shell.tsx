import { useEffect, useMemo, useRef, useState } from "react";
import { CostsPanel } from "../features/costs/CostsPanel";
import { HostsPanel } from "../features/hosts/HostsPanel";
import { ModelsPanel } from "../features/models/ModelsPanel";
import { OverviewPanel } from "../features/overview/OverviewPanel";
import { ProjectsPanel } from "../features/projects/ProjectsPanel";
import { SourcesPanel } from "../features/sources/SourcesPanel";
import { deriveRuntimeStatus, StatusPanel } from "../features/status/StatusPanel";
import { SyncCenter } from "../features/sync/SyncCenter";
import { TrendsPanel } from "../features/trends/TrendsPanel";
import {
  invokeCommand,
  normalizeInvokeError,
  type DesktopCommandError,
} from "../runtime/invoke";
import { COPY, NAV_ITEMS, type Copy } from "./i18n";
import { applyRange, syncOptionsFromState } from "./filters";
import { createLoadController } from "./load-state";
import type {
  FilterState,
  InteractiveSnapshot,
  JobSnapshot,
  Locale,
  RangePreset,
  RuntimeInfoDto,
  ThemeName,
} from "./types";

const DEFAULT_FILTER: FilterState = { range: "all", window: "all" };
const RANGES: RangePreset[] = ["1d", "7d", "30d", "all", "custom"];

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

function Placeholder({
  id,
  title,
  note,
}: {
  id: string;
  title: string;
  note: string;
}) {
  return (
    <section id={id} className="block" data-testid={`${id}-panel`}>
      <h2 className="section-title">{title}</h2>
      <p className="muted">{note}</p>
    </section>
  );
}

export function Shell() {
  const [locale, setLocale] = useState<Locale>("zh");
  const [theme, setTheme] = useState<ThemeName>("dark");
  const [filter, setFilter] = useState<FilterState>(DEFAULT_FILTER);
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeInfoDto | null>(null);
  const [snapshot, setSnapshot] = useState<InteractiveSnapshot | null>(null);
  const [slow, setSlow] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [job, setJob] = useState<JobSnapshot | null>(null);
  const [commandError, setCommandError] = useState<DesktopCommandError | null>(null);
  const copy = COPY[locale];

  const controllerRef = useRef(
    createLoadController({
      onSlow: () => setSlow(true),
      onFail: (_generation, error) => {
        const parsed = normalizeInvokeError(error);
        setCommandError(parsed);
        setLoadError(parsed.message);
        setSlow(false);
      },
      onSnapshot: (_generation, next) => {
        setSnapshot(next);
        setSlow(false);
        setLoadError(null);
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
    setSlow(false);
    void controllerRef.current.loadDashboardProgressive(filter);
  }, [filter]);

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
                  onClick={() => setFilter((current) => applyRange(current, range))}
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
            <OverviewPanel overview={snapshot.overview} copy={copy} />
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
            <Placeholder id="behavior" title={copy.behaviorTitle} note={copy.secondaryLoading} />
            <Placeholder id="explorer" title={copy.explorerTitle} note={copy.secondaryLoading} />
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
            <Placeholder id="logs" title={copy.logsTitle} note={copy.secondaryLoading} />
            <Placeholder id="quota" title={copy.quotaTitle} note={copy.quotaPlaceholder} />
          </div>
        ) : null}
      </main>
    </div>
  );
}
