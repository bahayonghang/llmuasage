import { escapeHtml, formatNumber, formatPercent, ratio } from '../data.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：渲染模型分布区
 * ========================================================================
 * 目标：
 * 1) 填充模型用量条形图（前 8 个）
 * 2) 填充模型用量表格
 */
export function renderModels(context) {
  logger.info('开始渲染模型分布区');

  const { panels } = context;
  const modelRows = panels.models || [];
  const max = Number(modelRows[0]?.total_tokens || 1);

  // 1.1 填充模型用量条形图
  const barHtml = modelRows
    .slice(0, 8)
    .map((row) => {
      const totalTokens = Number(row.total_tokens || 0);
      const widthPct = ratio(totalTokens, max);
      const label = row.model || '--';
      const value = formatNumber(totalTokens);

      return `
        <div class="bar-row">
          <div class="name">${escapeHtml(label)}</div>
          <div class="bar-track"><div class="bar-fill" style="width: ${widthPct}%"></div></div>
          <div class="num">${escapeHtml(value)}</div>
        </div>
      `;
    })
    .join('');

  document.getElementById('models-bars').innerHTML = barHtml;

  // 1.2 填充模型用量表格
  const tableHtml = modelRows
    .slice(0, 8)
    .map((row) => {
      const totalTokens = Number(row.total_tokens || 0);
      const inputTokens = Number(row.input_tokens || 0);
      const outputTokens = Number(row.output_tokens || 0) + Number(row.reasoning_output_tokens || 0);
      const cachedTokens = Number(row.cached_input_tokens || 0);

      const inputPct = formatPercent(inputTokens, totalTokens);
      const outputPct = formatPercent(outputTokens, totalTokens);
      const cachedPct = formatPercent(cachedTokens, totalTokens);

      const inputClass = parseFloat(inputPct) > 95 ? 'pct high' : 'pct';
      const outputClass = parseFloat(outputPct) < 5 ? 'pct low' : 'pct';
      const cachedClass = parseFloat(cachedPct) > 90 ? 'pct high' : 'pct';

      return `
        <tr>
          <td class="name-cell">${escapeHtml(row.model || '--')}</td>
          <td class="r">${formatNumber(totalTokens)}</td>
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
