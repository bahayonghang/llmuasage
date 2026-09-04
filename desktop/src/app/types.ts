export type RangePreset = "1d" | "7d" | "30d" | "all" | "custom";
export type ThemeName = "light" | "dark";
export type Locale = "zh" | "en";

export type FilterDto = {
  source?: string;
  model?: string;
  since?: string;
  until?: string;
  project_hash?: string;
  host_id?: string;
  timezone?: string;
};

export type SyncStartDto = {
  source?: string;
  recent_days?: number;
};

export type FilterState = {
  range: RangePreset;
  window?: string;
  since?: string;
  until?: string;
  source?: string;
  model?: string;
  project_hash?: string;
  host_id?: string;
};

export type TokenSummary = {
  input_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  output_tokens: number;
  reasoning_output_tokens: number;
  total_tokens: number;
};

export type OverviewPayload = {
  generated_at: string;
  total: TokenSummary;
  last_24h: TokenSummary;
  source_count: number;
  bucket_count: number;
  total_events: number;
  last_24h_events: number;
  total_cost_usd: number;
  cache_efficiency: number;
  last_sync_at: string | null;
  last_export_at: string | null;
};

export type TrendPoint = {
  label: string;
  total_tokens: number;
};

export type ModelBreakdown = {
  model: string;
  total_tokens: number;
  event_count: number;
  cost_with_cache_usd: number;
};

export type SourceBreakdown = {
  source: string;
  total_tokens: number;
  last_event_at: string | null;
  event_count: number;
};

export type HostBreakdown = {
  host_id: string;
  label: string;
  total_tokens: number;
  last_event_at: string | null;
  event_count: number;
};

export type ProjectBreakdown = {
  project_hash: string;
  project_label: string;
  project_ref: string | null;
  total_tokens: number;
  event_count: number;
  total_cost_usd: number;
};

export type CostLine = {
  source: string;
  model: string;
  total_tokens: number;
  estimated_cost_usd: number;
  event_count: number;
};

export type RunRecord = {
  id: number;
  command: string;
  status: string;
  summary: string | null;
  error: string | null;
  started_at: string;
  finished_at: string | null;
};

export type HealthSummaryPayload = {
  cursor_count: number;
  recent_failures: RunRecord[];
};

export type SourceDiagnostics = {
  source: string;
  live_files: number;
  missing_files: number;
  deleted_files: number;
  missing_file_count: number;
  protected_event_count: number;
  lossy_rebuild_risk: boolean;
  recent_completed_at: string | null;
  history_completed_at: string | null;
};

export type DiagnosticsPayload = {
  archive_root: string;
  by_source: SourceDiagnostics[];
  recent_failures: RunRecord[];
};

export type SyncSafetyPayload = {
  ordinary_sync_safe: boolean;
  worker_lock: string;
  worker_lock_holder: string | null;
  lossy_rebuild_risk: boolean;
  risk_sources: string[];
  recent_failures: number;
};

export type SyncMetricsPayload = {
  events_seen: number;
  inserted_delta: number;
  stored_events: number;
  sources_ready: number;
  sources_total: number;
};

export type SyncSourcePayload = {
  source: string;
  status: string;
  events_seen: number;
  stored_events: number;
};

export type SyncLastRunPayload = {
  status: string;
  command: string;
  started_at: string;
  finished_at: string | null;
  error_key: string | null;
};

export type SyncCurrentJobPayload = {
  job_id: string;
  status: string;
  started_at: string;
  finished_at: string | null;
};

export type SyncCommandCenterPayload = {
  mode: string;
  tone: string;
  headline_key: string;
  reason_key: string;
  generated_at: string;
  current_job: SyncCurrentJobPayload | null;
  last_run: SyncLastRunPayload | null;
  safety: SyncSafetyPayload;
  metrics: SyncMetricsPayload;
  sources: SyncSourcePayload[];
};

export type InteractiveSnapshot = {
  overview: OverviewPayload;
  sync_command_center: SyncCommandCenterPayload;
  trends: TrendPoint[];
  models: ModelBreakdown[];
  sources: SourceBreakdown[];
  hosts: HostBreakdown[];
  projects: ProjectBreakdown[];
  costs: CostLine[];
  health: HealthSummaryPayload;
  diagnostics: DiagnosticsPayload;
};

export type WorkerLockMeta = {
  holder_pid: number;
  holder_kind: string;
  acquired_at: string;
  lease_expires_at: string;
  updated_at: string;
};

export type RuntimeInfoDto = {
  version: string;
  root_dir: string;
  db_path: string;
  schema_version: number;
  lock: WorkerLockMeta | null;
};

export type JobSnapshot = {
  job_id: string;
  status: string;
  summary: string | null;
  error: string | null;
  started_at: string;
  finished_at: string | null;
};
