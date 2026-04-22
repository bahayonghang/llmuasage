import {
  escapeHtml,
  formatCompact,
  formatNumber,
} from '../data.js';
import { UI_COPY } from '../copy.js';
import { renderRankChart } from './charts.js';

const logger = window.console;

function renderEmptyState(message) {
  return `<div class="empty-state">${escapeHtml(message)}</div>`;
}

function renderTable(headers, rows, emptyMessage) {
  if (!rows.length) {
    return renderEmptyState(emptyMessage);
  }

  return `
    <div class="table-shell">
      <div class="table-scroll">
        <table class="compare-table">
          <thead>
            <tr>
              ${headers
                .map(
                  (column) => `
                    <th${column.align === 'right' ? ' data-align="right"' : ''}>${escapeHtml(column.label)}</th>
                  `,
                )
                .join('')}
            </tr>
          </thead>
          <tbody>
            ${rows
              .map(
                (row) => `
                  <tr>
                    ${headers
                      .map((column) => {
                        const cell = row[column.key] ?? '';
                        return `
                          <td${column.align === 'right' ? ' data-align="right"' : ''} data-label="${escapeHtml(column.label)}">
                            <span class="table-cell-value">${cell}</span>
                          </td>
                        `;
                      })
                      .join('')}
                  </tr>
                `,
              )
              .join('')}
          </tbody>
        </table>
      </div>
    </div>
  `;
}

/*
 * ========================================================================
 * 步骤1：渲染模型分析区
 * ========================================================================
 * 目标：
 * 1) 用横向柱状图展示 Top 8 模型
 * 2) 用紧凑对比表展示输入/输出/缓存占比
 * 3) 折叠完整模型排行，压缩首屏长度
 */
function renderModels(context, uiState) {
  logger.info('开始渲染模型分析区');

  const rows = context.panels.models;
  const visibleRows = uiState.expanded.models ? rows : rows.slice(0, 8);
  const modelCopy = UI_COPY.sections.models;
  const chartHtml = renderRankChart(
    visibleRows,
    (row) => row.total_tokens,
    (row) => row.model,
    {
      caption: uiState.expanded.models ? modelCopy.expandedChartCaption : modelCopy.chartCaption,
      ariaLabel: modelCopy.chartAria,
      empty: modelCopy.emptyChart,
    },
  );

  const tableHtml = renderTable(
    [
      { key: 'model', label: modelCopy.headers.model },
      { key: 'totalTokens', label: modelCopy.headers.totalTokens, align: 'right' },
      { key: 'inputShare', label: modelCopy.headers.inputShare, align: 'right' },
      { key: 'outputShare', label: modelCopy.headers.outputShare, align: 'right' },
      { key: 'cachedShare', label: modelCopy.headers.cachedShare, align: 'right' },
    ],
    context.panels.modelTableRows.map((row) => ({
      model: escapeHtml(row.model),
      totalTokens: `<span class="mono">${escapeHtml(row.totalTokens)}</span>`,
      inputShare: `<span class="mono">${escapeHtml(row.inputShare)}</span>`,
      outputShare: `<span class="mono">${escapeHtml(row.outputShare)}</span>`,
      cachedShare: `<span class="mono">${escapeHtml(row.cachedShare)}</span>`,
    })),
    modelCopy.emptyTable,
  );

  const hasMore = rows.length > 8;
  document.getElementById('models-chart').innerHTML = chartHtml;
  document.getElementById('models-table').innerHTML = tableHtml;
  document.getElementById('models-ledger').innerHTML =
    hasMore
      ? `
        <div class="ledger-disclosure">
          <button class="panel-toggle" data-toggle-panel="models" aria-expanded="${Boolean(uiState.expanded.models)}">
            ${uiState.expanded.models ? modelCopy.collapseLabel : `${modelCopy.expandLabel} · ${rows.length} 项`}
          </button>
        </div>
      `
      : '';

  logger.info('完成模型分析区渲染');
}

/*
 * ========================================================================
 * 步骤2：渲染来源、项目与成本区
 * ========================================================================
 * 目标：
 * 1) 把来源、项目、成本收敛成图表 + 小表结构
 * 2) 默认只展示 Top N，超出内容折叠
 * 3) 压缩右侧长内容，避免连续大列表
 */
function renderSources(context, uiState) {
  logger.info('开始渲染来源区');
  const rows = context.panels.sources;
  const visibleRows = uiState.expanded.sources ? rows : rows.slice(0, 4);
  const sourceCopy = UI_COPY.sections.sources;
  document.getElementById('sources-chart').innerHTML = renderRankChart(
    visibleRows,
    (row) => row.total_tokens,
    (row) => row.source,
    {
      caption: uiState.expanded.sources ? sourceCopy.expandedChartCaption : sourceCopy.chartCaption,
      ariaLabel: sourceCopy.chartAria,
      empty: sourceCopy.emptyChart,
    },
  );
  document.getElementById('sources-table').innerHTML = renderTable(
    [
      { key: 'source', label: sourceCopy.headers.source },
      { key: 'lastEventAt', label: sourceCopy.headers.lastEventAt },
    ],
    context.panels.sourceTableRows.map((row) => ({
      source: `${escapeHtml(row.source)}<small>${escapeHtml(row.totalTokens)} Token</small>`,
      lastEventAt: `<span class="mono">${escapeHtml(row.lastEventAt)}</span>`,
    })),
    sourceCopy.emptyTable,
  );
  document.getElementById('sources-ledger').innerHTML =
    rows.length > 4
      ? `
        <div class="ledger-disclosure">
          <button class="panel-toggle" data-toggle-panel="sources" aria-expanded="${Boolean(uiState.expanded.sources)}">
            ${uiState.expanded.sources ? sourceCopy.collapseLabel : `${sourceCopy.expandLabel} · ${rows.length} 项`}
          </button>
        </div>
      `
      : '';
  logger.info('完成来源区渲染');
}

function renderProjects(context, uiState) {
  logger.info('开始渲染项目区');
  const rows = context.panels.projects;
  const visibleRows = uiState.expanded.projects ? rows : rows.slice(0, 5);
  const projectCopy = UI_COPY.sections.projects;
  document.getElementById('projects-table').innerHTML = renderTable(
    [
      { key: 'project', label: projectCopy.headers.project },
      { key: 'ref', label: projectCopy.headers.ref },
      { key: 'tokens', label: projectCopy.headers.tokens, align: 'right' },
    ],
    visibleRows.map((row) => ({
      project: escapeHtml(row.project_label),
      ref: escapeHtml(row.project_ref || row.project_hash),
      tokens: `<span class="mono">${escapeHtml(formatCompact(row.total_tokens))}</span>`,
    })),
    projectCopy.emptyTable,
  );
  document.getElementById('projects-ledger').innerHTML =
    rows.length > 5
      ? `
        <div class="ledger-disclosure">
          <button class="panel-toggle" data-toggle-panel="projects" aria-expanded="${Boolean(uiState.expanded.projects)}">
            ${uiState.expanded.projects ? projectCopy.collapseLabel : `${projectCopy.expandLabel} · ${rows.length} 项`}
          </button>
        </div>
      `
      : '';
  logger.info('完成项目区渲染');
}

function renderCosts(context, uiState) {
  logger.info('开始渲染成本区');
  const rows = context.panels.costs;
  const visibleRows = uiState.expanded.costs ? rows : rows.slice(0, 5);
  const costCopy = UI_COPY.sections.costs;
  document.getElementById('costs-chart').innerHTML = renderRankChart(
    visibleRows,
    (row) => row.estimated_cost_usd,
    (row) => `${row.source} · ${row.model}`,
    {
      caption: uiState.expanded.costs ? costCopy.expandedChartCaption : costCopy.chartCaption,
      ariaLabel: costCopy.chartAria,
      empty: costCopy.emptyChart,
    },
  );
  document.getElementById('costs-table').innerHTML = renderTable(
    [
      { key: 'model', label: costCopy.headers.model },
      { key: 'source', label: costCopy.headers.source },
      { key: 'estimatedCostUsd', label: costCopy.headers.estimatedCostUsd, align: 'right' },
    ],
    context.panels.costTableRows.map((row) => ({
      model: escapeHtml(row.model),
      source: escapeHtml(row.source),
      estimatedCostUsd: `<span class="mono">${escapeHtml(row.estimatedCostUsd)}</span>`,
    })),
    costCopy.emptyTable,
  );
  document.getElementById('costs-ledger').innerHTML =
    rows.length > 5
      ? `
        <div class="ledger-disclosure">
          <button class="panel-toggle" data-toggle-panel="costs" aria-expanded="${Boolean(uiState.expanded.costs)}">
            ${uiState.expanded.costs ? costCopy.collapseLabel : `${costCopy.expandLabel} · ${rows.length} 项`}
          </button>
        </div>
      `
      : '';
  logger.info('完成成本区渲染');
}

export function renderTables(context, uiState) {
  logger.info('开始渲染分析表格区');
  renderModels(context, uiState);
  renderSources(context, uiState);
  renderProjects(context, uiState);
  renderCosts(context, uiState);
  logger.info('完成分析表格区渲染');
}
