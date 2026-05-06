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

  // 1.2 填充来源行
  const rowsHtml = sourceRows
    .slice(0, 4)
    .map((row) => {
      const totalTokens = Number(row.total_tokens || 0);
      const widthPct = ratio(totalTokens, max);
      const sharePct = ((totalTokens / totals.totalTokens) * 100).toFixed(1);
      const lastEventAt = row.last_event_at ? row.last_event_at.slice(11, 19) : '--';

      return `
        <div class="source-row">
          <div>
            <div class="src-name">${escapeHtml(row.source || '--')}</div>
            <div class="src-meta">${lastEventAt}</div>
          </div>
          <div>
            <div class="src-bar-track"><div class="src-bar-fill" style="width: ${widthPct}%"></div></div>
            <div class="src-meta">${formatNumber(totalTokens)} Token</div>
          </div>
          <div>
            <div class="src-value">${formatCompact(totalTokens)}</div>
            <div class="src-pct">${sharePct}%</div>
          </div>
        </div>
      `;
    })
    .join('');

  document.getElementById('sources-rows').innerHTML = rowsHtml;

  logger.info('完成来源分布区渲染');
}
