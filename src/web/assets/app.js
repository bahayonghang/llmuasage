import { buildContext } from './data/derive.js';
import { renderHero } from './render/hero.js';
import { renderTrends } from './render/trends.js';
import { renderModels } from './render/models.js';
import { renderSources } from './render/sources.js';
import { renderProjects } from './render/projects.js';
import { renderCosts } from './render/costs.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：主入口
 * ========================================================================
 * 目标：
 * 1) 从 window.LLMUSAGE_DATA 读取原始数据
 * 2) 调用 buildContext 派生渲染上下文
 * 3) 依次调用各区域 render 函数
 * 4) 设置 IntersectionObserver 实现侧边栏高亮
 */
async function main() {
  logger.info('llmusage dashboard 启动');

  // 1.1 读取原始数据
  const rawData = window.LLMUSAGE_DATA;
  if (!rawData) {
    logger.error('未找到 window.LLMUSAGE_DATA');
    return;
  }

  // 1.2 构建渲染上下文
  const context = buildContext(rawData);

  // 1.3 依次渲染各区域
  renderHero(context);
  renderTrends(context);
  renderModels(context);
  renderSources(context);
  renderProjects(context);
  renderCosts(context);

  // 1.4 设置侧边栏导航高亮
  setupNavigation();

  // 1.5 设置趋势区时间窗口切换
  setupTrendSegments();

  logger.info('llmusage dashboard 渲染完成');
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

  // 覆盖 projects 链接，滚动到内联面板
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
 * 2) 切换 active 状态
 */
function setupTrendSegments() {
  const seg = document.getElementById('seg');
  if (!seg) return;

  seg.addEventListener('click', (e) => {
    if (e.target.tagName === 'BUTTON') {
      seg.querySelectorAll('button').forEach((b) => b.classList.remove('active'));
      e.target.classList.add('active');
    }
  });
}

// 启动
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', main);
} else {
  main();
}
