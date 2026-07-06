import { escapeHtml, formatNumber, formatTokenAmount, ratio } from '../data.js';

const logger = window.console;

/*
 * ========================================================================
 * 步骤1：渲染项目排行区
 * ========================================================================
 * 目标：
 * 1) 填充项目数标签
 * 2) 填充项目行（前 6 个）
 */
export function renderProjects(context, state = {}) {
  logger.info('开始渲染项目排行区');

  const { panels } = context;
  const projectRows = panels.projects || [];
  const expanded = Boolean(state?.expanded?.projects);
  const visibleRows = expanded ? projectRows : projectRows.slice(0, 6);
  const max = Number(projectRows[0]?.total_tokens || 1);

  // 1.1 填充项目数标签
  document.getElementById('projects-count').textContent = `${projectRows.length} 个项目`;

  if (!visibleRows.length) {
    document.getElementById('projects-rows').innerHTML = `
      <div class="empty-state compact">暂无项目数据。</div>
    `;
    logger.info('完成项目排行区渲染');
    return;
  }

  // 1.2 填充项目行
  const rowsHtml = visibleRows
    .map((row) => {
      const total_tokens = Number(row.total_tokens || 0);
      const widthPct = ratio(total_tokens, max);
      const projectName = row.project_label || row.project_name || '--';
      const project_ref = row.project_ref || row.project_hash || row.project_url || '--';
      const exactTokens = `${formatNumber(total_tokens)} Token`;

      return `
        <div class="project-row">
          <div class="project-name">${escapeHtml(projectName)}</div>
          <div class="project-url">${escapeHtml(project_ref)}</div>
          <div class="project-bar-wrap"><div class="project-bar-track"><div class="project-bar-fill" style="width: ${widthPct}%"></div></div></div>
          <div class="project-value" title="${escapeHtml(exactTokens)}">${escapeHtml(formatTokenAmount(total_tokens))}</div>
        </div>
      `;
    })
    .join('');

  document.getElementById('projects-rows').innerHTML = rowsHtml;

  logger.info('完成项目排行区渲染');
}
