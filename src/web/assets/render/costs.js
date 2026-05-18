import { escapeHtml, formatUsd, formatCompact, ratio, statusTone } from '../data.js';
import { buildCostStats } from '../data/derive.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：渲染成本估算区
 * ========================================================================
 * 目标：
 * 1) 填充成本条形图（前 5 个）
 * 2) 填充成本表格
 * 3) 填充 4 个成本统计卡
 * 4) 填充失败记录和集成状态
 */
export function renderCosts(context, state = {}) {
  logger.info('开始渲染成本估算区');

  const { panels, health, ledgerSummary } = context;
  const costRows = panels.costs || [];
  const expanded = Boolean(state?.expanded?.costs);
  const visibleCostRows = expanded ? costRows : costRows.slice(0, 5);
  const max = Number(costRows[0]?.estimated_cost_usd || 1);

  // 1.1 填充成本条形图
  const barHtml = visibleCostRows
    .map((row) => {
      const cost = Number(row.estimated_cost_usd || 0);
      const widthPct = ratio(cost, max);
      const label = `${row.source || '--'} · ${row.model || '--'}`;
      const value = formatUsd(cost);

      return `
        <div class="bar-row">
          <div class="name">${escapeHtml(label)}</div>
          <div class="bar-track"><div class="bar-fill" style="width: ${widthPct}%"></div></div>
          <div class="num">${escapeHtml(value)}</div>
        </div>
      `;
    })
    .join('');

  document.getElementById('costs-bars').innerHTML = barHtml;

  // 1.2 填充成本表格
  const tableHtml = visibleCostRows
    .map((row) => {
      const total_tokens = Number(row.total_tokens || 0);
      const cost = Number(row.estimated_cost_usd || 0);

      return `
        <tr>
          <td class="name-cell">${escapeHtml(row.model || '--')}</td>
          <td>${escapeHtml(row.source || '--')}</td>
          <td class="r">${formatCompact(total_tokens)}</td>
          <td class="r">${formatUsd(cost)}</td>
        </tr>
      `;
    })
    .join('');

  document.getElementById('costs-table').innerHTML = `
    <table class="panel-table">
      <thead>
        <tr>
          <th>模型</th>
          <th>来源</th>
          <th class="r">总用量</th>
          <th class="r">估算成本</th>
        </tr>
      </thead>
      <tbody>
        ${tableHtml}
      </tbody>
    </table>
  `;

  // 1.3 填充 4 个成本统计卡
  const stats = buildCostStats(context);

  document.getElementById('costs-stats').innerHTML = stats
    .map(
      (stat) => `
      <div class="cost-stat">
        <div class="cost-stat-label">${escapeHtml(stat.label)}</div>
        <div class="cost-stat-value">${escapeHtml(stat.value)}</div>
        <div class="cost-stat-foot">${escapeHtml(stat.foot)}</div>
      </div>
    `,
    )
    .join('');

  // 1.4 填充失败记录
  const failureRows = health.failures || [];

  if (failureRows.length === 0) {
    document.getElementById('failures-card').innerHTML = `
      <div style="background: var(--surface-2); border: 1px dashed var(--line); border-radius: 10px; padding: 18px; text-align: center; color: var(--muted); font-size: 12.5px;">
        <svg class="i" viewBox="0 0 24 24" style="width: 18px; height: 18px; color: var(--good); margin-bottom: 6px;"><polyline points="20 6 9 17 4 12"/></svg>
        <div>当前没有失败记录</div>
      </div>
    `;
  } else {
    const failureHtml = failureRows
      .slice(0, 5)
      .map((row) => {
        const label = row.command || '--';
        const detail = row.error || row.summary || row.started_at || '--';
        const tone = statusTone(row.status);
        const statusLabel = tone === 'good' ? '成功' : row.status || '异常';
        return `
        <div style="padding: 10px 12px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 8px; font-size: 12px;">
          <div style="display: flex; justify-content: space-between; gap: 8px;">
            <div style="font-family: var(--font-mono); font-weight: 500;">${escapeHtml(label)}</div>
            <div style="color: var(--muted);">${escapeHtml(statusLabel)}</div>
          </div>
          <div style="color: var(--muted); margin-top: 4px;">${escapeHtml(detail)}</div>
        </div>
      `;
      })
      .join('');

    document.getElementById('failures-card').innerHTML = `
      <div style="display: grid; gap: 10px;">
        ${failureHtml}
      </div>
    `;
  }

  // 1.5 填充集成状态
  const integrationRows = health.integrations || [];

  const integrationHtml = integrationRows
    .map((row) => {
      const tone = statusTone(row.status);
      const statusLabel = tone === 'good' ? '● 正常' : `● ${row.status || '未知'}`;
      return `
      <div style="display: flex; justify-content: space-between; align-items: center; padding: 12px 14px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 10px;">
        <div>
          <div class="mono" style="font-weight: 600; font-size: 13px;">${escapeHtml(row.source || '--')}</div>
          <div style="font-size: 11px; color: var(--muted); margin-top: 2px;" class="mono">${escapeHtml(row.install_type || 'probe')} · ${escapeHtml(row.updated_at || '--')}</div>
        </div>
        <span class="tag ok">${escapeHtml(statusLabel)}</span>
      </div>
    `;
    })
    .join('');

  document.getElementById('integrations-rows').innerHTML = integrationHtml;

  logger.info('完成成本估算区渲染');
}
