import { buildContext, loadSection, loadTrendWindow } from './data.js';
import { renderHero } from './render/hero.js';
import { renderTrends } from './render/trends.js';
import { renderModels } from './render/models.js';
import { renderSources } from './render/sources.js';
import { renderProjects } from './render/projects.js';
import { renderCosts } from './render/costs.js';

const logger = window.console;
const DEFAULT_TREND_WINDOW = 'day';

/*
 * ========================================================================
 * 步骤1：主入口
 * ========================================================================
 * 目标：
 * 1) 按 live / snapshot 模式加载 dashboard 数据
 * 2) 调用 buildContext 派生渲染上下文
 * 3) 依次调用各区域 render 函数
 * 4) 设置侧边栏高亮与趋势窗口切换
 */
async function main() {
  logger.info('llmusage dashboard 启动');

  const state = {
    mode: document.body?.dataset?.mode === 'snapshot' ? 'snapshot' : 'live',
    trendWindow: DEFAULT_TREND_WINDOW,
    rawData: null,
  };

  try {
    // 1.1 先加载首屏需要的全部数据
    state.rawData = await loadDashboardData(state);

    // 1.2 首次渲染
    renderDashboard(state.rawData);

    // 1.3 绑定交互
    setupNavigation();
    setupTrendSegments(state);

    logger.info('llmusage dashboard 渲染完成');
  } catch (error) {
    logger.error('llmusage dashboard 数据加载失败', error);
    renderBootstrapError(error);
  }
}

async function loadDashboardData(state) {
  logger.info('开始加载 dashboard 数据');

  const [overview, trends, models, sources, projects, costs, health] = await Promise.all([
    loadSection(state, 'overview', '/api/overview'),
    loadTrendWindow(state, state.trendWindow),
    loadSection(state, 'models', '/api/models'),
    loadSection(state, 'sources', '/api/sources'),
    loadSection(state, 'projects', '/api/projects'),
    loadSection(state, 'costs', '/api/costs'),
    loadSection(state, 'health', '/api/health'),
  ]);

  logger.info('完成 dashboard 数据加载');
  return { overview, trends, models, sources, projects, costs, health };
}

function renderDashboard(rawData) {
  const context = buildContext(rawData);

  renderHero(context);
  renderTrends(context);
  renderModels(context);
  renderSources(context);
  renderProjects(context);
  renderCosts(context);
}

function renderBootstrapError(error) {
  const message = error?.message || '读取本地数据失败';
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
        <div class="status-eyebrow">运行概览</div>
        <div style="font-size: 18px; font-weight: 600; margin-top: 2px;">数据加载失败</div>
      </div>
      <span class="status-pill" data-tone="warn"><span class="pulse"></span>异常</span>
    </div>
    <div class="status-list">
      <div class="status-row">
        <span class="status-row-name">detail</span>
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
        数据读取<span class="mono">失败</span>
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
  const sections = ['overview', 'trends', 'models', 'sources', 'projects', 'cost', 'status'];
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

  const projAnchor = document.getElementById('projects-anchor');
  document.querySelectorAll('a[data-target="projects"]').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      if (projAnchor) projAnchor.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
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

    seg.querySelectorAll('button').forEach((b) => b.classList.remove('active'));
    e.target.classList.add('active');

    try {
      state.trendWindow = nextWindow;
      state.rawData = {
        ...state.rawData,
        trends: await loadTrendWindow(state, nextWindow),
      };
      renderDashboard(state.rawData);
    } catch (error) {
      logger.error('趋势窗口切换失败', error);
      renderBootstrapError(error);
    }
  });
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', main);
} else {
  main();
}
