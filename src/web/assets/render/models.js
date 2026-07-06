import { escapeHtml, formatNumber, formatPercent, formatTokenAmount, ratio } from '../data.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：渲染模型分布区
 * ========================================================================
 * 目标：
 * 1) 填充模型用量条形图（前 8 个）
 * 2) 填充模型用量表格
 */
export function renderModels(context, state = {}) {
  logger.info('开始渲染模型分布区');

  const { panels } = context;
  const modelRows = panels.models || [];
  const expanded = Boolean(state?.expanded?.models);
  const visibleRows = expanded ? modelRows : modelRows.slice(0, 8);
  const max = Number(modelRows[0]?.total_tokens || 1);

  if (!visibleRows.length) {
    document.getElementById('models-bars').innerHTML = `
      <div class="empty-state compact">暂无模型用量数据。</div>
    `;
    document.getElementById('models-table').innerHTML = '';
    logger.info('完成模型分布区渲染');
    return;
  }

  // 1.1 填充模型用量条形图
  const barHtml = visibleRows
    .map((row) => {
      const total_tokens = Number(row.total_tokens || 0);
      const widthPct = ratio(total_tokens, max);
      const label = row.model || '--';
      const value = formatTokenAmount(total_tokens);
      const exactValue = `${formatNumber(total_tokens)} Token`;

      return `
        <div class="bar-row">
          <div class="name">${escapeHtml(label)}</div>
          <div class="bar-track"><div class="bar-fill" style="width: ${widthPct}%"></div></div>
          <div class="num" title="${escapeHtml(exactValue)}">${escapeHtml(value)}</div>
        </div>
      `;
    })
    .join('');

  document.getElementById('models-bars').innerHTML = barHtml;

  // 1.2 填充模型用量表格
  const tableHtml = visibleRows
    .map((row) => {
      const total_tokens = Number(row.total_tokens || 0);
      const input_tokens = Number(row.input_tokens || 0);
      const output_tokens = Number(row.output_tokens || 0) + Number(row.reasoning_output_tokens || 0);
      const cached_tokens = Number(row.cache_read_tokens || 0);

      const inputPct = formatPercent(input_tokens, total_tokens);
      const outputPct = formatPercent(output_tokens, total_tokens);
      const cachedPct = formatPercent(cached_tokens, total_tokens);

      const inputClass = parseFloat(inputPct) > 95 ? 'pct high' : 'pct';
      const outputClass = parseFloat(outputPct) < 5 ? 'pct low' : 'pct';
      const cachedClass = parseFloat(cachedPct) > 90 ? 'pct high' : 'pct';

      return `
        <tr>
          <td class="name-cell">${escapeHtml(row.model || '--')}</td>
          <td class="r" title="${escapeHtml(`${formatNumber(total_tokens)} Token`)}">${escapeHtml(formatTokenAmount(total_tokens))}</td>
          <td class="r"><span class="${inputClass}">${inputPct}</span></td>
          <td class="r"><span class="${outputClass}">${outputPct}</span></td>
          <td class="r"><span class="${cachedClass}">${cachedPct}</span></td>
        </tr>
      `;
    })
    .join('');

  document.getElementById('models-table').innerHTML = `
    <table class="panel-table">
      <thead>
        <tr>
          <th>模型</th>
          <th class="r">总用量</th>
          <th class="r">输入</th>
          <th class="r">输出</th>
          <th class="r">缓存</th>
        </tr>
      </thead>
      <tbody>
        ${tableHtml}
      </tbody>
    </table>
  `;

  logger.info('完成模型分布区渲染');
}
