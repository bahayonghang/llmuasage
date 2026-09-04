import type { Copy } from "../../app/i18n";
import type { HomeOverviewSummary, SecondarySectionStatus } from "../../app/types";

const CARD_KEYS = [
  "sessions",
  "requests",
  "tokens",
  "cost",
  "active_days",
  "cache_efficiency",
] as const;

function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}

function formatUsd(value: number): string {
  return `$${value.toFixed(2)}`;
}

function formatPercent(value: number): string {
  return `${(value * 100).toFixed(1)}%`;
}

function cardLabel(copy: Copy, key: (typeof CARD_KEYS)[number]): string {
  switch (key) {
    case "sessions":
      return copy.cardSessions;
    case "requests":
      return copy.cardRequests;
    case "tokens":
      return copy.cardTokens;
    case "cost":
      return copy.cardCost;
    case "active_days":
      return copy.cardActiveDays;
    case "cache_efficiency":
      return copy.cardCache;
  }
}

function cardValue(summary: HomeOverviewSummary, key: (typeof CARD_KEYS)[number]): string {
  switch (key) {
    case "sessions":
      return formatNumber(summary.total_sessions);
    case "requests":
      return formatNumber(summary.total_requests);
    case "tokens":
      return formatNumber(summary.total_tokens);
    case "cost":
      return formatUsd(summary.total_cost_usd);
    case "active_days":
      return formatNumber(summary.active_days);
    case "cache_efficiency":
      return formatPercent(summary.cache_efficiency);
  }
}

export function SummaryCards({
  status,
  summary,
  reason,
  copy,
}: {
  status: SecondarySectionStatus;
  summary: HomeOverviewSummary | null;
  reason?: string;
  copy: Copy;
}) {
  if (status === "loading") {
    return (
      <div className="six-cards" data-testid="home-overview-placeholder" data-state="loading">
        {CARD_KEYS.map((key) => (
          <div className="kpi" key={key} data-testid={`summary-card-${key}`}>
            <div className="kpi-label">{copy.sixCardsLoading}</div>
            <div className="muted">…</div>
          </div>
        ))}
      </div>
    );
  }

  const degraded = status === "degraded" || !summary;
  return (
    <div
      className="six-cards"
      data-testid="summary-cards"
      data-state={degraded ? "degraded" : "ready"}
    >
      {CARD_KEYS.map((key) => (
        <div className="kpi" key={key} data-testid={`summary-card-${key}`}>
          <div className="kpi-label">{cardLabel(copy, key)}</div>
          <div className={degraded ? "muted" : "kpi-value num"}>
            {degraded ? "—" : cardValue(summary, key)}
          </div>
        </div>
      ))}
      {degraded && reason ? (
        <p className="muted" data-testid="summary-cards-reason">
          {reason}
        </p>
      ) : null}
    </div>
  );
}
