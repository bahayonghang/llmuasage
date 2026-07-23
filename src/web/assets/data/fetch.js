const logger = window.console;
const LIVE_CACHE_TTL_MS = 10000;
const LIVE_CACHE_MAX_ENTRIES = 32;
const liveCache = new Map();
const liveInflight = new Map();
let liveCacheEpoch = 0;

/*
 * ========================================================================
 * 步骤1：请求 live / snapshot 数据
 * ========================================================================
 * 目标：
 * 1) 统一处理 fetch 错误
 * 2) 兼容 live API 与 snapshot.json 双来源
 * 3) 把请求入口收敛给 app.js 调度
 */
export async function loadJson(path, options = {}) {
  logger.info('开始请求页面 JSON 数据');

  // 1.1 发起请求并校验 HTTP 状态
  const response = await fetch(path, { signal: options.signal });
  if (!response.ok) {
    let detail = '';
    try {
      const payload = await response.clone().json();
      detail = payload?.error?.detail || payload?.error?.message || '';
    } catch (_) {}

    if (!detail) {
      detail = await response.text().catch(() => '');
    }

    const message = detail || `请求失败：${response.status}`;
    const error = new Error(message);
    error.status = response.status;
    throw error;
  }

  // 1.2 返回解析后的 JSON 结果
  const payload = await response.json();
  logger.info('完成页面 JSON 数据请求');
  return payload;
}

export function clearLiveRequestCache() {
  liveCacheEpoch += 1;
  liveCache.clear();
  for (const entry of liveInflight.values()) {
    entry.controller.abort();
  }
  liveInflight.clear();
}

function normalizedRequestKey(path) {
  const url = new URL(path, window.location.origin);
  const params = new URLSearchParams(url.search);
  params.sort();
  const query = params.toString();
  return `${url.pathname}${query ? `?${query}` : ''}`;
}

async function loadLiveJson(path, options = {}) {
  const key = normalizedRequestKey(path);
  const cacheable = options.cache !== false;
  if (cacheable) {
    const cached = liveCache.get(key);
    if (cached && Date.now() - cached.receivedAt < LIVE_CACHE_TTL_MS) {
      liveCache.delete(key);
      liveCache.set(key, cached);
      return cached.payload;
    }
  }

  if (liveInflight.has(key)) {
    return liveInflight.get(key).promise;
  }

  const epoch = liveCacheEpoch;
  const controller = new AbortController();
  const abort = () => controller.abort();
  if (options.signal?.aborted) {
    controller.abort();
  } else {
    options.signal?.addEventListener('abort', abort, { once: true });
  }
  const request = loadJson(path, { signal: controller.signal })
    .then((payload) => {
      if (cacheable && epoch === liveCacheEpoch) {
        liveCache.set(key, { payload, receivedAt: Date.now() });
        while (liveCache.size > LIVE_CACHE_MAX_ENTRIES) {
          liveCache.delete(liveCache.keys().next().value);
        }
      }
      return payload;
    })
    .finally(() => {
      options.signal?.removeEventListener('abort', abort);
      if (liveInflight.get(key)?.promise === request) {
        liveInflight.delete(key);
      }
    });
  liveInflight.set(key, { promise: request, controller });
  return request;
}

export async function ensureSnapshot(state) {
  if (!state.snapshot) {
    state.snapshot = await loadJson('snapshot.json');
  }
  return state.snapshot;
}

export function buildFilterQuery(state, options = {}) {
  const params = new URLSearchParams();
  const filter = state?.filters || {};
  const includeWindow = options.includeWindow !== false;

  if (includeWindow && state?.trendWindow) {
    params.set('window', state.trendWindow);
  }
  if (!filter.since && !filter.until && state?.rangePreset && state.rangePreset !== 'custom') {
    params.set('range', state.rangePreset);
  }
  for (const key of ['source', 'model', 'since', 'until', 'project_hash', 'timezone']) {
    const value = filter[key];
    if (value && value !== 'all') {
      params.set(key, value);
    }
  }

  const query = params.toString();
  return query ? `?${query}` : '';
}

export function buildExplorerQuery(state) {
  const params = new URLSearchParams(buildFilterQuery(state, { includeWindow: false }).slice(1));
  const explorer = state?.explorer || {};

  params.set('granularity', explorer.granularity || 'day');
  params.set('metric', explorer.metric || 'attributed_cost_usd');
  params.set('group_by', explorer.groupBy || 'source');
  params.set('limit', String(explorer.limit || 8));
  params.set('include_other', explorer.includeOther === false ? 'false' : 'true');

  if (explorer.includeNonTool === false) {
    params.set('is_tool', 'true');
  }
  if (explorer.sessionId) {
    params.set('session_id', explorer.sessionId);
  }
  if (explorer.toolName) {
    params.set('tool_name', explorer.toolName);
  }
  if (explorer.toolKind) {
    params.set('tool_kind', explorer.toolKind);
  }
  if (explorer.tokenType) {
    params.set('token_type', explorer.tokenType);
  }

  const query = params.toString();
  return query ? `?${query}` : '';
}

function snapshotTrendRows(snapshot, windowName) {
  return snapshot?.trends || snapshot?.[`${windowName}_trends`] || [];
}

function buildDashboardQuery(state, options = {}) {
  const params = new URLSearchParams(buildFilterQuery(state).slice(1));
  if (options.scope) {
    params.set('scope', options.scope);
  }
  const query = params.toString();
  return query ? `?${query}` : '';
}

export async function loadDashboardSnapshot(state, options = {}) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return {
      overview: snapshot?.overview,
      trends: snapshotTrendRows(snapshot, state.trendWindow),
      models: snapshot?.models,
      sources: snapshot?.sources,
      projects: snapshot?.projects,
      costs: snapshot?.costs,
      activity: snapshot?.activity,
      tools: snapshot?.tools,
      optimize: snapshot?.optimize,
      explorer: snapshot?.explorer,
      compare: snapshot?.compare,
      health: snapshot?.health,
      diagnostics: snapshot?.diagnostics,
      sync_command_center: snapshot?.sync_command_center,
    };
  }

  let snapshot;
  try {
    snapshot = await loadLiveJson(`/api/dashboard${buildDashboardQuery(state, options)}`, options);
  } catch (error) {
    if (error?.name === 'AbortError') throw error;
    if (options.legacyFallback === false) throw error;
    logger.warn('/api/dashboard 不可用，回退到旧分段 API', error);
    const [overview, trends, models, sources, projects, costs, activity, tools, optimize, explorer, compare, health, diagnostics] = await Promise.all([
      loadSection(state, 'overview', '/api/overview'),
      loadTrendWindow(state, state.trendWindow),
      loadSection(state, 'models', '/api/models'),
      loadSection(state, 'sources', '/api/sources'),
      loadSection(state, 'projects', '/api/projects'),
      loadSection(state, 'costs', '/api/costs'),
      loadOptionalSection(state, 'activity', '/api/activity', emptyActivity),
      loadOptionalSection(state, 'tools', '/api/tools', emptyTools),
      loadOptionalSection(state, 'optimize', '/api/optimize', emptyOptimize),
      loadOptionalExplorer(state),
      loadOptionalSection(state, 'compare', '/api/compare', emptyCompare),
      loadSection(state, 'health', '/api/health'),
      loadSection(state, 'diagnostics', '/api/diagnostics'),
    ]);
    return { overview, trends, models, sources, projects, costs, activity, tools, optimize, explorer, compare, health, diagnostics, sync_command_center: null };
  }
  return {
    overview: snapshot?.overview,
    trends: snapshotTrendRows(snapshot, state.trendWindow),
    models: snapshot?.models,
    sources: snapshot?.sources,
    projects: snapshot?.projects,
    costs: snapshot?.costs,
    activity: snapshot?.activity,
    tools: snapshot?.tools,
    optimize: snapshot?.optimize,
    explorer: snapshot?.explorer,
    compare: snapshot?.compare,
    health: snapshot?.health,
    diagnostics: snapshot?.diagnostics,
    sync_command_center: snapshot?.sync_command_center,
  };
}

export async function loadDashboardCoreSnapshot(state, options = {}) {
  return loadDashboardSnapshot(state, { ...options, scope: 'core' });
}

export async function loadDashboardInteractiveSnapshot(state, options = {}) {
  return loadDashboardSnapshot(state, { ...options, scope: 'interactive' });
}

export async function loadSection(state, section, path, options = {}) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.[section];
  }
  return loadLiveJson(`${path}${buildFilterQuery(state)}`, options);
}

async function loadOptionalSection(state, section, path, fallback, options = {}) {
  try {
    return await loadSection(state, section, path, options);
  } catch (error) {
    if (error?.name === 'AbortError') throw error;
    logger.warn(`${path} degraded`, error);
    return fallbackFor(error, fallback);
  }
}

async function loadOptionalExplorer(state, options = {}) {
  try {
    return await loadExplorer(state, options);
  } catch (error) {
    if (error?.name === 'AbortError') throw error;
    logger.warn('/api/explorer degraded', error);
    return fallbackFor(error, emptyExplorer);
  }
}

export async function loadTrendWindow(state, windowName, options = {}) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.[`${windowName}_trends`];
  }
  return loadLiveJson(`/api/trends${buildFilterQuery({ ...state, trendWindow: windowName })}`, options);
}

export async function loadExplorer(state, options = {}) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.explorer;
  }
  return loadLiveJson(`/api/explorer${buildExplorerQuery(state)}`, options);
}

export function loadDashboardSecondarySections(state, options = {}) {
  return {
    activity: () => loadOptionalSection(state, 'activity', '/api/activity', emptyActivity, options),
    tools: () => loadOptionalSection(state, 'tools', '/api/tools', emptyTools, options),
    optimize: () => loadOptionalSection(state, 'optimize', '/api/optimize', emptyOptimize, options),
    explorer: () => loadOptionalExplorer(state, options),
    compare: () => loadOptionalSection(state, 'compare', '/api/compare', emptyCompare, options),
  };
}

function degradedSupport(error) {
  return {
    supported: false,
    level: 'degraded',
    reason: error?.message || 'Behavior analytics timed out or failed; core usage data is still available.',
  };
}

function fallbackFor(error, fallback) {
  return typeof fallback === 'function' ? fallback(error) : fallback;
}

function emptyActivity(error) {
  return { support: degradedSupport(error), breakdown: [] };
}

function emptyTools(error) {
  return { support: degradedSupport(error), breakdown: [] };
}

function emptyOptimize(error) {
  return {
    support: degradedSupport(error),
    score: 100,
    grade: 'A',
    estimated_savings_tokens: 0,
    estimated_savings_usd: 0,
    findings: [],
  };
}

function emptyExplorer(error) {
  return {
    support: {
      supported: false,
      level: 'degraded',
      reason: error?.message || 'Explorer query is degraded; fixed dashboard panels are still available.',
      strategy: 'unknown',
    },
    warning: error?.message || 'Explorer query is degraded.',
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

function emptyCompare(error) {
  return {
    support: degradedSupport(error),
    candidates: [],
    model_a: null,
    model_b: null,
    metrics: [],
    category_head_to_head: [],
    working_style: [],
    warning: error?.message || 'Behavior model comparison is degraded.',
  };
}
