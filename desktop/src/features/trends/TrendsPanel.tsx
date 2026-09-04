import type { Copy } from "../../app/i18n";
import type { TrendPoint } from "../../app/types";

export function TrendsPanel({ trends, copy }: { trends: TrendPoint[]; copy: Copy }) {
  return (
    <section id="trends" className="block" data-testid="trends-panel">
      <div className="section-eyebrow">{copy.navTrend}</div>
      <h2 className="section-title">{copy.trendsTitle}</h2>
      {trends.length === 0 ? (
        <p className="muted">{copy.emptyRows}</p>
      ) : (
        <table className="data-table">
          <thead>
            <tr>
              <th>{copy.range}</th>
              <th>{copy.kpiTotal}</th>
            </tr>
          </thead>
          <tbody>
            {trends.map((row) => (
              <tr key={row.label}>
                <td>{row.label}</td>
                <td className="num">{row.total_tokens}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
