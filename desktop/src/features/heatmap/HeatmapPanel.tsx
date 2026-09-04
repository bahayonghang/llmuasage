import type { Copy } from "../../app/i18n";
import type { HeatmapPoint, SecondarySectionStatus } from "../../app/types";
import { heatmapLevels } from "./heatmap";

export function HeatmapPanel({
  status,
  rows,
  reason,
  selectedDate,
  copy,
  onDateClick,
}: {
  status: SecondarySectionStatus;
  rows: HeatmapPoint[] | null;
  reason?: string;
  selectedDate?: string | null;
  copy: Copy;
  onDateClick: (date: string) => void;
}) {
  const points = Array.isArray(rows) ? rows : [];
  const degraded = status === "degraded";
  const loading = status === "loading";
  const hasEvents = points.some((row) => Number(row.event_count) > 0);
  const levels = heatmapLevels(points.map((row) => Number(row.total_tokens) || 0));

  return (
    <section id="heatmap" className="block" data-testid="heatmap-panel" data-state={status}>
      <div className="section-eyebrow">{copy.heatmapTitle}</div>
      <h2 className="section-title">{copy.heatmapTitle}</h2>
      {loading ? (
        <p className="muted">{copy.secondaryLoading}</p>
      ) : degraded || !hasEvents ? (
        <p className="muted" data-testid="heatmap-empty">
          {reason || copy.emptyRows}
        </p>
      ) : (
        <div className="heatmap-grid">
          {points.map((row, index) => (
            <button
              key={row.date}
              type="button"
              className={`heatmap-cell hm-l${levels[index] ?? 0}${selectedDate === row.date ? " selected" : ""}`}
              data-date={row.date}
              data-testid={`heatmap-date-${row.date}`}
              aria-pressed={selectedDate === row.date}
              onClick={() => onDateClick(row.date)}
            >
              <span className="sr-only">{row.date}</span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
