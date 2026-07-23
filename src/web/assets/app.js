import { UI_COPY, getLocale, getShellCopy, onLocaleChange, setLocale } from './copy.js';
import {
  buildContext,
  buildExplorerQuery,
  buildFilterQuery,
  clearLiveRequestCache,
  loadDashboardInteractiveSnapshot,
  loadDashboardSecondarySections,
  loadDashboardSnapshot,
  loadExplorer,
} from './data.js';
import { panelFingerprint, stableSerialize } from './data/render-key.js';
import {
  SECONDARY_SECTIONS,
  armDashboardCoreDeadline,
  classifyDashboardError,
  createDashboardLoadState,
  dashboardLocaleRenderMode,
  isDegradedSectionPayload,
  reduceDashboardLoadState,
  runLoadersWithConcurrency,
} from './load-state.js';
import { renderHero } from './render/hero.js';
import { renderDashboardLoadInstrument, renderSyncCommandCenter } from './render/sync-command-center.js';
import { renderTrends } from './render/trends.js';
import { renderModels } from './render/models.js';
import { renderSources } from './render/sources.js';
import { renderProjects } from './render/projects.js';
import { renderCosts } from './render/costs.js';
import { renderInsights } from './render/insights.js';
import {
  renderActivity,
  renderCompare,
  renderOptimize,
  renderTools,
} from './render/behavior.js';
import { renderExplorer } from './render/explorer.js';
import { applyDomI18n, bindI18nDomSync } from './i18n.js';
import { initTheme, toggleTheme } from './theme.js';
import { setRenderer, setRuntimeState } from './runtime.js';

window.__LLMUSAGE_BOOTSTRAP__?.claim?.();

const logger = window.console;
const DEFAULT_TREND_WINDOW = 'day';
const VALID_TREND_WINDOWS = new Set(['day', 'week', 'month', 'all']);
const DEFAULT_RANGE_PRESET = '1d';
const CUSTOM_RANGE_PRESET = 'custom';
const VALID_RANGE_PRESETS = new Set(['1d', '7d', '30d', 'all', CUSTOM_RANGE_PRESET]);
const RANGE_TO_TREND_WINDOW = Object.freeze({ '1d': 'day', '7d': 'week', '30d': 'month', all: 'all' });
const TREND_WINDOW_TO_RANGE = Object.freeze({ day: '1d', week: '7d', month: '30d', all: 'all' });
const ISO_DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/;
const AUTO_REFRESH_STORAGE_KEY = 'llmusage:autoRefreshMs';
const VALID_AUTO_REFRESH_MS = new Set([0, 30000, 60000]);
const SECONDARY_LOAD_CONCURRENCY = 2;
const JOB_POLL_INTERVAL_MS = 900;
const JOB_POLL_MAX_DURATION_MS = 30 * 60 * 1000;
const DEFAULT_EXPLORER = Object.freeze({
  granularity: 'day',
  metric: 'attributed_cost_usd',
  groupBy: 'source',
  sessionId: '',
  toolName: '',
  toolKind: '',
  tokenType: '',
  limit: 8,
  includeOther: true,
  includeNonTool: true,
});
let dashboardState = null;

/*
 * ========================================================================
 * 步骤1：主入口
 * ========================================================================
 * 目标：
 * 1) 主题与语言先于数据生效，避免渲染过程中读到错误的 UI_COPY
 * 2) 按 live / snapshot 模式加载 dashboard 数据
 * 3) 调用 buildContext 派生渲染上下文
 * 4) 依次调用各区域 render 函数
 * 5) 设置侧边栏高亮、趋势窗口切换、JSON 导出、主题与语言切换
 */
async function main() {
  logger.info('llmusage dashboard 启动');

  // 1.1 提前应用持久化的主题与 locale
  initTheme();
  applyDomI18n(document);
  bindI18nDomSync();

  const state = {
    mode: document.body?.dataset?.mode === 'snapshot' ? 'snapshot' : 'live',
    trendWindow: initialTrendWindowFromUrl(),
    rangePreset: initialRangePresetFromUrl(),
    filters: readFiltersFromUrl(),
    explorer: initialExplorerStateFromUrl(),
    autoRefreshMs: readAutoRefreshPreference(),
    autoRefreshTimer: null,
    reloadPromise: null,
    reloadGeneration: 0,
    rangeReloadController: null,
    loadCompletionFrame: null,
    secondaryRefreshing: false,
    loadState: createDashboardLoadState(0),
    rawData: null,
    expanded: {
      models: false,
      projects: false,
      costs: false,
    },
  };
  dashboardState = state;

  setRuntimeState(state);
  setRenderer(renderDashboard);
  syncFilterControls(state);
  setupNavigation();
  setupFilterControls(state);
  setupExplorerControls(state);
  setupPanelToggles(state);
  setupExport(state);
  setupSyncJob(state);
  setupAutoRefresh(state);
  setupThemeToggle();
  setupLocaleToggle(state);
  setupDashboardRetry(state);
  renderDashboardLoadInstrument(state.loadState);
  window.__LLMUSAGE_BOOTSTRAP__?.ready?.();
  onLocaleChange(() => {
    const renderMode = dashboardLocaleRenderMode(state);
    if (renderMode === 'error') {
      renderBootstrapError(new Error(state.loadState.errorMessage || getShellCopy('shell.load.errorDetail')), state);
      return;
    }
    if (renderMode === 'data') renderDashboard(state.rawData);
    renderDashboardLoadInstrument(state.loadState);
  });

  try {
    // 1.2 live 首屏先加载 interactive core；snapshot 保持完整载入。
    if (state.mode === 'snapshot') {
      state.rawData = await loadDashboardData(state);
      state.loadState = { ...state.loadState, phase: 'complete' };
      renderDashboard(state.rawData);
    } else {
      await loadDashboardProgressive(state);
    }

    logger.info('llmusage dashboard 渲染完成');
  } catch (error) {
    logger.error('llmusage dashboard 数据加载失败', error);
    renderBootstrapError(error);
  }
}

async function loadDashboardData(state) {
  logger.info('开始加载 dashboard 数据');
  let snapshot = await loadDashboardSnapshot(state);
  if (state.mode !== 'snapshot' && (!hasUsableExplorer(snapshot) || !isDefaultExplorerState(state))) {
    snapshot = { ...snapshot };
    try {
      snapshot.explorer = await loadExplorer(state);
    } catch (error) {
      logger.warn('/api/explorer degraded', error);
      snapshot.explorer = degradedExplorerPayload(error);
    }
  }
  logger.info('完成 dashboard 数据加载');
  return snapshot;
}

function hasUsableExplorer(snapshot) {
  return Boolean(snapshot?.explorer?.support || snapshot?.explorer?.rows || snapshot?.explorer?.series);
}

function isDefaultExplorerState(state) {
  const explorer = { ...DEFAULT_EXPLORER, ...(state?.explorer || {}) };
  return (
    explorer.granularity === DEFAULT_EXPLORER.granularity
    && explorer.metric === DEFAULT_EXPLORER.metric
    && explorer.groupBy === DEFAULT_EXPLORER.groupBy
    && !explorer.sessionId
    && !explorer.toolName
    && !explorer.toolKind
    && !explorer.tokenType
    && clampExplorerLimit(explorer.limit) === DEFAULT_EXPLORER.limit
    && explorer.includeOther !== false
    && explorer.includeNonTool !== false
  );
}

function stableFilterKey(filters = {}) {
  const params = new URLSearchParams();
  for (const key of ['source', 'model', 'project_hash', 'timezone']) {
    if (filters[key]) {
      params.set(key, filters[key]);
    }
  }
  return params.toString();
}

function sameStableFilters(left, right) {
  return stableFilterKey(left) === stableFilterKey(right);
}

function mergeCoreSnapshot(previous, core, options = {}) {
  return {
    ...(previous || {}),
    overview: core?.overview,
    trends: core?.trends,
    models: core?.models,
    sources: core?.sources,
    projects: core?.projects,
    costs: core?.costs,
    health: core?.health,
    diagnostics: core?.diagnostics,
    sync_command_center: core?.sync_command_center,
    _meta: {
      ...((previous && previous._meta) || {}),
      secondary_refreshing: Boolean(options.secondaryRefreshing),
    },
  };
}

/*
 * 面板级 dirty-check 注册表：每个渲染入口先比对"本面板数据指纹"，
 * 未变则跳过 DOM 写入。指纹由 data/render-key.js 计算（稳定序列化 +
 * 易变字段剔除，key 含 locale——locale 切换令指纹自然失效，无需特判）。
 */
const panelFingerprintCache = new Map();

function renderPanel(panel, fingerprint, render) {
  if (fingerprint && panelFingerprintCache.get(panel) === fingerprint) {
    return false;
  }
  render();
  if (fingerprint) {
    panelFingerprintCache.set(panel, fingerprint);
  }
  return true;
}

function jobFingerprintPart(snapshot) {
  if (!snapshot) return null;
  return {
    status: snapshot.status,
    job_id: snapshot.job_id,
    summary: snapshot.summary,
    started_at: snapshot.started_at,
    finished_at: snapshot.finished_at,
    last_event: snapshot.last_event ?? null,
  };
}

function renderDashboard(rawData) {
  renderPrimaryDashboard(rawData);
  renderBehaviorSections(rawData);
  renderExplorerPanel(rawData);
}

function renderPrimaryDashboard(rawData) {
  const context = buildContext(rawData);
  const locale = getLocale();
  const expanded = dashboardState?.expanded || {};

  renderPanel(
    'syncCommandCenter',
    panelFingerprint('syncCommandCenter', rawData, { locale, extra: jobFingerprintPart(dashboardState?.activeJobSnapshot) }),
    () => renderSyncCommandCenter(context, dashboardState),
  );
  renderPanel('hero', panelFingerprint('hero', rawData, { locale }), () => renderHero(context));
  renderPanel('trends', panelFingerprint('trends', rawData, { locale }), () => renderTrends(context));
  renderPanel(
    'models',
    panelFingerprint('models', rawData, { locale, extra: { expanded: Boolean(expanded.models) } }),
    () => renderModels(context, dashboardState),
  );
  renderPanel('sources', panelFingerprint('sources', rawData, { locale }), () => renderSources(context));
  renderPanel(
    'projects',
    panelFingerprint('projects', rawData, { locale, extra: { expanded: Boolean(expanded.projects) } }),
    () => renderProjects(context, dashboardState),
  );
  renderPanel(
    'costs',
    panelFingerprint('costs', rawData, { locale, extra: { expanded: Boolean(expanded.costs) } }),
    () => renderCosts(context, dashboardState),
  );
  renderPanel('insights', panelFingerprint('insights', rawData, { locale }), () => renderInsights(context));
  syncPanelToggleControls(context, dashboardState);
  syncFilterControls(dashboardState, context);
}

const SECONDARY_SECTION_RENDERERS = {
  activity: renderActivity,
  tools: renderTools,
  optimize: renderOptimize,
  compare: renderCompare,
};

function secondaryPanelOptions(rawData) {
  return {
    locale: getLocale(),
    extra: { refreshing: Boolean(rawData?._meta?.secondary_refreshing) },
  };
}

function renderBehaviorSections(rawData) {
  const options = secondaryPanelOptions(rawData);
  for (const [section, renderer] of Object.entries(SECONDARY_SECTION_RENDERERS)) {
    // 先算指纹、脏才 buildContext：数据未变时整条链零派生、零 DOM 写入
    renderPanel(section, panelFingerprint(section, rawData, options), () => renderer(buildContext(rawData)));
  }
}

function secondaryLoadingPayload(section) {
  const support = { supported: false, level: 'loading', reason: null };
  switch (section) {
    case 'activity': return { support, breakdown: [] };
    case 'tools': return { support, breakdown: [] };
    case 'optimize': return { support, findings: [], score: null, grade: null };
    case 'explorer': return { support, rows: [], series: [], totals: { value: 0 } };
    case 'compare': return { support, candidates: [], metrics: [], working_style: [] };
    default: return { support };
  }
}

function secondaryFailurePayload(section, error) {
  const payload = secondaryLoadingPayload(section);
  return {
    ...payload,
    support: {
      supported: false,
      level: 'degraded',
      reason: error?.message || 'Secondary dashboard section failed.',
    },
  };
}

function renderCurrentLoadState(state) {
  if (state.loadState?.phase === 'complete') {
    renderDashboardLoadInstrument({
      ...state.loadState,
      phase: 'secondary_loading',
      terminal: true,
    });
    if (state.loadCompletionFrame === null) {
      const generation = state.loadState.generation;
      state.loadCompletionFrame = window.requestAnimationFrame(() => {
        state.loadCompletionFrame = null;
        if (state.loadState?.phase !== 'complete' || state.loadState.generation !== generation) return;
        panelFingerprintCache.delete('syncCommandCenter');
        refreshSyncCommandCenter(state);
      });
    }
    return;
  }
  renderDashboardLoadInstrument(state.loadState);
}

async function loadInteractiveCore(state, generation, controller, options = {}) {
  let timedOut = false;
  const cancelDeadline = armDashboardCoreDeadline({
    setTimer: (callback, delay) => window.setTimeout(callback, delay),
    clearTimer: (timer) => window.clearTimeout(timer),
    onSlow: () => {
      if (generation !== state.reloadGeneration || controller.signal.aborted) return;
      state.loadState = reduceDashboardLoadState(state.loadState, { type: 'slow', generation });
      if (!options.silent) renderCurrentLoadState(state);
    },
    onTimeout: () => {
      timedOut = true;
      controller.abort();
    },
  });

  try {
    return await loadDashboardInteractiveSnapshot(state, {
      signal: controller.signal,
      legacyFallback: false,
    });
  } catch (error) {
    if (timedOut) {
      const timeoutError = new Error(getShellCopy('shell.load.timeout'));
      timeoutError.dashboardErrorKind = 'timeout';
      throw timeoutError;
    }
    throw error;
  } finally {
    cancelDeadline();
  }
}

async function loadDashboardProgressive(state, options = {}) {
  const generation = ++state.reloadGeneration;
  if (state.loadCompletionFrame !== null) {
    window.cancelAnimationFrame(state.loadCompletionFrame);
    state.loadCompletionFrame = null;
  }
  state.rangeReloadController?.abort();
  const controller = new AbortController();
  state.rangeReloadController = controller;
  const previous = options.retainPrevious ? state.rawData : null;
  state.secondaryRefreshing = Boolean(previous) && !options.silent;
  state.loadState = reduceDashboardLoadState(state.loadState, {
    type: 'retry',
    generation,
    startedAtMs: Date.now(),
  });
  if (!options.silent) renderCurrentLoadState(state);
  if (previous && !options.silent) {
    state.rawData = {
      ...previous,
      _meta: { ...previous?._meta, secondary_refreshing: true },
    };
    renderBehaviorSections(state.rawData);
    renderExplorerPanel(state.rawData);
  }

  try {
    const core = await loadInteractiveCore(state, generation, controller, { silent: Boolean(options.silent) });
    if (generation !== state.reloadGeneration || controller.signal.aborted) return state.rawData;

    state.loadState = reduceDashboardLoadState(state.loadState, { type: 'core_succeeded', generation });
    state.rawData = mergeCoreSnapshot(previous, core, { secondaryRefreshing: state.secondaryRefreshing });
    if (!previous) {
      for (const section of SECONDARY_SECTIONS) {
        state.rawData = { ...state.rawData, [section]: secondaryLoadingPayload(section) };
      }
    }
    syncUrlFromState(state);
    renderDashboard(state.rawData);
    if (!options.silent) renderCurrentLoadState(state);
    updateSyncButton(state);

    void settleSecondarySections(state, generation, controller, { silent: Boolean(options.silent) });
    return state.rawData;
  } catch (error) {
    if (generation !== state.reloadGeneration) return state.rawData;
    const errorKind = error.dashboardErrorKind || classifyDashboardError(error);
    if (errorKind === 'cancelled') return state.rawData;
    state.loadState = reduceDashboardLoadState(state.loadState, {
      type: 'failed',
      generation,
      errorKind,
      message: error?.message,
    });
    if (options.silent) {
      state.loadState = { ...state.loadState, phase: 'complete' };
      throw error;
    }
    renderBootstrapError(error, state);
    throw error;
  }
}

async function settleSecondarySections(state, generation, controller, options = {}) {
  const loaders = loadDashboardSecondarySections(state, { signal: controller.signal });
  await runSecondaryLoaders(loaders, (section, payload, error) => {
    if (generation !== state.reloadGeneration || controller.signal.aborted) return;
    const sectionPayload = error ? secondaryFailurePayload(section, error) : payload;
    const degraded = isDegradedSectionPayload(sectionPayload);
    state.rawData = { ...state.rawData, [section]: sectionPayload };
    state.loadState = reduceDashboardLoadState(state.loadState, {
      type: 'secondary_settled',
      generation,
      section,
      degraded,
    });
    renderSecondarySection(section, state.rawData);
    if (!options.silent) renderCurrentLoadState(state);
  });
  if (generation !== state.reloadGeneration || controller.signal.aborted) return;
  state.secondaryRefreshing = false;
  state.rawData = {
    ...state.rawData,
    _meta: { ...state.rawData?._meta, secondary_refreshing: false },
  };
  renderBehaviorSections(state.rawData);
  renderExplorerPanel(state.rawData);
  renderCurrentLoadState(state);
}

function renderExplorerPanel(rawData) {
  renderPanel(
    'explorer',
    panelFingerprint('explorer', rawData, secondaryPanelOptions(rawData)),
    () => {
      // 先算指纹、脏才 buildContext：数据未变时整条链零派生、零 DOM 写入
      const context = buildContext(rawData);
      renderExplorer(context, dashboardState);
    },
  );
}

// secondary 到达回调：按 section 路由，只更新自己的 section。
function renderSecondarySection(section, rawData) {
  if (section === 'explorer') {
    renderExplorerPanel(rawData);
    return;
  }
  const renderer = SECONDARY_SECTION_RENDERERS[section];
  if (!renderer) return;
  renderPanel(section, panelFingerprint(section, rawData, secondaryPanelOptions(rawData)), () => renderer(buildContext(rawData)));
}

function isAbortError(error) {
  return error?.name === 'AbortError';
}

function runSecondaryLoaders(loaders, onResult) {
  return runLoadersWithConcurrency(loaders, SECONDARY_LOAD_CONCURRENCY, onResult);
}

function refreshSyncCommandCenter(state = dashboardState) {
  if (!state?.rawData) return;
  const context = buildContext(state.rawData);
  renderPanel(
    'syncCommandCenter',
    panelFingerprint('syncCommandCenter', state.rawData, { locale: getLocale(), extra: jobFingerprintPart(state.activeJobSnapshot) }),
    () => renderSyncCommandCenter(context, state),
  );
}

function appVersion() {
  return document.body?.dataset?.appVersion || 'unknown';
}

function readFiltersFromUrl() {
  const params = new URLSearchParams(window.location.search || '');
  const filters = {};
  for (const key of ['source', 'model', 'since', 'until', 'project_hash', 'timezone']) {
    const value = params.get(key);
    if (value && value !== 'all') {
      filters[key] = value;
    }
  }
  return filters;
}

function initialTrendWindowFromUrl() {
  const params = new URLSearchParams(window.location.search || '');
  const requestedWindow = params.get('window');
  if (VALID_TREND_WINDOWS.has(requestedWindow)) {
    return requestedWindow;
  }
  return RANGE_TO_TREND_WINDOW[params.get('range')] || DEFAULT_TREND_WINDOW;
}

function initialRangePresetFromUrl() {
  const params = new URLSearchParams(window.location.search || '');
  if (params.get('since') || params.get('until')) {
    return CUSTOM_RANGE_PRESET;
  }

  const requestedRange = params.get('range');
  if (VALID_RANGE_PRESETS.has(requestedRange) && requestedRange !== CUSTOM_RANGE_PRESET) {
    return requestedRange;
  }

  return TREND_WINDOW_TO_RANGE[params.get('window')] || DEFAULT_RANGE_PRESET;
}

function initialExplorerStateFromUrl() {
  const params = new URLSearchParams(window.location.search || '');
  const next = { ...DEFAULT_EXPLORER };
  if (params.get('granularity')) next.granularity = params.get('granularity');
  if (params.get('metric')) next.metric = params.get('metric');
  if (params.get('group_by')) next.groupBy = params.get('group_by');
  if (params.get('session_id') || params.get('session')) {
    next.sessionId = params.get('session_id') || params.get('session');
  }
  if (params.get('tool_name') || params.get('tool')) {
    next.toolName = params.get('tool_name') || params.get('tool');
  }
  if (params.get('tool_kind')) next.toolKind = params.get('tool_kind');
  if (params.get('token_type')) next.tokenType = params.get('token_type');
  if (params.get('limit')) {
    next.limit = clampExplorerLimit(params.get('limit'));
  }
  if (params.has('include_other')) {
    next.includeOther = parseQueryBool(params.get('include_other'));
  }
  if (params.has('is_tool')) {
    next.includeNonTool = !parseQueryBool(params.get('is_tool'));
  }
  return next;
}

function readAutoRefreshPreference() {
  try {
    const stored = Number(window.localStorage?.getItem(AUTO_REFRESH_STORAGE_KEY) || 0);
    return VALID_AUTO_REFRESH_MS.has(stored) ? stored : 0;
  } catch (_) {
    return 0;
  }
}

function degradedExplorerPayload(error) {
  return {
    support: {
      supported: false,
      level: 'degraded',
      reason: error?.message || 'Explorer query failed; fixed dashboard panels are still available.',
      strategy: 'unknown',
    },
    warning: error?.message || 'Explorer query failed.',
    granularity: 'day',
    metric: 'attributed_cost_usd',
    group_by: 'source',
    limit: 8,
    include_other: true,
    totals: { value: 0 },
    rows: [],
    series: [],
  };
}

function persistAutoRefreshPreference(value) {
  try {
    window.localStorage?.setItem(AUTO_REFRESH_STORAGE_KEY, String(value));
  } catch (_) {
    /* localStorage may be unavailable in restricted contexts. */
  }
}

function renderBootstrapError(error, state = dashboardState) {
  if (state?.loadState?.phase !== 'error') {
    state.loadState = reduceDashboardLoadState(state.loadState, {
      type: 'failed',
      generation: state.loadState?.generation,
      errorKind: classifyDashboardError(error),
      message: error?.message,
    });
  }
  const errorCopy = UI_COPY.hero.error;
  const message = error?.message || errorCopy.generic;
  const hostEl = document.getElementById('endpoint-host');
  if (hostEl) {
    hostEl.textContent = window.location.host;
  }

  const syncEl = document.getElementById('endpoint-sync');
  if (syncEl) {
    syncEl.textContent = '--';
  }

  const statusCard = `
    <div class="status-panel-head">
      <div>
        <div class="status-eyebrow">${escapeHtml(UI_COPY.hero.statusEyebrow)}</div>
        <div class="status-panel-title">${escapeHtml(errorCopy.title)}</div>
      </div>
      <span class="status-pill" data-tone="warn"><span class="pulse"></span>${escapeHtml(errorCopy.pill)}</span>
    </div>
    <div class="status-list">
      <div class="status-row">
        <span class="status-row-name">${escapeHtml(errorCopy.detail)}</span>
        <span class="status-row-time mono">${escapeHtml(message)}</span>
      </div>
    </div>
  `;
  const errorBlock = `
    <div class="bootstrap-error">
      ${escapeHtml(message)}
    </div>
  `;

  const heroMeta = document.getElementById('hero-meta');
  if (heroMeta) {
    heroMeta.innerHTML = `
      <div class="hero-meta-item">
        ${escapeHtml(errorCopy.heroMeta)}<span class="mono">${escapeHtml(errorCopy.heroMetaState)}</span>
      </div>
    `;
  }

  const statusPanel = document.getElementById('status-panel');
  if (statusPanel) {
    statusPanel.innerHTML = statusCard;
  }

  for (const id of ['kpi-grid', 'trends-stats', 'trends-table', 'trends-sources']) {
    const el = document.getElementById(id);
    if (el) {
      el.innerHTML = errorBlock;
    }
  }

  const bars = document.getElementById('trends-bars');
  if (bars) {
    bars.innerHTML = '';
  }

  const labels = document.getElementById('trends-labels');
  if (labels) {
    labels.innerHTML = '';
  }
  renderDashboardLoadInstrument(state?.loadState);
}

function setupDashboardRetry(state) {
  document.addEventListener('click', (event) => {
    const button = event.target.closest?.('[data-dashboard-retry]');
    if (!button || button.disabled || state.mode === 'snapshot') return;
    button.disabled = true;
    clearLiveRequestCache();
    state.reloadPromise = null;
    void loadDashboardProgressive(state, { retainPrevious: Boolean(state.rawData) }).catch((error) => {
      if (!isAbortError(error)) logger.error('dashboard retry failed', error);
    });
  });
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

/*
 * ========================================================================
 * 步骤2：设置侧边栏导航高亮
 * ========================================================================
 * 目标：
 * 1) 使用 IntersectionObserver 监听各区域
 * 2) 当区域进入视口时，高亮对应侧边栏链接
 */
function setupNavigation() {
  const sections = ['overview', 'trends', 'models', 'sources', 'projects', 'behavior', 'explorer', 'cost', 'status'];
  const navLinks = document.querySelectorAll('aside nav a');

  function setActive(id) {
    navLinks.forEach((a) => {
      a.classList.toggle('active', a.dataset.target === id);
    });
  }

  const observer = new IntersectionObserver(
    (entries) => {
      const visible = entries
        .filter((e) => e.isIntersecting)
        .sort((a, b) => b.intersectionRatio - a.intersectionRatio);

      if (visible[0]) {
        setActive(visible[0].target.id);
      }
    },
    {
      threshold: [0.1, 0.4, 0.7],
      rootMargin: '-100px 0px -50% 0px',
    },
  );

  sections.forEach((id) => {
    const el = document.getElementById(id);
    if (el) observer.observe(el);
  });

  document.querySelectorAll('a[data-target="projects"]').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      const reduceMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
      document.getElementById('projects')?.scrollIntoView({ behavior: reduceMotion ? 'auto' : 'smooth', block: 'start' });
      setActive('projects');
    });
  });
}

function sourceOptions() {
  const configured = document.body?.dataset?.supportedSources || '';
  return configured
    .split(',')
    .map((source) => source.trim())
    .filter(Boolean);
}

function populateSourceFilter() {
  const select = document.querySelector('[data-filter="source"]');
  if (!select || select.dataset.populated === 'true') return;

  const options = sourceOptions()
    .map((source) => `<option value="${escapeHtml(source)}">${escapeHtml(source)}</option>`)
    .join('');
  select.insertAdjacentHTML('beforeend', options);
  select.dataset.populated = 'true';
}

function syncFilterControls(state, context = null) {
  if (!state) return;
  populateSourceFilter();

  document.querySelectorAll('[data-window]').forEach((button) => {
    const active = button.dataset.window === state.trendWindow;
    button.classList.toggle('active', active);
    const pressed = String(active);
    if (button.getAttribute('aria-pressed') !== pressed) {
      button.setAttribute('aria-pressed', pressed);
    }
  });
  syncRangePresetControls(state);

  const filters = state.filters || {};
  const snapshotMode = state.mode === 'snapshot';
  const source = document.querySelector('[data-filter="source"]');
  if (source) source.value = filters.source || 'all';
  for (const key of ['model', 'since', 'until']) {
    const input = document.querySelector(`[data-filter="${key}"]`);
    if (input && document.activeElement !== input) {
      input.value = filters[key] || '';
    }
  }

  if (context) {
    const models = new Set((context.panels?.models || []).map((row) => row.model).filter(Boolean));
    const modelInput = document.querySelector('[data-filter="model"]');
    if (modelInput) {
      modelInput.setAttribute('list', 'filter-model-options');
      let list = document.getElementById('filter-model-options');
      if (!list) {
        list = document.createElement('datalist');
        list.id = 'filter-model-options';
        modelInput.after(list);
      }
      const modelOptions = [...models].slice(0, 50);
      const modelOptionsKey = modelOptions.join(' ');
      if (list.dataset.optionsKey !== modelOptionsKey) {
        list.dataset.optionsKey = modelOptionsKey;
        list.innerHTML = modelOptions
          .map((model) => `<option value="${escapeHtml(model)}"></option>`)
          .join('');
      }
    }
  }

  document
    .querySelectorAll('#filter-rail [data-filter], #filter-rail [data-range-preset], #filters-apply, #filters-reset')
    .forEach((el) => {
      el.disabled = snapshotMode;
      if (snapshotMode) {
        el.setAttribute('title', getShellCopy('shell.filters.snapshotDisabled'));
      } else {
        el.removeAttribute('title');
      }
    });
}

function currentFilterInputs() {
  const source = document.querySelector('[data-filter="source"]')?.value || 'all';
  const filters = {};
  if (source && source !== 'all') filters.source = source;
  for (const key of ['model', 'since', 'until']) {
    const value = document.querySelector(`[data-filter="${key}"]`)?.value?.trim();
    if (!value) continue;
    if (key === 'since' || key === 'until') {
      const normalized = normalizeIsoDateValue(value);
      if (normalized) filters[key] = normalized;
    } else {
      filters[key] = value;
    }
  }
  return filters;
}

function parseQueryBool(value) {
  return !['0', 'false', 'no', 'off'].includes(String(value || '').trim().toLowerCase());
}

function clampExplorerLimit(value) {
  const parsed = Number.parseInt(String(value || ''), 10);
  if (!Number.isFinite(parsed)) return DEFAULT_EXPLORER.limit;
  return Math.max(1, Math.min(50, parsed));
}

function normalizeRangePreset(value) {
  return VALID_RANGE_PRESETS.has(value) ? value : DEFAULT_RANGE_PRESET;
}

function syncRangePresetControls(state) {
  const activePreset = normalizeRangePreset(state?.rangePreset);
  document.querySelectorAll('[data-range-preset]').forEach((button) => {
    const active = activePreset !== CUSTOM_RANGE_PRESET && button.dataset.rangePreset === activePreset;
    button.classList.toggle('active', active);
    const pressed = String(active);
    if (button.getAttribute('aria-pressed') !== pressed) {
      button.setAttribute('aria-pressed', pressed);
    }
  });
}

function setCustomRangeFromInputs(state) {
  const hasDate = Boolean(
    document.querySelector('[data-filter="since"]')?.value?.trim() ||
      document.querySelector('[data-filter="until"]')?.value?.trim(),
  );
  state.rangePreset = hasDate ? CUSTOM_RANGE_PRESET : DEFAULT_RANGE_PRESET;
  syncRangePresetControls(state);
}

function normalizeIsoDateValue(value) {
  const parsed = parseIsoDate(value);
  return parsed ? formatIsoDate(parsed) : null;
}

function parseIsoDate(value) {
  const raw = String(value || '').trim();
  if (!ISO_DATE_PATTERN.test(raw)) return null;
  const [year, month, day] = raw.split('-').map((part) => Number(part));
  const date = new Date(Date.UTC(year, month - 1, day));
  if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) {
    return null;
  }
  return date;
}

function formatIsoDate(date) {
  const year = date.getUTCFullYear();
  const month = String(date.getUTCMonth() + 1).padStart(2, '0');
  const day = String(date.getUTCDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function utcMonthStart(date) {
  return new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), 1));
}

function addUtcMonths(date, delta) {
  return new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + delta, 1));
}

function localTodayAsUtcDate() {
  const now = new Date();
  return new Date(Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()));
}

function sameUtcDate(a, b) {
  return Boolean(
    a &&
      b &&
      a.getUTCFullYear() === b.getUTCFullYear() &&
      a.getUTCMonth() === b.getUTCMonth() &&
      a.getUTCDate() === b.getUTCDate(),
  );
}

function datePickerLocale() {
  return getLocale() === 'zh' ? 'zh-CN' : 'en-US';
}

function datePickerMonthTitle(date) {
  return new Intl.DateTimeFormat(datePickerLocale(), {
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  }).format(date);
}

function datePickerWeekdays() {
  const value = getShellCopy('shell.date.weekdays');
  return value.split('|').filter(Boolean);
}

const datePickerState = {
  popover: null,
  input: null,
  state: null,
  month: null,
};

function ensureDatePickerPopover() {
  if (datePickerState.popover) return datePickerState.popover;
  const popover = document.createElement('div');
  popover.className = 'date-picker-popover';
  popover.setAttribute('role', 'dialog');
  popover.setAttribute('aria-modal', 'false');
  popover.addEventListener('click', handleDatePickerClick);
  document.body.appendChild(popover);
  datePickerState.popover = popover;
  return popover;
}

function openDatePicker(input, state) {
  if (input.disabled) return;
  const selected = parseIsoDate(input.value) || localTodayAsUtcDate();
  datePickerState.input = input;
  datePickerState.state = state;
  datePickerState.month = utcMonthStart(selected);
  ensureDatePickerPopover();
  renderDatePicker();
}

function closeDatePicker() {
  datePickerState.popover?.remove();
  datePickerState.popover = null;
  datePickerState.input = null;
  datePickerState.state = null;
  datePickerState.month = null;
}

function positionDatePicker() {
  const { popover, input } = datePickerState;
  if (!popover || !input) return;
  const rect = input.getBoundingClientRect();
  const width = popover.offsetWidth || 260;
  const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
  const top = Math.min(rect.bottom + 6, window.innerHeight - popover.offsetHeight - 8);
  popover.style.left = `${left}px`;
  popover.style.top = `${Math.max(8, top)}px`;
}

function renderDatePicker() {
  const { popover, input } = datePickerState;
  if (!popover || !input) return;

  const selected = parseIsoDate(input.value);
  const today = localTodayAsUtcDate();
  const month = datePickerState.month || utcMonthStart(selected || today);
  const firstDay = month.getUTCDay();
  const gridStart = new Date(Date.UTC(month.getUTCFullYear(), month.getUTCMonth(), 1 - firstDay));
  const weekdays = datePickerWeekdays();

  let cells = weekdays
    .map((day) => `<div class="date-picker-weekday">${escapeHtml(day)}</div>`)
    .join('');

  for (let index = 0; index < 42; index += 1) {
    const day = new Date(Date.UTC(gridStart.getUTCFullYear(), gridStart.getUTCMonth(), gridStart.getUTCDate() + index));
    const iso = formatIsoDate(day);
    const classes = ['date-picker-day'];
    if (day.getUTCMonth() !== month.getUTCMonth()) classes.push('is-outside');
    if (sameUtcDate(day, today)) classes.push('is-today');
    if (sameUtcDate(day, selected)) classes.push('is-selected');
    cells += `<button type="button" class="${classes.join(' ')}" data-date-value="${iso}">${day.getUTCDate()}</button>`;
  }

  popover.innerHTML = `
    <div class="date-picker-head">
      <button class="date-picker-nav" type="button" data-date-nav="-1" aria-label="${escapeHtml(getShellCopy('shell.date.prevMonth'))}">‹</button>
      <div class="date-picker-title">${escapeHtml(datePickerMonthTitle(month))}</div>
      <button class="date-picker-nav" type="button" data-date-nav="1" aria-label="${escapeHtml(getShellCopy('shell.date.nextMonth'))}">›</button>
    </div>
    <div class="date-picker-grid">${cells}</div>
    <div class="date-picker-actions">
      <button type="button" data-date-action="clear">${escapeHtml(getShellCopy('shell.date.clear'))}</button>
      <button type="button" data-date-action="today">${escapeHtml(getShellCopy('shell.date.today'))}</button>
    </div>
  `;
  positionDatePicker();
}

function handleDatePickerClick(event) {
  const nav = event.target.closest('[data-date-nav]');
  if (nav) {
    datePickerState.month = addUtcMonths(datePickerState.month || localTodayAsUtcDate(), Number(nav.dataset.dateNav || 0));
    renderDatePicker();
    return;
  }

  const action = event.target.closest('[data-date-action]')?.dataset?.dateAction;
  if (action === 'clear') {
    datePickerState.input.value = '';
    datePickerState.input.dispatchEvent(new Event('input', { bubbles: true }));
    closeDatePicker();
    return;
  }
  if (action === 'today') {
    datePickerState.input.value = formatIsoDate(localTodayAsUtcDate());
    datePickerState.input.dispatchEvent(new Event('input', { bubbles: true }));
    closeDatePicker();
    return;
  }

  const day = event.target.closest('[data-date-value]');
  if (day) {
    datePickerState.input.value = day.dataset.dateValue;
    datePickerState.input.dispatchEvent(new Event('input', { bubbles: true }));
    closeDatePicker();
  }
}

function setupDateInputs(state) {
  document.querySelectorAll('[data-date-input]').forEach((input) => {
    input.addEventListener('focus', () => openDatePicker(input, state));
    input.addEventListener('click', () => openDatePicker(input, state));
    input.addEventListener('input', () => setCustomRangeFromInputs(state));
    input.addEventListener('change', () => {
      const normalized = normalizeIsoDateValue(input.value);
      if (normalized) input.value = normalized;
      setCustomRangeFromInputs(state);
    });
    input.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') closeDatePicker();
    });
  });

  document.addEventListener('click', (event) => {
    if (!datePickerState.popover) return;
    if (event.target.closest('.date-picker-popover') || event.target.closest('[data-date-input]')) return;
    closeDatePicker();
  });
  window.addEventListener('resize', positionDatePicker);
  window.addEventListener('scroll', positionDatePicker, true);
  onLocaleChange(renderDatePicker);
}

function setupRangePresetControls(state) {
  syncRangePresetControls(state);
  const group = document.getElementById('range-presets');
  if (!group || state.mode === 'snapshot') return;

  group.addEventListener('click', async (event) => {
    const button = event.target.closest('[data-range-preset]');
    if (!button || button.disabled) return;
    const preset = button.dataset.rangePreset;
    if (!VALID_RANGE_PRESETS.has(preset) || preset === CUSTOM_RANGE_PRESET) return;

    try {
      const previousFilters = { ...(state.filters || {}) };
      const filters = currentFilterInputs();
      delete filters.since;
      delete filters.until;
      state.filters = filters;
      state.rangePreset = preset;
      state.trendWindow = RANGE_TO_TREND_WINDOW[preset] || state.trendWindow;
      syncRangePresetControls(state);
      closeDatePicker();
      if (sameStableFilters(previousFilters, filters)) {
        await reloadDashboardFastRange(state);
      } else {
        clearLiveRequestCache();
        await reloadDashboard(state);
      }
      syncFilterControls(state);
    } catch (error) {
      if (isAbortError(error)) return;
      logger.error('快捷时间范围切换失败', error);
      renderBootstrapError(error);
    }
  });
}

function syncUrlFromState(state) {
  if (state.mode === 'snapshot') return;
  const query = buildFilterQuery(state);
  const params = new URLSearchParams(query.slice(1));
  const explorerParams = new URLSearchParams(buildExplorerQuery(state).slice(1));
  for (const [key, value] of explorerParams.entries()) {
    params.set(key, value);
  }
  const mergedQuery = params.toString();
  const next = `${window.location.pathname}${mergedQuery ? `?${mergedQuery}` : ''}${window.location.hash || ''}`;
  window.history.replaceState(null, '', next);
}

export async function reloadDashboard(state) {
  if (state.reloadPromise) {
    return state.reloadPromise;
  }
  state.rangeReloadController?.abort();
  state.rangeReloadController = null;

  state.reloadPromise = state.mode === 'snapshot'
    ? loadDashboardData(state).then((snapshot) => {
      state.rawData = snapshot;
      renderDashboard(snapshot);
      return snapshot;
    })
    : loadDashboardProgressive(state);

  try {
    return await state.reloadPromise;
  } finally {
    state.reloadPromise = null;
  }
}

async function reloadDashboardFastRange(state, options = {}) {
  if (state.mode === 'snapshot') {
    return reloadDashboard(state);
  }
  return loadDashboardProgressive(state, {
    retainPrevious: Boolean(state.rawData),
    silent: Boolean(options.silent),
  });
}

// 自动刷新 / sync 完成后的重载统一走 interactive 路径（fast-range 流程）。
// interactive 契约接受 since/until；自定义日期筛选无需回退 full scope。
async function reloadDashboardAfterDataChange(state, options = {}) {
  return reloadDashboardFastRange(state, options);
}

async function refreshDashboardInPlace(state) {
  if (state.mode === 'snapshot') return;
  try {
    await reloadDashboardAfterDataChange(state, { silent: true });
  } catch (error) {
    logger.error('自动刷新失败', error);
    const endpointSync = document.getElementById('endpoint-sync');
    if (endpointSync) endpointSync.textContent = error?.message || getShellCopy('shell.refresh.failed');
  }
}

function stopAutoRefresh(state) {
  if (state.autoRefreshTimer) {
    window.clearInterval(state.autoRefreshTimer);
    state.autoRefreshTimer = null;
  }
}

function scheduleAutoRefresh(state) {
  stopAutoRefresh(state);
  if (state.mode === 'snapshot' || !state.autoRefreshMs) {
    return;
  }
  state.autoRefreshTimer = window.setInterval(() => {
    if (document.hidden || state.activeJobSnapshot?.status === 'running') {
      return;
    }
    void refreshDashboardInPlace(state);
  }, state.autoRefreshMs);
}

function syncAutoRefreshControls(state) {
  document.querySelectorAll('[data-refresh-interval]').forEach((button) => {
    const interval = Number(button.dataset.refreshInterval || 0);
    const active = interval === Number(state.autoRefreshMs || 0);
    button.classList.toggle('active', active);
    const pressed = String(active);
    if (button.getAttribute('aria-pressed') !== pressed) {
      button.setAttribute('aria-pressed', pressed);
    }
    button.disabled = state.mode === 'snapshot';
    if (state.mode === 'snapshot') {
      button.setAttribute('title', getShellCopy('shell.refresh.snapshotDisabled'));
    } else {
      button.removeAttribute('title');
    }
  });
}

function setupAutoRefresh(state) {
  syncAutoRefreshControls(state);
  scheduleAutoRefresh(state);
  const group = document.getElementById('auto-refresh');
  if (!group) return;

  if (state.mode === 'snapshot') {
    return;
  }

  group.addEventListener('click', (event) => {
    const button = event.target.closest('[data-refresh-interval]');
    if (!button) return;
    const next = Number(button.dataset.refreshInterval || 0);
    if (!VALID_AUTO_REFRESH_MS.has(next)) {
      return;
    }
    state.autoRefreshMs = next;
    persistAutoRefreshPreference(next);
    syncAutoRefreshControls(state);
    scheduleAutoRefresh(state);
  });
}

function setupFilterControls(state) {
  populateSourceFilter();
  setupTrendSegments(state);
  setupRangePresetControls(state);
  setupDateInputs(state);

  const apply = document.getElementById('filters-apply');
  const reset = document.getElementById('filters-reset');
  const rail = document.getElementById('filter-rail');

  if (state.mode === 'snapshot') {
    return;
  }

  apply?.addEventListener('click', async () => {
    try {
      state.filters = currentFilterInputs();
      if (state.filters.since || state.filters.until) {
        state.rangePreset = CUSTOM_RANGE_PRESET;
      } else if (state.rangePreset === CUSTOM_RANGE_PRESET) {
        state.rangePreset = DEFAULT_RANGE_PRESET;
        state.trendWindow = DEFAULT_TREND_WINDOW;
      }
      closeDatePicker();
      clearLiveRequestCache();
      await reloadDashboard(state);
    } catch (error) {
      logger.error('筛选加载失败', error);
      renderBootstrapError(error);
    }
  });

  reset?.addEventListener('click', async () => {
    try {
      state.filters = {};
      state.rangePreset = DEFAULT_RANGE_PRESET;
      state.trendWindow = DEFAULT_TREND_WINDOW;
      closeDatePicker();
      clearLiveRequestCache();
      await reloadDashboard(state);
      syncFilterControls(state);
    } catch (error) {
      logger.error('筛选重置失败', error);
      renderBootstrapError(error);
    }
  });

  rail?.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      apply?.click();
    }
  });
}

function syncExplorerControls(state) {
  if (!state) return;
  const explorer = { ...DEFAULT_EXPLORER, ...(state.explorer || {}) };
  const snapshotMode = state.mode === 'snapshot';
  const values = {
    metric: explorer.metric,
    groupBy: explorer.groupBy,
    granularity: explorer.granularity,
    limit: clampExplorerLimit(explorer.limit),
    sessionId: explorer.sessionId || '',
    toolName: explorer.toolName || '',
    toolKind: explorer.toolKind || '',
    tokenType: explorer.tokenType || '',
    includeOther: explorer.includeOther !== false,
    includeNonTool: explorer.includeNonTool !== false,
  };

  for (const [key, value] of Object.entries(values)) {
    const control = document.querySelector(`[data-explorer-control="${key}"]`);
    if (!control) continue;
    if (control.type === 'checkbox') {
      control.checked = Boolean(value);
    } else if (document.activeElement !== control) {
      control.value = String(value);
    }
  }

  document
    .querySelectorAll('#explorer-controls [data-explorer-control], #explorer-apply, #explorer-reset')
    .forEach((el) => {
      el.disabled = snapshotMode;
      if (snapshotMode) {
        el.setAttribute('title', getShellCopy('shell.explorer.snapshotDisabled'));
      } else {
        el.removeAttribute('title');
      }
    });
}

function currentExplorerInputs() {
  const valueFor = (key) => document.querySelector(`[data-explorer-control="${key}"]`)?.value?.trim() || '';
  return {
    granularity: valueFor('granularity') || DEFAULT_EXPLORER.granularity,
    metric: valueFor('metric') || DEFAULT_EXPLORER.metric,
    groupBy: valueFor('groupBy') || DEFAULT_EXPLORER.groupBy,
    sessionId: valueFor('sessionId'),
    toolName: valueFor('toolName'),
    toolKind: valueFor('toolKind'),
    tokenType: valueFor('tokenType'),
    limit: clampExplorerLimit(valueFor('limit')),
    includeOther: document.querySelector('[data-explorer-control="includeOther"]')?.checked !== false,
    includeNonTool: document.querySelector('[data-explorer-control="includeNonTool"]')?.checked !== false,
  };
}

async function reloadExplorer(state) {
  if (state.mode === 'snapshot') return state.rawData?.explorer;
  try {
    const explorer = await loadExplorer(state);
    state.rawData = { ...(state.rawData || {}), explorer };
    syncUrlFromState(state);
    renderExplorerPanel(state.rawData);
    return explorer;
  } catch (error) {
    logger.error('Explorer 加载失败', error);
    const explorer = degradedExplorerPayload(error);
    state.rawData = { ...(state.rawData || {}), explorer };
    renderExplorerPanel(state.rawData);
    return explorer;
  }
}

function setupExplorerControls(state) {
  syncExplorerControls(state);
  const controls = document.getElementById('explorer-controls');
  const apply = document.getElementById('explorer-apply');
  const reset = document.getElementById('explorer-reset');

  if (!controls || state.mode === 'snapshot') {
    return;
  }

  apply?.addEventListener('click', async () => {
    state.explorer = currentExplorerInputs();
    await reloadExplorer(state);
  });

  reset?.addEventListener('click', async () => {
    state.explorer = { ...DEFAULT_EXPLORER };
    syncExplorerControls(state);
    await reloadExplorer(state);
  });

  controls.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' && !event.target.closest('button')) {
      event.preventDefault();
      apply?.click();
    }
  });
}

/*
 * ========================================================================
 * 步骤2.1：设置面板展开/折叠
 * ========================================================================
 * 目标：
 * 1) shell 中的 data-toggle-panel 按钮必须有真实行为
 * 2) 不足以展开的面板隐藏按钮，避免假 affordance
 * 3) aria-expanded 与当前状态同步
 */
// 面板展开/折叠只重渲对应面板，无关面板零 DOM 写入。
function renderExpandedPanel(panel) {
  const rawData = dashboardState?.rawData;
  if (!rawData) return;
  const context = buildContext(rawData);
  const locale = getLocale();
  const expanded = dashboardState?.expanded || {};
  if (panel === 'models') {
    renderPanel('models', panelFingerprint('models', rawData, { locale, extra: { expanded: Boolean(expanded.models) } }), () => renderModels(context, dashboardState));
  } else if (panel === 'projects') {
    renderPanel('projects', panelFingerprint('projects', rawData, { locale, extra: { expanded: Boolean(expanded.projects) } }), () => renderProjects(context, dashboardState));
  } else if (panel === 'costs') {
    renderPanel('costs', panelFingerprint('costs', rawData, { locale, extra: { expanded: Boolean(expanded.costs) } }), () => renderCosts(context, dashboardState));
  }
  syncPanelToggleControls(context, dashboardState);
}

function setupPanelToggles(state) {
  document.addEventListener('click', (event) => {
    const button = event.target.closest('[data-toggle-panel]');
    if (!button) return;

    const panel = button.dataset.togglePanel;
    if (!panel || !Object.prototype.hasOwnProperty.call(state.expanded, panel)) {
      return;
    }
    if (button.disabled || button.hidden) {
      return;
    }

    state.expanded[panel] = !state.expanded[panel];
    renderExpandedPanel(panel);
  });
}

function syncPanelToggleControls(context, state = dashboardState) {
  const panelConfigs = {
    models: {
      count: context.panels.models?.length || 0,
      limit: 8,
      expandKey: 'shell.models.expand',
      collapseKey: 'shell.models.collapse',
    },
    projects: {
      count: context.panels.projects?.length || 0,
      limit: 6,
      expandKey: 'shell.projects.expand',
      collapseKey: 'shell.projects.collapse',
    },
    costs: {
      count: context.panels.costs?.length || 0,
      limit: 5,
      expandKey: 'shell.cost.expand',
      collapseKey: 'shell.cost.collapse',
    },
  };

  Object.entries(panelConfigs).forEach(([panel, config]) => {
    const button = document.querySelector(`[data-toggle-panel="${panel}"]`);
    if (!button) return;

    const hasMore = config.count > config.limit;
    button.hidden = !hasMore;
    button.disabled = !hasMore;
    const expanded = Boolean(state?.expanded?.[panel]);
    const expandedValue = String(expanded);
    if (button.getAttribute('aria-expanded') !== expandedValue) {
      button.setAttribute('aria-expanded', expandedValue);
    }
    if (hasMore) {
      const label = getShellCopy(expanded ? config.collapseKey : config.expandKey);
      if (button.textContent !== label) {
        button.textContent = label;
      }
    }
  });
}

/*
 * ========================================================================
 * 步骤3：设置趋势区时间窗口切换
 * ========================================================================
 * 目标：
 * 1) 监听 seg 按钮点击
 * 2) 重新请求趋势数据并重渲染整页
 */
function setupTrendSegments(state) {
  const seg = document.getElementById('seg');
  if (!seg) return;

  seg.addEventListener('click', async (e) => {
    if (e.target.tagName !== 'BUTTON') {
      return;
    }

    const nextWindow = e.target.dataset.window || DEFAULT_TREND_WINDOW;
    if (nextWindow === state.trendWindow) {
      return;
    }

    try {
      const previousFilters = { ...(state.filters || {}) };
      state.trendWindow = nextWindow;
      state.rangePreset = TREND_WINDOW_TO_RANGE[nextWindow] || DEFAULT_RANGE_PRESET;
      syncRangePresetControls(state);
      state.filters = currentFilterInputs();
      delete state.filters.since;
      delete state.filters.until;
      closeDatePicker();
      if (sameStableFilters(previousFilters, state.filters)) {
        await reloadDashboardFastRange(state);
      } else {
        clearLiveRequestCache();
        await reloadDashboard(state);
      }
      syncFilterControls(state);
    } catch (error) {
      if (isAbortError(error)) return;
      logger.error('趋势窗口切换失败', error);
      renderBootstrapError(error);
    }
  });
}

/*
 * ========================================================================
 * 步骤4：把当前 dashboard 数据导出为本地 JSON 文件
 * ========================================================================
 * 目标：
 * 1) 复用浏览器内已加载的 state.rawData，不发起额外网络请求
 * 2) 在 payload 头部写入生成时间、模式、来源、当前窗口和应用版本，便于回查
 * 3) 通过 Blob + 临时 anchor 触发下载，文件名带 ISO 时间戳
 * 4) 按钮短暂置为「已导出」，给用户一个完成反馈
 */
function setupExport(state) {
  const btn = document.getElementById('btn-export');
  if (!btn) return;

  btn.addEventListener('click', () => {
    // 4.1 数据未就绪时静默拒绝，避免导出空对象
    if (!state.rawData) {
      logger.warn('数据尚未加载，无法导出');
      return;
    }

    // 4.2 组装 payload，所见即所得地反映当前窗口
    const payload = {
      generated_at: new Date().toISOString(),
      mode: state.mode,
      source: window.location.host,
      trend_window: state.trendWindow,
      range_preset: state.rangePreset,
      filter: state.filters || {},
      app: { name: 'llmusage', version: appVersion() },
      data: state.rawData,
    };

    // 4.3 序列化并触发下载
    const json = JSON.stringify(payload, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const stamp = new Date().toISOString().replace(/[:.]/g, '-');
    const a = document.createElement('a');
    a.href = url;
    a.download = `llmusage-${stamp}.json`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
    logger.info('完成本地 JSON 导出');

    // 4.4 临时反馈，避免用户怀疑没生效
    const original = btn.innerHTML;
    btn.disabled = true;
    btn.innerHTML = `<span>${escapeHtml(UI_COPY.actions.exportDone)}</span>`;
    setTimeout(() => {
      btn.innerHTML = original;
      btn.disabled = false;
    }, 1200);
  });
}

function syncOptionsFromState(state) {
  const options = { rebuild: false };
  if (state.filters?.source) {
    options.source = state.filters.source;
  }
  const recentDays = {
    '1d': 1,
    '7d': 7,
    '30d': 30,
  }[state.rangePreset] || {
    day: 1,
    week: 7,
    month: 30,
  }[state.trendWindow];
  if (recentDays) {
    options.recent_days = recentDays;
  }
  return options;
}

async function postJson(path, body = {}) {
  const response = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    let detail = '';
    try {
      const payload = await response.clone().json();
      detail = payload?.error?.detail || payload?.error?.message || '';
    } catch (_) {}
    throw new Error(detail || `请求失败：${response.status}`);
  }
  return response.json();
}

async function getJson(path) {
  const response = await fetch(path, { headers: { Accept: 'application/json' } });
  if (!response.ok) {
    let detail = '';
    try {
      const payload = await response.clone().json();
      detail = payload?.error?.detail || payload?.error?.message || '';
    } catch (_) {}
    throw new Error(detail || `请求失败：${response.status}`);
  }
  return response.json();
}

function jobStatusLabel(snapshot) {
  if (!snapshot) return getShellCopy('shell.sync.idle');
  const status = snapshot.status || 'running';
  if (status === 'running') {
    return getShellCopy('shell.sync.running');
  }
  if (status === 'cancelling') {
    return getShellCopy('shell.sync.cancelling');
  }
  if (status === 'completed') {
    return getShellCopy('shell.sync.completed');
  }
  if (status === 'cancelled') {
    return getShellCopy('shell.sync.cancelled');
  }
  if (status === 'failed') {
    return getShellCopy('shell.sync.failed');
  }
  return status;
}

// 幂等写保护：自动刷新 tick / 轮询都会调这里，内容未变时不触碰 DOM。
let lastSyncButtonHtml = null;

function updateSyncButton(state, snapshot = state.activeJobSnapshot) {
  const btn = document.getElementById('btn-sync');
  if (!btn) return;

  const snapshotMode = state.mode === 'snapshot';
  const running = ['running', 'cancelling'].includes(snapshot?.status);
  btn.disabled = snapshotMode;
  const jobStatus = snapshot?.status || 'idle';
  if (btn.dataset.jobStatus !== jobStatus) {
    btn.dataset.jobStatus = jobStatus;
  }
  btn.title = snapshotMode ? getShellCopy('shell.sync.snapshotDisabled') : snapshot?.summary || '';
  const buttonHtml = `
    <svg class="i" viewBox="0 0 24 24" style="width: 13px; height: 13px;"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15A9 9 0 1 1 18 6.36L23 10"/></svg>
    <span>${escapeHtml(running ? getShellCopy('shell.sync.cancel') : getShellCopy('shell.btn.sync'))}</span>
  `;
  if (lastSyncButtonHtml !== buttonHtml) {
    lastSyncButtonHtml = buttonHtml;
    btn.innerHTML = buttonHtml;
  }

  const endpointSync = document.getElementById('endpoint-sync');
  if (endpointSync && snapshot) {
    const endpointText = `${jobStatusLabel(snapshot)} · ${snapshot.summary || snapshot.job_id}`;
    if (endpointSync.textContent !== endpointText) {
      endpointSync.textContent = endpointText;
    }
  }
}

function isTerminalJob(snapshot) {
  return ['completed', 'failed', 'cancelled'].includes(snapshot?.status);
}

// 轮询快照浅比对：只有这些字段变化才写 DOM（状态行、进度文本），
// 未变化的 tick 完全跳过 updateSyncButton 与 sync-command-center 重渲。
function jobPollFingerprint(snapshot) {
  return stableSerialize(jobFingerprintPart(snapshot));
}

async function pollJobUntilTerminal(state, jobId) {
  let snapshot = state.activeJobSnapshot;
  let lastRenderedFingerprint = jobPollFingerprint(snapshot);
  const deadline = Date.now() + JOB_POLL_MAX_DURATION_MS;
  while (jobId && !isTerminalJob(snapshot) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, JOB_POLL_INTERVAL_MS));
    snapshot = await getJson(`/api/jobs/${encodeURIComponent(jobId)}`);
    state.activeJobSnapshot = snapshot;
    const fingerprint = jobPollFingerprint(snapshot);
    if (fingerprint === lastRenderedFingerprint) {
      continue;
    }
    lastRenderedFingerprint = fingerprint;
    updateSyncButton(state, snapshot);
    refreshSyncCommandCenter(state);
  }
  return snapshot;
}

function setupSyncJob(state) {
  const btn = document.getElementById('btn-sync');
  if (!btn) return;
  if (state.mode === 'snapshot') {
    updateSyncButton(state);
    return;
  }

  btn.addEventListener('click', async () => {
    try {
      if (state.activeJobId && state.activeJobSnapshot?.status === 'running') {
        const payload = await postJson(`/api/jobs/${encodeURIComponent(state.activeJobId)}/cancel`, {});
        state.activeJobSnapshot = payload.snapshot;
        updateSyncButton(state, state.activeJobSnapshot);
        refreshSyncCommandCenter(state);
        return;
      }

      btn.disabled = true;
      const payload = await postJson('/api/jobs', syncOptionsFromState(state));
      state.activeJobId = payload.job_id;
      state.activeJobSnapshot = payload.snapshot;
      updateSyncButton(state, state.activeJobSnapshot);
      refreshSyncCommandCenter(state);

      const terminal = await pollJobUntilTerminal(state, state.activeJobId);
      updateSyncButton(state, terminal);
      refreshSyncCommandCenter(state);
      if (terminal?.status === 'completed') {
        clearLiveRequestCache();
        await reloadDashboardAfterDataChange(state);
      }
    } catch (error) {
      logger.error('同步任务失败', error);
      const endpointSync = document.getElementById('endpoint-sync');
      if (endpointSync) endpointSync.textContent = error?.message || getShellCopy('shell.sync.failed');
    } finally {
      if (!['failed', 'cancelled'].includes(state.activeJobSnapshot?.status)) {
        state.activeJobId = null;
        state.activeJobSnapshot = null;
      }
      updateSyncButton(state);
      refreshSyncCommandCenter(state);
    }
  });
}

/*
 * ========================================================================
 * 步骤5：主题与语言切换按钮
 * ========================================================================
 */
function setupThemeToggle() {
  const btn = document.getElementById('toggle-theme');
  if (!btn) return;

  btn.addEventListener('click', () => {
    toggleTheme();
  });
}

function setupLocaleToggle(_state) {
  const btn = document.getElementById('toggle-locale');
  if (!btn) return;

  btn.addEventListener('click', () => {
    setLocale(getLocale() === 'zh' ? 'en' : 'zh');
  });
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', main);
} else {
  main();
}
