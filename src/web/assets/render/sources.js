import { escapeHtml, formatNumber, formatCompact, ratio } from '../data.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：渲染来源分布区
 * ========================================================================
 * 目标：
 * 1) 填充来源数标签
 * 2) 填充来源行（前 4 个）
 */
export function renderSources(context) {
  logger.info('开始渲染来源分布区');

  const { panels, totals } = context;
  const sourceRows = panels.sources || [];
  const max = Number(sourceRows[0]?.total_tokens || 1);

  // 1.1 填充来源数标签
  document.getElementById('sources-count').textContent = `${sourceRows.length} 个来源`;

  if (!sourceRows.length) {
    document.getElementById('sources-rows').innerHTML = `
      <div class="empty-state compact">暂无来源数据。</div>
    `;
    logger.info('完成来源分布区渲染');
    return;
  }

  // 1.2 填充来源行
  const rowsHtml = sourceRows
    .slice(0, 4)
    .map((row) => {
      const total_tokens = Number(row.total_tokens || 0);
      const widthPct = ratio(total_tokens, max);
      const sharePct = ((total_tokens / totals.total_tokens) * 100).toFixed(1);
      const last_event_at = row.last_event_at ? row.last_event_at.slice(11, 19) : '--';

      return `
        <div class="source-row">
          <div>
            <div class="src-name">${escapeHtml(row.source || '--')}</div>
            <div class="src-meta">${last_event_at}</div>
          </div>
          <div>
            <div class="src-bar-track"><div class="src-bar-fill" style="width: ${widthPct}%"></div></div>
            <div class="src-meta">${formatNumber(total_tokens)} Token</div>
          </div>
          <div>
            <div class="src-value">${formatCompact(total_tokens)}</div>
            <div class="src-pct">${sharePct}%</div>
          </div>
        </div>
      `;
    })
    .join('');

  document.getElementById('sources-rows').innerHTML = rowsHtml;

  logger.info('完成来源分布区渲染');
}
