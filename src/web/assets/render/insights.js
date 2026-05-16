import { UI_COPY } from '../copy.js';
import { escapeHtml } from '../data.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：渲染诊断型洞察
 * ========================================================================
 * 目标：
 * 1) 所有 signal 都来自真实 overview / pricing / health / diagnostics 数据
 * 2) 文案强调“线索 / 下一步”，避免把 dashboard 数字包装成最终诊断
 * 3) 在无异常时仍显示健康空态，避免用户误以为面板未加载
 */
export function renderInsights(context) {
  logger.info('开始渲染诊断洞察面板');

  const host = document.getElementById('insights-card');
  if (!host) return;

  const copy = UI_COPY.sections.insights;
  const rows = context.insights || [];

  if (rows.length === 0) {
    host.innerHTML = `
      <div class="insight-empty">
        <div class="insight-empty-title">${escapeHtml(copy.emptyTitle)}</div>
        <div>${escapeHtml(copy.emptyBody)}</div>
      </div>
    `;
    logger.info('完成诊断洞察面板渲染');
    return;
  }

  host.innerHTML = `
    <div class="insight-note">${escapeHtml(copy.disclaimer)}</div>
    <div class="insight-list">
      ${rows
        .map(
          (row) => `
            <article class="insight-row" data-tone="${escapeHtml(row.tone || 'neutral')}">
              <div class="insight-row-head">
                <span class="insight-label">${escapeHtml(row.label || copy.defaultLabel)}</span>
                <strong>${escapeHtml(row.title || '--')}</strong>
              </div>
              <div class="insight-evidence">${escapeHtml(row.evidence || '--')}</div>
              <div class="insight-action">${escapeHtml(row.action || copy.defaultAction)}</div>
            </article>
          `,
        )
        .join('')}
    </div>
  `;

  logger.info('完成诊断洞察面板渲染');
}
