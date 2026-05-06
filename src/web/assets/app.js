import { UI_COPY, getLocale, onLocaleChange, setLocale } from './copy.js';
import { buildContext, loadSection, loadTrendWindow } from './data.js';
import { renderHero } from './render/hero.js';
import { renderTrends } from './render/trends.js';
import { renderModels } from './render/models.js';
import { renderSources } from './render/sources.js';
import { renderProjects } from './render/projects.js';
import { renderCosts } from './render/costs.js';
import { applyDomI18n, bindI18nDomSync } from './i18n.js';
import { initTheme, toggleTheme } from './theme.js';
import { setRenderer, setRuntimeState } from './runtime.js';

const logger = window.console;
const DEFAULT_TREND_WINDOW = 'day';

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
    trendWindow: DEFAULT_TREND_WINDOW,
    rawData: null,
  };

  setRuntimeState(state);
  setRenderer(renderDashboard);

  try {
    // 1.2 先加载首屏需要的全部数据
    state.rawData = await loadDashboardData(state);

    // 1.3 首次渲染
    renderDashboard(state.rawData);

    // 1.4 绑定交互
    setupNavigation();
    setupTrendSegments(state);
    setupExport(state);
    setupThemeToggle();
    setupLocaleToggle(state);

    // 1.5 切语言时基于已有 rawData 直接重渲，不再请求接口
    onLocaleChange(() => {
      if (state.rawData) {
        renderDashboard(state.rawData);
      }
    });

    logger.info('llmusage dashboard 渲染完成');
  } catch (error) {
    logger.error('llmusage dashboard 数据加载失败', error);
    renderBootstrapError(error);
    setupThemeToggle();
    setupLocaleToggle(state);
    onLocaleChange(() => renderBootstrapError(error));
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
      app: { name: 'llmusage', version: 'v0.4.2' },
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
