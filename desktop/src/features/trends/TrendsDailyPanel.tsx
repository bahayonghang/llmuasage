import type { Copy } from "../../app/i18n";
import type { DailyTrendPoint, SecondarySectionStatus } from "../../app/types";

function otherTokens(row: DailyTrendPoint): number {
  const known =
    Number(row.input_tokens || 0) +
    Number(row.cache_read_tokens || 0) +
    Number(row.cache_creation_tokens || 0) +
    Number(row.output_tokens || 0);
  return Math.max(0, Number(row.total_tokens || 0) - known);
}

export function TrendsDailyPanel({
  status,
  rows,
  reason,
  copy,
}: {
  status: SecondarySectionStatus;
  rows: DailyTrendPoint[] | null;
  reason?: string;
  copy: Copy;
}) {
  const list = Array.isArray(rows) ? rows : [];
  const loading = status === "loading";
  const degraded = status === "degraded";
  const hasData = list.some((row) => Number(row.total_tokens) > 0);

  return (
    <section id="trends-daily" className="block" data-testid="trends-daily-panel" data-state={status}>
      <div className="section-eyebrow">{copy.trendsDailyTitle}</div>
      <h2 className="section-title">{copy.trendsDailyTitle}</h2>
      {loading ? (
        <p className="muted">{copy.secondaryLoading}</p>
      ) : degraded || !hasData ? (
        <p className="muted">{reason || copy.emptyRows}</p>
      ) : (
        <table className="data-table">
          <thead>
            <tr>
              <th>{copy.range}</th>
              <th>{copy.inputTokens}</th>
              <th>{copy.cacheReadTokens}</th>
              <th>{copy.cacheCreationTokens}</th>
              <th>{copy.outputTokens}</th>
              <th>{copy.otherTokens}</th>
            </tr>
          </thead>
          <tbody>
            {list.map((row) => (
              <tr key={row.date}>
                <td>{row.date}</td>
                <td className="num">{row.input_tokens}</td>
                <td className="num">{row.cache_read_tokens}</td>
                <td className="num">{row.cache_creation_tokens}</td>
                <td className="num">{row.output_tokens}</td>
                <td className="num">{otherTokens(row)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
