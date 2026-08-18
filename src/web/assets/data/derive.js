import { UI_COPY } from '../copy.js';
import {
  formatCompact,
  formatCompactCurrency,
  formatDateTime,
  formatNumber,
  formatPercent,
  formatTokenAmount,
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

function emptyExplorerPayload() {
  return {
    support: { supported: false, level: 'no_data', strategy: 'none' },
    warning: null,
    granularity: 'day',
    metric: 'attributed_cost_usd',
    group_by: 'source',
    limit: 8,
    include_other: true,
    totals: { value: 0 },
    rows: [],
    series: [],
  };
}

function normalizeExplorer(explorer) {
  const payload = explorer || emptyExplorerPayload();
  const rows = sortDesc(payload.rows, (row) => row?.value);
  const series = normalizeRows(payload.series).map((point) => ({
    bucket: point?.bucket || '--',
    key: point?.key || '',
    label: point?.label || point?.key || '--',
    value: Number(point?.value || 0),
    is_other: Boolean(point?.is_other),
  }));
  return {
    ...emptyExplorerPayload(),
    ...payload,
    support: payload.support || emptyExplorerPayload().support,
    totals: payload.totals || { value: 0 },
    rows,
    series,
  };
}

function normalizeSyncCommandCenter(payload) {
  const metrics = payload?.metrics || {};
  const safety = payload?.safety || {};
  return {
    mode: payload?.mode || 'live',
    tone: payload?.tone || 'neutral',
    headline_key: payload?.headline_key || 'syncCenter.headline.empty',
    reason_key: payload?.reason_key || 'syncCenter.reason.empty',
    generated_at: payload?.generated_at || '',
    current_job: payload?.current_job || null,
    last_run: payload?.last_run || null,
    safety: {
      ordinary_sync_safe: safety?.ordinary_sync_safe !== false,
      worker_lock: safety?.worker_lock || 'unknown',
      worker_lock_holder: safety?.worker_lock_holder || null,
      lossy_rebuild_risk: Boolean(safety?.lossy_rebuild_risk),
      risk_sources: normalizeRows(safety?.risk_sources),
      recent_failures: Number(safety?.recent_failures || 0),
    },
    metrics: {
      events_seen: Number(metrics?.events_seen || 0),
      inserted_delta: Number(metrics?.inserted_delta || 0),
      stored_events: Number(metrics?.stored_events || 0),
      sources_ready: Number(metrics?.sources_ready || 0),
      sources_total: Number(metrics?.sources_total || 0),
    },
    sources: normalizeRows(payload?.sources).map((row) => ({
      source: row?.source || '--',
      status: row?.status || 'idle',
      tone: row?.tone || 'neutral',
      events_seen: Number(row?.events_seen || 0),
      events_inserted: Number(row?.events_inserted || 0),
      stored_events: Number(row?.stored_events || 0),
      malformed_lines: Number(row?.malformed_lines || 0),
      oversized_lines: Number(row?.oversized_lines || 0),
      skipped_lines: Number(row?.skipped_lines || 0),
      accounting_anomaly_lines: Number(row?.accounting_anomaly_lines || 0),
      updated_at: row?.updated_at || '',
      share: Math.max(0, Math.min(1, Number(row?.share || 0))),
      error_key: row?.error_key || null,
      lossy_rebuild_risk: Boolean(row?.lossy_rebuild_risk),
    })),
    actions: normalizeRows(payload?.actions),
  };
}

function positiveRows(rows, select) {
  return normalizeRows(rows).filter((row) => Number(select(row) || 0) > 0);
}

function normalizeTrendRows(rows) {
  return normalizeRows(rows).map((row) => ({
    label: row?.label ?? row?.time_bucket ?? '--',
    total_tokens: Number(row?.total_tokens || 0),
  }));
}

function hasPricingConcern(status) {
  const normalized = String(status || '').toLowerCase();
  return normalized === 'mixed' || normalized === 'unpriced';
}

function staleSourceRows(rows) {
  const cutoff = Date.now() - 14 * 24 * 60 * 60 * 1000;
  return normalizeRows(rows).filter((row) => {
    if (!row?.last_event_at) return false;
    const timestamp = Date.parse(row.last_event_at);
    return Number.isFinite(timestamp) && timestamp < cutoff;
  });
}

function buildInsights({ overview, modelRows, projectRows, costRows, sourceRows, diagnosticsRows, failureRows }) {
  const insights = [];
  const cacheEfficiency = Number(overview?.cache_efficiency || 0);
  const totalTokens = Number(overview?.total?.total_tokens || 0);
  const pricingConcernRows = modelRows.filter((row) => hasPricingConcern(row?.pricing_status));
  const lossyRows = diagnosticsRows.filter((row) => row?.lossy_rebuild_risk);
  const missingRows = diagnosticsRows.filter(
    (row) => Number(row?.missing_file_count || row?.missing_files || 0) > 0 && !row?.lossy_rebuild_risk,
  );
  const staleRows = staleSourceRows(sourceRows);
  const topCost = costRows.find((row) => Number(row?.estimated_cost_usd || 0) > 0) || null;
  const topModel = modelRows[0] || null;
  const topProject = projectRows[0] || null;

  if (totalTokens > 0 && cacheEfficiency < 0.05) {
    insights.push({
      id: 'cache_low',
      tone: 'warn',
      params: { percentage: (cacheEfficiency * 100).toFixed(1) },
    });
  }

  if (pricingConcernRows.length > 0) {
    const row = pricingConcernRows[0];
    insights.push({
      id: 'pricing_gap',
      tone: 'warn',
      params: { count: pricingConcernRows.length, model: row.model || '--', status: row.pricing_status || '--' },
    });
  }

  if (failureRows.length > 0) {
    const row = failureRows[0];
    insights.push({
      id: 'sync_failure',
      tone: 'warn',
      params: { count: failureRows.length, command: row.command || '--' },
    });
  }

  if (lossyRows.length > 0) {
    const row = lossyRows[0];
    insights.push({
      id: 'lossy_rebuild',
      tone: 'warn',
      params: { source: row.source || '--', missingCount: formatNumber(row.missing_file_count), protectedCount: formatNumber(row.protected_event_count) },
    });
  } else if (missingRows.length > 0) {
    const row = missingRows[0];
    insights.push({
      id: 'missing_source',
      tone: 'neutral',
      params: { source: row.source || '--', missingCount: formatNumber(row.missing_file_count || row.missing_files) },
    });
  }

  if (staleRows.length > 0) {
    const row = staleRows[0];
    insights.push({
      id: 'stale_source',
      tone: 'neutral',
      params: { source: row.source || '--', lastEvent: row.last_event_at || '--' },
    });
  }

  if (topCost) {
    insights.push({
      id: 'top_cost',
      tone: 'good',
      params: { source: topCost.source || '--', model: topCost.model || '--', cost: formatUsd(topCost.estimated_cost_usd) },
    });
  } else if (topModel && Number(topModel.total_tokens || 0) > 0) {
    insights.push({
      id: 'top_model',
      tone: 'neutral',
      params: { model: topModel.model || '--', tokens: formatTokenAmount(topModel.total_tokens) },
    });
  }

  if (topProject && Number(topProject.total_tokens || 0) > 0) {
    insights.push({
      id: 'top_project',
      tone: 'neutral',
      params: { project: topProject.project_label || topProject.project_hash || '--', tokens: formatTokenAmount(topProject.total_tokens) },
    });
  }

  return insights.slice(0, 6);
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
 * 2) 固定显示上限、图表序列和对比表行
 * 3) 为各面板补齐总量、峰值、占比和紧凑显示值
 */
/*
 * buildContext 按 rawData 引用做 memo：rawData 约定为不可变替换式更新，
 * 引用相同即内容相同，直接复用上次派生结果——一次数据到达只派生一次，
 * 面板级重渲（展开/折叠、locale 切换）不再重复全量派生。
 */
let contextMemo = { raw: null, ctx: null };
let contextComputeCount = 0;
let contextMemoHitCount = 0;

export function buildContext(rawData) {
  if (rawData && rawData === contextMemo.raw) {
    contextMemoHitCount += 1;
    return contextMemo.ctx;
  }
  contextComputeCount += 1;
  const context = deriveContext(rawData || {});
  contextMemo = { raw: rawData || null, ctx: context };
  return context;
}

// 插桩：派生次数应约等于数据到达次数，而不是渲染次数（供性能回归测试断言）。
export function buildContextStats() {
  return {
    computes: contextComputeCount,
    memoHits: contextMemoHitCount,
  };
}

function deriveContext({ overview, trends, models, sources, projects, costs, activity, tools, optimize, compare, explorer, home_overview, heatmap, trends_daily, top_sessions, hour_of_week, health, diagnostics, sync_command_center, _meta }) {
  logger.info('开始构建页面上下文');

  // 1.1 规范化并排序趋势、排行和健康数据
  const chronologicalRows = normalizeTrendRows(trends);
  const recentRowsDesc = [...chronologicalRows].reverse();
  const spotlightRows = recentRowsDesc
    .slice(0, PANEL_LIMITS.trendSpotlight)
    .reverse();
  const tableRows = recentRowsDesc.slice(0, PANEL_LIMITS.trendTable);
  const modelRows = sortDesc(models, (row) => row?.total_tokens);
  const sourceRows = sortDesc(sources, (row) => row?.total_tokens);
  const projectRows = sortDesc(projects, (row) => row?.total_tokens);
  const costRows = sortDesc(costs, (row) => row?.estimated_cost_usd);
  const activityRows = sortDesc(activity?.breakdown, (row) => row?.turns);
  const toolRows = sortDesc(tools?.breakdown, (row) => row?.calls);
  const pricedCostRows = positiveRows(costRows, (row) => row?.estimated_cost_usd);
  const pricedModelRows = positiveRows(modelRows, (row) => row?.cost_with_cache_usd);
  const cursorRows = normalizeRows(health?.cursors);
  const diagnosticRows = normalizeRows(diagnostics?.by_source);
  const diagnosticFailureRows = normalizeRows(diagnostics?.recent_failures);
  const failureRows = normalizeRows(health?.recent_failures);
  const combinedFailureRows = failureRows.length ? failureRows : diagnosticFailureRows;
  const explorerPayload = normalizeExplorer(explorer);

  // 1.2 计算账本摘要、趋势聚合和健康聚合
  const trendTotal = chronologicalRows.reduce(
    (sum, row) => sum + Number(row?.total_tokens || 0),
    0,
  );
  const trendPeak = chronologicalRows.reduce((best, row) => {
    if (!best || Number(row?.total_tokens || 0) > Number(best?.total_tokens || 0)) {
      return row;
    }
    return best;
  }, null);
  const trendAverage = chronologicalRows.length ? Math.round(trendTotal / chronologicalRows.length) : 0;
  const trendActive = chronologicalRows.filter((row) => Number(row?.total_tokens || 0) > 0).length;
  const total_cost = costRows.reduce(
    (sum, row) => sum + Number(row?.estimated_cost_usd || 0),
    0,
  );
  const total_cache_savings = modelRows.reduce(
    (sum, row) => sum + Number(row?.cache_savings_usd || 0),
    0,
  );
  const cost_event_count = costRows.reduce(
    (sum, row) => sum + Number(row?.event_count || 0),
    0,
  );
  const average_cost_per_event = cost_event_count > 0 ? total_cost / cost_event_count : 0;
  const top_cost_row = pricedCostRows[0] || costRows[0] || null;
  const top_model_cost_row = pricedModelRows[0] || modelRows[0] || null;
  // 1.3 派生图表与表格数据，避免 render 层重复计算
  const model_table_rows = modelRows.slice(0, PANEL_LIMITS.modelTable).map((row) => {
    const output_tokens = Number(row.output_tokens || 0);
    return {
      model: row.model,
      total_tokens: formatTokenAmount(row.total_tokens),
      input_share: formatPercent(row.input_tokens, row.total_tokens),
      output_share: formatPercent(output_tokens, row.total_tokens),
      cached_share: formatPercent(row.cache_read_tokens, row.total_tokens),
    };
  });

  const source_table_rows = sourceRows.slice(0, PANEL_LIMITS.sourceTable).map((row) => ({
    source: row.source,
    last_event_at: row.last_event_at || '尚未记录',
    total_tokens: formatTokenAmount(row.total_tokens),
  }));

  const cost_table_rows = costRows.slice(0, PANEL_LIMITS.costTable).map((row) => ({
    model: row.model,
    source: row.source,
    estimated_cost_usd: formatUsd(row.estimated_cost_usd),
  }));

  const context = {
    overview: overview || {},
    ledgerSummary: {
      generated_at: overview?.generated_at,
      last_sync_at: overview?.last_sync_at,
      last_export_at: overview?.last_export_at,
      active_sources: overview?.source_count ?? sourceRows.length,
      failure_count: combinedFailureRows.length,
    },
    syncCommandCenter: normalizeSyncCommandCenter(sync_command_center),
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
      chronologicalRows,
      recentRowsDesc,
      spotlightRows,
      ledgerRows: recentRowsDesc,
      tableRows,
    },
    panels: {
      models: modelRows,
      model_table_rows,
      sources: sourceRows,
      source_table_rows,
      projects: projectRows,
      costs: costRows,
      cost_table_rows,
      activity: activityRows,
      tools: toolRows,
      optimize: optimize || { support: { supported: false, level: 'no_data' }, findings: [] },
      compare: compare || { support: { supported: false, level: 'no_data' }, candidates: [] },
      explorer: explorerPayload,
      home_overview: home_overview || null,
      heatmap: Array.isArray(heatmap) ? heatmap : normalizeRows(heatmap?.rows),
      heatmap_support: heatmap?.support || null,
      trends_daily: Array.isArray(trends_daily) ? trends_daily : normalizeRows(trends_daily?.rows),
      trends_daily_support: trends_daily?.support || null,
      top_sessions: top_sessions || null,
      hour_of_week: hour_of_week || null,
      activity_support: activity?.support || { supported: false, level: 'no_data' },
      tools_support: tools?.support || { supported: false, level: 'no_data' },
      secondary_refreshing: Boolean(_meta?.secondary_refreshing),
    },
    health: {
      cursors: cursorRows,
      cursor_count: Number(health?.cursor_count ?? cursorRows.length),
      failures: combinedFailureRows,
    },
    diagnostics: {
      archive_root: diagnostics?.archive_root || '',
      by_source: diagnosticRows,
      recent_failures: diagnosticFailureRows,
    },
    insights: buildInsights({
      overview,
      modelRows,
      projectRows,
      costRows,
      sourceRows,
      diagnosticsRows: diagnosticRows,
      failureRows: combinedFailureRows,
    }),
    totals: {
      total_tokens: Number(overview?.total?.total_tokens || 0),
      total_tokens_compact: formatCompact(overview?.total?.total_tokens || 0),
      total_tokens_raw: formatNumber(overview?.total?.total_tokens || 0),
      last_24h_tokens: Number(overview?.last_24h?.total_tokens || 0),
      last_24h_tokens_compact: formatCompact(overview?.last_24h?.total_tokens || 0),
      last_24h_tokens_raw: formatNumber(overview?.last_24h?.total_tokens || 0),
      total_cost,
      total_cost_compact: formatCompactCurrency(total_cost),
      total_cost_raw: formatUsd(total_cost),
      total_cache_savings,
      total_cache_savings_raw: formatUsd(total_cache_savings),
      average_cost_per_event,
      average_cost_per_event_raw: formatUsd(average_cost_per_event),
      cost_event_count,
      priced_cost_rows: pricedCostRows.length,
      top_cost_raw: top_cost_row ? formatUsd(top_cost_row.estimated_cost_usd) : '--',
      top_cost_label: top_cost_row ? `${top_cost_row.source || '--'} · ${top_cost_row.model || '--'}` : '--',
      top_model_cost_raw: top_model_cost_row ? formatUsd(top_model_cost_row.cost_with_cache_usd) : '--',
      top_model_cost_label: top_model_cost_row?.model || '--',
    },
  };

  logger.info('完成页面上下文构建');
  return context;
}

/*
 * ========================================================================
 * Ready widgets: summary cards and heatmap levels
 * ========================================================================
 */
export function buildSummaryCards(homeOverview) {
  const summary = homeOverview?.summary;
  if (!summary) return [];
  const cardCopy = UI_COPY.readyWidgets.summary;
  const platforms = Object.entries(homeOverview?.by_platform || {});
  const topPlatform = platforms.reduce((best, entry) => (
    !best || Number(entry[1]?.tokens || 0) > Number(best[1]?.tokens || 0) ? entry : best
  ), null);
  const sessions = Number(summary.total_sessions || 0);
  const requests = Number(summary.total_requests || 0);
  const requestAverage = sessions > 0 ? requests / sessions : 0;
  const cacheEfficiency = Math.max(0, Number(summary.cache_efficiency || 0));

  return [
    {
      label: cardCopy.sessions,
      value: formatNumber(sessions),
      sub: `${formatNumber(summary.platforms || 0)} ${cardCopy.platforms}`,
    },
    {
      label: cardCopy.requests,
      value: formatNumber(requests),
      sub: `${requestAverage.toFixed(1)} ${cardCopy.perSession}`,
    },
    {
      featured: true,
      label: cardCopy.tokens,
      value: formatTokenAmount(summary.total_tokens || 0),
      sub: topPlatform ? `${cardCopy.topPlatform}: ${topPlatform[0]}` : '--',
    },
    {
      label: cardCopy.cost,
      value: formatUsd(summary.total_cost_usd || 0),
      sub: '',
    },
    {
      label: cardCopy.activeDays,
      value: formatNumber(summary.active_days || 0),
      sub: cardCopy.currentRange,
    },
    {
      label: cardCopy.cacheEfficiency,
      value: `${(cacheEfficiency * 100).toFixed(1)}%`,
      sub: cardCopy.cacheHint,
    },
  ];
}

export function heatmapLevels(values) {
  const normalized = normalizeRows(values).map((value) => Math.max(0, Number(value || 0)));
  const nonzero = normalized.filter((value) => value > 0).sort((a, b) => a - b);
  if (!nonzero.length) return normalized.map(() => 0);
  if (nonzero[0] === nonzero[nonzero.length - 1]) {
    return normalized.map((value) => (value > 0 ? 4 : 0));
  }
  const quantile = (fraction) => nonzero[Math.max(0, Math.ceil(nonzero.length * fraction) - 1)];
  const [q25, q50, q75] = [quantile(0.25), quantile(0.5), quantile(0.75)];
  return normalized.map((value) => {
    if (value <= 0) return 0;
    if (value <= q25) return 1;
    if (value <= q50) return 2;
    if (value <= q75) return 3;
    return 4;
  });
}

/*
 * ========================================================================
 * 步骤3：构建趋势统计卡数据
 * ========================================================================
 * 目标：
 * 1) 为趋势区 3 个统计卡生成标题、数值、脚注
 * 2) 返回渲染就绪的数组
 */
export function buildTrendStats(context) {
  const { trend } = context;

  return [
    {
      label: '时间窗口总量',
      value: formatTokenAmount(trend.total),
      foot: `原始值 ${formatNumber(trend.total)}`,
    },
    {
      label: '最高单段用量',
      value: formatTokenAmount(trend.peak?.total_tokens || 0),
      foot: `最高时段 ${formatDateTime(trend.peak?.label)}`,
    },
    {
      label: '平均每段用量',
      value: formatTokenAmount(trend.average),
      foot: `${trend.active} 个有记录时段`,
    },
  ];
}

/*
 * ========================================================================
 * 步骤4：构建成本统计卡数据
 * ========================================================================
 * 目标：
 * 1) 为成本区 4 个统计卡生成标题、数值、脚注
 * 2) 返回渲染就绪的数组
 */
export function buildCostStats(context) {
  const { totals } = context;
  const hasCostData = totals.priced_cost_rows > 0 || totals.total_cost > 0;
  const cacheSavingsValue = totals.total_cache_savings > 0 ? totals.total_cache_savings_raw : '--';
  const cacheSavingsFoot = totals.total_cache_savings > 0
    ? '基于 cost_without_cache_usd 与真实缓存成本差值'
    : '暂无可估算的缓存节省数据';

  return [
    {
      label: '当前累计',
      value: hasCostData ? totals.total_cost_compact : '--',
      foot: `来自 ${totals.priced_cost_rows} 个有成本的来源/模型项`,
    },
    {
      label: '平均每事件',
      value: hasCostData ? totals.average_cost_per_event_raw : '--',
      foot: totals.cost_event_count > 0 ? `${formatNumber(totals.cost_event_count)} 个事件` : '暂无事件成本数据',
    },
    {
      label: '最高成本项',
      value: hasCostData ? totals.top_cost_raw : '--',
      foot: totals.top_cost_label,
    },
    {
      label: '缓存节省',
      value: cacheSavingsValue,
      foot: cacheSavingsFoot,
    },
  ];
}
