export function escapeCsvCell(value) {
  let text = value == null ? '' : String(value);
  if (/^[=+\-@\t\r\n]/.test(text)) text = `'${text}`;
  if (/[",\r\n]/.test(text)) text = `"${text.replaceAll('"', '""')}"`;
  return text;
}
function section(title, headers, rows) { return [[title], headers, ...rows].map((row) => row.map(escapeCsvCell).join(',')); }
export function buildAnalyticsCsv(data = {}, locale = 'zh') {
  const labels = locale === 'zh' ? { summary: '汇总', metric: '指标', value: '值', daily: '每日 Token 用量', date: '日期', tokens: 'Token 用量', cost: '估算成本', projects: '项目', project: '项目', models: '模型', model: '模型', sources: '来源', source: '来源', sessions: '高用量会话', session: '会话', duration: '活跃分钟', sessionsMetric: '会话数', requestsMetric: '请求数', tokensMetric: 'Token 用量', costMetric: '估算成本', activeDaysMetric: '活跃天数', cacheEfficiencyMetric: '缓存读取占比' } : { summary: 'Summary', metric: 'Metric', value: 'Value', daily: 'Daily token usage', date: 'Date', tokens: 'Token usage', cost: 'Estimated cost', projects: 'Projects', project: 'Project', models: 'Models', model: 'Model', sources: 'Sources', source: 'Source', sessions: 'Highest-usage sessions', session: 'Session', duration: 'Active minutes', sessionsMetric: 'Sessions', requestsMetric: 'Requests', tokensMetric: 'Token usage', costMetric: 'Estimated cost', activeDaysMetric: 'Active days', cacheEfficiencyMetric: 'Cache-read share' };
  const summary = data.home_overview?.summary || {};
  const summaryRows = [
    [labels.sessionsMetric, summary.total_sessions || 0],
    [labels.requestsMetric, summary.total_requests || 0],
    [labels.tokensMetric, summary.total_tokens || 0],
    [labels.costMetric, summary.total_cost_usd || 0],
    [labels.activeDaysMetric, summary.active_days || 0],
    [labels.cacheEfficiencyMetric, (Number(summary.cache_efficiency || 0) * 100).toFixed(1) + '%'],
  ];
  const top = Array.isArray(data.top_sessions) ? data.top_sessions : (data.top_sessions?.rows || []);
  const blocks = [
    section(labels.summary, [labels.metric, labels.value], summaryRows),
    section(labels.daily, [labels.date, labels.tokens, labels.cost], (data.trends_daily || []).map((r) => [r.date, r.total_tokens, r.cost_with_cache_usd])),
    section(labels.projects, [labels.project, labels.tokens, labels.cost], (data.projects || []).map((r) => [r.project_label, r.total_tokens, r.total_cost_usd])),
    section(labels.models, [labels.model, labels.tokens, labels.cost], (data.models || []).map((r) => [r.model, r.total_tokens, r.cost_with_cache_usd])),
    section(labels.sources, [labels.source, labels.tokens], (data.sources || []).map((r) => [r.source, r.total_tokens])),
    section(labels.sessions, [labels.session, labels.tokens, labels.duration, labels.cost], top.map((r) => [r.session_label || r.session_id, r.total_tokens, r.active_minutes, r.cost_usd])),
  ];
  return `\uFEFF${blocks.map((lines) => lines.join('\r\n')).join('\r\n\r\n')}`;
}
export function downloadAnalyticsCsv(data, locale = 'zh', now = new Date()) {
  const blob = new Blob([buildAnalyticsCsv(data, locale)], { type: 'text/csv;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = `llmusage-analytics-${now.toISOString().slice(0, 10).replaceAll('-', '')}.csv`;
  document.body.appendChild(link); link.click(); link.remove(); URL.revokeObjectURL(url);
}
