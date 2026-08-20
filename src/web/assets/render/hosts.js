import { escapeHtml, formatNumber, formatTokenAmount, ratio } from '../data.js';
import { UI_COPY } from '../copy.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：渲染主机分布区
 * ========================================================================
 * 目标：
 * 1) 仅在 hosts.length > 1 时展示
 * 2) 填充主机数标签
 * 3) 填充主机行（前 4 个）
 */
export function renderHosts(context) {
  logger.info('开始渲染主机分布区');

  const panel = document.getElementById('hosts');
  if (!panel) {
    logger.info('完成主机分布区渲染');
    return;
  }

  const { panels, totals } = context;
  const hostRows = panels.hosts || [];
  const copy = (UI_COPY.sections && UI_COPY.sections.hosts) || {};

  if (hostRows.length <= 1) {
    panel.hidden = true;
    logger.info('完成主机分布区渲染');
    return;
  }

  panel.hidden = false;
  const max = Number(hostRows[0]?.total_tokens || 1);
  const countEl = document.getElementById('hosts-count');
  if (countEl) {
    const template = copy.countLabel || '{count}';
    countEl.textContent = String(template).replace('{count}', String(hostRows.length));
  }

  const rowsEl = document.getElementById('hosts-rows');
  if (!rowsEl) {
    logger.info('完成主机分布区渲染');
    return;
  }

  const rowsHtml = hostRows
    .slice(0, 4)
    .map((row) => {
      const total_tokens = Number(row.total_tokens || 0);
      const widthPct = ratio(total_tokens, max);
      const sharePct = ((total_tokens / (Number(totals.total_tokens) || 1)) * 100).toFixed(1);
      const last_event_at = row.last_event_at ? row.last_event_at.slice(11, 19) : '--';
      const compactTokens = formatTokenAmount(total_tokens);
      const exactTokens = `${formatNumber(total_tokens)} Token`;
      const name = row.label || row.host_id || '--';

      return `
        <div class="source-row">
          <div>
            <div class="src-name">${escapeHtml(name)}</div>
            <div class="src-meta">${last_event_at}</div>
          </div>
          <div>
            <div class="src-bar-track"><div class="src-bar-fill" style="width: ${widthPct}%"></div></div>
            <div class="src-meta" title="${escapeHtml(exactTokens)}">${escapeHtml(compactTokens)} Token</div>
          </div>
          <div>
            <div class="src-value" title="${escapeHtml(exactTokens)}">${escapeHtml(compactTokens)}</div>
            <div class="src-pct">${sharePct}%</div>
          </div>
        </div>
      `;
    })
    .join('');

  rowsEl.innerHTML = rowsHtml;

  logger.info('完成主机分布区渲染');
}
