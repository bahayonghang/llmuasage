export type AnalyticsCsvInput = {
  home_overview?: {
    summary?: {
      total_sessions?: number;
      total_requests?: number;
      total_tokens?: number;
      total_cost_usd?: number;
      active_days?: number;
      cache_efficiency?: number;
    };
  };
  trends_daily?: { date?: string; total_tokens?: number; cost_with_cache_usd?: number }[];
  projects?: { project_label?: string; total_tokens?: number; total_cost_usd?: number }[];
  models?: { model?: string; total_tokens?: number; cost_with_cache_usd?: number }[];
  sources?: { source?: string; total_tokens?: number }[];
  top_sessions?:
    | {
        session_label?: string | null;
        session_id?: string | null;
        total_tokens?: number;
        active_minutes?: number;
        cost_usd?: number;
      }[]
    | {
        rows?: {
          session_label?: string | null;
          session_id?: string | null;
          total_tokens?: number;
          active_minutes?: number;
          cost_usd?: number;
        }[];
      };
};

export function escapeCsvCell(value: unknown): string {
  let text = value == null ? "" : String(value);
  if (/^[=+\-@\t\r\n]/.test(text)) {
    text = `'${text}`;
  }
  if (/[",\r\n]/.test(text)) {
    text = `"${text.replaceAll('"', '""')}"`;
  }
  return text;
}

function section(title: string, headers: unknown[], rows: unknown[][]): string[] {
  return [[title], headers, ...rows].map((row) => row.map(escapeCsvCell).join(","));
}

export function buildAnalyticsCsv(data: AnalyticsCsvInput = {}, locale = "zh"): string {
  const labels =
    locale === "zh"
      ? {
          summary: "汇总",
          metric: "指标",
          value: "值",
          daily: "每日 Token 用量",
          date: "日期",
          tokens: "Token 用量",
          cost: "估算成本",
          projects: "项目",
          project: "项目",
          models: "模型",
          model: "模型",
          sources: "来源",
          source: "来源",
          sessions: "高用量会话",
          session: "会话",
          duration: "活跃分钟",
          sessionsMetric: "会话数",
          requestsMetric: "请求数",
          tokensMetric: "Token 用量",
          costMetric: "估算成本",
          activeDaysMetric: "活跃天数",
          cacheEfficiencyMetric: "缓存读取占比",
        }
      : {
          summary: "Summary",
          metric: "Metric",
          value: "Value",
          daily: "Daily token usage",
          date: "Date",
          tokens: "Token usage",
          cost: "Estimated cost",
          projects: "Projects",
          project: "Project",
          models: "Models",
          model: "Model",
          sources: "Sources",
          source: "Source",
          sessions: "Highest-usage sessions",
          session: "Session",
          duration: "Active minutes",
          sessionsMetric: "Sessions",
          requestsMetric: "Requests",
          tokensMetric: "Token usage",
          costMetric: "Estimated cost",
          activeDaysMetric: "Active days",
          cacheEfficiencyMetric: "Cache-read share",
        };
  const summary = data.home_overview?.summary || {};
  const summaryRows = [
    [labels.sessionsMetric, summary.total_sessions || 0],
    [labels.requestsMetric, summary.total_requests || 0],
    [labels.tokensMetric, summary.total_tokens || 0],
    [labels.costMetric, summary.total_cost_usd || 0],
    [labels.activeDaysMetric, summary.active_days || 0],
    [labels.cacheEfficiencyMetric, `${(Number(summary.cache_efficiency || 0) * 100).toFixed(1)}%`],
  ];
  const top = Array.isArray(data.top_sessions) ? data.top_sessions : data.top_sessions?.rows || [];
  const blocks = [
    section(labels.summary, [labels.metric, labels.value], summaryRows),
    section(
      labels.daily,
      [labels.date, labels.tokens, labels.cost],
      (data.trends_daily || []).map((row) => [row.date, row.total_tokens, row.cost_with_cache_usd]),
    ),
    section(
      labels.projects,
      [labels.project, labels.tokens, labels.cost],
      (data.projects || []).map((row) => [row.project_label, row.total_tokens, row.total_cost_usd]),
    ),
    section(
      labels.models,
      [labels.model, labels.tokens, labels.cost],
      (data.models || []).map((row) => [row.model, row.total_tokens, row.cost_with_cache_usd]),
    ),
    section(
      labels.sources,
      [labels.source, labels.tokens],
      (data.sources || []).map((row) => [row.source, row.total_tokens]),
    ),
    section(
      labels.sessions,
      [labels.session, labels.tokens, labels.duration, labels.cost],
      top.map((row) => [row.session_label || row.session_id, row.total_tokens, row.active_minutes, row.cost_usd]),
    ),
  ];
  return `\uFEFF${blocks.map((lines) => lines.join("\r\n")).join("\r\n\r\n")}`;
}

export function analyticsCsvFileName(now = new Date()): string {
  return `llmusage-analytics-${now.toISOString().slice(0, 10).replaceAll("-", "")}.csv`;
}
