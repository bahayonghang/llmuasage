import { UI_COPY } from '../copy.js';
import { escapeHtml, formatNumber } from '../data.js';
import { heatmapLevels } from '../data/derive.js';

const CELL_SIZE = 16;
const CELL_GAP = 2;
const STEP = CELL_SIZE + CELL_GAP;
const LABEL_WIDTH = 36;
const HEADER_HEIGHT = 16;
const STRIP_MAX_DAYS = 31;
let metric = 'tokens';
let previousRange = null;

function activeMetricValue(row) {
  return metric === 'events' ? Number(row?.event_count || 0) : Number(row?.total_tokens || 0);
}

function isSingleDayRange(state) {
  return state?.rangePreset === '1d'
    || Boolean(state?.filters?.since && state.filters.since === state.filters.until);
}

function setDateInputs(since, until) {
  const sinceInput = document.getElementById('filter-since');
  const untilInput = document.getElementById('filter-until');
  if (sinceInput) sinceInput.value = since || '';
  if (untilInput) untilInput.value = until || '';
}

function applyDate(date, state) {
  state.filters ||= {};
  const selected = state.filters.since === date && state.filters.until === date;
  if (selected && previousRange) {
    setDateInputs(previousRange.since, previousRange.until);
    state.filters.since = previousRange.since;
    state.filters.until = previousRange.until;
    state.rangePreset = previousRange.rangePreset;
    previousRange = null;
  } else {
    previousRange = {
      since: state.filters.since || '',
      until: state.filters.until || '',
      rangePreset: state.rangePreset || 'all',
    };
    setDateInputs(date, date);
    state.filters.since = date;
    state.filters.until = date;
    state.rangePreset = 'custom';
  }
  document.getElementById('filters-apply')?.click();
}

function bindTooltip(container) {
  const tooltip = container.querySelector('.chart-tooltip');
  container.querySelectorAll('[data-tooltip]').forEach((cell) => {
    cell.addEventListener('mousemove', (event) => {
      tooltip.textContent = cell.dataset.tooltip;
      tooltip.hidden = false;
      tooltip.style.left = `${event.clientX}px`;
      tooltip.style.top = `${event.clientY - 8}px`;
    });
    cell.addEventListener('mouseleave', () => { tooltip.hidden = true; });
  });
}

function rowTooltip(row, copy) {
  return `${row.date} · ${formatNumber(row.event_count || 0)} ${copy.eventCount} · ${formatNumber(row.total_tokens || 0)} ${copy.tokenCount}`;
}

function weekdayFull(date, copy) {
  const labels = copy.weekdaysFull || [];
  const day = new Date(`${date}T00:00:00Z`).getUTCDay();
  return labels[day] || '';
}

function widgetHead(copy, rangeNote) {
  return `
    <div class="ready-widget-head">
      <div><h3>${escapeHtml(copy.title)}</h3><p>${escapeHtml(copy.sub)} ${rangeNote}</p></div>
      <div class="seg heatmap-metric" role="group" aria-label="${escapeHtml(copy.metricAria)}">
        <button type="button" data-heatmap-metric="tokens" class="${metric === 'tokens' ? 'active' : ''}" aria-pressed="${metric === 'tokens'}">${escapeHtml(copy.tokens)}</button>
        <button type="button" data-heatmap-metric="events" class="${metric === 'events' ? 'active' : ''}" aria-pressed="${metric === 'events'}">${escapeHtml(copy.events)}</button>
      </div>
    </div>`;
}

function scaleHtml(copy) {
  return `<div class="heatmap-scale"><span>${escapeHtml(copy.less)}</span>${[0, 1, 2, 3, 4].map((level) => `<span class="hm-key hm-l${level}"></span>`).join('')}<span>${escapeHtml(copy.more)}</span></div>`;
}

function bindHeatmap(container, context, state) {
  container.querySelectorAll('[data-heatmap-metric]').forEach((button) => {
    button.addEventListener('click', () => {
      metric = button.dataset.heatmapMetric;
      renderCalendarHeatmap(context, state);
    });
  });
  container.querySelectorAll('[data-date]').forEach((cell) => {
    cell.addEventListener('click', () => applyDate(cell.dataset.date, state));
    if (cell.tagName !== 'BUTTON') {
      cell.addEventListener('keydown', (event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          applyDate(cell.dataset.date, state);
        }
      });
    }
  });
  bindTooltip(container);
}

function renderDayStrip(rows, levels, copy, selectedDate) {
  const cells = rows.map((row, index) => {
    const date = String(row.date || '');
    const tooltip = rowTooltip(row, copy);
    const selected = selectedDate === date;
    return `<button type="button" class="heatmap-day-cell${selected ? ' selected' : ''}" data-date="${escapeHtml(date)}" data-tooltip="${escapeHtml(tooltip)}" aria-label="${escapeHtml(tooltip)}" aria-pressed="${selected ? 'true' : 'false'}"><span class="heatmap-day-dow">${escapeHtml(weekdayFull(date, copy))}</span><span class="heatmap-day-date">${escapeHtml(date.slice(5))}</span><span class="heatmap-day-swatch hm-l${levels[index]}"></span></button>`;
  }).join('');
  return `<div class="heatmap-day-strip">${cells}</div>`;
}

function renderWeekCalendar(rows, levels, copy, selectedDate) {
  const firstDow = new Date(`${rows[0].date}T00:00:00Z`).getUTCDay();
  const weekCount = Math.floor((rows.length - 1 + firstDow) / 7) + 1;
  const width = LABEL_WIDTH + weekCount * STEP;
  const rangeClass = weekCount >= 40 ? ' is-long-range' : '';
  const weekdays = copy.weekdays;
  const weekdayLabels = weekdays.map((label, index) => label
    ? `<text class="heatmap-axis" x="0" y="${HEADER_HEIGHT + index * STEP + 12}">${label}</text>`
    : '').join('');
  let previousMonth = '';
  let monthLabels = '';
  const cells = rows.map((row, index) => {
    const date = String(row.date || '');
    const day = new Date(`${date}T00:00:00Z`).getUTCDay();
    const week = Math.floor((index + firstDow) / 7);
    const month = date.slice(0, 7);
    if (month !== previousMonth && day <= 3) {
      monthLabels += `<text class="heatmap-axis" x="${LABEL_WIDTH + week * STEP}" y="10">${escapeHtml(date.slice(5, 7))}</text>`;
    }
    previousMonth = month;
    const tooltip = rowTooltip(row, copy);
    return `<rect class="hm-cell hm-l${levels[index]}${selectedDate === date ? ' selected' : ''}" data-date="${escapeHtml(date)}" data-tooltip="${escapeHtml(tooltip)}" role="button" tabindex="0" aria-label="${escapeHtml(tooltip)}" x="${LABEL_WIDTH + week * STEP}" y="${HEADER_HEIGHT + day * STEP}" width="${CELL_SIZE}" height="${CELL_SIZE}" rx="2"><title>${escapeHtml(tooltip)}</title></rect>`;
  }).join('');
  return `<div class="heatmap-scroll"><svg class="calendar-heatmap-svg${rangeClass}" viewBox="0 0 ${width} 146" width="${width}" height="146" role="img" aria-label="${escapeHtml(copy.title)}">${monthLabels}${weekdayLabels}${cells}</svg></div>`;
}

export function renderCalendarHeatmap(context, state = {}) {
  const container = document.getElementById('calendar-heatmap');
  if (!container) return;
  const copy = UI_COPY.readyWidgets.heatmap;
  const support = context.panels.heatmap_support;

  if (isSingleDayRange(state)) {
    container.hidden = true;
    container.innerHTML = '';
    return;
  }

  container.hidden = false;
  const rows = [...(context.panels.heatmap || [])].sort((a, b) => String(a.date).localeCompare(String(b.date)));
  if (support?.level === 'loading') {
    container.innerHTML = `<div class="ready-widget-head"><div><h3>${escapeHtml(copy.title)}</h3><p>${escapeHtml(copy.sub)}</p></div></div><div class="empty-state compact">${escapeHtml(copy.loading)}</div>`;
    return;
  }
  if (!rows.some((row) => Number(row.event_count || 0) > 0)) {
    const message = support?.level === 'degraded' && support.reason ? support.reason : copy.empty;
    container.innerHTML = `<div class="ready-widget-head"><div><h3>${escapeHtml(copy.title)}</h3><p>${escapeHtml(copy.sub)}</p></div></div><div class="empty-state compact">${escapeHtml(message)}</div>`;
    return;
  }

  const values = rows.map(activeMetricValue);
  const levels = heatmapLevels(values);
  const selectedDate = state?.filters?.since === state?.filters?.until ? state.filters.since : null;
  const rangeNote = state?.rangePreset === 'all' ? `<span>${escapeHtml(copy.recentYear)}</span>` : '';
  const chart = rows.length <= STRIP_MAX_DAYS
    ? renderDayStrip(rows, levels, copy, selectedDate)
    : renderWeekCalendar(rows, levels, copy, selectedDate);

  container.innerHTML = `${widgetHead(copy, rangeNote)}${chart}${scaleHtml(copy)}<div class="chart-tooltip" hidden></div>`;
  bindHeatmap(container, context, state);
}
