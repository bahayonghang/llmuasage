import { UI_COPY } from '../copy.js';
import { escapeHtml, formatNumber, statusTone } from '../data.js';
import { buildKpis } from '../data/derive.js';

const logger = window.console;
const STATUS_PANEL_MOBILE_QUERY = '(max-width: 720px)';
let statusPanelMediaQuery = null;
let statusPanelMediaBound = false;

function syncStatusPanelDisclosure() {
  const details = document.querySelector('.status-panel-details');
  if (details && statusPanelMediaQuery) {
    details.open = !statusPanelMediaQuery.matches;
  }
}

function ensureStatusPanelResponsive() {
  if (!window.matchMedia) return;
  statusPanelMediaQuery ||= window.matchMedia(STATUS_PANEL_MOBILE_QUERY);
  if (!statusPanelMediaBound) {
    if (statusPanelMediaQuery.addEventListener) {
      statusPanelMediaQuery.addEventListener('change', syncStatusPanelDisclosure);
    } else {
      statusPanelMediaQuery.addListener(syncStatusPanelDisclosure);
    }
    statusPanelMediaBound = true;
  }
  syncStatusPanelDisclosure();
}

function supportedSourcesLabel() {
  const value = document.body?.dataset?.supportedSources || '';
  return value
    .split(',')
    .map((source) => source.trim())
    .filter(Boolean)
    .join(' / ') || '--';
}

/*
 * ========================================================================
 * 步骤1：渲染首屏 hero 区
 * ========================================================================
 * 目标：
 * 1) 填充 hero-meta（生成时间、最近同步、来源数）
 * 2) 填充右侧 status-panel（运行概览卡）
 * 3) 填充 4 张 KPI 卡
 * 4) 文案统一从 UI_COPY 取，不在渲染层散落硬编码字符串
 */
export function renderHero(context) {
  logger.info('开始渲染首屏 hero 区');

  // 1.1 填充 hero-meta
  const { ledgerSummary } = context;
  const heroCopy = UI_COPY.hero;
  const metaItems = [
    { label: heroCopy.rows.generated_at, value: ledgerSummary.generated_at || '--' },
    { label: heroCopy.rows.last_sync_at, value: ledgerSummary.last_sync_at || '--' },
    {
      label: heroCopy.rows.sourceCount,
      value: `${ledgerSummary.active_sources} · ${supportedSourcesLabel()}`,
    },
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
  const panelTone = ledgerSummary.failure_count > 0 ? 'warn' : 'good';
  const statusLabel = ledgerSummary.failure_count > 0 ? heroCopy.statusWarn : heroCopy.statusOk;
  const statusPanelSummary = `${heroCopy.statusTitle} · ${statusLabel} ${health.ready_integrations}/${health.total_integrations}`;
  const statusPanelOpen = !window.matchMedia?.(STATUS_PANEL_MOBILE_QUERY).matches;

  const integrationRows = (health.integrations || [])
    .slice(0, 3)
    .map((row) => {
      const tone = statusTone(row.status);
      const label = tone === 'good' ? heroCopy.statusOk : row.status || heroCopy.statusUnknown;
      return `
      <div class="status-row">
        <span class="status-row-name">${escapeHtml(row.source || '--')}</span>
        <span class="status-row-time">${escapeHtml(row.updated_at || '--')}</span>
        <span class="status-row-state"><span class="dot"></span>${escapeHtml(label)}</span>
      </div>
    `;
    })
    .join('');

  document.getElementById('status-panel').innerHTML = `
    <details class="status-panel-details" ${statusPanelOpen ? 'open' : ''}>
      <summary class="status-panel-summary"><span>${escapeHtml(statusPanelSummary)}</span></summary>
      <div class="status-panel-head">
        <div>
          <div class="status-eyebrow">${escapeHtml(heroCopy.statusEyebrow)}</div>
          <div class="status-panel-title">${escapeHtml(heroCopy.statusTitle)}</div>
        </div>
        <span class="status-pill" data-tone="${panelTone}"><span class="pulse"></span>${escapeHtml(statusLabel)}</span>
      </div>
      <div class="status-grid">
        <div class="status-cell">
          <div class="status-cell-label">${escapeHtml(heroCopy.cell.integrations)}</div>
          <div class="status-cell-value">${health.ready_integrations} / ${health.total_integrations}</div>
        </div>
        <div class="status-cell">
          <div class="status-cell-label">${escapeHtml(heroCopy.cell.cursors)}</div>
          <div class="status-cell-value">${formatNumber(health.cursor_count ?? health.cursors?.length ?? 0)}</div>
        </div>
        <div class="status-cell">
          <div class="status-cell-label">${escapeHtml(heroCopy.cell.failures)}</div>
          <div class="status-cell-value">${ledgerSummary.failure_count}</div>
        </div>
      </div>
      <div class="status-list">
        ${integrationRows}
      </div>
    </details>
  `;
  ensureStatusPanelResponsive();

  // 1.3 填充 4 张 KPI 卡
  const kpis = buildKpis(context);

  document.getElementById('kpi-grid').innerHTML = kpis
    .map(
      (kpi) => `
      <div class="kpi${kpi.featured ? ' featured' : ''}">
        <div class="kpi-label">${escapeHtml(kpi.label)}</div>
        <div class="kpi-value num">${escapeHtml(kpi.value)}<span class="unit">${escapeHtml(kpi.unit)}</span></div>
        <div class="kpi-foot">
          ${kpi.foot.map((line) => `<span>${escapeHtml(line.label)} · <strong class="num">${escapeHtml(line.value)}</strong></span>`).join('')}
        </div>
      </div>
    `,
    )
    .join('');

  const endpointHost = document.getElementById('endpoint-host');
  if (endpointHost) {
    endpointHost.textContent = window.location.host;
  }

  const endpointSync = document.getElementById('endpoint-sync');
  if (endpointSync) {
    endpointSync.textContent = ledgerSummary.last_sync_at || '--';
  }

  logger.info('完成首屏 hero 区渲染');
}
