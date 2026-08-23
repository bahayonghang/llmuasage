import { UI_COPY } from '../copy.js';
import {
  escapeHtml,
  formatNumber,
  formatPercent,
  formatTokenAmount,
  formatUsd,
} from '../data.js';

export const TOKEN_COMPOSITION_SERIES = Object.freeze([
  Object.freeze({ key: 'input_tokens', label: 'input' }),
  Object.freeze({ key: 'cache_read_tokens', label: 'cacheRead' }),
  Object.freeze({ key: 'cache_creation_tokens', label: 'cacheCreation' }),
  Object.freeze({ key: 'output_tokens', label: 'output' }),
  Object.freeze({ key: 'other_tokens', label: 'other' }),
]);

export function niceScale(value) {
  const max = Math.max(0, Number(value || 0));
  if (max <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(max));
  const normalized = max / power;
  const nice = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
  return nice * power;
}

export function isOneDay(state) {
  return state?.rangePreset === '1d'
    || (state?.filters?.since && state.filters.since === state.filters.until);
}

function normalizedToken(value) {
  const number = Number(value ?? 0);
  return Number.isFinite(number) && number >= 0 ? number : null;
}

export function deriveDailyComposition(row = {}) {
  const values = {};
  for (const { key } of TOKEN_COMPOSITION_SERIES.slice(0, 4)) {
    const value = normalizedToken(row[key]);
    if (value === null) {
      return { status: 'inconsistent', reason: 'invalid_channel', date: String(row.date || '') };
    }
    values[key] = value;
  }
  const total = normalizedToken(row.total_tokens);
  if (total === null) {
    return { status: 'inconsistent', reason: 'invalid_total', date: String(row.date || '') };
  }
  const known = values.input_tokens
    + values.cache_read_tokens
    + values.cache_creation_tokens
    + values.output_tokens;
  if (known > total) {
    return {
      status: 'inconsistent',
      reason: 'known_exceeds_total',
      date: String(row.date || ''),
      known,
      total,
    };
  }

  values.other_tokens = total - known;
  const segments = TOKEN_COMPOSITION_SERIES.map(({ key, label }, index) => ({
    key,
    label,
    className: `daily-series-${index}`,
    value: values[key],
    ratio: total > 0 ? values[key] / total : 0,
  }));
  return {
    status: total > 0 ? 'ok' : 'no_data',
    date: String(row.date || ''),
    total,
    known,
    cost: Number.isFinite(Number(row.cost_with_cache_usd))
      ? Number(row.cost_with_cache_usd)
      : 0,
    eventCount: normalizedToken(row.event_count) ?? 0,
    segments,
  };
}

export function aggregateDailyComposition(rows) {
  const derivedRows = (Array.isArray(rows) ? rows : []).map(deriveDailyComposition);
  const inconsistent = derivedRows.find((row) => row.status === 'inconsistent');
  if (inconsistent) return inconsistent;
  if (!derivedRows.length) return { status: 'no_data', total: 0, segments: [] };

  const aggregate = {
    date: derivedRows.map((row) => row.date).filter(Boolean).join('–'),
    input_tokens: 0,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    output_tokens: 0,
    total_tokens: 0,
    cost_with_cache_usd: 0,
    event_count: 0,
  };
  for (const row of derivedRows) {
    aggregate.total_tokens += row.total;
    aggregate.cost_with_cache_usd += row.cost;
    aggregate.event_count += row.eventCount;
    for (const segment of row.segments.slice(0, 4)) {
      aggregate[segment.key] += segment.value;
    }
  }
  return deriveDailyComposition(aggregate);
}

function replaceFields(template, fields) {
  return Object.entries(fields).reduce(
    (value, [key, replacement]) => value.replaceAll(`{${key}}`, replacement),
    String(template),
  );
}

function dataQualityMessage(issue, copy) {
  if (issue.reason !== 'known_exceeds_total') return copy.invalidData;
  return replaceFields(copy.inconsistent, {
    date: issue.date || copy.unknownDate,
    known: formatNumber(issue.known),
    total: formatNumber(issue.total),
  });
}

function compositionDescription(composition, copy, prefix = '') {
  const segments = composition.segments
    .map((segment) => `${copy[segment.label]} ${formatNumber(segment.value)}`)
    .join(' · ');
  return `${prefix ? `${prefix} · ` : ''}${copy.total} ${formatNumber(composition.total)} · ${segments} · ${copy.cost} ${formatUsd(composition.cost)}`;
}

function bindTooltip(container) {
  const tooltip = container.querySelector('.chart-tooltip');
  if (!tooltip) return;
  container.querySelectorAll('[data-daily-tooltip]').forEach((bar) => {
    bar.addEventListener('mousemove', (event) => {
      tooltip.textContent = bar.dataset.dailyTooltip;
      tooltip.hidden = false;
      tooltip.style.left = `${event.clientX}px`;
      tooltip.style.top = `${event.clientY - 8}px`;
    });
    bar.addEventListener('mouseleave', () => { tooltip.hidden = true; });
    bar.addEventListener('focus', () => {
      tooltip.textContent = bar.dataset.dailyTooltip;
      const bounds = bar.getBoundingClientRect?.();
      if (bounds) {
        tooltip.style.left = `${bounds.left + bounds.width / 2}px`;
        tooltip.style.top = `${bounds.top}px`;
      }
      tooltip.hidden = false;
    });
    bar.addEventListener('blur', () => { tooltip.hidden = true; });
  });
}

function renderOneDayComposition(rows, copy, state) {
  const composition = aggregateDailyComposition(rows);
  if (composition.status === 'inconsistent') {
    return `<div class="empty-state compact composition-error" role="status">${escapeHtml(dataQualityMessage(composition, copy))}</div>`;
  }
  if (composition.status === 'no_data') {
    return `<div class="empty-state compact">${escapeHtml(copy.empty)}</div>`;
  }

  const rangeLabel = state?.rangePreset === '1d' ? copy.oneDayRange : copy.sameDayRange;
  const description = compositionDescription(composition, copy, rangeLabel);
  const strip = composition.segments.map((segment) => (
    `<span class="daily-composition-segment ${segment.className}" style="width:${(segment.ratio * 100).toFixed(3)}%"></span>`
  )).join('');
  const stats = composition.segments.map((segment) => `
    <div class="daily-composition-stat">
      <span class="daily-composition-key"><i class="daily-legend ${segment.className}" aria-hidden="true"></i>${escapeHtml(copy[segment.label])}</span>
      <strong>${escapeHtml(formatNumber(segment.value))}</strong>
      <small>${escapeHtml(formatPercent(segment.value, composition.total))}</small>
    </div>
  `).join('');
  return `
    <div class="daily-composition-summary">
      <div><span>${escapeHtml(rangeLabel)}</span><strong>${escapeHtml(formatNumber(composition.total))}</strong></div>
      <span class="daily-composition-total-label">${escapeHtml(copy.total)}</span>
    </div>
    <div class="daily-composition-strip" role="img" aria-label="${escapeHtml(description)}">${strip}</div>
    <div class="daily-composition-stats">${stats}</div>
  `;
}

export function renderTrendsDaily(context, state = {}) {
  const container = document.getElementById('trends-daily');
  if (!container) return;
  const copy = UI_COPY.readyWidgets.trendsDaily;
  const support = context?.panels?.trends_daily_support;
  const rows = context?.panels?.trends_daily || [];
  const header = `<div class="ready-widget-head"><div><h3>${escapeHtml(copy.title)}</h3><p>${escapeHtml(copy.sub)}</p></div></div>`;
  if (support?.level === 'loading') {
    container.innerHTML = `${header}<div class="empty-state compact">${escapeHtml(copy.loading)}</div>`;
    return;
  }
  if (support?.level === 'degraded') {
    container.innerHTML = `${header}<div class="empty-state compact composition-error" role="status">${escapeHtml(support.reason || copy.degraded)}</div>`;
    return;
  }
  if (isOneDay(state)) {
    container.innerHTML = `${header}${renderOneDayComposition(rows, copy, state)}`;
    return;
  }
  if (!rows.length) {
    container.innerHTML = `${header}<div class="empty-state compact">${escapeHtml(copy.empty)}</div>`;
    return;
  }

  const compositions = rows.map(deriveDailyComposition);
  const inconsistent = compositions.find((row) => row.status === 'inconsistent');
  if (inconsistent) {
    container.innerHTML = `${header}<div class="empty-state compact composition-error" role="status">${escapeHtml(dataQualityMessage(inconsistent, copy))}</div>`;
    return;
  }
  if (!compositions.some((row) => row.total > 0)) {
    container.innerHTML = `${header}<div class="empty-state compact">${escapeHtml(copy.empty)}</div>`;
    return;
  }

  const chartHeight = 180;
  const top = 12;
  const left = 54;
  const width = Math.max(720, rows.length * 11 + left + 12);
  const plotWidth = width - left - 12;
  const barSlot = plotWidth / rows.length;
  const barWidth = Math.max(2, Math.min(18, barSlot - 2));
  const totals = compositions.map((row) => row.total);
  const scale = niceScale(Math.max(...totals, 0));
  const grid = [0, 0.25, 0.5, 0.75, 1].map((ratio) => {
    const y = chartHeight - ratio * (chartHeight - top);
    return `<line class="daily-grid" x1="${left}" x2="${width - 12}" y1="${y}" y2="${y}"></line><text class="daily-axis" x="${left - 7}" y="${y + 3}" text-anchor="end">${escapeHtml(formatTokenAmount(scale * ratio))}</text>`;
  }).join('');
  const bars = compositions.map((row, index) => {
    const x = left + index * barSlot + (barSlot - barWidth) / 2;
    let cursor = chartHeight;
    const tooltip = compositionDescription(row, copy, row.date);
    const rects = row.segments.map((segment) => {
      const height = segment.value / scale * (chartHeight - top);
      cursor -= height;
      return `<rect class="${segment.className}" x="${x}" y="${cursor}" width="${barWidth}" height="${Math.max(0, height)}"></rect>`;
    }).join('');
    const showLabel = index === 0 || index === rows.length - 1 || String(row.date).slice(8) === '01';
    return `<g data-daily-tooltip="${escapeHtml(tooltip)}" role="img" tabindex="0" aria-label="${escapeHtml(tooltip)}">${rects}<title>${escapeHtml(tooltip)}</title>${showLabel ? `<text class="daily-axis" x="${x + barWidth / 2}" y="200" text-anchor="middle">${escapeHtml(String(row.date).slice(5))}</text>` : ''}</g>`;
  }).join('');
  const legend = TOKEN_COMPOSITION_SERIES.map(({ label }, index) => `<span><i class="daily-legend daily-series-${index}" aria-hidden="true"></i>${escapeHtml(copy[label])}</span>`).join('');
  container.innerHTML = `
    ${header}
    <div class="daily-legend-row">${legend}</div>
    <div class="daily-chart-scroll"><svg class="daily-chart" viewBox="0 0 ${width} 210" width="${width}" height="210" role="group" aria-label="${escapeHtml(copy.title)}">${grid}${bars}</svg></div>
    <div class="chart-tooltip" hidden></div>
  `;
  bindTooltip(container);
}
