import { UI_COPY } from '../copy.js';
import { escapeHtml, formatNumber, formatTokenAmount, formatUsd } from '../data.js';

const SERIES = [
  ['input_tokens', 'input'],
  ['cache_read_tokens', 'cacheRead'],
  ['cache_creation_tokens', 'cacheCreation'],
  ['output_tokens', 'output'],
];

export function niceScale(value) {
  const max = Math.max(0, Number(value || 0));
  if (max <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(max));
  const normalized = max / power;
  const nice = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
  return nice * power;
}

function isOneDay(state) {
  return state?.rangePreset === '1d'
    || (state?.filters?.since && state.filters.since === state.filters.until);
}

function bindTooltip(container) {
  const tooltip = container.querySelector('.chart-tooltip');
  container.querySelectorAll('[data-daily-tooltip]').forEach((bar) => {
    bar.addEventListener('mousemove', (event) => {
      tooltip.textContent = bar.dataset.dailyTooltip;
      tooltip.hidden = false;
      tooltip.style.left = `${event.clientX}px`;
      tooltip.style.top = `${event.clientY - 8}px`;
    });
    bar.addEventListener('mouseleave', () => { tooltip.hidden = true; });
  });
}

export function renderTrendsDaily(context, state = {}) {
  const container = document.getElementById('trends-daily');
  if (!container) return;
  const copy = UI_COPY.readyWidgets.trendsDaily;
  const support = context.panels.trends_daily_support;
  const rows = context.panels.trends_daily || [];
  const header = `<div class="ready-widget-head"><div><h3>${escapeHtml(copy.title)}</h3><p>${escapeHtml(copy.sub)}</p></div></div>`;
  if (support?.level === 'loading') {
    container.innerHTML = `${header}<div class="empty-state compact">${escapeHtml(copy.loading)}</div>`;
    return;
  }
  if (isOneDay(state)) {
    container.innerHTML = `${header}<div class="empty-state compact">${escapeHtml(copy.oneDay)}</div>`;
    return;
  }
  if (!rows.length) {
    const message = support?.level === 'degraded' && support.reason ? support.reason : copy.empty;
    container.innerHTML = `${header}<div class="empty-state compact">${escapeHtml(message)}</div>`;
    return;
  }

  const chartHeight = 180;
  const top = 12;
  const left = 54;
  const width = Math.max(720, rows.length * 11 + left + 12);
  const plotWidth = width - left - 12;
  const barSlot = plotWidth / rows.length;
  const barWidth = Math.max(2, Math.min(18, barSlot - 2));
  const totals = rows.map((row) => SERIES.reduce((sum, [key]) => sum + Number(row?.[key] || 0), 0));
  const scale = niceScale(Math.max(...totals, 0));
  const grid = [0, 0.25, 0.5, 0.75, 1].map((ratio) => {
    const y = chartHeight - ratio * (chartHeight - top);
    return `<line class="daily-grid" x1="${left}" x2="${width - 12}" y1="${y}" y2="${y}"></line><text class="daily-axis" x="${left - 7}" y="${y + 3}" text-anchor="end">${escapeHtml(formatTokenAmount(scale * ratio))}</text>`;
  }).join('');
  const bars = rows.map((row, index) => {
    const x = left + index * barSlot + (barSlot - barWidth) / 2;
    let cursor = chartHeight;
    const tooltip = `${row.date} · ${SERIES.map(([key, label]) => `${copy[label]} ${formatNumber(row[key] || 0)}`).join(' · ')} · ${copy.cost} ${formatUsd(row.cost_with_cache_usd || 0)}`;
    const rects = SERIES.map(([key], seriesIndex) => {
      const height = Number(row[key] || 0) / scale * (chartHeight - top);
      cursor -= height;
      return `<rect class="daily-series-${seriesIndex}" x="${x}" y="${cursor}" width="${barWidth}" height="${Math.max(0, height)}"></rect>`;
    }).join('');
    const showLabel = index === 0 || index === rows.length - 1 || String(row.date).slice(8) === '01';
    return `<g data-daily-tooltip="${escapeHtml(tooltip)}" role="img" aria-label="${escapeHtml(tooltip)}">${rects}<title>${escapeHtml(tooltip)}</title>${showLabel ? `<text class="daily-axis" x="${x + barWidth / 2}" y="200" text-anchor="middle">${escapeHtml(String(row.date).slice(5))}</text>` : ''}</g>`;
  }).join('');
  const legend = SERIES.map(([, label], index) => `<span><i class="daily-legend daily-series-${index}"></i>${escapeHtml(copy[label])}</span>`).join('');
  container.innerHTML = `
    ${header}
    <div class="daily-legend-row">${legend}</div>
    <div class="daily-chart-scroll"><svg class="daily-chart" viewBox="0 0 ${width} 210" width="${width}" height="210" role="img" aria-label="${escapeHtml(copy.title)}">${grid}${bars}</svg></div>
    <div class="chart-tooltip" hidden></div>
  `;
  bindTooltip(container);
}
