import type {
  ActivityPayload,
  ExplorerPayload,
  HostBreakdown,
  HomeOverviewPayload,
  InteractiveSnapshot,
  ModelComparePayload,
  OptimizePayload,
  ProjectBreakdown,
  RuntimeInfoDto,
  ToolsPayload,
} from "../app/types";

export function zeroTokens() {
  return {
    input_tokens: 0,
    cache_creation_tokens: 0,
    cache_read_tokens: 0,
    output_tokens: 0,
    reasoning_output_tokens: 0,
    total_tokens: 0,
  };
}

export function emptyInteractiveSnapshot(
  overrides: Partial<InteractiveSnapshot> = {},
): InteractiveSnapshot {
  return {
    overview: {
      generated_at: "2026-09-04T00:00:00Z",
      total: { ...zeroTokens(), total_tokens: 12 },
      last_24h: zeroTokens(),
      source_count: 1,
      bucket_count: 1,
      total_events: 12,
      last_24h_events: 0,
      total_cost_usd: 0.12,
      cache_efficiency: 0,
      last_sync_at: null,
      last_export_at: null,
    },
    sync_command_center: {
      mode: "ready",
      tone: "neutral",
      headline_key: "syncCenter.headline.ready",
      reason_key: "syncCenter.reason.ready",
      generated_at: "2026-09-04T00:00:00Z",
      current_job: null,
      last_run: null,
      safety: {
        ordinary_sync_safe: true,
        worker_lock: "free",
        worker_lock_holder: null,
        lossy_rebuild_risk: false,
        risk_sources: [],
        recent_failures: 0,
      },
      metrics: {
        events_seen: 0,
        inserted_delta: 0,
        stored_events: 0,
        sources_ready: 0,
        sources_total: 0,
      },
      sources: [],
    },
    trends: [{ label: "all", total_tokens: 12 }],
    models: [{ model: "gpt-4.1", total_tokens: 12, event_count: 12, cost_with_cache_usd: 0.12 }],
    sources: [{ source: "codex", total_tokens: 12, last_event_at: null, event_count: 12 }],
    hosts: [],
    projects: [],
    costs: [
      {
        source: "codex",
        model: "gpt-4.1",
        total_tokens: 12,
        estimated_cost_usd: 0.12,
        event_count: 12,
      },
    ],
    health: { cursor_count: 0, recent_failures: [] },
    diagnostics: { archive_root: "C:\\tmp\\llmusage-home", by_source: [], recent_failures: [] },
    ...overrides,
  };
}

export function hostRow(id: string, label = id): HostBreakdown {
  return {
    host_id: id,
    label,
    total_tokens: 10,
    last_event_at: null,
    event_count: 1,
  };
}

export function projectRow(hash: string, label = hash): ProjectBreakdown {
  return {
    project_hash: hash,
    project_label: label,
    project_ref: null,
    total_tokens: 10,
    event_count: 1,
    total_cost_usd: 0,
  };
}

export function runtimeInfo(overrides: Partial<RuntimeInfoDto> = {}): RuntimeInfoDto {
  return {
    version: "1.3.0",
    root_dir: "C:\\tmp\\llmusage-home",
    db_path: "C:\\tmp\\llmusage-home\\llmusage.db",
    schema_version: 24,
    lock: null,
    ...overrides,
  };
}

export function emptyHomeOverview(
  overrides: Partial<HomeOverviewPayload> = {},
): HomeOverviewPayload {
  return {
    summary: {
      total_sessions: 4,
      total_requests: 12,
      total_tokens: 100,
      total_cost_usd: 0.5,
      cache_efficiency: 0.25,
      active_days: 3,
      platforms: 1,
    },
    by_platform: {},
    ...overrides,
  };
}

export function emptyExplorer(overrides: Partial<ExplorerPayload> = {}): ExplorerPayload {
  return {
    support: { supported: true, level: "normalized", reason: null, strategy: "event" },
    warning: null,
    granularity: "day",
    metric: "attributed_cost_usd",
    group_by: "source",
    limit: 8,
    include_other: true,
    totals: { value: 0 },
    rows: [],
    series: [],
    ...overrides,
  };
}

export function emptyActivity(): ActivityPayload {
  return { support: { supported: true, level: "normalized", reason: null }, breakdown: [] };
}

export function emptyTools(): ToolsPayload {
  return { support: { supported: true, level: "normalized", reason: null }, breakdown: [] };
}

export function emptyOptimize(): OptimizePayload {
  return {
    support: { supported: true, level: "normalized", reason: null },
    score: 90,
    grade: "A",
    estimated_savings_tokens: 0,
    estimated_savings_usd: 0,
    findings: [],
  };
}

export function insufficientCompare(): ModelComparePayload {
  return {
    support: {
      supported: false,
      level: "insufficient_models",
      reason: "At least two models with local usage are required for comparison.",
    },
    candidates: [],
    model_a: null,
    model_b: null,
    metrics: [
      { id: "cost_per_call", label: "cost", model_a_value: 0, model_b_value: 0 },
    ],
    category_head_to_head: [],
    working_style: [],
    warning: "Need at least two models in the current filter.",
  };
}

export function defaultSecondaryPayload(command: string): unknown {
  switch (command) {
    case "home_overview":
      return emptyHomeOverview();
    case "heatmap":
      return [{ date: "2026-09-01", event_count: 2, total_tokens: 40 }];
    case "trends_daily":
      return [
        {
          date: "2026-09-01",
          input_tokens: 10,
          cache_read_tokens: 2,
          cache_creation_tokens: 1,
          output_tokens: 7,
          total_tokens: 20,
          event_count: 2,
          cost_with_cache_usd: 0.1,
        },
      ];
    case "hour_of_week":
      return [{ dow: 0, hour: 9, total_tokens: 12, event_count: 1 }];
    case "top_sessions":
      return [
        {
          session_id: "sess-1",
          session_label: "alpha",
          project_label: "proj",
          source: "codex",
          first_event_at: "2026-09-01T00:00:00Z",
          last_event_at: "2026-09-01T01:00:00Z",
          total_tokens: 40,
          output_tokens: 10,
          cost_usd: 0.2,
          span_minutes: 60,
          active_minutes: 20,
          event_count: 4,
        },
      ];
    case "activity":
      return emptyActivity();
    case "tools":
      return emptyTools();
    case "optimize":
      return emptyOptimize();
    case "compare":
      return insufficientCompare();
    case "explorer":
      return emptyExplorer();
    default:
      return null;
  }
}
