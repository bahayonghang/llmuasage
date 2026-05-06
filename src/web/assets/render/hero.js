import { escapeHtml, formatNumber } from '../data.js';
import { buildKpis } from '../data/derive.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：渲染首屏 hero 区
 * ========================================================================
 * 目标：
 * 1) 填充 hero-meta（生成时间、最近同步、来源数）
 * 2) 填充右侧 status-panel（运行概览卡）
 * 3) 填充 4 张 KPI 卡
 */
export function renderHero(context) {
  logger.info('开始渲染首屏 hero 区');

  // 1.1 填充 hero-meta
  const { ledgerSummary } = context;
  const metaItems = [
    { label: '生成时间', value: ledgerSummary.generatedAt || '--' },
    { label: '最近同步', value: ledgerSummary.lastSyncAt || '--' },
    { label: '来源数', value: `${ledgerSummary.activeSources} · codex / claude` },
  ];

  document.getElementById('hero-meta').innerHTML = metaItems
    .map(
      (item) => `
      <div class="hero-meta-item">
        ${escapeHtml(item.label)}<span class="mono">${escapeHtml(item.value)}</span>
      </div>
    `,
    )
    .join('');

  // 1.2 填充 status-panel
  const { health } = context;
  const statusTone = ledgerSummary.failureCount > 0 ? 'warn' : 'good';
  const statusLabel = ledgerSummary.failureCount > 0 ? '有失败' : '正常';

  const integrationRows = (health.integrations || [])
    .slice(0, 3)
    .map(
      (row) => `
      <div class="status-row">
        <span class="status-row-name">${escapeHtml(row.source || '--')}</span>
        <span class="status-row-time">${escapeHtml(row.initialized_at || '--')}</span>
        <span class="status-row-state"><span class="dot"></span>正常</span>
      </div>
    `,
    )
    .join('');

  document.getElementById('status-panel').innerHTML = `
    <div class="status-panel-head">
      <div>
        <div class="status-eyebrow">运行概览</div>
        <div style="font-size: 18px; font-weight: 600; margin-top: 2px;">系统健康</div>
      </div>
      <span class="status-pill" data-tone="${statusTone}"><span class="pulse"></span>${statusLabel}</span>
    </div>
    <div class="status-grid">
      <div class="status-cell">
        <div class="status-cell-label">集成</div>
        <div class="status-cell-value">${health.readyIntegrations} / ${health.totalIntegrations}</div>
      </div>
      <div class="status-cell">
        <div class="status-cell-label">游标数</div>
        <div class="status-cell-value">${formatNumber(health.cursors?.length || 0)}</div>
      </div>
      <div class="status-cell">
        <div class="status-cell-label">失败</div>
        <div class="status-cell-value">${ledgerSummary.failureCount}</div>
      </div>
    </div>
    <div class="status-list">
      ${integrationRows}
    </div>
  `;

  // 1.3 填充 4 张 KPI 卡
  const kpis = buildKpis(context);

  document.getElementById('kpi-grid').innerHTML = kpis
    .map(
      (kpi) => `
      <div class="kpi${kpi.featured ? ' featured' : ''}">
        <div class="kpi-label">${escapeHtml(kpi.label)}</div>
        <div class="kpi-value num">${escapeHtml(kpi.value)}<span class="unit">${escapeHtml(kpi.unit)}</span></div>
        <div class="kpi-foot">
          ${kpi.foot.map((line) => `<span>${escapeHtml(line)}</span>`).join('')}
        </div>
        <svg class="kpi-spark" width="62" height="22" viewBox="0 0 62 22" fill="none">
          <polyline points="0,18 10,16 18,10 26,12 34,4 42,7 50,3 62,1" stroke="#9a3e2b" stroke-width="1.5" fill="none"/>
        </svg>
      </div>
    `,
    )
    .join('');

  logger.info('完成首屏 hero 区渲染');
}
