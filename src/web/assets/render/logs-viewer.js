import { UI_COPY } from '../copy.js';
import { escapeHtml, fetchLogs, formatNumber, formatUsd } from '../data.js';
const view = { generation: 0, signature: '', session: '', rows: [], cursor: null, loading: false, raw: new Map() };
export function logsGenerationIsCurrent(currentGeneration, requestGeneration, currentSignature, requestSignature) { return currentGeneration === requestGeneration && currentSignature === requestSignature; }
function signature(state) { return JSON.stringify({ filters: state?.filters || {}, range: state?.rangePreset, window: state?.trendWindow, session: view.session }); }
function render(state) {
  const root = document.getElementById('logs-viewer');
  if (!root) return;
  const copy = UI_COPY.sessionAnalytics.logs;
  if (state.mode === 'snapshot') { root.innerHTML = `<div class="empty-state">${escapeHtml(copy.liveOnly)}</div>`; return; }
  const filter = view.session ? `<div class="logs-session-filter"><span>${escapeHtml(copy.session)}: <strong>${escapeHtml(view.session)}</strong></span><button type="button" class="btn" data-clear-session>${escapeHtml(copy.clear)}</button></div>` : '';
  const rows = view.rows.map((row) => `<tr data-log-key="${escapeHtml(row.event_key)}" tabindex="0"><td>${escapeHtml(row.event_at || '--')}</td><td><span class="agent-tag" data-source="${escapeHtml(row.source || '')}">${escapeHtml(row.source || '--')}</span></td><td>${escapeHtml(row.model || '--')}</td><td>${escapeHtml(row.session_label || row.session_id || '--')}</td><td class="num">${formatNumber(row.total_tokens || 0)}</td><td class="num">${formatUsd(row.cost_usd || 0)}</td><td>${escapeHtml(row.project_label || '--')}</td></tr><tr class="log-raw-row" data-raw-for="${escapeHtml(row.event_key)}" hidden><td colspan="7"><pre>${escapeHtml(view.raw.get(row.event_key) || copy.rawLoading)}</pre></td></tr>`).join('');
  const table = rows ? `<div class="logs-table-wrap"><table class="data-table logs-table"><thead><tr><th>${copy.time}</th><th>${copy.source}</th><th>${copy.model}</th><th>${copy.session}</th><th>${copy.tokens}</th><th>${copy.cost}</th><th>${copy.project}</th></tr></thead><tbody>${rows}</tbody></table></div>` : `<div class="empty-state compact">${escapeHtml(view.loading ? copy.loading : copy.empty)}</div>`;
  root.innerHTML = `${filter}${table}${view.cursor ? `<button class="btn logs-more" type="button" data-load-more ${view.loading ? 'disabled' : ''}>${copy.more}</button>` : ''}`;
  root.querySelector('[data-clear-session]')?.addEventListener('click', () => { view.session = ''; resetLogsViewer(state); void loadPage(state); });
  root.querySelector('[data-load-more]')?.addEventListener('click', () => void loadPage(state, true));
  root.querySelectorAll('[data-log-key]').forEach((row) => row.addEventListener('click', () => void toggleRaw(state, row.dataset.logKey)));
}
async function toggleRaw(state, eventKey) {
  const detail = document.querySelector(`[data-raw-for="${CSS.escape(eventKey)}"]`);
  if (!detail) return;
  detail.hidden = !detail.hidden;
  if (detail.hidden || view.raw.has(eventKey)) return;
  const generation = view.generation;
  const page = await fetchLogs(state, { eventKey, cache: false });
  if (generation !== view.generation) return;
  const raw = page?.records?.[0]?.raw_json || UI_COPY.sessionAnalytics.logs.rawUnavailable;
  view.raw.set(eventKey, raw);
  detail.querySelector('pre').textContent = raw;
}
async function loadPage(state, append = false) {
  if (state.mode === 'snapshot' || view.loading) return;
  const nextSignature = signature(state);
  if (nextSignature !== view.signature) { view.signature = nextSignature; view.rows = []; view.cursor = null; view.raw.clear(); }
  const generation = ++view.generation;
  view.loading = true;
  render(state);
  try {
    const page = await fetchLogs(state, { session: view.session, cursor: append ? view.cursor : null, cache: false });
    if (!logsGenerationIsCurrent(view.generation, generation, signature(state), nextSignature)) return;
    view.rows = append ? [...view.rows, ...(page.records || [])] : (page.records || []);
    view.cursor = page.next_cursor || null;
  } finally { if (generation === view.generation) { view.loading = false; render(state); } }
}
export function resetLogsViewer(state) { view.generation += 1; view.signature = ''; view.rows = []; view.cursor = null; view.loading = false; view.raw.clear(); render(state); }
export function refreshLogsViewer(state) {
  resetLogsViewer(state);
  if (state.mode !== 'snapshot' && window.location.hash === '#logs') void loadPage(state);
}
export function setupLogsViewer(state) {
  render(state);
  window.addEventListener('llmusage:session-select', (event) => { view.session = String(event.detail?.session || ''); resetLogsViewer(state); void loadPage(state); });
  window.addEventListener('hashchange', () => { if (window.location.hash === '#logs') void loadPage(state); });
  if (window.location.hash === '#logs') void loadPage(state);
}
