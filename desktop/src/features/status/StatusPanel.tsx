import type { Copy } from "../../app/i18n";
import type { DiagnosticsPayload } from "../../app/types";

export type RuntimeStatus = "idle" | "running" | "failed" | "lock_busy" | "lock_lost";

export function deriveRuntimeStatus(input: {
  lastErrorCode?: string | null;
  jobStatus?: string | null;
  lockPresent?: boolean;
  lastRunStatus?: string | null;
}): RuntimeStatus {
  if (input.lastErrorCode === "lock_lost") {
    return "lock_lost";
  }
  if (input.lastErrorCode === "lock_busy") {
    return "lock_busy";
  }
  if (input.jobStatus === "running" || input.jobStatus === "cancelling") {
    return "running";
  }
  if (input.lockPresent) {
    return "lock_busy";
  }
  if (input.jobStatus === "failed" || input.lastRunStatus === "failed") {
    return "failed";
  }
  return "idle";
}

const STATUS_COPY: Record<RuntimeStatus, keyof Copy> = {
  idle: "statusIdle",
  running: "statusRunning",
  failed: "statusFailed",
  lock_busy: "statusLockBusy",
  lock_lost: "statusLockLost",
};

export function StatusPanel({
  status,
  holder,
  diagnostics,
  copy,
}: {
  status: RuntimeStatus;
  holder?: string | null;
  diagnostics: DiagnosticsPayload | null;
  copy: Copy;
}) {
  return (
    <section id="status" className="block" data-testid="status-panel" data-status={status}>
      <div className="section-eyebrow">{copy.navStatus}</div>
      <h2 className="section-title">{copy.statusTitle}</h2>
      <p data-testid="status-label">
        {copy[STATUS_COPY[status]]}
        {holder ? ` · ${holder}` : ""}
      </p>
      {status === "lock_lost" ? (
        <p className="alert" role="alert" data-testid="lock-lost-alert">
          {copy.lockLostAlert}
        </p>
      ) : null}
      <h3>{copy.diagnosticsTitle}</h3>
      {diagnostics ? (
        <div data-testid="diagnostics">
          <p className="mono">{diagnostics.archive_root}</p>
          <ul>
            {diagnostics.by_source.map((row) => (
              <li key={row.source}>
                {row.source}: live {row.live_files}, missing {row.missing_file_count}
              </li>
            ))}
          </ul>
          {diagnostics.recent_failures.length > 0 ? (
            <ul>
              {diagnostics.recent_failures.map((row) => (
                <li key={row.id}>
                  {row.command} {row.status}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : (
        <p className="muted" data-testid="diagnostics-empty">
          {copy.diagnosticsEmpty}
        </p>
      )}
    </section>
  );
}
