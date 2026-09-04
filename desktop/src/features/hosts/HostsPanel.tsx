import type { Copy } from "../../app/i18n";
import type { HostBreakdown } from "../../app/types";

export function shouldRenderHosts(hosts: readonly unknown[] | undefined): boolean {
  return (hosts?.length ?? 0) > 1;
}

export function HostsPanel({
  hosts,
  copy,
  onHostClick,
}: {
  hosts: HostBreakdown[];
  copy: Copy;
  onHostClick: (hostId: string) => void;
}) {
  if (!shouldRenderHosts(hosts)) {
    return null;
  }

  return (
    <section id="hosts" className="block" data-testid="hosts-panel">
      <div className="section-eyebrow">{copy.hostsTitle}</div>
      <h2 className="section-title">{copy.hostsTitle}</h2>
      <div className="row-list">
        {hosts.map((row) => (
          <button
            type="button"
            className="row-btn"
            key={row.host_id}
            data-testid={`host-row-${row.host_id}`}
            onClick={() => onHostClick(row.host_id)}
          >
            <div>{row.label || row.host_id}</div>
            <div className="muted">{row.last_event_at ?? "--"}</div>
            <div className="num">{row.total_tokens}</div>
          </button>
        ))}
      </div>
    </section>
  );
}
