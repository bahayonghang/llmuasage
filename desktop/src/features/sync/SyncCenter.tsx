import type { Copy } from "../../app/i18n";
import type { JobSnapshot, SyncCommandCenterPayload } from "../../app/types";

export function SyncCenter({
  center,
  job,
  copy,
  lockBusy = false,
  holder,
}: {
  center: SyncCommandCenterPayload;
  job: JobSnapshot | null;
  copy: Copy;
  lockBusy?: boolean;
  holder?: string | null;
}) {
  const running = job?.status === "running" || job?.status === "cancelling";
  const busyHolder = holder || center.safety.worker_lock_holder;
  return (
    <section id="sync-center" className="block" data-testid="sync-center">
      <div className="section-eyebrow">{copy.syncTitle}</div>
      <h2 className="section-title">{copy.syncTitle}</h2>
      <p>{center.headline_key}</p>
      <p className="muted">{center.reason_key}</p>
      <p data-testid="sync-lock">
        {copy.lockHolder}: {busyHolder || center.safety.worker_lock}
      </p>
      {running ? <p data-testid="sync-running">{copy.statusRunning}</p> : null}
      {lockBusy ? (
        <p data-testid="sync-lock-busy">
          {copy.statusLockBusy}
          {busyHolder ? ` · ${busyHolder}` : ""}
        </p>
      ) : null}
      <div className="kpi-grid">
        <div className="kpi">
          <div className="kpi-label">events</div>
          <div className="num">{center.metrics.events_seen}</div>
        </div>
        <div className="kpi">
          <div className="kpi-label">stored</div>
          <div className="num">{center.metrics.stored_events}</div>
        </div>
      </div>
      {center.sources.map((row) => (
        <div className="source-row" key={row.source}>
          <div>{row.source}</div>
          <div className="muted">{row.status}</div>
          <div className="num">{row.stored_events}</div>
        </div>
      ))}
    </section>
  );
}
