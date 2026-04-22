import {
  escapeHtml,
  formatCompact,
  formatNumber,
  formatMaybe,
} from '../data.js';
import { UI_COPY } from '../copy.js';

const logger = window.console;

function renderSummaryRow(label, value, mono = false) {
  const valueClass = mono ? 'summary-value mono' : 'summary-value';
  return `
    <div class="summary-row">
      <div class="summary-label">${escapeHtml(label)}</div>
      <div class="${valueClass}">${escapeHtml(value)}</div>
    </div>
  `;
}

/*
 * ========================================================================
 * 步骤1：渲染概览卡与 KPI 条带
 * ========================================================================
 * 目标：
 * 1) 把首屏动态内容压缩成概览卡和 4 张 KPI 卡
 * 2) 统一用紧凑值显示长数字，原始值移到辅助行
 * 3) 避免任何断点下 KPI 文字越出卡片
 */
export function renderHero(context) {
  logger.info('开始渲染概览卡与 KPI 条带');

  // 1.1 渲染右侧概览卡
  const leaderSource = context.leaders.source?.source || '等待同步';
  const leaderModel = context.leaders.model?.model || '等待同步';
  const summaryTone = context.ledgerSummary.failureCount > 0 ? 'warn' : 'neutral';

  document.getElementById('ledger-summary').innerHTML = `
    <div class="section-header section-header--tight">
      <div>
        <p class="section-kicker">${escapeHtml(UI_COPY.hero.summaryKicker)}</p>
        <h2>${escapeHtml(UI_COPY.hero.summaryTitle)}</h2>
      </div>
      <span class="status-pill" data-tone="${summaryTone}">${context.ledgerSummary.failureCount > 0 ? UI_COPY.hero.statusWarn : UI_COPY.hero.statusStable}</span>
    </div>
    <div class="summary-list">
      ${renderSummaryRow(UI_COPY.hero.rows.generatedAt, formatMaybe(context.ledgerSummary.generatedAt), true)}
      ${renderSummaryRow(UI_COPY.hero.rows.lastSyncAt, formatMaybe(context.ledgerSummary.lastSyncAt), true)}
      ${renderSummaryRow(UI_COPY.hero.rows.lastExportAt, formatMaybe(context.ledgerSummary.lastExportAt), true)}
      ${renderSummaryRow(UI_COPY.hero.rows.sourceCount, `${formatNumber(context.ledgerSummary.activeSources)} · ${leaderSource}`)}
      ${renderSummaryRow(UI_COPY.hero.rows.failureCount, formatNumber(context.ledgerSummary.failureCount))}
      ${renderSummaryRow(UI_COPY.hero.rows.topModel, leaderModel)}
    </div>
  `;

  // 1.2 渲染 4 张 KPI 卡，长数字只保留紧凑值和原始值脚注
  const cards = [
    {
      tone: 'metric-card metric-card--accent',
      label: UI_COPY.hero.metrics.total.label,
      value: context.totals.totalTokensCompact,
      body: UI_COPY.hero.metrics.total.body,
      footPrimary: `用量最高模型：${context.leaders.model?.model || '等待同步'}`,
      footRaw: `原始值 ${context.totals.totalTokensRaw}`,
    },
    {
      tone: 'metric-card',
      label: UI_COPY.hero.metrics.last24h.label,
      value: context.totals.last24hTokensCompact,
      body: UI_COPY.hero.metrics.last24h.body,
      footPrimary: `平均每段 ${formatCompact(context.trend.average)}`,
      footRaw: `原始值 ${context.totals.last24hTokensRaw}`,
    },
    {
      tone: 'metric-card',
      label: UI_COPY.hero.metrics.sources.label,
      value: formatNumber(context.ledgerSummary.activeSources),
      body: UI_COPY.hero.metrics.sources.body,
      footPrimary: `主要来源：${context.leaders.source?.source || '等待同步'}`,
      footRaw: `最近记录 ${context.leaders.source?.last_event_at || '尚未记录'}`,
    },
    {
      tone: 'metric-card',
      label: UI_COPY.hero.metrics.cost.label,
      value: context.totals.totalCostCompact,
      body: UI_COPY.hero.metrics.cost.body,
      footPrimary:
        context.leaders.cost?.model && context.leaders.cost?.source
          ? `成本最高组合：${context.leaders.cost.source} · ${context.leaders.cost.model}`
          : '等待成本数据',
      footRaw: `原始值 ${context.totals.totalCostRaw}`,
    },
  ];

  document.getElementById('overview').innerHTML = cards
    .map(
      (card) => `
        <article class="${card.tone}">
          <div>
            <p class="section-kicker">${escapeHtml(card.label)}</p>
            <div class="metric-value mono">${escapeHtml(card.value)}</div>
          </div>
          <div>
            <div class="metric-copy">${escapeHtml(card.body)}</div>
            <div class="metric-foot">
              <div>${escapeHtml(card.footPrimary)}</div>
              <div class="metric-foot-raw mono">${escapeHtml(card.footRaw)}</div>
            </div>
          </div>
        </article>
      `,
    )
    .join('');

  logger.info('完成概览卡与 KPI 条带渲染');
}
