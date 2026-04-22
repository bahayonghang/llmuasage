import { escapeHtml, formatNumber, statusTone, truncate } from '../data.js';
import { UI_COPY, translateStatusLabel } from '../copy.js';

const logger = window.console;

function renderEmptyState(message) {
  return `<div class="empty-state">${escapeHtml(message)}</div>`;
}

/*
 * ========================================================================
 * 步骤1：渲染运行状态面板
 * ========================================================================
 * 目标：
 * 1) 把健康状态压缩成集成、游标、失败三个扫读数字
 * 2) 只展示前 5 条失败记录
 * 3) 集成摘要只保留最重要的 3 条状态行
 */
export function renderHealth(context) {
  logger.info('开始渲染运行状态面板');

  // 1.1 渲染健康摘要 chip
  const failureRows = context.health.failures.slice(0, 5);
  const integrationPreview = context.health.integrations.slice(0, 3);
  const healthCopy = UI_COPY.sections.health;

  document.getElementById('health').innerHTML = `
    <div class="health-stack">
      <div class="health-summary">
        <article class="health-chip">
          <div class="health-chip-label">${escapeHtml(healthCopy.chips.integrations)}</div>
          <div class="health-chip-value mono">${formatNumber(context.health.readyIntegrations)} / ${formatNumber(context.health.totalIntegrations)}</div>
        </article>
        <article class="health-chip">
          <div class="health-chip-label">${escapeHtml(healthCopy.chips.cursors)}</div>
          <div class="health-chip-value mono">${formatNumber(context.health.cursors.length)}</div>
        </article>
        <article class="health-chip">
          <div class="health-chip-label">${escapeHtml(healthCopy.chips.failures)}</div>
          <div class="health-chip-value mono">${formatNumber(context.health.failures.length)}</div>
        </article>
      </div>

      <section>
        <h3 class="health-section-title">${escapeHtml(healthCopy.failuresTitle)}</h3>
        <div class="health-failure-list">
          ${
            failureRows.length
              ? failureRows
                  .map(
                    (row) => `
                      <article class="health-failure-row">
                        <div class="section-header section-header--tight">
                          <div class="health-failure-title">${escapeHtml(row.command)}</div>
                          <span class="status-pill" data-tone="${statusTone(row.status)}">${escapeHtml(translateStatusLabel(row.status))}</span>
                        </div>
                        <div class="muted-copy">${escapeHtml(truncate(row.error || row.summary || row.started_at || '无失败详情'))}</div>
                      </article>
                    `,
                  )
                  .join('')
              : renderEmptyState(healthCopy.failuresEmpty)
          }
        </div>
      </section>

      <section>
        <h3 class="health-section-title">${escapeHtml(healthCopy.integrationsTitle)}</h3>
        <div class="integration-list">
          ${
            integrationPreview.length
              ? integrationPreview
                  .map(
                    (row) => `
                      <article class="integration-row">
                        <div class="section-header section-header--tight">
                          <div>${escapeHtml(row.source)}</div>
                          <span class="status-pill" data-tone="${statusTone(row.status)}">${escapeHtml(translateStatusLabel(row.status))}</span>
                        </div>
                        <div class="muted-copy">${escapeHtml(row.install_type || '安装方式未知')} · ${escapeHtml(row.updated_at || '时间未知')}</div>
                      </article>
                    `,
                  )
                  .join('')
              : renderEmptyState(healthCopy.integrationsEmpty)
          }
        </div>
      </section>
    </div>
  `;

  logger.info('完成运行状态面板渲染');
}
