import { UI_COPY, getLocale, getShellCopy, onLocaleChange, setLocale } from './copy.js';
import { buildContext, buildFilterQuery, loadDashboardSnapshot } from './data.js';
import { renderHero } from './render/hero.js';
import { renderTrends } from './render/trends.js';
import { renderModels } from './render/models.js';
import { renderSources } from './render/sources.js';
import { renderProjects } from './render/projects.js';
import { renderCosts } from './render/costs.js';
import { renderInsights } from './render/insights.js';
import { renderBehavior } from './render/behavior.js';
import { applyDomI18n, bindI18nDomSync } from './i18n.js';
import { initTheme, toggleTheme } from './theme.js';
import { setRenderer, setRuntimeState } from './runtime.js';

const logger = window.console;
const DEFAULT_TREND_WINDOW = 'day';
const VALID_TREND_WINDOWS = new Set(['day', 'week', 'month', 'all']);
const AUTO_REFRESH_STORAGE_KEY = 'llmusage:autoRefreshMs';
const VALID_AUTO_REFRESH_MS = new Set([0, 30000, 60000]);
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
    filters: readFiltersFromUrl(),
    autoRefreshMs: readAutoRefreshPreference(),
    autoRefreshTimer: null,
    reloadPromise: null,
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
  setupPanelToggles(state);
  setupExport(state);
  setupSyncJob(state);
  setupAutoRefresh(state);
  setupThemeToggle();
  setupLocaleToggle(state);

  try {
    // 1.2 先加载首屏需要的全部数据
    state.rawData = await loadDashboardData(state);

    // 1.3 首次渲染
    renderDashboard(state.rawData);

    // 1.4 切语言时基于已有 rawData 直接重渲，不再请求接口
    onLocaleChange(() => {
      if (state.rawData) {
        renderDashboard(state.rawData);
      }
    });

    logger.info('llmusage dashboard 渲染完成');
  } catch (error) {
    logger.error('llmusage dashboard 数据加载失败', error);
    renderBootstrapError(error);
    onLocaleChange(() => renderBootstrapError(error));
  }
}

async function loadDashboardData(state) {
  logger.info('开始加载 dashboard 数据');
  const snapshot = await loadDashboardSnapshot(state);
  logger.info('完成 dashboard 数据加载');
  return snapshot;
}

function renderDashboard(rawData) {
  const context = buildContext(rawData);

  renderHero(context);
  renderTrends(context);
  renderModels(context, dashboardState);
  renderSources(context);
  renderProjects(context, dashboardState);
  renderBehavior(context);
  renderCosts(context, dashboardState);
  renderInsights(context);
  syncPanelToggleControls(context, dashboardState);
  syncFilterControls(dashboardState, context);
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
  const requestedWindow = params.get('window') || DEFAULT_TREND_WINDOW;
  return VALID_TREND_WINDOWS.has(requestedWindow) ? requestedWindow : DEFAULT_TREND_WINDOW;
}

function readAutoRefreshPreference() {
  try {
    const stored = Number(window.localStorage?.getItem(AUTO_REFRESH_STORAGE_KEY) || 0);
    return VALID_AUTO_REFRESH_MS.has(stored) ? stored : 0;
  } catch (_) {
    return 0;
  }
}

function persistAutoRefreshPreference(value) {
  try {
    window.localStorage?.setItem(AUTO_REFRESH_STORAGE_KEY, String(value));
  } catch (_) {
    /* localStorage may be unavailable in restricted contexts. */
  }
}

function renderBootstrapError(error) {
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
        <div style="font-size: 18px; font-weight: 600; margin-top: 2px;">${escapeHtml(errorCopy.title)}</div>
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
    <div style="padding: 18px; border: 1px dashed rgba(200,85,61,0.35); border-radius: 14px; color: #f5a890; font-size: 13px;">
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
  const sections = ['overview', 'trends', 'models', 'sources', 'projects', 'behavior', 'cost', 'status'];
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
      document.getElementById('projects')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
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
    button.classList.toggle('active', button.dataset.window === state.trendWindow);
  });

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
      list.innerHTML = [...models]
        .slice(0, 50)
        .map((model) => `<option value="${escapeHtml(model)}"></option>`)
        .join('');
    }
  }

  document.querySelectorAll('#filter-rail [data-filter], #filters-apply, #filters-reset').forEach((el) => {
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
    if (value) filters[key] = value;
  }
  return filters;
}

function syncUrlFromState(state) {
  if (state.mode === 'snapshot') return;
  const query = buildFilterQuery(state);
  const next = `${window.location.pathname}${query}${window.location.hash || ''}`;
  window.history.replaceState(null, '', next);
}

async function reloadDashboard(state) {
  if (state.reloadPromise) {
    return state.reloadPromise;
  }

  state.reloadPromise = (async () => {
    state.rawData = await loadDashboardData(state);
    syncUrlFromState(state);
    renderDashboard(state.rawData);
    updateSyncButton(state);
    return state.rawData;
  })();

  try {
    return await state.reloadPromise;
  } finally {
    state.reloadPromise = null;
  }
}

async function refreshDashboardInPlace(state) {
  if (state.mode === 'snapshot') return;
  try {
    await reloadDashboard(state);
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
    button.setAttribute('aria-pressed', String(active));
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

  const apply = document.getElementById('filters-apply');
  const reset = document.getElementById('filters-reset');
  const rail = document.getElementById('filter-rail');

  if (state.mode === 'snapshot') {
    return;
  }

  apply?.addEventListener('click', async () => {
    try {
      state.filters = currentFilterInputs();
      await reloadDashboard(state);
    } catch (error) {
      logger.error('筛选加载失败', error);
      renderBootstrapError(error);
    }
  });

  reset?.addEventListener('click', async () => {
    try {
      state.filters = {};
      state.trendWindow = DEFAULT_TREND_WINDOW;
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

/*
 * ========================================================================
 * 步骤2.1：设置面板展开/折叠
 * ========================================================================
 * 目标：
 * 1) shell 中的 data-toggle-panel 按钮必须有真实行为
 * 2) 不足以展开的面板隐藏按钮，避免假 affordance
 * 3) aria-expanded 与当前状态同步
 */
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
    renderDashboard(state.rawData);
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
    button.setAttribute('aria-expanded', String(expanded));
    if (hasMore) {
      button.textContent = getShellCopy(expanded ? config.collapseKey : config.expandKey);
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
      state.trendWindow = nextWindow;
      await reloadDashboard(state);
    } catch (error) {
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

function updateSyncButton(state, snapshot = state.activeJobSnapshot) {
  const btn = document.getElementById('btn-sync');
  if (!btn) return;

  const snapshotMode = state.mode === 'snapshot';
  const running = snapshot?.status === 'running';
  btn.disabled = snapshotMode;
  btn.dataset.jobStatus = snapshot?.status || 'idle';
  btn.title = snapshotMode ? getShellCopy('shell.sync.snapshotDisabled') : snapshot?.summary || '';
  btn.innerHTML = `
    <svg class="i" viewBox="0 0 24 24" style="width: 13px; height: 13px;"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15A9 9 0 1 1 18 6.36L23 10"/></svg>
    <span>${escapeHtml(running ? getShellCopy('shell.sync.cancel') : getShellCopy('shell.btn.sync'))}</span>
  `;

  const endpointSync = document.getElementById('endpoint-sync');
  if (endpointSync && snapshot) {
    endpointSync.textContent = `${jobStatusLabel(snapshot)} · ${snapshot.summary || snapshot.job_id}`;
  }
}

function isTerminalJob(snapshot) {
  return ['completed', 'failed', 'cancelled'].includes(snapshot?.status);
}

async function pollJobUntilTerminal(state, jobId) {
  let snapshot = state.activeJobSnapshot;
  while (jobId && !isTerminalJob(snapshot)) {
    await new Promise((resolve) => setTimeout(resolve, 900));
    snapshot = await getJson(`/api/jobs/${encodeURIComponent(jobId)}`);
    state.activeJobSnapshot = snapshot;
    updateSyncButton(state, snapshot);
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
        return;
      }

      btn.disabled = true;
      const payload = await postJson('/api/jobs', syncOptionsFromState(state));
      state.activeJobId = payload.job_id;
      state.activeJobSnapshot = payload.snapshot;
      updateSyncButton(state, state.activeJobSnapshot);

      const terminal = await pollJobUntilTerminal(state, state.activeJobId);
      updateSyncButton(state, terminal);
      if (terminal?.status === 'completed') {
        await reloadDashboard(state);
      }
    } catch (error) {
      logger.error('同步任务失败', error);
      const endpointSync = document.getElementById('endpoint-sync');
      if (endpointSync) endpointSync.textContent = error?.message || getShellCopy('shell.sync.failed');
    } finally {
      if (state.activeJobSnapshot?.status !== 'failed') {
        state.activeJobId = null;
        state.activeJobSnapshot = null;
      }
      updateSyncButton(state);
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
