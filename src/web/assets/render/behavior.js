import { getShellCopy } from '../copy.js';
import { escapeHtml, formatCompact, formatNumber, formatUsd, ratio } from '../data.js';

const logger = window.console;

function supportLabel(support) {
  if (support?.supported) {
    return 'normalized';
  }
  return support?.level || 'no_data';
}

function emptyState(support, fallback, compact = false) {
  const reason = support?.reason || fallback;
  return `
    <div class="empty-state${compact ? ' compact' : ''}">
      ${escapeHtml(reason)}
    </div>
  `;
}

function refreshNotice(refreshing) {
  return refreshing
    ? `<div class="empty-state stale-refresh-notice">${escapeHtml(getShellCopy('shell.refresh.secondaryStale'))}</div>`
    : '';
}

function renderBars(rows, valueKey, labelFn, valueFn) {
  const safeRows = Array.isArray(rows) ? rows : [];
  if (!safeRows.length) {
    return '';
  }
  const max = Number(safeRows[0]?.[valueKey] || 1);
  return safeRows
    .slice(0, 8)
    .map((row) => {
      const value = Number(row?.[valueKey] || 0);
      return `
        <div class="bar-row">
          <div class="name">${escapeHtml(labelFn(row))}</div>
          <div class="bar-track"><div class="bar-fill" style="width: ${ratio(value, max)}%"></div></div>
          <div class="num">${escapeHtml(valueFn(row))}</div>
        </div>
      `;
    })
    .join('');
}

function renderActivityTable(rows, support) {
  if (!rows.length) {
    return emptyState(support, '暂无 activity 数据。', true);
  }
  const rowsHtml = rows
    .slice(0, 8)
    .map((row) => `
      <tr>
        <td class="name-cell">${escapeHtml(row.category || '--')}</td>
        <td class="r">${formatNumber(row.turns)}</td>
        <td class="r">${formatNumber(row.edit_turns)}</td>
        <td class="r">${formatNumber(Number(row.one_shot_rate || 0) * 100)}%</td>
        <td class="r">${formatUsd(row.estimated_cost_usd)}</td>
      </tr>
    `)
    .join('');
  return `
    <table class="panel-table">
      <thead>
        <tr>
          <th>类别</th>
          <th class="r">Turns</th>
          <th class="r">Edit</th>
          <th class="r">One-shot</th>
          <th class="r">估算成本</th>
        </tr>
      </thead>
      <tbody>${rowsHtml}</tbody>
    </table>
  `;
}

function renderToolsTable(rows, support) {
  if (!rows.length) {
    return emptyState(support, '暂无 tool 数据。', true);
  }
  const rowsHtml = rows
    .slice(0, 8)
    .map((row) => {
      const name = row.mcp_server ? `${row.mcp_server} / ${row.tool_name}` : row.tool_name;
      return `
        <tr>
          <td class="name-cell">${escapeHtml(name || '--')}</td>
          <td>${escapeHtml(row.tool_kind || '--')}</td>
          <td class="r">${formatNumber(row.calls)}</td>
          <td class="r">${formatNumber(Number(row.call_share || 0) * 100)}%</td>
          <td class="r">${formatUsd(row.estimated_cost_usd)}</td>
        </tr>
      `;
    })
    .join('');
  return `
    <table class="panel-table">
      <thead>
        <tr>
          <th>工具</th>
          <th>类型</th>
          <th class="r">Calls</th>
          <th class="r">占比</th>
          <th class="r">估算成本</th>
        </tr>
      </thead>
      <tbody>${rowsHtml}</tbody>
    </table>
  `;
}

function renderOptimize(optimize) {
  const support = optimize?.support;
  const findings = Array.isArray(optimize?.findings) ? optimize.findings : [];
  const grade = optimize?.grade || '--';
  const score = Number(optimize?.score ?? 0);
  const savingsTokens = formatCompact(optimize?.estimated_savings_tokens || 0);
  const savingsUsd = formatUsd(optimize?.estimated_savings_usd || 0);

  const summary = document.getElementById('optimize-summary');
  if (summary) {
    summary.innerHTML = `
      <div class="mini-stat">
        <span>Grade</span>
        <strong>${escapeHtml(grade)}</strong>
        <small>${formatNumber(score)} / 100</small>
      </div>
      <div class="mini-stat">
        <span>Potential</span>
        <strong>${escapeHtml(savingsTokens)}</strong>
        <small>${escapeHtml(savingsUsd)} est.</small>
      </div>
      <div class="mini-stat">
        <span>Mode</span>
        <strong>Read-only</strong>
        <small>${escapeHtml(supportLabel(support))}</small>
      </div>
    `;
  }

  const host = document.getElementById('optimize-findings');
  if (!host) return;
  if (!findings.length) {
    host.innerHTML = emptyState(
      support,
      '暂无 optimize finding；建议仅基于 normalized facts 生成，不会自动执行清理。',
      true,
    );
    return;
  }
  host.innerHTML = findings
    .slice(0, 4)
    .map((finding) => `
      <div class="finding-card" data-severity="${escapeHtml(finding.severity || 'low')}">
        <div class="finding-head">
          <span class="tag">${escapeHtml(finding.severity || 'low')}</span>
          <strong>${escapeHtml(finding.title || finding.id || '--')}</strong>
        </div>
        <div class="finding-evidence">${escapeHtml(finding.evidence || '')}</div>
        <div class="finding-rec">${escapeHtml(finding.recommendation || '')}</div>
      </div>
    `)
    .join('');
}

function metricValue(metric, key) {
  const value = Number(metric?.[key] || 0);
  if (String(metric?.id || '').includes('cost')) {
    return formatUsd(value);
  }
  if (String(metric?.id || '').includes('rate') || String(metric?.id || '').includes('efficiency')) {
    return `${formatNumber(value * 100)}%`;
  }
  return formatNumber(value);
}

function renderCompare(compare) {
  const host = document.getElementById('compare-panel');
  if (!host) return;
  const support = compare?.support;
  const metrics = Array.isArray(compare?.metrics) ? compare.metrics : [];
  const style = Array.isArray(compare?.working_style) ? compare.working_style : [];
  const left = compare?.model_a?.model || '--';
  const right = compare?.model_b?.model || '--';
  const warning = compare?.warning || support?.reason || '';
  if (!metrics.length) {
    host.innerHTML = emptyState(
      support,
      '至少需要两个模型才会显示 compare；低样本会以 warning 形式显式降级。',
      true,
    );
    return;
  }
  const metricRows = [...metrics, ...style].slice(0, 8).map((metric) => `
    <tr>
      <td class="name-cell">${escapeHtml(metric.label || metric.id || '--')}</td>
      <td class="r">${escapeHtml(metricValue(metric, 'model_a_value'))}</td>
      <td class="r">${escapeHtml(metricValue(metric, 'model_b_value'))}</td>
    </tr>
  `).join('');
  host.innerHTML = `
    ${warning ? `<div class="empty-state compact">${escapeHtml(warning)}</div>` : ''}
    <table class="panel-table">
      <thead>
        <tr>
          <th>Metric</th>
          <th class="r">${escapeHtml(left)}</th>
          <th class="r">${escapeHtml(right)}</th>
        </tr>
      </thead>
      <tbody>${metricRows}</tbody>
    </table>
  `;
}

/*
 * ========================================================================
 * 步骤1：渲染行为分析区
 * ========================================================================
 * 目标：
 * 1) 展示 normalized activity 和 tool facts 的首批聚合
 * 2) 对 no_data/unsupported 状态显式降级，不伪造零值
 * 3) 保持只读分析，不提供破坏性优化动作
 */
export function renderBehavior(context) {
  logger.info('开始渲染行为分析区');

  const { panels } = context;
  const activityRows = panels.activity || [];
  const toolRows = panels.tools || [];
  const activitySupport = panels.activity_support;
  const toolsSupport = panels.tools_support;
  const optimize = panels.optimize;
  const compare = panels.compare;
  const refreshing = Boolean(panels.secondary_refreshing);

  document.getElementById('activity-support').textContent = refreshing ? 'refreshing' : supportLabel(activitySupport);
  document.getElementById('tools-support').textContent = refreshing ? 'refreshing' : supportLabel(toolsSupport);

  document.getElementById('activity-bars').innerHTML = renderBars(
    activityRows,
    'turns',
    (row) => row.category || '--',
    (row) => `${formatCompact(row.turns)} turns`,
  );
  document.getElementById('activity-table').innerHTML = renderActivityTable(
    activityRows,
    activitySupport,
  ) + refreshNotice(refreshing);

  document.getElementById('tools-bars').innerHTML = renderBars(
    toolRows,
    'calls',
    (row) => row.mcp_server ? `${row.mcp_server} / ${row.tool_name}` : row.tool_name || '--',
    (row) => `${formatCompact(row.calls)} calls`,
  );
  document.getElementById('tools-table').innerHTML = renderToolsTable(toolRows, toolsSupport) + refreshNotice(refreshing);
  renderOptimize(optimize);
  renderCompare(compare);
  if (refreshing) {
    const optimizeHost = document.getElementById('optimize-findings');
    if (optimizeHost) optimizeHost.insertAdjacentHTML('afterbegin', refreshNotice(true));
    const compareHost = document.getElementById('compare-panel');
    if (compareHost) compareHost.insertAdjacentHTML('afterbegin', refreshNotice(true));
  }

  logger.info('完成行为分析区渲染');
}
