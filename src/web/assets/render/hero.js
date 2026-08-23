import { UI_COPY, getShellCopy, translateStatusLabel } from '../copy.js';
import { escapeHtml, formatDateTime, formatNumber } from '../data.js';

const logger = window.console;
const STATUS_PANEL_MOBILE_QUERY = '(max-width: 720px)';
const FALLBACK_AGENT_LOGO_URL = 'assets/agent-logos/fallback.svg';
const AGENT_LOGO_URL_PATTERN = /^assets\/agent-logos\/[a-z0-9_-]+\.svg$/;
const SOURCE_ID_PATTERN = /^[a-z0-9][a-z0-9_-]*$/;
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

export function supportedSourceIds(value = '') {
  return String(value)
    .split(',')
    .map((source) => source.trim())
    .filter((source) => SOURCE_ID_PATTERN.test(source));
}

function fallbackDisplayName(stableId) {
  return stableId
    .split(/[_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function normalizeCatalogEntry(entry) {
  const id = String(entry?.id || '').trim();
  if (!SOURCE_ID_PATTERN.test(id)) return null;

  const displayName = String(entry?.display_name || '').trim() || fallbackDisplayName(id);
  const requestedLogoUrl = String(entry?.logo_url || '').trim();
  const logoUrl = AGENT_LOGO_URL_PATTERN.test(requestedLogoUrl)
    ? requestedLogoUrl
    : FALLBACK_AGENT_LOGO_URL;
  return { id, display_name: displayName, logo_url: logoUrl };
}

export function parseSourceBadgeCatalog(rawCatalog, supportedSources = '') {
  const supportedIds = Array.isArray(supportedSources)
    ? supportedSources.filter((source) => SOURCE_ID_PATTERN.test(String(source)))
    : supportedSourceIds(supportedSources);
  const catalog = [];
  const seen = new Set();

  try {
    const parsed = typeof rawCatalog === 'string' ? JSON.parse(rawCatalog) : rawCatalog;
    if (Array.isArray(parsed)) {
      for (const candidate of parsed) {
        const entry = normalizeCatalogEntry(candidate);
        if (!entry || seen.has(entry.id)) continue;
        catalog.push(entry);
        seen.add(entry.id);
      }
    }
  } catch (_error) {
    // The compatibility attribute below remains the recovery source.
  }

  for (const id of supportedIds) {
    const stableId = String(id);
    if (seen.has(stableId)) continue;
    catalog.push({
      id: stableId,
      display_name: fallbackDisplayName(stableId),
      logo_url: FALLBACK_AGENT_LOGO_URL,
    });
    seen.add(stableId);
  }
  return catalog;
}

export function renderSourceBadgeList(catalog) {
  const items = catalog
    .map(
      (entry) => `
        <li class="agent-badge" data-source="${escapeHtml(entry.id)}">
          <span class="agent-badge-logo-frame" aria-hidden="true">
            <img class="agent-badge-logo" src="${escapeHtml(entry.logo_url)}" alt="" aria-hidden="true" />
          </span>
          <span class="agent-badge-name">${escapeHtml(entry.display_name)}</span>
        </li>
      `,
    )
    .join('');
  return `<ul class="agent-badge-list" role="list">${items}</ul>`;
}

export function formatSourceSummary(template, activeSources, supportedSources) {
  return String(template)
    .replace('{active}', formatNumber(Number(activeSources) || 0))
    .replace('{supported}', formatNumber(Number(supportedSources) || 0));
}

function readSourceBadgeCatalog() {
  const rawCatalog = document.getElementById('source-badge-catalog')?.textContent || '';
  const supportedSources = document.body?.dataset?.supportedSources || '';
  return parseSourceBadgeCatalog(rawCatalog, supportedSources);
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
  ];
  const sourceCatalog = readSourceBadgeCatalog();
  const sourceSummary = formatSourceSummary(
    heroCopy.sourceSummary,
    ledgerSummary.active_sources,
    sourceCatalog.length,
  );

  const timeMeta = metaItems
    .map(
      (item) => `
      <div class="hero-meta-item">
        ${escapeHtml(item.label)}<span class="mono">${escapeHtml(item.value)}</span>
      </div>
    `,
    )
    .join('');
  document.getElementById('hero-meta').innerHTML = `${timeMeta}
    <div class="hero-meta-item hero-meta-sources">
      <div class="hero-source-heading">
        <span class="hero-source-label">${escapeHtml(heroCopy.rows.sourceCount)}</span>
        <span class="mono hero-source-summary">${escapeHtml(sourceSummary)}</span>
      </div>
      ${renderSourceBadgeList(sourceCatalog)}
    </div>
  `;

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
