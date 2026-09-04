import type { Copy } from "../../app/i18n";
import type { CostLine } from "../../app/types";

export function CostsPanel({ costs, copy }: { costs: CostLine[]; copy: Copy }) {
  return (
    <section id="cost" className="block" data-testid="costs-panel">
      <div className="section-eyebrow">{copy.navCost}</div>
      <h2 className="section-title">{copy.costsTitle}</h2>
      {costs.length === 0 ? (
        <p className="muted">{copy.emptyRows}</p>
      ) : (
        <table className="data-table">
          <thead>
            <tr>
              <th>{copy.source}</th>
              <th>{copy.navModels}</th>
              <th>{copy.kpiCost}</th>
            </tr>
          </thead>
          <tbody>
            {costs.map((row) => (
              <tr key={`${row.source}-${row.model}`}>
                <td>{row.source}</td>
                <td>{row.model}</td>
                <td className="num">${row.estimated_cost_usd.toFixed(4)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
