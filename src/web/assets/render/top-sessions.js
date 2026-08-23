import { UI_COPY } from '../copy.js';
import {
  escapeHtml,
  fetchTopSessions,
  formatDateTime,
  formatNumber,
  formatUsd,
} from '../data.js';
import { sourceDisplayName } from '../data/source-catalog.js';

const SORTS = new Set(['tokens', 'duration', 'cost']);
let requestGeneration = 0;

function currentSort(state) {
  return SORTS.has(state?.topSessionsSort) ? state.topSessionsSort : 'tokens';
}

function appliedSort(state) {
  return SORTS.has(state?.topSessionsAppliedSort)
    ? state.topSessionsAppliedSort
    : currentSort(state);
}

function safeMetricNumber(value) {
  const number = Number(value ?? 0);
  return Number.isFinite(number) ? Math.max(0, number) : 0;
}

export function sessionMetricValue(row, sort) {
  if (sort === 'duration') return safeMetricNumber(row?.active_minutes);
  if (sort === 'cost') return safeMetricNumber(row?.cost_usd);
  return safeMetricNumber(row?.total_tokens);
}

export function sessionMetricRatio(value, maximum) {
  const safeValue = safeMetricNumber(value);
  const safeMaximum = safeMetricNumber(maximum);
  return safeMaximum > 0 ? Math.min(1, safeValue / safeMaximum) : 0;
}

export function formatSessionMetric(row, sort) {
  const value = sessionMetricValue(row, sort);
  if (sort === 'duration') return `${formatNumber(value)}m`;
  if (sort === 'cost') return formatUsd(value);
  return formatNumber(value);
}

function replaceFields(template, fields) {
  return Object.entries(fields).reduce(
    (value, [key, replacement]) => value.replaceAll(`{${key}}`, replacement),
    String(template),
  );
}

export function sessionPresentation(row, options = {}) {
  const copy = options.copy || UI_COPY.sessionAnalytics.topSessions;
  const timeZone = options.timeZone;
  const agent = options.agentName || sourceDisplayName(row?.source) || copy.unknownAgent;
  const project = String(row?.project_label || '').trim();
  const title = project || replaceFields(copy.sessionFallback, { agent });
  const firstAt = String(row?.first_event_at || '').trim();
  const lastAt = String(row?.last_event_at || '').trim();

  let subtitle;
  if (firstAt && lastAt) {
    const first = formatDateTime(firstAt, timeZone);
    const last = formatDateTime(lastAt, timeZone);
    subtitle = `${agent} · ${first === last ? last : `${first}–${last}`}`;
  } else {
    subtitle = replaceFields(copy.snapshotFallback, {
      agent,
      events: formatNumber(safeMetricNumber(row?.event_count)),
      minutes: formatNumber(safeMetricNumber(row?.active_minutes)),
    });
  }

  return { title, subtitle, agent };
}

export function prepareSessionRows(rows, sort, options = {}) {
  const safeRows = Array.isArray(rows) ? rows : [];
  const maximum = Math.max(0, ...safeRows.map((row) => sessionMetricValue(row, sort)));
  const copy = options.copy || UI_COPY.sessionAnalytics.topSessions;
  return safeRows.map((row) => {
    const value = sessionMetricValue(row, sort);
    const formattedValue = formatSessionMetric(row, sort);
    const presentation = sessionPresentation(row, options);
    const accessibleName = replaceFields(copy.openLogs, {
      label: `${presentation.title} · ${presentation.subtitle}`,
      metric: `${copy.sort[sort]} ${formattedValue}`,
    });
    return {
      row,
      ...presentation,
      value,
      formattedValue,
      ratio: sessionMetricRatio(value, maximum),
      accessibleName,
    };
  });
}

function panelPayloadFromState(state) {
  return state?.rawData?.top_sessions ?? [];
}

export async function changeTopSessionsSort(next, state, load = fetchTopSessions) {
  const requested = currentSort(state);
  const previousApplied = appliedSort(state);
  if (!state || !SORTS.has(next) || next === requested) return;

  state.topSessionsAppliedSort = previousApplied;
  state.topSessionsSort = next;
  state.topSessionsLoading = true;
  state.topSessionsError = null;
  const request = ++requestGeneration;
  const dashboardGeneration = state.reloadGeneration;
  renderTopSessions({ panels: { top_sessions: panelPayloadFromState(state) } }, state);

  try {
    const payload = await load(state, {
      sort: next,
      cache: false,
      signal: state.rangeReloadController?.signal,
    });
    if (request !== requestGeneration || dashboardGeneration !== state.reloadGeneration) return;
    state.rawData = { ...state.rawData, top_sessions: payload };
    state.topSessionsAppliedSort = next;
  } catch (error) {
    if (request !== requestGeneration || dashboardGeneration !== state.reloadGeneration) return;
    state.topSessionsSort = previousApplied;
    if (error?.name !== 'AbortError') {
      state.topSessionsError = UI_COPY.sessionAnalytics.topSessions.sortError;
    }
  } finally {
    if (request !== requestGeneration) return;
    state.topSessionsLoading = false;
    if (dashboardGeneration !== state.reloadGeneration) return;
    renderTopSessions({ panels: { top_sessions: panelPayloadFromState(state) } }, state);
  }
}

export function renderTopSessions(context, state = {}) {
  const root = document.getElementById('top-sessions');
  if (!root) return;
  const copy = UI_COPY.sessionAnalytics.topSessions;
  const payload = context?.panels?.top_sessions || {};
  const rows = Array.isArray(payload) ? payload : (payload.rows || []);
  const support = Array.isArray(payload) ? null : payload.support;
  const sort = currentSort(state);
  const displaySort = appliedSort(state);
  const globalRefreshing = support?.level === 'loading' || Boolean(state.secondaryRefreshing);
  const busy = globalRefreshing || Boolean(state.topSessionsLoading);
  const controls = ['tokens', 'duration', 'cost']
    .map((key) => {
      const pending = Boolean(state.topSessionsLoading) && sort === key;
      const disabled = globalRefreshing || pending;
      return `<button type="button" data-session-sort="${key}" class="${sort === key ? 'active' : ''}" aria-pressed="${sort === key}" aria-disabled="${disabled}"${disabled ? ' disabled' : ''}>${escapeHtml(copy.sort[key])}</button>`;
    })
    .join('');
  const head = `<div class="ready-widget-head"><div><h3>${escapeHtml(copy.title)}</h3><p>${escapeHtml(copy.sub)}</p></div><div class="seg top-session-sort" role="group" aria-label="${escapeHtml(copy.sortAria)}" aria-busy="${busy}">${controls}</div></div>`;
  const status = state.topSessionsLoading
    ? replaceFields(copy.sortLoading, { metric: copy.sort[sort] })
    : state.topSessionsError;
  const statusMarkup = status
    ? `<div class="top-session-status" data-tone="${state.topSessionsError ? 'warn' : 'neutral'}" aria-live="polite">${escapeHtml(status)}</div>`
    : '<div class="top-session-status" aria-live="polite"></div>';

  if (!rows.length) {
    const empty = support?.level === 'loading'
      ? copy.loading
      : (support?.level === 'degraded' && support.reason ? support.reason : copy.empty);
    root.innerHTML = `${head}${statusMarkup}<div class="empty-state compact">${escapeHtml(empty)}</div>`;
  } else {
    const preparedRows = prepareSessionRows(rows, displaySort, {
      copy,
      timeZone: state?.filters?.timezone,
    });
    const rowMarkup = preparedRows.map((item, index) => {
      const percentage = (item.ratio * 100).toFixed(3);
      const fillClass = item.ratio > 0 ? 'top-session-fill has-value' : 'top-session-fill';
      return `<button class="top-session-row" type="button" data-session-id="${escapeHtml(item.row.session_id || '')}" aria-label="${escapeHtml(item.accessibleName)}"><span class="top-session-rank" aria-hidden="true">${index + 1}</span><span class="top-session-context"><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(item.subtitle)}</small></span><span class="top-session-track" aria-hidden="true"><span class="${fillClass}" style="width:${percentage}%"></span></span><span class="top-session-value">${escapeHtml(item.formattedValue)}</span></button>`;
    }).join('');
    root.innerHTML = `${head}${statusMarkup}<div class="top-session-list" aria-busy="${busy}">${rowMarkup}</div>`;
  }

  root.querySelectorAll('[data-session-sort]').forEach((button) => {
    button.addEventListener('click', () => void changeTopSessionsSort(button.dataset.sessionSort, state));
  });
  root.querySelectorAll('[data-session-id]').forEach((button) => {
    button.addEventListener('click', () => {
      window.dispatchEvent(new CustomEvent('llmusage:session-select', {
        detail: { session: button.dataset.sessionId },
      }));
      window.location.hash = '#logs';
    });
  });
}
