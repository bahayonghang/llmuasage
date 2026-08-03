import { UI_COPY } from '../copy.js';
import { escapeHtml, formatNumber } from '../data.js';
const CELL = 17, STEP = 19, LEFT = 29, TOP = 18;
const DOW_ORDER = [6, 0, 1, 2, 3, 4, 5];
function level(value, max) { if (!value || !max) return 0; if (value <= max * .25) return 1; if (value <= max * .5) return 2; if (value <= max * .75) return 3; return 4; }
export function renderHourOfWeek(context, state = {}) {
  const root = document.getElementById('hour-of-week');
  if (!root) return;
  const copy = UI_COPY.sessionAnalytics.hourOfWeek;
  const payload = context?.panels?.hour_of_week;
  const rows = Array.isArray(payload) ? payload : (payload?.rows || []);
  const max = Math.max(0, ...rows.map((row) => Number(row.total_tokens || 0)));
  const timezone = state?.filters?.timezone || Intl.DateTimeFormat().resolvedOptions().timeZone || 'local';
  const head = `<div class="ready-widget-head"><div><h3>${escapeHtml(copy.title)} <span class="timezone-note">${escapeHtml(timezone)}</span></h3><p>${escapeHtml(copy.sub)}</p></div></div>`;
  if (!max) { root.innerHTML = `${head}<div class="empty-state compact">${escapeHtml(payload?.support?.level === 'loading' ? copy.loading : copy.empty)}</div>`; return; }
  const byCell = new Map(rows.map((row) => [`${row.dow}:${row.hour}`, row]));
  const hours = Array.from({ length: 24 }, (_, hour) => hour % 3 === 0 ? `<text class="heatmap-axis" x="${LEFT + hour * STEP + 2}" y="11">${hour}</text>` : '').join('');
  const days = DOW_ORDER.map((dow, row) => `<text class="heatmap-axis" x="0" y="${TOP + row * STEP + 12}">${escapeHtml(copy.weekdays[dow])}</text>`).join('');
  const cells = DOW_ORDER.flatMap((dow, displayRow) => Array.from({ length: 24 }, (_, hour) => { const row = byCell.get(`${dow}:${hour}`) || {}; const tip = `${copy.weekdays[dow]} ${String(hour).padStart(2, '0')}:00 · ${formatNumber(row.total_tokens || 0)} ${copy.tokens} · ${formatNumber(row.event_count || 0)} ${copy.events}`; return `<rect class="hm-cell hm-l${level(Number(row.total_tokens || 0), max)}" x="${LEFT + hour * STEP}" y="${TOP + displayRow * STEP}" width="${CELL}" height="${CELL}" rx="2" data-tooltip="${escapeHtml(tip)}"><title>${escapeHtml(tip)}</title></rect>`; })).join('');
  root.innerHTML = `${head}<div class="heatmap-scroll"><svg class="hour-week-svg" viewBox="0 0 489 155" width="489" height="155" role="img" aria-label="${escapeHtml(copy.title)}">${hours}${days}${cells}</svg></div><div class="chart-tooltip" hidden></div>`;
  const tooltip = root.querySelector('.chart-tooltip');
  root.querySelectorAll('[data-tooltip]').forEach((cell) => { cell.addEventListener('mousemove', (event) => { tooltip.textContent = cell.dataset.tooltip; tooltip.hidden = false; tooltip.style.left = `${event.clientX}px`; tooltip.style.top = `${event.clientY - 8}px`; }); cell.addEventListener('mouseleave', () => { tooltip.hidden = true; }); });
}
