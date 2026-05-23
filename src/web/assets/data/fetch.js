const logger = window.console;

/*
 * ========================================================================
 * 步骤1：请求 live / snapshot 数据
 * ========================================================================
 * 目标：
 * 1) 统一处理 fetch 错误
 * 2) 兼容 live API 与 snapshot.json 双来源
 * 3) 把请求入口收敛给 app.js 调度
 */
export async function loadJson(path) {
  logger.info('开始请求页面 JSON 数据');

  // 1.1 发起请求并校验 HTTP 状态
  const response = await fetch(path);
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

function snapshotTrendRows(snapshot, windowName) {
  return snapshot?.[`${windowName}_trends`] || [];
}

export async function loadDashboardSnapshot(state) {
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
      compare: snapshot?.compare,
      health: snapshot?.health,
      diagnostics: snapshot?.diagnostics,
    };
  }

  let snapshot;
  try {
    snapshot = await loadJson(`/api/dashboard${buildFilterQuery(state)}`);
  } catch (error) {
    logger.warn('/api/dashboard 不可用，回退到旧分段 API', error);
    const [overview, trends, models, sources, projects, costs, activity, tools, optimize, compare, health, diagnostics] = await Promise.all([
      loadSection(state, 'overview', '/api/overview'),
      loadTrendWindow(state, state.trendWindow),
      loadSection(state, 'models', '/api/models'),
      loadSection(state, 'sources', '/api/sources'),
      loadSection(state, 'projects', '/api/projects'),
      loadSection(state, 'costs', '/api/costs'),
      loadOptionalSection(state, 'activity', '/api/activity', emptyActivity),
      loadOptionalSection(state, 'tools', '/api/tools', emptyTools),
      loadOptionalSection(state, 'optimize', '/api/optimize', emptyOptimize),
      loadOptionalSection(state, 'compare', '/api/compare', emptyCompare),
      loadSection(state, 'health', '/api/health'),
      loadSection(state, 'diagnostics', '/api/diagnostics'),
    ]);
    return { overview, trends, models, sources, projects, costs, activity, tools, optimize, compare, health, diagnostics };
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
    compare: snapshot?.compare,
    health: snapshot?.health,
    diagnostics: snapshot?.diagnostics,
  };
}

export async function loadSection(state, section, path) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.[section];
  }
  return loadJson(`${path}${buildFilterQuery(state)}`);
}

async function loadOptionalSection(state, section, path, fallback) {
  try {
    return await loadSection(state, section, path);
  } catch (error) {
    logger.warn(`${path} degraded`, error);
    return fallbackFor(error, fallback);
  }
}

export async function loadTrendWindow(state, windowName) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.[`${windowName}_trends`];
  }
  return loadJson(`/api/trends${buildFilterQuery({ ...state, trendWindow: windowName })}`);
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
