import { getLocale, getShellCopy, UI_COPY } from '../copy.js';
import { escapeHtml, formatCompact, formatNumber, formatTokenAmount, formatUsd, ratio } from '../data.js';

const logger = window.console;

function supportLabel(support) {
  const level = support?.level || (support?.supported ? 'normalized' : 'no_data');
  return UI_COPY.behavior.support[level] || level;
}

function localizedReason(reason, fallback) {
  const raw = String(reason || '');
  if (!raw) return fallback;
  const reasons = UI_COPY.behavior.reasons;
  if (raw.startsWith('No normalized behavior facts')) return reasons.noFacts;
  if (raw.startsWith('At least two models') || raw.startsWith('Need at least two models')) return reasons.insufficientModels;
  if (raw.startsWith('Low sample:')) return reasons.lowSample;
  if (raw.startsWith('One selected model')) return reasons.missingModel;
  return getLocale() === 'zh' ? fallback : raw;
}

function emptyState(support, fallback, compact = false) {
  const reason = localizedReason(support?.reason, fallback);
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
  const copy = UI_COPY.behavior.activity;
  if (!rows.length) {
    return emptyState(support, copy.empty, true);
  }
  const rowsHtml = rows
    .slice(0, 8)
    .map((row) => `
      <tr>
        <td class="name-cell">${escapeHtml(copy.categories[row.category] || row.category || '--')}</td>
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
          <th>${escapeHtml(copy.category)}</th>
          <th class="r">${escapeHtml(copy.turns)}</th>
          <th class="r">${escapeHtml(copy.editTurns)}</th>
          <th class="r">${escapeHtml(copy.oneShot)}</th>
          <th class="r">${escapeHtml(copy.cost)}</th>
        </tr>
      </thead>
      <tbody>${rowsHtml}</tbody>
    </table>
  `;
}

function renderToolsTable(rows, support) {
  const copy = UI_COPY.behavior.tools;
  if (!rows.length) {
    return emptyState(support, copy.empty, true);
  }
  const rowsHtml = rows
    .slice(0, 8)
    .map((row) => {
      const rawName = row.tool_name === '(non-tool)' ? copy.kinds['(non-tool)'] : row.tool_name;
      const name = row.mcp_server ? `${row.mcp_server} / ${rawName}` : rawName;
      return `
        <tr>
          <td class="name-cell">${escapeHtml(name || '--')}</td>
          <td>${escapeHtml(copy.kinds[row.tool_kind] || row.tool_kind || '--')}</td>
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
          <th>${escapeHtml(copy.tool)}</th>
          <th>${escapeHtml(copy.type)}</th>
          <th class="r">${escapeHtml(copy.calls)}</th>
          <th class="r">${escapeHtml(copy.share)}</th>
          <th class="r">${escapeHtml(copy.cost)}</th>
        </tr>
      </thead>
      <tbody>${rowsHtml}</tbody>
    </table>
  `;
}

function writeOptimize(optimize, refreshing = false) {
  const copy = UI_COPY.behavior.optimize;
  const support = optimize?.support;
  const findings = Array.isArray(optimize?.findings) ? optimize.findings : [];
  const grade = optimize?.grade || '--';
  const score = Number(optimize?.score ?? 0);
  const savingsTokens = formatTokenAmount(optimize?.estimated_savings_tokens || 0);
  const savingsUsd = formatUsd(optimize?.estimated_savings_usd || 0);

  const summary = document.getElementById('optimize-summary');
  if (summary) {
    summary.innerHTML = `
      <div class="mini-stat">
        <span>${escapeHtml(copy.score)}</span>
        <strong>${escapeHtml(grade)}</strong>
        <small>${formatNumber(score)} / 100</small>
      </div>
      <div class="mini-stat">
        <span>${escapeHtml(copy.potential)}</span>
        <strong>${escapeHtml(savingsTokens)}</strong>
        <small>${escapeHtml(savingsUsd)} ${escapeHtml(copy.estimated)}</small>
      </div>
      <div class="mini-stat">
        <span>${escapeHtml(copy.mode)}</span>
        <strong>${escapeHtml(copy.readOnly)}</strong>
        <small>${escapeHtml(supportLabel(support))}</small>
      </div>
    `;
  }

  const host = document.getElementById('optimize-findings');
  if (!host) return;
  if (!findings.length) {
    host.innerHTML = refreshNotice(refreshing) + emptyState(
      support,
      copy.empty,
      true,
    );
    return;
  }
  host.innerHTML = refreshNotice(refreshing) + findings
    .slice(0, 4)
    .map((finding) => {
      const localized = copy.findings[finding.id];
      const title = localized?.title || finding.title || finding.id || '--';
      const evidence = getLocale() === 'zh' && localized ? localized.evidence : finding.evidence || localized?.evidence || '';
      const recommendation = localized?.recommendation || finding.recommendation || '';
      const severity = copy.severity[finding.severity] || finding.severity || copy.severity.low;
      return `
      <div class="finding-card" data-severity="${escapeHtml(finding.severity || 'low')}">
        <div class="finding-head">
          <span class="tag">${escapeHtml(severity)}</span>
          <strong>${escapeHtml(title)}</strong>
        </div>
        <div class="finding-evidence">${escapeHtml(evidence)}</div>
        <div class="finding-rec">${escapeHtml(recommendation)}</div>
      </div>
    `;
    })
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

function writeCompare(compare, refreshing = false) {
  const copy = UI_COPY.behavior.compare;
  const host = document.getElementById('compare-panel');
  if (!host) return;
  const support = compare?.support;
  const metrics = Array.isArray(compare?.metrics) ? compare.metrics : [];
  const style = Array.isArray(compare?.working_style) ? compare.working_style : [];
  const left = compare?.model_a?.model || '--';
  const right = compare?.model_b?.model || '--';
  const warning = localizedReason(compare?.warning || support?.reason, '');
  if (!metrics.length) {
    host.innerHTML = refreshNotice(refreshing) + emptyState(
      support,
      copy.empty,
      true,
    );
    return;
  }
  const metricRows = [...metrics, ...style].slice(0, 8).map((metric) => `
    <tr>
      <td class="name-cell">${escapeHtml(copy.metrics[metric.id] || metric.label || metric.id || '--')}</td>
      <td class="r">${escapeHtml(metricValue(metric, 'model_a_value'))}</td>
      <td class="r">${escapeHtml(metricValue(metric, 'model_b_value'))}</td>
    </tr>
  `).join('');
  host.innerHTML = `
    ${refreshNotice(refreshing)}${warning ? `<div class="empty-state compact">${escapeHtml(warning)}</div>` : ''}
    <table class="panel-table">
      <thead>
        <tr>
          <th>${escapeHtml(copy.metric)}</th>
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
 * 步骤1：渲染行为分析区（按 section 拆分）
 * ========================================================================
 * 目标：
 * 1) 每个 secondary section 只写自己的子容器，secondary 到达互不重渲
 * 2) 对 no_data/unsupported 状态显式降级，不伪造零值
 * 3) stale/refreshing 元数据（secondary_refreshing）在各 section 内原样保留
 * 4) dirty-check 由调用方（app.js 面板注册表）按数据指纹完成，这里只写 DOM
 */
export function renderActivity(context) {
  const { panels } = context;
  const activityRows = panels.activity || [];
  const activitySupport = panels.activity_support;
  const refreshing = Boolean(panels.secondary_refreshing);

  const supportEl = document.getElementById('activity-support');
  if (supportEl) {
    supportEl.textContent = refreshing ? UI_COPY.behavior.support.refreshing : supportLabel(activitySupport);
  }

  const bars = document.getElementById('activity-bars');
  if (bars) {
    bars.innerHTML = renderBars(
      activityRows,
      'turns',
      (row) => UI_COPY.behavior.activity.categories[row.category] || row.category || '--',
      (row) => `${formatCompact(row.turns)} ${UI_COPY.behavior.activity.turnsUnit}`,
    );
  }

  const table = document.getElementById('activity-table');
  if (table) {
    table.innerHTML = renderActivityTable(activityRows, activitySupport) + refreshNotice(refreshing);
  }
}

export function renderTools(context) {
  const { panels } = context;
  const toolRows = panels.tools || [];
  const toolsSupport = panels.tools_support;
  const refreshing = Boolean(panels.secondary_refreshing);

  const supportEl = document.getElementById('tools-support');
  if (supportEl) {
    supportEl.textContent = refreshing ? UI_COPY.behavior.support.refreshing : supportLabel(toolsSupport);
  }

  const bars = document.getElementById('tools-bars');
  if (bars) {
    bars.innerHTML = renderBars(
      toolRows,
      'calls',
      (row) => {
        const name = row.tool_name === '(non-tool)' ? UI_COPY.behavior.tools.kinds['(non-tool)'] : row.tool_name;
        return row.mcp_server ? `${row.mcp_server} / ${name}` : name || '--';
      },
      (row) => `${formatCompact(row.calls)} ${UI_COPY.behavior.tools.callsUnit}`,
    );
  }

  const table = document.getElementById('tools-table');
  if (table) {
    table.innerHTML = renderToolsTable(toolRows, toolsSupport) + refreshNotice(refreshing);
  }
}

export function renderOptimize(context) {
  writeOptimize(context?.panels?.optimize, Boolean(context?.panels?.secondary_refreshing));
}

export function renderCompare(context) {
  writeCompare(context?.panels?.compare, Boolean(context?.panels?.secondary_refreshing));
}

// 全量路径（首屏 / 整页重渲）仍一次渲染全部 4 个 section。
export function renderBehavior(context) {
  logger.info('开始渲染行为分析区');
  renderActivity(context);
  renderTools(context);
  renderOptimize(context);
  renderCompare(context);
  logger.info('完成行为分析区渲染');
}
