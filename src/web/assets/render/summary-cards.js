import { UI_COPY } from '../copy.js';
import { escapeHtml } from '../data.js';
import { buildSummaryCards } from '../data/derive.js';

export function renderSummaryCards(context) {
  const container = document.getElementById('summary-cards');
  if (!container) return;
  const payload = context.panels.home_overview;
  const copy = UI_COPY.readyWidgets.summary;
  const level = payload?.support?.level;
  if (level === 'loading') {
    container.innerHTML = `<div class="empty-state compact">${escapeHtml(copy.loading)}</div>`;
    return;
  }
  const cards = buildSummaryCards(payload);
  const summary = payload?.summary;
  if (!cards.length || (!Number(summary?.total_sessions || 0) && !Number(summary?.total_requests || 0))) {
    const message = level === 'degraded' && payload?.support?.reason ? payload.support.reason : copy.empty;
    container.innerHTML = `<div class="empty-state compact">${escapeHtml(message)}</div>`;
    return;
  }
  container.innerHTML = cards.map((card) => `
    <div class="summary-card${card.featured ? ' featured' : ''}">
      <div class="summary-card-value num">${escapeHtml(card.value)}</div>
      <div class="summary-card-label">${escapeHtml(card.label)}</div>
      ${card.sub ? `<div class="summary-card-sub">${escapeHtml(card.sub)}</div>` : ''}
    </div>
  `).join('');
}
