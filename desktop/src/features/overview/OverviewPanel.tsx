import type { Copy } from "../../app/i18n";
import type { OverviewPayload } from "../../app/types";

function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}

function formatUsd(value: number): string {
  return `$${value.toFixed(2)}`;
}

export function OverviewPanel({
  overview,
  copy,
}: {
  overview: OverviewPayload;
  copy: Copy;
}) {
  return (
    <section id="overview" className="block" data-testid="overview-panel">
      <div className="hero" data-testid="hero">
        <div>
          <div className="section-eyebrow">{copy.navGroupOverview}</div>
          <h1 className="hero-title">
            {copy.heroTitle}
          </h1>
          <p className="muted">{copy.heroDesc}</p>
          <div className="kpi-grid">
            <div className="kpi">
              <div className="kpi-label">{copy.kpiTotal}</div>
              <div className="kpi-value num">{formatNumber(overview.total.total_tokens)}</div>
            </div>
            <div className="kpi">
              <div className="kpi-label">{copy.kpiDay}</div>
              <div className="kpi-value num">{formatNumber(overview.last_24h.total_tokens)}</div>
            </div>
            <div className="kpi">
              <div className="kpi-label">{copy.kpiCost}</div>
              <div className="kpi-value num">{formatUsd(overview.total_cost_usd)}</div>
            </div>
            <div className="kpi">
              <div className="kpi-label">{copy.kpiSources}</div>
              <div className="kpi-value num">{formatNumber(overview.source_count)}</div>
            </div>
          </div>
        </div>
      </div>
      <div className="six-cards" data-testid="home-overview-placeholder">
        {Array.from({ length: 6 }, (_, index) => (
          <div className="kpi" key={index}>
            <div className="kpi-label">{copy.sixCardsLoading}</div>
            <div className="muted">…</div>
          </div>
        ))}
      </div>
    </section>
  );
}
