import { UI_COPY } from '../copy.js';
import { escapeHtml, fetchTopSessions, formatNumber, formatUsd } from '../data.js';
let requestGeneration = 0;
function currentSort(state) { return state?.topSessionsSort || 'tokens'; }
function metric(row, sort) {
  if (sort === 'duration') return `${formatNumber(row.active_minutes || 0)}m (${formatNumber(row.span_minutes || 0)}m)`;
  if (sort === 'cost') return formatUsd(row.cost_usd || 0);
  return formatNumber(row.total_tokens || 0);
}
async function changeSort(next, state) {
  if (!state || next === currentSort(state)) return;
  state.topSessionsSort = next;
  const request = ++requestGeneration;
  const dashboardGeneration = state.reloadGeneration;
  const payload = await fetchTopSessions(state, { sort: next, cache: false });
  if (request !== requestGeneration || dashboardGeneration !== state.reloadGeneration) return;
  state.rawData = { ...state.rawData, top_sessions: payload };
  renderTopSessions({ panels: { top_sessions: payload } }, state);
}
export function renderTopSessions(context, state = {}) {
  const root = document.getElementById('top-sessions');
  if (!root) return;
  const copy = UI_COPY.sessionAnalytics.topSessions;
  const payload = context?.panels?.top_sessions || {};
  const rows = Array.isArray(payload) ? payload : (payload.rows || []);
  const sort = currentSort(state);
  const controls = ['tokens', 'duration', 'cost'].map((key) => `<button type="button" data-session-sort="${key}" class="${sort === key ? 'active' : ''}" aria-pressed="${sort === key}">${escapeHtml(copy.sort[key])}</button>`).join('');
  const head = `<div class="ready-widget-head"><div><h3>${escapeHtml(copy.title)}</h3><p>${escapeHtml(copy.sub)}</p></div><div class="seg top-session-sort" role="group" aria-label="${escapeHtml(copy.sortAria)}">${controls}</div></div>`;
  const empty = payload?.support?.level === 'loading' ? copy.loading : (payload?.support?.level === 'degraded' && payload.support.reason ? payload.support.reason : copy.empty);
  root.innerHTML = rows.length ? `${head}<div class="top-session-list">${rows.map((row, index) => `<button class="top-session-row" type="button" data-session-id="${escapeHtml(row.session_id || '')}"><span class="top-session-rank">${index + 1}</span><span class="top-session-dot"></span><span class="top-session-name"><strong>${escapeHtml(row.session_label || row.session_id || copy.untitled)}</strong><small>${escapeHtml(row.project_label || row.source || copy.noProject)}</small></span><span class="top-session-value">${escapeHtml(metric(row, sort))}</span></button>`).join('')}</div>` : `${head}<div class="empty-state compact">${escapeHtml(empty)}</div>`;
  root.querySelectorAll('[data-session-sort]').forEach((button) => button.addEventListener('click', () => void changeSort(button.dataset.sessionSort, state)));
  root.querySelectorAll('[data-session-id]').forEach((button) => button.addEventListener('click', () => {
    window.dispatchEvent(new CustomEvent('llmusage:session-select', { detail: { session: button.dataset.sessionId } }));
    window.location.hash = '#logs';
  }));
}
