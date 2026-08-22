import { UI_COPY, getShellCopy, translateStatusLabel } from '../copy.js';
import { escapeHtml, formatDateTime, formatNumber } from '../data.js';

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
 * 3) 文案统一从 UI_COPY 取，不在渲染层散落硬编码字符串
 */
export function renderHero(context) {
  logger.info('开始渲染首屏 hero 区');

  // 1.1 填充 hero-meta
  const { ledgerSummary } = context;
  const heroCopy = UI_COPY.hero;
  const metaItems = [
    { label: heroCopy.rows.generated_at, value: formatDateTime(ledgerSummary.generated_at) },
    { label: heroCopy.rows.last_sync_at, value: formatDateTime(ledgerSummary.last_sync_at) },
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
  const syncCenter = context.syncCommandCenter;
  const panelTone = ['good', 'warn'].includes(syncCenter?.tone) ? syncCenter.tone : 'neutral';
  const statusLabel = panelTone === 'good'
    ? heroCopy.statusOk
    : panelTone === 'warn'
      ? heroCopy.statusWarn
      : heroCopy.statusUnknown;
  const statusHeadline = getShellCopy(syncCenter?.headline_key || 'syncCenter.headline.empty');
  const statusPanelSummary = `${heroCopy.statusTitle} · ${statusHeadline}`;
  const sourcesReady = Number(syncCenter?.metrics?.sources_ready || 0);
  const sourcesTotal = Number(syncCenter?.metrics?.sources_total || 0);
  const sourcesReadyLabel = sourcesTotal > 0
    ? `${formatNumber(sourcesReady)} / ${formatNumber(sourcesTotal)}`
    : '--';
  const latestSyncLabel = syncCenter?.last_run?.status
    ? translateStatusLabel(syncCenter.last_run.status)
    : '--';
  const statusPanelOpen = !window.matchMedia?.(STATUS_PANEL_MOBILE_QUERY).matches;

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
          <div class="status-cell-label">${escapeHtml(heroCopy.cell.sourcesReady)}</div>
          <div class="status-cell-value">${escapeHtml(sourcesReadyLabel)}</div>
        </div>
        <div class="status-cell">
          <div class="status-cell-label">${escapeHtml(heroCopy.cell.latestSync)}</div>
          <div class="status-cell-value small">${escapeHtml(latestSyncLabel)}</div>
        </div>
      </div>
    </details>
  `;
  ensureStatusPanelResponsive();

  const endpointHost = document.getElementById('endpoint-host');
  if (endpointHost) {
    endpointHost.textContent = window.location.host || getShellCopy('shell.endpoint.localFile');
  }

  const endpointSync = document.getElementById('endpoint-sync');
  if (endpointSync) {
    endpointSync.textContent = formatDateTime(ledgerSummary.last_sync_at);
  }

  logger.info('完成首屏 hero 区渲染');
}
