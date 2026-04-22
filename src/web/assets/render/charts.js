import { escapeHtml, formatCompact, formatNumber, ratio, shortLabel } from '../data.js';
import { UI_COPY, resolveTrendWindowCopy } from '../copy.js';

const logger = window.console;

function renderEmptyState(message) {
  return `<div class="empty-state">${escapeHtml(message)}</div>`;
}

function renderVerticalBarChart(rows, valueAccessor, labelAccessor, options = {}) {
  const safeRows = Array.isArray(rows) ? rows : [];
  if (!safeRows.length) return renderEmptyState(options.empty || '暂无图表数据。');

  const width = 640;
  const height = 220;
  const paddingX = 22;
  const paddingTop = 16;
  const paddingBottom = 28;
  const chartWidth = width - paddingX * 2;
  const chartHeight = height - paddingTop - paddingBottom;
  const maxValue = Math.max(1, ...safeRows.map((row) => Number(valueAccessor(row) || 0)));
  const step = chartWidth / safeRows.length;
  const barWidth = Math.max(12, step * 0.56);

  const bars = safeRows
    .map((row, index) => {
      const value = Number(valueAccessor(row) || 0);
      const scaled = ratio(value, maxValue) / 100;
      const barHeight = chartHeight * scaled;
      const x = paddingX + step * index + (step - barWidth) / 2;
      const y = paddingTop + chartHeight - barHeight;
      const label = shortLabel(labelAccessor(row), 10);
      return `
        <rect class="chart-bar" x="${x}" y="${y}" width="${barWidth}" height="${barHeight}" rx="7" ry="7" />
        <text class="chart-label" x="${x + barWidth / 2}" y="${height - 10}" text-anchor="middle">${escapeHtml(label)}</text>
      `;
    })
    .join('');

  return `
    <div class="chart-shell chart-shell--dark">
      <div class="chart-caption">${escapeHtml(options.caption || '')}</div>
      <div class="chart-frame">
        <svg class="chart-svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeHtml(options.ariaLabel || '柱状图')}">
          <line class="chart-axis" x1="${paddingX}" y1="${paddingTop + chartHeight}" x2="${width - paddingX}" y2="${paddingTop + chartHeight}" />
          ${bars}
        </svg>
      </div>
    </div>
  `;
}

function renderHorizontalBarChart(rows, valueAccessor, labelAccessor, options = {}) {
  const safeRows = Array.isArray(rows) ? rows : [];
  if (!safeRows.length) return renderEmptyState(options.empty || '暂无图表数据。');

  const width = 640;
  const rowHeight = 34;
  const height = 18 + safeRows.length * rowHeight;
  const labelWidth = 180;
  const valueWidth = 72;
  const chartWidth = width - labelWidth - valueWidth - 36;
  const maxValue = Math.max(1, ...safeRows.map((row) => Number(valueAccessor(row) || 0)));

  const rowsSvg = safeRows
    .map((row, index) => {
      const value = Number(valueAccessor(row) || 0);
      const widthRatio = ratio(value, maxValue);
      const y = 18 + index * rowHeight;
      const barWidth = Math.max(10, (chartWidth * widthRatio) / 100);
      return `
        <text class="chart-label" x="0" y="${y + 11}">${escapeHtml(shortLabel(labelAccessor(row), 28))}</text>
        <rect class="chart-bar-soft" x="${labelWidth}" y="${y}" width="${chartWidth}" height="12" rx="6" ry="6" />
        <rect class="chart-bar" x="${labelWidth}" y="${y}" width="${barWidth}" height="12" rx="6" ry="6" />
        <text class="chart-value-label" x="${labelWidth + chartWidth + 12}" y="${y + 11}">${escapeHtml(formatCompact(value))}</text>
      `;
    })
    .join('');

  return `
    <div class="chart-shell">
      <div class="chart-caption">${escapeHtml(options.caption || '')}</div>
      <div class="chart-frame">
        <svg class="chart-svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeHtml(options.ariaLabel || '横向柱状图')}">
          ${rowsSvg}
        </svg>
      </div>
    </div>
  `;
}

function renderTrendTable(rows) {
  const safeRows = Array.isArray(rows) ? rows : [];
  return `
    <div class="table-shell">
      <table>
        <thead>
          <tr>
            <th>${escapeHtml(UI_COPY.sections.trend.tableTime)}</th>
            <th data-align="right">${escapeHtml(UI_COPY.sections.trend.tableTokens)}</th>
          </tr>
        </thead>
        <tbody>
          ${
            safeRows.length
              ? safeRows
                  .map(
                    (row) => `
                      <tr>
                        <td class="mono">${escapeHtml(row.label)}</td>
                        <td class="mono" data-align="right">${formatNumber(row.total_tokens)}</td>
                      </tr>
                    `,
                  )
                  .join('')
              : `<tr><td colspan="2">${renderEmptyState(UI_COPY.sections.trend.tableEmpty)}</td></tr>`
          }
        </tbody>
      </table>
    </div>
  `;
}

/*
 * ========================================================================
 * 步骤1：渲染趋势图表区
 * ========================================================================
 * 目标：
 * 1) 用主图替换原来的长条列表首屏
 * 2) 同时提供最近时间对比表格
 * 3) 保留完整明细折叠区，避免内容失真
 */
export function renderTrend(context, uiState) {
  logger.info('开始渲染趋势图表区');

  // 1.1 渲染趋势摘要卡、主图和最近时间对比表
  const spotlightRows = context.trend.spotlightRows;
  const trendCopy = UI_COPY.sections.trend;
  const windowCopy = resolveTrendWindowCopy(uiState.window);
  const summaryHtml = `
    <div class="spotlight-metrics">
      <article class="spotlight-card">
        <p class="section-kicker">${escapeHtml(trendCopy.totalLabel)}</p>
        <div class="summary-number mono">${formatCompact(context.trend.total)}</div>
        <div class="summary-foot">${escapeHtml(trendCopy.rawPrefix)} ${formatNumber(context.trend.total)}</div>
      </article>
      <article class="spotlight-card">
        <p class="section-kicker">${escapeHtml(trendCopy.peakLabel)}</p>
        <div class="summary-number mono">${context.trend.peak ? formatCompact(context.trend.peak.total_tokens) : '暂无数据'}</div>
        <div class="summary-foot">${escapeHtml(windowCopy.peakFootLabel)} ${escapeHtml(context.trend.peak?.label || '等待同步')}</div>
      </article>
      <article class="spotlight-card">
        <p class="section-kicker">${escapeHtml(trendCopy.averageLabel)}</p>
        <div class="summary-number mono">${formatCompact(context.trend.average)}</div>
        <div class="summary-foot">${context.trend.active} ${escapeHtml(windowCopy.activeFootSuffix)}</div>
      </article>
    </div>
  `;

  const chartHtml = renderVerticalBarChart(
    spotlightRows,
    (row) => row.total_tokens,
    (row) => row.label,
    {
      caption: windowCopy.chartCaption,
      ariaLabel: trendCopy.chartAria,
      empty: trendCopy.emptyChart,
    },
  );

  const tableHtml = `
    <div class="chart-shell chart-shell--dark">
      <div class="chart-caption">${escapeHtml(windowCopy.compareCaption)}</div>
      ${renderTrendTable(context.trend.tableRows)}
    </div>
  `;

  document.getElementById('trend-spotlight').innerHTML = `
    ${summaryHtml}
    <div class="trend-overview">
      ${chartHtml}
      ${tableHtml}
    </div>
  `;

  // 1.2 渲染完整趋势明细折叠区
  const ledgerRows = context.trend.ledgerRows;
  const hasMore = ledgerRows.length > context.trend.spotlightRows.length;
  const expanded = Boolean(uiState.expanded.trendLedger);

  document.getElementById('trend-ledger').innerHTML = `
    <div class="ledger-disclosure">
      <div class="section-header section-header--tight">
        <div>
          <p class="section-kicker">${escapeHtml(trendCopy.detailKicker)}</p>
          <div class="muted-copy">${escapeHtml(trendCopy.detailCopy)}</div>
        </div>
        ${
          hasMore
            ? `<button class="panel-toggle panel-toggle--dark" data-toggle-panel="trendLedger" aria-expanded="${expanded}">
                ${expanded ? trendCopy.collapseLabel : `${trendCopy.expandLabel} · ${ledgerRows.length} 项`}
              </button>`
            : ''
        }
      </div>
      ${
        expanded || !hasMore
          ? renderTrendTable(ledgerRows)
          : ''
      }
    </div>
  `;

  logger.info('完成趋势图表区渲染');
}

/*
 * ========================================================================
 * 步骤2：渲染通用柱状图
 * ========================================================================
 * 目标：
 * 1) 为模型、来源、成本提供统一的横向柱状图
 * 2) 保持图表可离线导出和截图
 * 3) 避免引入任何图表依赖
 */
export function renderRankChart(rows, valueAccessor, labelAccessor, options) {
  logger.info('开始渲染通用排行图表');
  const output = renderHorizontalBarChart(rows, valueAccessor, labelAccessor, options);
  logger.info('完成通用排行图表渲染');
  return output;
}
