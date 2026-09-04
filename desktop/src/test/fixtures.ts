import type {
  HostBreakdown,
  InteractiveSnapshot,
  ProjectBreakdown,
  RuntimeInfoDto,
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
