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
      health: snapshot?.health,
      diagnostics: snapshot?.diagnostics,
    };
  }

  let snapshot;
  try {
    snapshot = await loadJson(`/api/dashboard${buildFilterQuery(state)}`);
  } catch (error) {
    if (error.status !== 404) {
      throw error;
    }
    logger.warn('/api/dashboard 不可用，回退到旧分段 API', error);
    const [overview, trends, models, sources, projects, costs, health, diagnostics] = await Promise.all([
      loadSection(state, 'overview', '/api/overview'),
      loadTrendWindow(state, state.trendWindow),
      loadSection(state, 'models', '/api/models'),
      loadSection(state, 'sources', '/api/sources'),
      loadSection(state, 'projects', '/api/projects'),
      loadSection(state, 'costs', '/api/costs'),
      loadSection(state, 'health', '/api/health'),
      loadSection(state, 'diagnostics', '/api/diagnostics'),
    ]);
    return { overview, trends, models, sources, projects, costs, health, diagnostics };
  }
  return {
    overview: snapshot?.overview,
    trends: snapshotTrendRows(snapshot, state.trendWindow),
    models: snapshot?.models,
    sources: snapshot?.sources,
    projects: snapshot?.projects,
    costs: snapshot?.costs,
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

export async function loadTrendWindow(state, windowName) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.[`${windowName}_trends`];
  }
  return loadJson(`/api/trends${buildFilterQuery({ ...state, trendWindow: windowName })}`);
}
