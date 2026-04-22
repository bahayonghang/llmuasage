import { renderHero } from './render/hero.js';
import { renderTrend } from './render/charts.js';
import { renderTables } from './render/tables.js';
import { renderHealth } from './render/health.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：编排整页渲染
 * ========================================================================
 * 目标：
 * 1) 让 hero、charts、tables、health 四个渲染模块顺序执行
 * 2) 把页面编排职责留在单一入口文件
 * 3) 让后续新增版块不再回流到 app.js
 */
export function renderPage(context, uiState) {
  logger.info('开始渲染整页');

  // 1.1 依次刷新首屏、趋势区、分析区和健康区
  renderHero(context);
  renderTrend(context, uiState);
  renderTables(context, uiState);
  renderHealth(context);

  logger.info('完成整页渲染');
}
