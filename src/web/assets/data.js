export {
  loadJson,
  ensureSnapshot,
  loadSection,
  loadTrendWindow,
  loadDashboardSnapshot,
  buildFilterQuery,
} from './data/fetch.js';
export {
  escapeHtml,
  formatNumber,
  formatCompact,
  formatCompactCurrency,
  formatUsd,
  formatMaybe,
  formatPercent,
  truncate,
  shortLabel,
  statusTone,
  ratio,
} from './data/format.js';
export { PANEL_LIMITS, buildContext } from './data/derive.js';
