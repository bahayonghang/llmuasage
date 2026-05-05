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

export async function loadSection(state, section, path) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.[section];
  }
  return loadJson(path);
}

export async function loadTrendWindow(state, windowName) {
  if (state.mode === 'snapshot') {
    const snapshot = await ensureSnapshot(state);
    return snapshot?.[`${windowName}_trends`];
  }
  return loadJson(`/api/trends?window=${windowName}`);
}
