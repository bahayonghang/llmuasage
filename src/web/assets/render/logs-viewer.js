import { UI_COPY } from '../copy.js';
import { escapeHtml, fetchLogs, formatDateTime, formatNumber, formatUsd } from '../data.js';
const view = { generation: 0, signature: '', session: '', rows: [], cursor: null, loading: false, raw: new Map() };
export function logsGenerationIsCurrent(currentGeneration, requestGeneration, currentSignature, requestSignature) { return currentGeneration === requestGeneration && currentSignature === requestSignature; }
export function logsTimeContent(value) {
  const raw = String(value ?? '').trim();
  if (!raw) return { text: '--', title: '' };
  return { text: formatDateTime(raw), title: raw };
}
export function logsTextContent(value) {
  const text = String(value ?? '').trim();
  if (!text) return { text: '--', title: '' };
  return { text, title: text };
}
export function isLogsRowActivationKey(key) { return key === 'Enter' || key === ' ' || key === 'Spacebar'; }
function signature(state) { return JSON.stringify({ filters: state?.filters || {}, range: state?.rangePreset, window: state?.trendWindow, session: view.session }); }
function clippedCell(className, content) {
  const title = content.title ? ` title="${escapeHtml(content.title)}"` : '';
  return `<td class="${className}"><span class="logs-cell-clip"${title}>${escapeHtml(content.text)}</span></td>`;
}
function render(state) {
  const root = document.getElementById('logs-viewer');
  if (!root) return;
  const copy = UI_COPY.sessionAnalytics.logs;
  if (state.mode === 'snapshot') { root.innerHTML = `<div class="empty-state">${escapeHtml(copy.liveOnly)}</div>`; return; }
  const filter = view.session ? `<div class="logs-session-filter"><span>${escapeHtml(copy.session)}: <strong>${escapeHtml(view.session)}</strong></span><button type="button" class="btn" data-clear-session>${escapeHtml(copy.clear)}</button></div>` : '';
  const rows = view.rows.map((row) => {
    const eventKey = row.event_key;
    const rawId = `log-raw-${eventKey}`;
    const time = logsTimeContent(row.event_at);
    const timeTitle = time.title ? ` title="${escapeHtml(time.title)}"` : '';
    return `<tr data-log-key="${escapeHtml(eventKey)}" tabindex="0" aria-expanded="false" aria-controls="${escapeHtml(rawId)}"><td class="logs-cell-time"><span class="logs-cell-clip"${timeTitle}>${escapeHtml(time.text)}</span></td><td class="logs-cell-source"><span class="agent-tag" data-source="${escapeHtml(row.source || '')}">${escapeHtml(row.source || '--')}</span></td>${clippedCell('logs-cell-model', logsTextContent(row.model))}${clippedCell('logs-cell-session', logsTextContent(row.session_label || row.session_id))}<td class="logs-cell-tokens num">${formatNumber(row.total_tokens || 0)}</td><td class="logs-cell-cost num">${formatUsd(row.cost_usd || 0)}</td>${clippedCell('logs-cell-project', logsTextContent(row.project_label))}</tr><tr class="log-raw-row" id="${escapeHtml(rawId)}" data-raw-for="${escapeHtml(eventKey)}" hidden><td colspan="7"><pre>${escapeHtml(view.raw.get(eventKey) || copy.rawLoading)}</pre></td></tr>`;
  }).join('');
  const table = rows ? `<div class="logs-table-wrap"><table class="data-table logs-table"><thead><tr><th class="logs-col-time">${copy.time}</th><th class="logs-col-source">${copy.source}</th><th class="logs-col-model">${copy.model}</th><th class="logs-col-session">${copy.session}</th><th class="logs-col-tokens r">${copy.tokens}</th><th class="logs-col-cost r">${copy.cost}</th><th class="logs-col-project">${copy.project}</th></tr></thead><tbody>${rows}</tbody></table></div>` : `<div class="empty-state compact">${escapeHtml(view.loading ? copy.loading : copy.empty)}</div>`;
  root.innerHTML = `${filter}${table}${view.cursor ? `<button class="btn logs-more" type="button" data-load-more ${view.loading ? 'disabled' : ''}>${copy.more}</button>` : ''}`;
  root.querySelector('[data-clear-session]')?.addEventListener('click', () => { view.session = ''; resetLogsViewer(state); void loadPage(state); });
  root.querySelector('[data-load-more]')?.addEventListener('click', () => void loadPage(state, true));
  root.querySelectorAll('[data-log-key]').forEach((row) => {
    row.addEventListener('click', () => void toggleRaw(state, row.dataset.logKey));
    row.addEventListener('keydown', (event) => {
      if (!isLogsRowActivationKey(event.key)) return;
      event.preventDefault();
      void toggleRaw(state, row.dataset.logKey);
    });
  });
}
async function toggleRaw(state, eventKey) {
  const detail = document.querySelector(`[data-raw-for="${CSS.escape(eventKey)}"]`);
  if (!detail) return;
  const row = document.querySelector(`[data-log-key="${CSS.escape(eventKey)}"]`);
  detail.hidden = !detail.hidden;
  row?.setAttribute('aria-expanded', String(!detail.hidden));
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
