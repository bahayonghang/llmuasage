import { getShellCopy } from '../copy.js';
import { escapeHtml, formatNumber, formatTokenAmount, formatUsd, ratio } from '../data.js';

const logger = window.console;

function supportLabel(support) {
  if (support?.supported && support?.level === 'normalized') {
    return 'normalized';
  }
  return support?.level || 'no_data';
}

function metricLabel(metric) {
  return {
    attributed_cost_usd: '归因成本',
    calls: '调用数',
    turns: 'Turns',
    sessions: '会话数',
    total_tokens: '总 Token',
  }[metric] || metric || '--';
}

function groupLabel(groupBy) {
  return {
    source: '来源',
    model: '模型',
    project: '项目',
    session: '会话',
    tool: '工具',
    tool_kind: '工具类型',
    is_tool: '工具/非工具',
    token_type: 'Token 类型',
  }[groupBy] || groupBy || '--';
}

function granularityLabel(granularity) {
  return {
    total: '总计',
    day: '按日',
    week: '按周',
    month: '按月',
  }[granularity] || granularity || '--';
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
  if (explorer?.granularity === 'total') {
    return emptyState('当前粒度为总计，不返回时间序列。');
  }
  if (!series.length) {
    return emptyState(explorer?.support?.reason || '暂无 Explorer 时间序列。');
  }
  const metric = explorer?.metric || 'attributed_cost_usd';
  const rowsHtml = series
    .slice(0, 80)
    .map((point) => `
      <tr>
        <td class="name-cell">${escapeHtml(point?.bucket || '--')}</td>
        <td>${escapeHtml(point?.label || point?.key || '--')}</td>
        <td class="r">${escapeHtml(formatMetric(metric, point?.value))}</td>
      </tr>
    `)
    .join('');
  return `
    <table class="panel-table explorer-series-table">
      <thead>
        <tr>
          <th>时间</th>
          <th>维度</th>
          <th class="r">${escapeHtml(metricLabel(metric))}</th>
        </tr>
      </thead>
      <tbody>${rowsHtml}</tbody>
    </table>
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

  const seriesHost = document.getElementById('explorer-series');
  if (seriesHost) {
    seriesHost.innerHTML = renderSeriesTable(series, explorer);
  }

  logger.info('完成 Cost Explorer 工作台渲染');
}
