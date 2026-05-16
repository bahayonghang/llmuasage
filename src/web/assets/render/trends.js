import { escapeHtml, formatNumber, formatCompact } from '../data.js';
import { buildTrendStats } from '../data/derive.js';

const logger = window.console;

function compactTrendLabel(label) {
  const raw = String(label || '--');
  if (raw.includes('T')) {
    return raw.slice(11, 16);
  }
  if (/^\d{4}-\d{2}-\d{2}$/.test(raw)) {
    return raw.slice(5);
  }
  return raw;
}

/*
 * ========================================================================
 * 步骤1：渲染趋势区
 * ========================================================================
 * 目标：
 * 1) 填充 3 个趋势统计卡
 * 2) 绘制趋势图表（最近 10 个时段）
 * 3) 填充趋势表格和来源分布
 */
export function renderTrends(context) {
  logger.info('开始渲染趋势区');

  const stats = buildTrendStats(context);
  document.getElementById('trends-stats').innerHTML = stats
    .map(
      (stat) => `
      <div class="trends-stat">
        <div class="trends-stat-label">${escapeHtml(stat.label)}</div>
        <div class="trends-stat-value">${escapeHtml(stat.value)}</div>
        <div class="trends-stat-foot">${escapeHtml(stat.foot)}</div>
      </div>
    `,
    )
    .join('');

  const spotlightRows = context.trend.spotlightRows || [];
  const max = Math.max(1, ...spotlightRows.map((row) => Number(row.total_tokens || 0)));

  const W = 720;
  const baseline = 200;
  const colW = spotlightRows.length ? W / spotlightRows.length : W;
  const barW = Math.min(46, Math.max(14, colW * 0.58));

  let bars = '';
  let labels = '';

  if (!spotlightRows.length) {
    bars = `
      <text class="trend-empty-title" x="${W / 2}" y="96" text-anchor="middle">暂无趋势数据</text>
      <text class="trend-empty-copy" x="${W / 2}" y="118" text-anchor="middle">运行同步后将显示最近 10 个时段</text>
    `;
  } else {
    spotlightRows.forEach((row, i) => {
      const value = Number(row.total_tokens || 0);
      const h = (value / max) * (baseline - 24);
      const x = i * colW + (colW - barW) / 2;
      const y = baseline - h;
      const isMax = value === max;
      const valueLabel = formatCompact(value);
      const timeLabel = compactTrendLabel(row.label);

      bars += `
        <g class="trend-bar-group" aria-label="${escapeHtml(`${row.label || '--'} · ${valueLabel} Token`)}">
          <rect class="trend-bar-hit" x="${x - 4}" y="24" width="${barW + 8}" height="${baseline - 24}" rx="8"></rect>
          <rect class="trend-bar ${isMax ? 'is-peak' : ''}" x="${x}" y="${y}" width="${barW}" height="${h}" rx="5"></rect>
          <title>${escapeHtml(`${row.label || '--'} · ${formatNumber(value)} Token`)}</title>
        </g>
      `;

      if (isMax) {
        bars += `<text class="trend-peak-label" x="${x + barW / 2}" y="${Math.max(18, y - 7)}" text-anchor="middle">${escapeHtml(valueLabel)}</text>`;
      }

      labels += `<text class="trend-axis-label" x="${x + barW / 2}" y="216" text-anchor="middle">${escapeHtml(timeLabel)}</text>`;
    });
  }

  document.getElementById('trends-bars').innerHTML = bars;
  document.getElementById('trends-labels').innerHTML = labels;

  const tableRows = (context.trend.tableRows || [])
    .map(
      (row) => `
      <tr>
        <td>${escapeHtml(row.label || '--')}</td>
        <td class="r">${formatNumber(row.total_tokens || 0)}</td>
      </tr>
    `,
    )
    .join('');

  document.getElementById('trends-table').innerHTML = `
    <table class="data-table">
      <thead>
        <tr>
          <th>时间</th>
          <th class="r">总用量</th>
        </tr>
      </thead>
      <tbody>
        ${tableRows}
      </tbody>
    </table>
  `;

  const sourceRows = (context.panels.sources || [])
    .slice(0, 2)
    .map((row) => {
      const total_tokens = Number(row.total_tokens || 0);
      const sharePct = context.totals.total_tokens
        ? ((total_tokens / context.totals.total_tokens) * 100).toFixed(1)
        : '0.0';

      return `
        <tr>
          <td>${escapeHtml(row.source || '--')}</td>
          <td class="r">${formatNumber(total_tokens)}</td>
          <td class="r">${sharePct}%</td>
        </tr>
      `;
    })
    .join('');

  document.getElementById('trends-sources').innerHTML = `
    <table class="data-table">
      <thead>
        <tr><th>来源</th><th class="r">Token</th><th class="r">占比</th></tr>
      </thead>
      <tbody>
        ${sourceRows}
      </tbody>
    </table>
    <div style="background: rgba(200,85,61,0.08); border: 1px solid rgba(200,85,61,0.25); border-radius: 10px; padding: 14px 16px; margin-top: 14px;">
      <div style="font-size: 10.5px; color: var(--accent); letter-spacing: 0.12em; text-transform: uppercase; font-weight: 600; font-family: 'JetBrains Mono', monospace; margin-bottom: 6px;">观察</div>
      <div style="font-size: 12.5px; color: var(--dark-text); line-height: 1.5;">当前窗口峰值出现在 <span class="mono" style="color: #f5a890">${escapeHtml(context.trend.peak?.label || '--')}</span>，总用量约 <span class="mono" style="color: #f5a890">${escapeHtml(formatCompact(context.trend.peak?.total_tokens || 0))}</span> Token；当前主来源为 <span class="mono" style="color: #f5a890">${escapeHtml(context.leaders.source?.source || '--')}</span>。</div>
    </div>
  `;

  logger.info('完成趋势区渲染');
}
