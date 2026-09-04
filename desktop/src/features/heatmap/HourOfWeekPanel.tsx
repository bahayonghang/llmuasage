import type { Copy } from "../../app/i18n";
import type { HourOfWeekCell, SecondarySectionStatus } from "../../app/types";

const WEEKDAY_KEYS = [
  "weekdayMon",
  "weekdayTue",
  "weekdayWed",
  "weekdayThu",
  "weekdayFri",
  "weekdaySat",
  "weekdaySun",
] as const;

function cellLevel(value: number, max: number): number {
  if (!value || !max) {
    return 0;
  }
  if (value <= max * 0.25) {
    return 1;
  }
  if (value <= max * 0.5) {
    return 2;
  }
  if (value <= max * 0.75) {
    return 3;
  }
  return 4;
}

export function HourOfWeekPanel({
  status,
  cells,
  reason,
  copy,
}: {
  status: SecondarySectionStatus;
  cells: HourOfWeekCell[] | null;
  reason?: string;
  copy: Copy;
}) {
  const rows = Array.isArray(cells) ? cells : [];
  const max = Math.max(0, ...rows.map((row) => Number(row.total_tokens) || 0));
  const byCell = new Map(rows.map((row) => [`${row.dow}:${row.hour}`, row]));
  const degraded = status === "degraded";
  const loading = status === "loading";

  return (
    <section id="hour-of-week" className="block" data-testid="hour-of-week-panel" data-state={status}>
      <div className="section-eyebrow">{copy.hourOfWeekTitle}</div>
      <h2 className="section-title">{copy.hourOfWeekTitle}</h2>
      {loading ? (
        <p className="muted">{copy.secondaryLoading}</p>
      ) : degraded || !max ? (
        <p className="muted">{reason || copy.emptyRows}</p>
      ) : (
        <div className="hour-grid" role="grid">
          {WEEKDAY_KEYS.map((key, dow) => (
            <div key={key} className="hour-row" role="row">
              <div className="hour-dow">{copy[key]}</div>
              {Array.from({ length: 24 }, (_, hour) => {
                const cell = byCell.get(`${dow}:${hour}`);
                const tokens = Number(cell?.total_tokens) || 0;
                return (
                  <span
                    key={hour}
                    className={`hour-cell hm-l${cellLevel(tokens, max)}`}
                    title={`${copy[key]} ${hour}:00 · ${tokens}`}
                  />
                );
              })}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
