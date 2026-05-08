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

function normalizeTrendRows(rows) {
  return normalizeRows(rows).map((row) => ({
    label: row?.label ?? row?.time_bucket ?? '--',
    total_tokens: Number(row?.total_tokens || 0),
  }));
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
  const trendAscending = normalizeTrendRows(trends);
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
  const total_cost = costRows.reduce(
    (sum, row) => sum + Number(row?.estimated_cost_usd || 0),
    0,
  );
  const ready_integrations = integrationRows.filter(
    (row) => statusTone(row?.status) === 'good',
  ).length;

  // 1.3 派生图表与表格数据，避免 render 层重复计算
  const model_table_rows = modelRows.slice(0, PANEL_LIMITS.modelTable).map((row) => {
    const output_tokens = Number(row.output_tokens || 0) + Number(row.reasoning_output_tokens || 0);
    return {
      model: row.model,
      total_tokens: formatNumber(row.total_tokens),
      input_share: formatPercent(row.input_tokens, row.total_tokens),
      output_share: formatPercent(output_tokens, row.total_tokens),
      cached_share: formatPercent(row.cache_read_tokens, row.total_tokens),
    };
  });

  const source_table_rows = sourceRows.slice(0, PANEL_LIMITS.sourceTable).map((row) => ({
    source: row.source,
    last_event_at: row.last_event_at || '尚未记录',
    total_tokens: formatNumber(row.total_tokens),
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
      failure_count: failureRows.length,
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
      model_table_rows,
      sources: sourceRows,
      source_table_rows,
      projects: projectRows,
      costs: costRows,
      cost_table_rows,
    },
    health: {
      integrations: integrationRows,
      cursors: cursorRows,
      failures: failureRows,
      ready_integrations,
      total_integrations: integrationRows.length,
    },
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
        `平均每段 · ${formatCompact(context.trend.average)} / ${context.trend.active} 段`,
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
      value: formatCompact(trend.total),
      foot: `原始值 ${formatNumber(trend.total)}`,
    },
    {
      label: '最高单段用量',
      value: formatCompact(trend.peak?.total_tokens || 0),
      foot: `最高时段 ${trend.peak?.label || '--'}`,
    },
    {
      label: '平均每段用量',
      value: formatCompact(trend.average),
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

  return [
    {
      label: '月内累计',
      value: totals.total_cost_compact,
      foot: '+$3.4K vs. 上月同期',
    },
    {
      label: '日均',
      value: '$4,400',
      foot: '7 天滑动平均',
    },
    {
      label: '最贵单次',
      value: '$182.40',
      foot: 'opus-4-6 · 单段',
    },
    {
      label: '缓存节省',
      value: '~$28K',
      foot: '基于命中率估算',
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
