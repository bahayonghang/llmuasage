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
  if (!params.has('timezone')) {
    const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    if (timezone) {
      params.set('timezone', timezone);
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
      hosts: snapshot?.hosts ?? [],
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
      home_overview: snapshot?.home_overview ?? null,
      heatmap: snapshot?.heatmap ?? [],
      trends_daily: snapshot?.trends_daily ?? [],
      top_sessions: snapshot?.top_sessions ?? [],
      hour_of_week: snapshot?.hour_of_week ?? [],
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
    return { overview, trends, models, sources, hosts: [], projects, costs, activity, tools, optimize, explorer, compare, health, diagnostics, sync_command_center: null };
  }
  return {
    overview: snapshot?.overview,
    trends: snapshotTrendRows(snapshot, state.trendWindow),
    models: snapshot?.models,
    sources: snapshot?.sources,
    hosts: snapshot?.hosts ?? [],
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
    home_overview: snapshot?.home_overview ?? null,
    heatmap: snapshot?.heatmap ?? [],
    trends_daily: snapshot?.trends_daily ?? [],
    top_sessions: snapshot?.top_sessions ?? [],
    hour_of_week: snapshot?.hour_of_week ?? [],
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

export async function fetchHomeOverview(state, options = {}) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.home_overview ?? null;
  }
  const params = new URLSearchParams(buildFilterQuery(state).slice(1));
  params.set('compact', 'true');
  return loadLiveJson(`/api/home_overview?${params}`, options);
}

function heatmapDays(state) {
  const since = state?.filters?.since;
  const until = state?.filters?.until;
  if (since && until) {
    const start = Date.parse(`${since}T00:00:00Z`);
    const end = Date.parse(`${until}T00:00:00Z`);
    if (Number.isFinite(start) && Number.isFinite(end) && end >= start) {
      return Math.min(366, Math.floor((end - start) / 86400000) + 1);
    }
  }
  return { '1d': 1, '7d': 7, '30d': 30, all: 366 }[state?.rangePreset] || 366;
}

export async function fetchHeatmap(state, options = {}) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.heatmap ?? [];
  }
  const params = new URLSearchParams(buildFilterQuery(state).slice(1));
  params.set('days', String(heatmapDays(state)));
  return loadLiveJson(`/api/heatmap?${params.toString()}`, options);
}

export async function fetchTrendsDaily(state, options = {}) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.trends_daily ?? [];
  }
  return loadLiveJson(`/api/trends_daily${buildFilterQuery(state)}`, options);
}

export async function fetchTopSessions(state, options = {}) {
  if (state.mode === 'snapshot') return (await ensureSnapshot(state))?.top_sessions ?? [];
  const params = new URLSearchParams(buildFilterQuery(state).slice(1));
  params.set('sort', options.sort || state.topSessionsSort || 'tokens');
  params.set('limit', '10');
  return loadLiveJson(`/api/sessions?${params}`, options);
}

export async function fetchHourOfWeek(state, options = {}) {
  if (state.mode === 'snapshot') return (await ensureSnapshot(state))?.hour_of_week ?? [];
  return loadLiveJson(`/api/hour_of_week${buildFilterQuery(state)}`, options);
}

export const LOGS_PAGE_SIZE = 20;

export async function fetchLogs(state, options = {}) {
  if (state.mode === 'snapshot') return { records: [], next_cursor: null };
  const params = new URLSearchParams(buildFilterQuery(state).slice(1));
  params.set('page_size', String(LOGS_PAGE_SIZE));
  if (options.session) params.set('session', options.session);
  if (options.cursor) params.set('cursor', options.cursor);
  if (options.eventKey) params.set('event_key', options.eventKey);
  return loadLiveJson(`/api/logs?${params}`, options);
}

export function loadDashboardSecondarySections(state, options = {}) {
  return {
    activity: () => loadOptionalSection(state, 'activity', '/api/activity', emptyActivity, options),
    tools: () => loadOptionalSection(state, 'tools', '/api/tools', emptyTools, options),
    optimize: () => loadOptionalSection(state, 'optimize', '/api/optimize', emptyOptimize, options),
    explorer: () => loadOptionalExplorer(state, options),
    compare: () => loadOptionalSection(state, 'compare', '/api/compare', emptyCompare, options),
    home_overview: () => fetchHomeOverview(state, options),
    heatmap: () => fetchHeatmap(state, options),
    trends_daily: () => fetchTrendsDaily(state, options),
    top_sessions: () => fetchTopSessions(state, options),
    hour_of_week: () => fetchHourOfWeek(state, options),
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
