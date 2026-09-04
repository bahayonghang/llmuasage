import type { Copy } from "../../app/i18n";
import type { ModelBreakdown } from "../../app/types";

export function ModelsPanel({ models, copy }: { models: ModelBreakdown[]; copy: Copy }) {
  return (
    <section id="models" className="block" data-testid="models-panel">
      <div className="section-eyebrow">{copy.navModels}</div>
      <h2 className="section-title">{copy.modelsTitle}</h2>
      {models.length === 0 ? (
        <p className="muted">{copy.emptyRows}</p>
      ) : (
        <table className="data-table">
          <thead>
            <tr>
              <th>{copy.navModels}</th>
              <th>{copy.kpiTotal}</th>
              <th>{copy.kpiCost}</th>
            </tr>
          </thead>
          <tbody>
            {models.map((row) => (
              <tr key={row.model}>
                <td>{row.model}</td>
                <td className="num">{row.total_tokens}</td>
                <td className="num">${row.cost_with_cache_usd.toFixed(4)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
