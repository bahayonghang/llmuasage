import { getShellCopy } from '../copy.js';
import { escapeHtml, formatNumber, formatTokenAmount, formatUsd, ratio } from '../data.js';

const logger = window.console;
const MAX_CHART_SERIES = 5;
const SERIES_TABLE_LIMIT = 80;
const MINI_CHART_WIDTH = 360;
const MINI_CHART_HEIGHT = 44;

function shellCopy(key, replacements = {}) {
  return Object.entries(replacements).reduce(
    (value, [name, replacement]) => value.split(`{${name}}`).join(String(replacement)),
    getShellCopy(key),
  );
}

function finiteNumber(value) {
  const number = Number(value || 0);
  return Number.isFinite(number) ? number : 0;
}

function supportLabel(support) {
  if (support?.supported && support?.level === 'normalized') {
    return 'normalized';
  }
  return support?.level || 'no_data';
}

function metricLabel(metric) {
  const key = {
    attributed_cost_usd: 'shell.explorer.metric.cost',
    calls: 'shell.explorer.metric.calls',
    turns: 'shell.explorer.metric.turns',
    sessions: 'shell.explorer.metric.sessions',
    total_tokens: 'shell.explorer.metric.tokens',
  }[metric];
  return key ? getShellCopy(key) : metric || '--';
}

function groupLabel(groupBy) {
  const key = {
    source: 'shell.explorer.group.source',
    model: 'shell.explorer.group.model',
    project: 'shell.explorer.group.project',
    session: 'shell.explorer.group.session',
    tool: 'shell.explorer.group.tool',
    tool_kind: 'shell.explorer.group.toolKind',
    is_tool: 'shell.explorer.group.isTool',
    token_type: 'shell.explorer.group.tokenType',
  }[groupBy];
  return key ? getShellCopy(key) : groupBy || '--';
}

function granularityLabel(granularity) {
  const key = {
    total: 'shell.explorer.granularity.total',
    day: 'shell.explorer.granularity.day',
    week: 'shell.explorer.granularity.week',
    month: 'shell.explorer.granularity.month',
  }[granularity];
  return key ? getShellCopy(key) : granularity || '--';
}

function formatMetric(metric, value) {
  if (metric === 'attributed_cost_usd') {
    return formatUsd(value);
  }
  if (metric === 'total_tokens') {
    return formatTokenAmount(value);
  }
  return formatNumber(value);
}

function emptyState(message) {
  return `<div class="empty-state">${escapeHtml(message)}</div>`;
}

function refreshNotice(refreshing) {
  return refreshing
    ? `<div class="empty-state stale-refresh-notice">${escapeHtml(getShellCopy('shell.refresh.secondaryStale'))}</div>`
    : '';
}

function renderSummary(explorer) {
  const rows = Array.isArray(explorer?.rows) ? explorer.rows : [];
  const series = Array.isArray(explorer?.series) ? explorer.series : [];
  const total = Number(explorer?.totals?.value || 0);
  const metric = explorer?.metric || 'attributed_cost_usd';
  return `
    <div class="mini-stat">
      <span>Metric</span>
      <strong>${escapeHtml(formatMetric(metric, total))}</strong>
      <small>${escapeHtml(metricLabel(metric))}</small>
    </div>
    <div class="mini-stat">
      <span>Group by</span>
      <strong>${escapeHtml(groupLabel(explorer?.group_by))}</strong>
      <small>${escapeHtml(granularityLabel(explorer?.granularity))}</small>
    </div>
    <div class="mini-stat">
      <span>Rows</span>
      <strong>${formatNumber(rows.length)}</strong>
      <small>${formatNumber(series.length)} series points</small>
    </div>
  `;
}

function renderBars(rows, metric) {
  if (!rows.length) {
    return '';
  }
  const max = Math.max(...rows.map((row) => Number(row?.value || 0)), 1);
  return rows
    .slice(0, 10)
    .map((row) => {
      const value = Number(row?.value || 0);
      const label = row?.label || row?.key || '--';
      return `
        <div class="bar-row ${row?.is_other ? 'is-other' : ''}">
          <div class="name" title="${escapeHtml(label)}">${escapeHtml(label)}</div>
          <div class="bar-track"><div class="bar-fill" style="width: ${ratio(value, max)}%"></div></div>
          <div class="num">${escapeHtml(formatMetric(metric, value))}</div>
        </div>
      `;
    })
    .join('');
}

function renderRowsTable(rows, explorer) {
  if (!rows.length) {
    return emptyState(explorer?.support?.reason || '暂无 Explorer 维度结果。');
  }
  const metric = explorer?.metric || 'attributed_cost_usd';
  const rowsHtml = rows
    .slice(0, 50)
    .map((row) => `
      <tr>
        <td class="name-cell">${escapeHtml(row?.label || row?.key || '--')}</td>
        <td>${escapeHtml(row?.key || '--')}</td>
        <td class="r">${escapeHtml(formatMetric(metric, row?.value))}</td>
        <td class="r">${formatNumber(Number(row?.share || 0) * 100)}%</td>
      </tr>
    `)
    .join('');
  return `
    <table class="panel-table">
      <thead>
        <tr>
          <th>维度</th>
          <th>Key</th>
          <th class="r">${escapeHtml(metricLabel(metric))}</th>
          <th class="r">占比</th>
        </tr>
      </thead>
      <tbody>${rowsHtml}</tbody>
    </table>
  `;
}

function renderSeriesTable(series, explorer) {
  const metric = explorer?.metric || 'attributed_cost_usd';
  const visible = series.slice(-SERIES_TABLE_LIMIT);
  const rowsHtml = visible
    .map((point) => `
      <tr>
        <td class="name-cell">${escapeHtml(point?.bucket || '--')}</td>
        <td title="${escapeHtml(point?.label || point?.key || '--')}">${escapeHtml(point?.label || point?.key || '--')}</td>
        <td class="r">${escapeHtml(formatMetric(metric, finiteNumber(point?.value)))}</td>
      </tr>
    `)
    .join('');
  const truncated = series.length > SERIES_TABLE_LIMIT
    ? `<div class="explorer-series-truncation">${escapeHtml(shellCopy('shell.explorer.seriesTruncated', {
        shown: formatNumber(visible.length),
        total: formatNumber(series.length),
      }))}</div>`
    : '';
  return `
    ${truncated}
    <div class="explorer-series-scroll">
      <table class="panel-table explorer-series-table">
        <thead>
          <tr>
            <th>${escapeHtml(getShellCopy('shell.explorer.table.time'))}</th>
            <th>${escapeHtml(getShellCopy('shell.explorer.table.dimension'))}</th>
            <th class="r">${escapeHtml(metricLabel(metric))}</th>
          </tr>
        </thead>
        <tbody>${rowsHtml}</tbody>
      </table>
    </div>
  `;
}

function seriesBuckets(series) {
  return [...new Set(
    series
      .map((point) => String(point?.bucket || '').trim())
      .filter(Boolean),
  )].sort((left, right) => left.localeCompare(right));
}

function formatBucketRange(buckets) {
  if (!buckets.length) return '--';
  if (buckets.length === 1) return buckets[0];
  return `${buckets[0]} - ${buckets[buckets.length - 1]}`;
}

function compactBucketLabel(bucket) {
  const raw = String(bucket || '--');
  if (/^\d{4}-\d{2}-\d{2}$/.test(raw)) return raw.slice(5);
  return raw.length > 12 ? raw.slice(0, 12) : raw;
}

function buildChartSeries(rows, series, buckets) {
  const candidates = new Map();
  for (const point of series) {
    const key = String(point?.key ?? '');
    const current = candidates.get(key) || {
      key,
      label: point?.label || point?.key || '--',
      total: 0,
      valuesByBucket: new Map(),
    };
    const value = finiteNumber(point?.value);
    current.total += value;
    current.valuesByBucket.set(String(point?.bucket || ''), value);
    candidates.set(key, current);
  }

  for (const row of rows) {
    const key = String(row?.key ?? '');
    const candidate = candidates.get(key);
    if (!candidate) continue;
    candidate.label = row?.label || row?.key || candidate.label;
    candidate.total = finiteNumber(row?.value);
  }

  return [...candidates.values()]
    .sort((left, right) => right.total - left.total || left.label.localeCompare(right.label))
    .slice(0, MAX_CHART_SERIES)
    .map((candidate) => ({
      ...candidate,
      values: buckets.map((bucket) => candidate.valuesByBucket.get(bucket) || 0),
    }));
}

function miniChartGeometry(values) {
  const insetX = 4;
  const top = 4;
  const baseline = MINI_CHART_HEIGHT - 4;
  const peak = Math.max(0, ...values);
  const scale = peak > 0 ? peak : 1;
  const span = Math.max(1, values.length - 1);
  const points = values.map((value, index) => {
    const x = values.length === 1
      ? MINI_CHART_WIDTH / 2
      : insetX + (index / span) * (MINI_CHART_WIDTH - insetX * 2);
    const y = baseline - (value / scale) * (baseline - top);
    return { x, y, value };
  });
  const peakIndex = values.findIndex((value) => value === peak);
  return {
    baseline,
    peak,
    peakPoint: points[Math.max(0, peakIndex)],
    polyline: points.map(({ x, y }) => `${x.toFixed(2)},${y.toFixed(2)}`).join(' '),
  };
}

function renderBucketTicks(buckets) {
  const indexes = buckets.length <= 3
    ? buckets.map((_bucket, index) => index)
    : [0, Math.floor((buckets.length - 1) / 2), buckets.length - 1];
  return `
    <div class="explorer-series-axis" style="--tick-count: ${indexes.length}">
      ${indexes.map((index) => `<span>${escapeHtml(compactBucketLabel(buckets[index]))}</span>`).join('')}
    </div>
  `;
}

function renderSeriesChart(rows, series, explorer) {
  if (explorer?.granularity === 'total') {
    return emptyState(getShellCopy('shell.explorer.seriesTotalEmpty'));
  }
  if (!series.length) {
    return emptyState(explorer?.support?.reason || getShellCopy('shell.explorer.seriesEmpty'));
  }

  const metric = explorer?.metric || 'attributed_cost_usd';
  const buckets = seriesBuckets(series);
  const range = formatBucketRange(buckets);
  const chartSeries = buildChartSeries(rows, series, buckets);
  const availableSeriesCount = new Set(series.map((point) => String(point?.key ?? ''))).size;
  if (!buckets.length || !chartSeries.length) {
    return emptyState(explorer?.support?.reason || getShellCopy('shell.explorer.seriesEmpty'));
  }

  const seriesRows = chartSeries.map((entry) => {
    const geometry = miniChartGeometry(entry.values);
    const peakValue = formatMetric(metric, geometry.peak);
    const aria = shellCopy('shell.explorer.seriesAria', {
      label: entry.label,
      range,
      value: peakValue,
    });
    return `
      <div class="explorer-series-row">
        <div class="explorer-series-name" title="${escapeHtml(entry.label)}">${escapeHtml(entry.label)}</div>
        <svg class="explorer-series-plot" viewBox="0 0 ${MINI_CHART_WIDTH} ${MINI_CHART_HEIGHT}" preserveAspectRatio="none" role="img" aria-label="${escapeHtml(aria)}">
          <line class="explorer-series-baseline" x1="4" y1="${geometry.baseline}" x2="${MINI_CHART_WIDTH - 4}" y2="${geometry.baseline}"></line>
          <polyline class="explorer-series-line" points="${geometry.polyline}"></polyline>
          <circle class="explorer-series-peak-dot" cx="${geometry.peakPoint.x.toFixed(2)}" cy="${geometry.peakPoint.y.toFixed(2)}" r="3"></circle>
        </svg>
        <div class="explorer-series-peak">
          <span>${escapeHtml(getShellCopy('shell.explorer.seriesPeak'))}</span>
          <strong>${escapeHtml(peakValue)}</strong>
        </div>
      </div>
    `;
  }).join('');

  return `
    <div class="explorer-series-chart-card">
      <div class="explorer-series-chart-meta">
        <span>${escapeHtml(shellCopy(
          availableSeriesCount > chartSeries.length
            ? 'shell.explorer.seriesScope'
            : 'shell.explorer.seriesScopeAll',
          { shown: formatNumber(chartSeries.length) },
        ))}</span>
        <span class="explorer-series-scale">${escapeHtml(getShellCopy('shell.explorer.seriesIndependentScale'))}</span>
      </div>
      <div class="explorer-series-rows">${seriesRows}</div>
      <div class="explorer-series-axis-grid" aria-hidden="true">
        <span></span>
        ${renderBucketTicks(buckets)}
        <span></span>
      </div>
    </div>
  `;
}

function renderSeriesDetails(series, explorer, open) {
  if (explorer?.granularity === 'total' || !series.length) return '';
  const buckets = seriesBuckets(series);
  const range = formatBucketRange(buckets);
  const meta = shellCopy('shell.explorer.seriesDetailsMeta', {
    count: formatNumber(series.length),
    range,
  });
  return `
    <details class="explorer-series-details" ${open ? 'open' : ''}>
      <summary>
        <span>${escapeHtml(getShellCopy('shell.explorer.seriesDetails'))}</span>
        <span>${escapeHtml(meta)}</span>
      </summary>
      <div class="explorer-series-detail-body">
        ${renderSeriesTable(series, explorer)}
      </div>
    </details>
  `;
}

/*
 * ========================================================================
 * 步骤1：渲染 Cost Explorer 工作台
 * ========================================================================
 * 目标：
 * 1) 展示后端 Explorer payload，而不是在前端透视原始行
 * 2) 显示 support / degraded / no_data / unsupported 状态
 * 3) 在 live 和 snapshot/export 模式下复用同一渲染路径
 */
export function renderExplorer(context, _state = {}) {
  logger.info('开始渲染 Cost Explorer 工作台');

  const explorer = context?.panels?.explorer || {};
  const rows = Array.isArray(explorer.rows) ? explorer.rows : [];
  const series = Array.isArray(explorer.series) ? explorer.series : [];
  const support = explorer.support || { supported: false, level: 'no_data' };
  const warning = explorer.warning || support.reason || '';
  const refreshing = Boolean(context?.panels?.secondary_refreshing);

  const supportEl = document.getElementById('explorer-support');
  if (supportEl) {
    supportEl.textContent = refreshing ? 'refreshing' : supportLabel(support);
    supportEl.dataset.level = refreshing ? 'refreshing' : support.level || 'no_data';
    supportEl.title = support.reason || support.strategy || '';
  }

  const summary = document.getElementById('explorer-summary');
  if (summary) {
    summary.innerHTML = renderSummary(explorer);
  }

  const warningHost = document.getElementById('explorer-warning');
  if (warningHost) {
    warningHost.innerHTML = `${refreshNotice(refreshing)}${
      warning ? `<div class="empty-state explorer-warning">${escapeHtml(warning)}</div>` : ''
    }`;
  }

  const bars = document.getElementById('explorer-bars');
  if (bars) {
    bars.innerHTML = renderBars(rows, explorer.metric);
  }

  const rowsHost = document.getElementById('explorer-rows');
  if (rowsHost) {
    rowsHost.innerHTML = renderRowsTable(rows, explorer);
  }

  const chartHost = document.getElementById('explorer-series-chart');
  if (chartHost) {
    chartHost.innerHTML = renderSeriesChart(rows, series, explorer);
  }

  const detailsHost = document.getElementById('explorer-series-details');
  if (detailsHost) {
    const wasOpen = Boolean(detailsHost.querySelector('details')?.open);
    detailsHost.innerHTML = renderSeriesDetails(series, explorer, wasOpen);
  }

  logger.info('完成 Cost Explorer 工作台渲染');
}
