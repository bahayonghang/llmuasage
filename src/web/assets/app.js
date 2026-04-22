import { buildContext, loadSection, loadTrendWindow } from './data.js';
import { renderPage } from './render.js';

const logger = window.console;

const state = {
  mode: document.body.dataset.mode || 'live',
  snapshot: null,
  window: 'day',
  context: null,
  expanded: {
    trendLedger: false,
    models: false,
    sources: false,
    projects: false,
    costs: false,
  },
};

function renderCurrent() {
  if (!state.context) return;
  renderPage(state.context, state);
  document.querySelectorAll('[data-window]').forEach((button) => {
    button.classList.toggle('active', button.dataset.window === state.window);
  });
}

/*
 * ========================================================================
 * 步骤1：绑定页面交互
 * ========================================================================
 * 目标：
 * 1) 用事件委托统一处理窗口切换和展开收起
 * 2) 避免渲染后重复绑定监听器
 * 3) 让交互逻辑只保留在入口模块
 */
function attachInteractions() {
  logger.info('开始绑定页面交互');

  // 1.1 监听窗口切换和展开按钮
  document.addEventListener('click', async (event) => {
    const windowButton = event.target.closest('[data-window]');
    if (windowButton) {
      const nextWindow = windowButton.dataset.window;
      if (!nextWindow || nextWindow === state.window) return;

      logger.info('开始切换趋势窗口');
      state.window = nextWindow;
      state.expanded.trendLedger = false;
      await refresh();
      logger.info('完成趋势窗口切换');
      return;
    }

    const toggleButton = event.target.closest('[data-toggle-panel]');
    if (!toggleButton) return;

    const panelKey = toggleButton.dataset.togglePanel;
    if (!panelKey || !(panelKey in state.expanded)) return;

    logger.info('开始切换展开面板');
    state.expanded[panelKey] = !state.expanded[panelKey];
    renderCurrent();
    logger.info('完成展开面板切换');
  });

  logger.info('完成页面交互绑定');
}

function renderError(error) {
  document.body.innerHTML = `
    <main class="page-shell">
      <section class="panel">
        <div class="section-header section-header--tight">
          <div>
            <p class="section-kicker">错误</p>
            <h2>加载失败</h2>
          </div>
        </div>
        <div class="empty-state mono">${String(error?.stack || error?.message || error)}</div>
      </section>
    </main>
  `;
}

/*
 * ========================================================================
 * 步骤2：刷新页面数据并触发渲染
 * ========================================================================
 * 目标：
 * 1) 并行读取 overview、trend、rank、health 数据
 * 2) 把数据统一交给 context builder
 * 3) 在同一入口里完成错误兜底
 */
async function refresh() {
  logger.info('开始刷新本地分析页');

  // 2.1 并行请求当前窗口和各分析面板数据
  const overview = await loadSection(state, 'overview', '/api/overview');
  const [trends, models, sources, projects, costs, health] = await Promise.all([
    loadTrendWindow(state, state.window),
    loadSection(state, 'models', '/api/models'),
    loadSection(state, 'sources', '/api/sources'),
    loadSection(state, 'projects', '/api/projects'),
    loadSection(state, 'costs', '/api/costs'),
    loadSection(state, 'health', '/api/health'),
  ]);

  // 2.2 构建上下文并刷新整页
  state.context = buildContext({
    overview,
    trends,
    models,
    sources,
    projects,
    costs,
    health,
  });
  renderCurrent();

  logger.info('完成本地分析页刷新');
}

attachInteractions();
refresh().catch((error) => {
  logger.error(error);
  renderError(error);
});
