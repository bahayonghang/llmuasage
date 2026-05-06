import { escapeHtml, formatNumber, formatCompact } from '../data.js';
import { buildTrendStats } from '../data/derive.js';

const logger = window.console;

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

  // 1.1 填充 3 个趋势统计卡
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

  // 1.2 绘制趋势图表
  const spotlightRows = context.trend.spotlightRows || [];
  const max = Math.max(...spotlightRows.map((row) => Number(row.total_tokens || 0)));

  const W = 720;
  const H = 220;
  const baseline = 200;
  const colW = W / spotlightRows.length;
  const barW = colW * 0.55;

  let bars = '';
  let labels = '';

  spotlightRows.forEach((row, i) => {
    const value = Number(row.total_tokens || 0);
    const h = (value / max) * (baseline - 20);
    const x = i * colW + (colW - barW) / 2;
    const y = baseline - h;
    const isMax = value === max;

    bars += `<rect x="${x}" y="${y}" width="${barW}" height="${h}" rx="3" fill="${isMax ? '#e87155' : '#c8553d'}" opacity="${isMax ? 1 : 0.85}"/>`;

    if (isMax) {
      bars += `<text x="${x + barW / 2}" y="${y - 6}" fill="#f5a890" font-family="JetBrains Mono" font-size="10" text-anchor="middle">${(value / 1e6).toFixed(1)}M</text>`;
    }

    const timeLabel = row.time_bucket ? row.time_bucket.slice(11, 16) : '--';
    labels += `<text x="${x + barW / 2}" y="216" text-anchor="middle">${timeLabel}</text>`;
  });

  document.getElementById('trends-bars').innerHTML = bars;
  document.getElementById('trends-labels').innerHTML = labels;

  // 1.3 填充趋势表格
  const tableRows = (context.trend.tableRows || [])
    .map(
      (row) => `
      <tr>
        <td>${escapeHtml(row.time_bucket || '--')}</td>
        <td class="r">${formatNumber(row.input_tokens || 0)}</td>
        <td class="r">${formatNumber(row.output_tokens || 0)}</td>
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
          <th class="r">输入 Token</th>
          <th class="r">输出 Token</th>
          <th class="r">总用量</th>
        </tr>
      </thead>
      <tbody>
        ${tableRows}
      </tbody>
    </table>
  `;

  // 1.4 填充来源分布
  const sourceRows = (context.panels.sources || [])
    .slice(0, 2)
    .map((row) => {
      const totalTokens = Number(row.total_tokens || 0);
      const maxTokens = Number(context.panels.sources[0]?.total_tokens || 1);
      const widthPct = ((totalTokens / maxTokens) * 100).toFixed(1);
      const sharePct = ((totalTokens / context.totals.totalTokens) * 100).toFixed(1);

      return `
        <tr>
          <td>${escapeHtml(row.source || '--')}</td>
          <td class="r">${formatNumber(totalTokens)}</td>
          <td class="r">${sharePct}%</td>
        </tr>
      `;
    })
    .join('');

  document.getElementById('trends-sources').innerHTML = `
    <table class="data-table">
      <thead>
        <tr><th>来源</th><th class="r">条数</th><th class="r">占比</th></tr>
      </thead>
      <tbody>
        ${sourceRows}
      </tbody>
    </table>
    <div style="background: rgba(200,85,61,0.08); border: 1px solid rgba(200,85,61,0.25); border-radius: 10px; padding: 14px 16px; margin-top: 14px;">
      <div style="font-size: 10.5px; color: var(--accent); letter-spacing: 0.12em; text-transform: uppercase; font-weight: 600; font-family: 'JetBrains Mono', monospace; margin-bottom: 6px;">观察</div>
      <div style="font-size: 12.5px; color: var(--dark-text); line-height: 1.5;">02:00–02:30 出现明显峰值，单段用量超过 17M Token，主要来自 <span class="mono" style="color: #f5a890">codex</span> 的 <span class="mono" style="color: #f5a890">gpt-5.4</span>。</div>
    </div>
  `;

  logger.info('完成趋势区渲染');
}
