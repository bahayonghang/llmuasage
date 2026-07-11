import {
  formatCompact,
  formatCompactCurrency,
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
      tone: 'warn',
      label: '缓存线索',
      title: '缓存复用率偏低',
      evidence: `当前窗口 cache efficiency ${(cacheEfficiency * 100).toFixed(1)}%。`,
      action: '可检查提示复用、长上下文缓存或模型缓存支持；这是线索，不是最终诊断。',
    });
  }

  if (pricingConcernRows.length > 0) {
    const row = pricingConcernRows[0];
    insights.push({
      tone: 'warn',
      label: '定价可靠性',
      title: '存在 mixed / unpriced 成本项',
      evidence: `${pricingConcernRows.length} 个模型聚合项含不完整定价，示例 ${row.model || '--'} · ${row.pricing_status || '--'}。`,
      action: '成本估算可用于趋势判断；对账前先刷新 pricing snapshot 或检查未命中模型。',
    });
  }

  if (failureRows.length > 0) {
    const row = failureRows[0];
    insights.push({
      tone: 'warn',
      label: '同步失败',
      title: '最近有失败运行',
      evidence: `${failureRows.length} 条失败记录，最近命令 ${row.command || '--'}。`,
      action: '打开最近失败详情或重新运行 sync；完成后 Dashboard 会刷新当前筛选窗口。',
    });
  }

  if (lossyRows.length > 0) {
    const row = lossyRows[0];
    insights.push({
      tone: 'warn',
      label: '保留边界',
      title: '检测到 lossy rebuild 风险',
      evidence: `${row.source || '--'} 缺失 ${formatNumber(row.missing_file_count)} 个源文件，默认保护 ${formatNumber(row.protected_event_count)} 条已导入事件。`,
      action: '普通 sync 不会删除已导入历史；只有 sync --rebuild 可能触发保护，除非显式 allow-lossy-rebuild。',
    });
  } else if (missingRows.length > 0) {
    const row = missingRows[0];
    insights.push({
      tone: 'neutral',
      label: '源文件状态',
      title: '有源文件缺失记录',
      evidence: `${row.source || '--'} 当前记录 ${formatNumber(row.missing_file_count || row.missing_files)} 个缺失文件。`,
      action: '这通常只影响 diagnostics；普通 sync 会保留已导入 usage 历史。',
    });
  }

  if (staleRows.length > 0) {
    const row = staleRows[0];
    insights.push({
      tone: 'neutral',
      label: '来源新鲜度',
      title: '某些来源近期无新事件',
      evidence: `${row.source || '--'} 最近事件 ${row.last_event_at || '--'}。`,
      action: '如果该来源仍在使用，可检查 hook/integration 是否启用。',
    });
  }

  if (topCost) {
    insights.push({
      tone: 'good',
      label: '成本主因',
      title: '当前窗口主要成本来源',
      evidence: `${topCost.source || '--'} · ${topCost.model || '--'} 约 ${formatUsd(topCost.estimated_cost_usd)}。`,
      action: '优先从这个来源/模型组合排查成本变化。',
    });
  } else if (topModel && Number(topModel.total_tokens || 0) > 0) {
    insights.push({
      tone: 'neutral',
      label: '用量主因',
      title: '当前窗口主要模型来源',
      evidence: `${topModel.model || '--'} 使用 ${formatTokenAmount(topModel.total_tokens)} tokens。`,
      action: '无可用成本时，先用 token 排名定位主要消耗。',
    });
  }

  if (topProject && Number(topProject.total_tokens || 0) > 0) {
    insights.push({
      tone: 'neutral',
      label: '项目聚焦',
      title: '当前窗口主项目',
      evidence: `${topProject.project_label || topProject.project_hash || '--'} 使用 ${formatTokenAmount(topProject.total_tokens)} tokens。`,
      action: '若要降低用量，先从该项目的会话模式和模型选择入手。',
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
 * 2) 固定 Top N、图表序列和对比表行
 * 3) 为各面板补齐总量、峰值、占比和紧凑显示值
 */
export function buildContext({ overview, trends, models, sources, projects, costs, activity, tools, optimize, compare, explorer, health, diagnostics, sync_command_center, _meta }) {
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
  const integrationRows = normalizeRows(health?.integrations);
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
  const ready_integrations = integrationRows.filter(
    (row) => statusTone(row?.status) === 'good',
  ).length;

  // 1.3 派生图表与表格数据，避免 render 层重复计算
  const model_table_rows = modelRows.slice(0, PANEL_LIMITS.modelTable).map((row) => {
    const output_tokens = Number(row.output_tokens || 0) + Number(row.reasoning_output_tokens || 0);
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
      activity_support: activity?.support || { supported: false, level: 'no_data' },
      tools_support: tools?.support || { supported: false, level: 'no_data' },
      secondary_refreshing: Boolean(_meta?.secondary_refreshing),
    },
    health: {
      integrations: integrationRows,
      cursors: cursorRows,
      cursor_count: Number(health?.cursor_count ?? cursorRows.length),
      failures: combinedFailureRows,
      ready_integrations,
      total_integrations: integrationRows.length,
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
 * 步骤2：构建 KPI 卡片数据
 * ========================================================================
 * 目标：
 * 1) 为 4 个 KPI 卡生成标题、数值、单位、脚注
 * 2) 标记 featured 卡（总用量）
 * 3) 返回渲染就绪的数组
 */
export function buildKpis(context) {
  const { totals, ledgerSummary, leaders } = context;

  return [
    {
      featured: true,
      label: '总用量 · TOTAL',
      value: totals.total_tokens_compact,
      unit: '',
      foot: [
        `累计 Token · ${totals.total_tokens_raw}`,
        `用量最高模型 · ${leaders.model?.model || '--'}`,
      ],
    },
    {
      label: '近 24 小时',
      value: totals.last_24h_tokens_compact,
      unit: '',
      foot: [
        `原始值 · ${totals.last_24h_tokens_raw}`,
        `平均每段 · ${formatTokenAmount(context.trend.average)} / ${context.trend.active} 段`,
      ],
    },
    {
      label: '来源数',
      value: String(ledgerSummary.active_sources),
      unit: '',
      foot: [
        `主要来源 · ${leaders.source?.source || '--'}`,
        `最近记录 · ${leaders.source?.last_event_at || '--'}`,
      ],
    },
    {
      label: '估算成本',
      value: totals.total_cost_compact,
      unit: '',
      foot: [
        `累计成本 · ${totals.total_cost_raw}`,
        `最高 · ${leaders.cost?.source || '--'} · ${leaders.cost?.model || '--'}`,
      ],
    },
  ];
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
      foot: `最高时段 ${trend.peak?.label || '--'}`,
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

/*
 * ========================================================================
 * 步骤5：构建 sparkline SVG 路径
 * ========================================================================
 * 目标：
 * 1) 为 KPI 卡右上角生成简单折线图
 * 2) 输入数据点数组，输出 SVG polyline points 字符串
 */
export function buildSparkline(data, width = 62, height = 22) {
  if (!data || data.length === 0) return '';

  const max = Math.max(...data);
  const min = Math.min(...data);
  const range = max - min || 1;

  const points = data
    .map((value, index) => {
      const x = (index / (data.length - 1)) * width;
      const y = height - ((value - min) / range) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');

  return points;
}
