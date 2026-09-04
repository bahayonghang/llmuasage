import type { Copy } from "../../app/i18n";
import type { SecondarySectionStatus, TopSessionRow } from "../../app/types";

const SORTS = ["tokens", "duration", "cost"] as const;

function metricValue(row: TopSessionRow, sort: string): number {
  if (sort === "duration") {
    return row.active_minutes;
  }
  if (sort === "cost") {
    return row.cost_usd;
  }
  return row.total_tokens;
}

function formatMetric(row: TopSessionRow, sort: string): string {
  const value = metricValue(row, sort);
  if (sort === "duration") {
    return `${value}m`;
  }
  if (sort === "cost") {
    return `$${value.toFixed(2)}`;
  }
  return new Intl.NumberFormat().format(value);
}

export function TopSessionsPanel({
  status,
  rows,
  sort,
  reason,
  copy,
  onSortChange,
  onSessionClick,
}: {
  status: SecondarySectionStatus;
  rows: TopSessionRow[] | null;
  sort: string;
  reason?: string;
  copy: Copy;
  onSortChange: (sort: "tokens" | "duration" | "cost") => void;
  onSessionClick: (session: string) => void;
}) {
  const list = Array.isArray(rows) ? rows : [];
  const loading = status === "loading";
  const degraded = status === "degraded";
  const sortLabel: Record<(typeof SORTS)[number], string> = {
    tokens: copy.sortTokens,
    duration: copy.sortDuration,
    cost: copy.sortCost,
  };

  return (
    <section id="top-sessions" className="block" data-testid="top-sessions-panel" data-state={status}>
      <div className="ready-widget-head">
        <div>
          <div className="section-eyebrow">{copy.topSessionsTitle}</div>
          <h2 className="section-title">{copy.topSessionsTitle}</h2>
        </div>
        <div className="seg" role="group">
          {SORTS.map((key) => (
            <button
              key={key}
              type="button"
              className={sort === key ? "active" : ""}
              data-testid={`session-sort-${key}`}
              aria-pressed={sort === key}
              onClick={() => onSortChange(key)}
            >
              {sortLabel[key]}
            </button>
          ))}
        </div>
      </div>
      {loading ? (
        <p className="muted">{copy.secondaryLoading}</p>
      ) : degraded || list.length === 0 ? (
        <p className="muted">{reason || copy.emptyRows}</p>
      ) : (
        <div className="row-list">
          {list.map((row) => (
            <button
              type="button"
              className="session-row"
              key={row.session_id}
              data-testid={`session-row-${row.session_id}`}
              data-session-id={row.session_id}
              onClick={() => onSessionClick(row.session_id)}
            >
              <div>{row.project_label || row.session_label || row.session_id}</div>
              <div className="muted">{row.source || row.session_id}</div>
              <div className="num">{formatMetric(row, sort)}</div>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
