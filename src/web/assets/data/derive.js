import {
  formatCompact,
  formatCompactCurrency,
  formatNumber,
  formatPercent,
  formatUsd,
  statusTone,
} from './format.js';

const logger = window.console;

export const PANEL_LIMITS = Object.freeze({
  trendSpotlight: 10,
  trendTable: 5,
  models: 8,
  modelTable: 8,
  sources: 4,
  sourceTable: 4,
  projects: 5,
  costs: 5,
  costTable: 5,
  failures: 5,
});

function normalizeRows(rows) {
  return Array.isArray(rows) ? rows : [];
}

function sortDesc(rows, select) {
  return [...normalizeRows(rows)].sort((left, right) => {
    const rightValue = Number(select(right) || 0);
    const leftValue = Number(select(left) || 0);
    return rightValue - leftValue;
  });
}

/*
 * ========================================================================
 * 步骤1：构建页面上下文
 * ========================================================================
 * 目标：
 * 1) 把趋势、排行和健康状态整理成渲染友好的结构
 * 2) 固定 Top N、图表序列和对比表行
 * 3) 为各面板补齐总量、峰值、占比和紧凑显示值
 */
export function buildContext({ overview, trends, models, sources, projects, costs, health }) {
  logger.info('开始构建页面上下文');

  // 1.1 规范化并排序趋势、排行和健康数据
  const trendAscending = normalizeRows(trends);
  const trendLedgerRows = [...trendAscending].reverse();
  const trendSpotlightRows = trendLedgerRows.slice(0, PANEL_LIMITS.trendSpotlight);
  const modelRows = sortDesc(models, (row) => row?.total_tokens);
  const sourceRows = sortDesc(sources, (row) => row?.total_tokens);
  const projectRows = sortDesc(projects, (row) => row?.total_tokens);
  const costRows = sortDesc(costs, (row) => row?.estimated_cost_usd);
  const integrationRows = normalizeRows(health?.integrations);
  const cursorRows = normalizeRows(health?.cursors);
  const failureRows = normalizeRows(health?.recent_failures);

  // 1.2 计算账本摘要、趋势聚合和健康聚合
  const trendTotal = trendAscending.reduce(
    (sum, row) => sum + Number(row?.total_tokens || 0),
    0,
  );
  const trendPeak = trendAscending.reduce((best, row) => {
    if (!best || Number(row?.total_tokens || 0) > Number(best?.total_tokens || 0)) {
      return row;
    }
    return best;
  }, null);
  const trendAverage = trendAscending.length ? Math.round(trendTotal / trendAscending.length) : 0;
  const trendActive = trendAscending.filter((row) => Number(row?.total_tokens || 0) > 0).length;
  const totalCost = costRows.reduce(
    (sum, row) => sum + Number(row?.estimated_cost_usd || 0),
    0,
  );
  const readyIntegrations = integrationRows.filter(
    (row) => statusTone(row?.status) === 'good',
  ).length;

  // 1.3 派生图表与表格数据，避免 render 层重复计算
  const modelTableRows = modelRows.slice(0, PANEL_LIMITS.modelTable).map((row) => {
    const outputTokens = Number(row.output_tokens || 0) + Number(row.reasoning_output_tokens || 0);
    return {
      model: row.model,
      totalTokens: formatNumber(row.total_tokens),
      inputShare: formatPercent(row.input_tokens, row.total_tokens),
      outputShare: formatPercent(outputTokens, row.total_tokens),
      cachedShare: formatPercent(row.cached_input_tokens, row.total_tokens),
    };
  });

  const sourceTableRows = sourceRows.slice(0, PANEL_LIMITS.sourceTable).map((row) => ({
    source: row.source,
    lastEventAt: row.last_event_at || '尚未记录',
    totalTokens: formatNumber(row.total_tokens),
  }));

  const costTableRows = costRows.slice(0, PANEL_LIMITS.costTable).map((row) => ({
    model: row.model,
    source: row.source,
    estimatedCostUsd: formatUsd(row.estimated_cost_usd),
  }));

  const context = {
    overview: overview || {},
    ledgerSummary: {
      generatedAt: overview?.generated_at,
      lastSyncAt: overview?.last_sync_at,
      lastExportAt: overview?.last_export_at,
      activeSources: overview?.source_count ?? sourceRows.length,
      failureCount: failureRows.length,
    },
    leaders: {
      model: modelRows[0] ?? null,
      source: sourceRows[0] ?? null,
      project: projectRows[0] ?? null,
      cost: costRows[0] ?? null,
    },
    trend: {
      total: trendTotal,
      peak: trendPeak,
      average: trendAverage,
      active: trendActive,
      spotlightRows: trendSpotlightRows,
      ledgerRows: trendLedgerRows,
      tableRows: trendSpotlightRows.slice(0, PANEL_LIMITS.trendTable),
    },
    panels: {
      models: modelRows,
      modelTableRows,
      sources: sourceRows,
      sourceTableRows,
      projects: projectRows,
      costs: costRows,
      costTableRows,
    },
    health: {
      integrations: integrationRows,
      cursors: cursorRows,
      failures: failureRows,
      readyIntegrations,
      totalIntegrations: integrationRows.length,
    },
    totals: {
      totalTokens: Number(overview?.total?.total_tokens || 0),
      totalTokensCompact: formatCompact(overview?.total?.total_tokens || 0),
      totalTokensRaw: formatNumber(overview?.total?.total_tokens || 0),
      last24hTokens: Number(overview?.last_24h?.total_tokens || 0),
      last24hTokensCompact: formatCompact(overview?.last_24h?.total_tokens || 0),
      last24hTokensRaw: formatNumber(overview?.last_24h?.total_tokens || 0),
      totalCost,
      totalCostCompact: formatCompactCurrency(totalCost),
      totalCostRaw: formatUsd(totalCost),
    },
  };

  logger.info('完成页面上下文构建');
  return context;
}
