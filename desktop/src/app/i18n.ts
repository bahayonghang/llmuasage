import type { Locale } from "./types";

export type Copy = {
  brandSub: string;
  navGroupOverview: string;
  navGroupDistribution: string;
  navGroupOps: string;
  navUsage: string;
  navTrend: string;
  navModels: string;
  navSources: string;
  navProjects: string;
  navBehavior: string;
  navExplorer: string;
  navCost: string;
  navStatus: string;
  navLogs: string;
  navQuota: string;
  rootDir: string;
  lockHolder: string;
  noLock: string;
  themeToLight: string;
  themeToDark: string;
  localeToggle: string;
  range: string;
  range1d: string;
  range7d: string;
  range30d: string;
  rangeAll: string;
  rangeCustom: string;
  source: string;
  allSources: string;
  since: string;
  until: string;
  sync: string;
  cancelSync: string;
  heroTitle: string;
  heroDesc: string;
  kpiTotal: string;
  kpiDay: string;
  kpiCost: string;
  kpiSources: string;
  overviewTitle: string;
  trendsTitle: string;
  modelsTitle: string;
  sourcesTitle: string;
  hostsTitle: string;
  projectsTitle: string;
  costsTitle: string;
  syncTitle: string;
  statusTitle: string;
  logsTitle: string;
  behaviorTitle: string;
  explorerTitle: string;
  quotaTitle: string;
  secondaryLoading: string;
  quotaPlaceholder: string;
  exportCsv: string;
  autoRefreshOff: string;
  autoRefresh30: string;
  autoRefresh60: string;
  logsSession: string;
  logsClear: string;
  logsTime: string;
  logsSource: string;
  logsModel: string;
  logsTokens: string;
  logsCost: string;
  logsProject: string;
  logsLoading: string;
  logsEmpty: string;
  logsMore: string;
  logsRawLoading: string;
  logsRawUnavailable: string;
  quotaRefresh: string;
  quotaEmpty: string;
  quotaLoading: string;
  quotaFailed: string;
  quotaCacheHit: string;
  quotaShowEmail: string;
  quotaHideEmail: string;
  quotaPlan: string;
  statusIdle: string;
  statusRunning: string;
  statusFailed: string;
  statusLockBusy: string;
  statusLockLost: string;
  lockLostAlert: string;
  diagnosticsTitle: string;
  diagnosticsEmpty: string;
  loadSlow: string;
  loadFailed: string;
  emptyRows: string;
  sixCardsLoading: string;
  cardsDegraded: string;
  cardSessions: string;
  cardRequests: string;
  cardTokens: string;
  cardCost: string;
  cardActiveDays: string;
  cardCache: string;
  heatmapTitle: string;
  hourOfWeekTitle: string;
  topSessionsTitle: string;
  trendsDailyTitle: string;
  supportNormalized: string;
  supportNoData: string;
  supportDegraded: string;
  supportUnsupported: string;
  supportInsufficient: string;
  supportLowSample: string;
  activityTitle: string;
  toolsTitle: string;
  optimizeTitle: string;
  compareTitle: string;
  explorerMetric: string;
  explorerGroupBy: string;
  explorerGranularity: string;
  explorerLimit: string;
  explorerSession: string;
  explorerToolName: string;
  explorerToolKind: string;
  explorerTokenType: string;
  explorerIncludeOther: string;
  explorerIncludeNonTool: string;
  explorerAll: string;
  sortTokens: string;
  sortDuration: string;
  sortCost: string;
  weekdayMon: string;
  weekdayTue: string;
  weekdayWed: string;
  weekdayThu: string;
  weekdayFri: string;
  weekdaySat: string;
  weekdaySun: string;
  inputTokens: string;
  cacheReadTokens: string;
  cacheCreationTokens: string;
  outputTokens: string;
  otherTokens: string;
  optimizeScore: string;
  optimizeSavings: string;
  compareMetric: string;
  metricCost: string;
  metricCalls: string;
  metricTurns: string;
  metricSessions: string;
  metricTokens: string;
  groupSource: string;
  groupModel: string;
  groupProject: string;
  groupSession: string;
  groupTool: string;
  groupToolKind: string;
  groupIsTool: string;
  groupTokenType: string;
  granularityTotal: string;
  granularityDay: string;
  granularityWeek: string;
  granularityMonth: string;
};

export const COPY: Record<Locale, Copy> = {
  zh: {
    brandSub: "本地",
    navGroupOverview: "概览",
    navGroupDistribution: "分布",
    navGroupOps: "运行",
    navUsage: "用量概览",
    navTrend: "用量趋势",
    navModels: "模型分布",
    navSources: "来源分布",
    navProjects: "项目排行",
    navBehavior: "行为分析",
    navExplorer: "用量分析",
    navCost: "成本估算",
    navStatus: "运行状态",
    navLogs: "事件日志",
    navQuota: "额度",
    rootDir: "数据目录",
    lockHolder: "锁持有者",
    noLock: "无锁",
    themeToLight: "浅色",
    themeToDark: "深色",
    localeToggle: "EN",
    range: "时间范围",
    range1d: "近 1 天",
    range7d: "近 7 天",
    range30d: "近 30 天",
    rangeAll: "全部",
    rangeCustom: "自定义",
    source: "来源",
    allSources: "全部来源",
    since: "起始日期",
    until: "结束日期",
    sync: "同步",
    cancelSync: "取消同步",
    heroTitle: "本地用量概览",
    heroDesc: "核心块来自 dashboard_interactive。六张概览卡在核心绘制后加载。",
    kpiTotal: "总用量",
    kpiDay: "近 24 小时",
    kpiCost: "估算成本",
    kpiSources: "来源数",
    overviewTitle: "用量概览",
    trendsTitle: "用量趋势",
    modelsTitle: "模型分布",
    sourcesTitle: "来源分布",
    hostsTitle: "主机分布",
    projectsTitle: "项目排行",
    costsTitle: "成本估算",
    syncTitle: "同步命令中心",
    statusTitle: "运行状态",
    logsTitle: "事件日志",
    behaviorTitle: "行为分析",
    explorerTitle: "用量分析",
    quotaTitle: "额度",
    secondaryLoading: "次级面板稍后加载。",
    quotaPlaceholder: "额度页由 ops-quota 填充。",
    exportCsv: "导出 CSV",
    autoRefreshOff: "关闭刷新",
    autoRefresh30: "30秒",
    autoRefresh60: "60秒",
    logsSession: "会话",
    logsClear: "清除",
    logsTime: "时间",
    logsSource: "来源",
    logsModel: "模型",
    logsTokens: "Token 用量",
    logsCost: "估算成本",
    logsProject: "项目",
    logsLoading: "正在加载事件日志…",
    logsEmpty: "当前筛选暂无事件。",
    logsMore: "下一页",
    logsRawLoading: "正在读取原始记录…",
    logsRawUnavailable: "未保留原始记录。",
    quotaRefresh: "刷新",
    quotaEmpty: "无本地凭证，额度不可用。",
    quotaLoading: "正在读取额度…",
    quotaFailed: "额度读取失败",
    quotaCacheHit: "缓存命中",
    quotaShowEmail: "显示邮箱",
    quotaHideEmail: "隐藏邮箱",
    quotaPlan: "方案",
    statusIdle: "空闲",
    statusRunning: "运行中",
    statusFailed: "失败",
    statusLockBusy: "锁被占用",
    statusLockLost: "锁已丢失",
    lockLostAlert: "工作锁已丢失。已停止写入，避免损坏数据。",
    diagnosticsTitle: "诊断",
    diagnosticsEmpty: "暂无诊断数据",
    loadSlow: "核心查询较慢…",
    loadFailed: "核心查询失败",
    emptyRows: "暂无数据",
    sixCardsLoading: "概览六卡加载中",
    cardsDegraded: "概览六卡不可用",
    cardSessions: "会话数",
    cardRequests: "请求数",
    cardTokens: "Token 用量",
    cardCost: "估算成本",
    cardActiveDays: "活跃天数",
    cardCache: "缓存读取占比",
    heatmapTitle: "每日活跃度",
    hourOfWeekTitle: "每周活跃时段",
    topSessionsTitle: "会话消耗排行",
    trendsDailyTitle: "Token 用量构成",
    supportNormalized: "数据完整",
    supportNoData: "暂无数据",
    supportDegraded: "部分数据",
    supportUnsupported: "不支持",
    supportInsufficient: "模型不足",
    supportLowSample: "样本较少",
    activityTitle: "活动类型",
    toolsTitle: "工具调用",
    optimizeTitle: "优化建议",
    compareTitle: "模型对比",
    explorerMetric: "指标",
    explorerGroupBy: "分组维度",
    explorerGranularity: "时间粒度",
    explorerLimit: "最多显示",
    explorerSession: "会话过滤",
    explorerToolName: "工具过滤",
    explorerToolKind: "工具类型",
    explorerTokenType: "Token 类型",
    explorerIncludeOther: "合并其他项",
    explorerIncludeNonTool: "包含非工具",
    explorerAll: "全部",
    sortTokens: "Token 用量",
    sortDuration: "活跃时长",
    sortCost: "估算成本",
    weekdayMon: "周一",
    weekdayTue: "周二",
    weekdayWed: "周三",
    weekdayThu: "周四",
    weekdayFri: "周五",
    weekdaySat: "周六",
    weekdaySun: "周日",
    inputTokens: "输入",
    cacheReadTokens: "缓存读取",
    cacheCreationTokens: "缓存写入",
    outputTokens: "输出",
    otherTokens: "其他",
    optimizeScore: "优化评分",
    optimizeSavings: "可优化空间",
    compareMetric: "指标",
    metricCost: "归因成本",
    metricCalls: "调用数",
    metricTurns: "轮次",
    metricSessions: "会话数",
    metricTokens: "总 Token",
    groupSource: "来源",
    groupModel: "模型",
    groupProject: "项目",
    groupSession: "会话",
    groupTool: "工具",
    groupToolKind: "工具类型",
    groupIsTool: "工具/非工具",
    groupTokenType: "Token 类型",
    granularityTotal: "总计",
    granularityDay: "按日",
    granularityWeek: "按周",
    granularityMonth: "按月",
  },
  en: {
    brandSub: "local",
    navGroupOverview: "Overview",
    navGroupDistribution: "Distribution",
    navGroupOps: "Operations",
    navUsage: "Usage",
    navTrend: "Trends",
    navModels: "Models",
    navSources: "Sources",
    navProjects: "Projects",
    navBehavior: "Behavior",
    navExplorer: "Usage analysis",
    navCost: "Cost",
    navStatus: "Status",
    navLogs: "Event logs",
    navQuota: "Quota",
    rootDir: "Data directory",
    lockHolder: "Lock holder",
    noLock: "No lock",
    themeToLight: "Light",
    themeToDark: "Dark",
    localeToggle: "中文",
    range: "Range",
    range1d: "1 day",
    range7d: "7 days",
    range30d: "30 days",
    rangeAll: "All",
    rangeCustom: "Custom",
    source: "Source",
    allSources: "All sources",
    since: "Since",
    until: "Until",
    sync: "Sync",
    cancelSync: "Cancel sync",
    heroTitle: "Local usage overview",
    heroDesc: "Core blocks come from dashboard_interactive. The six summary cards load after core paint.",
    kpiTotal: "Total",
    kpiDay: "Last 24h",
    kpiCost: "Estimated cost",
    kpiSources: "Sources",
    overviewTitle: "Usage",
    trendsTitle: "Trends",
    modelsTitle: "Models",
    sourcesTitle: "Sources",
    hostsTitle: "Hosts",
    projectsTitle: "Projects",
    costsTitle: "Cost",
    syncTitle: "Sync command center",
    statusTitle: "Status",
    logsTitle: "Event logs",
    behaviorTitle: "Behavior",
    explorerTitle: "Usage analysis",
    quotaTitle: "Quota",
    secondaryLoading: "Secondary panels load later.",
    quotaPlaceholder: "Quota data is filled by ops-quota.",
    exportCsv: "Export CSV",
    autoRefreshOff: "Refresh off",
    autoRefresh30: "30s",
    autoRefresh60: "60s",
    logsSession: "Session",
    logsClear: "Clear",
    logsTime: "Time",
    logsSource: "Source",
    logsModel: "Model",
    logsTokens: "Token usage",
    logsCost: "Estimated cost",
    logsProject: "Project",
    logsLoading: "Loading events…",
    logsEmpty: "No events match these filters.",
    logsMore: "Next page",
    logsRawLoading: "Loading raw record…",
    logsRawUnavailable: "Raw record was not retained.",
    quotaRefresh: "Refresh",
    quotaEmpty: "No local credentials, quota is unavailable.",
    quotaLoading: "Loading quota…",
    quotaFailed: "Quota fetch failed",
    quotaCacheHit: "Cache hit",
    quotaShowEmail: "Show email",
    quotaHideEmail: "Hide email",
    quotaPlan: "Plan",
    statusIdle: "Idle",
    statusRunning: "Running",
    statusFailed: "Failed",
    statusLockBusy: "Lock busy",
    statusLockLost: "Lock lost",
    lockLostAlert: "The worker lock was lost. Writes have stopped to protect the database.",
    diagnosticsTitle: "Diagnostics",
    diagnosticsEmpty: "No diagnostics",
    loadSlow: "Core query is slow…",
    loadFailed: "Core query failed",
    emptyRows: "No data",
    sixCardsLoading: "Summary cards loading",
    cardsDegraded: "Summary cards unavailable",
    cardSessions: "Sessions",
    cardRequests: "Requests",
    cardTokens: "Token usage",
    cardCost: "Estimated cost",
    cardActiveDays: "Active days",
    cardCache: "Cache-read share",
    heatmapTitle: "Daily activity",
    hourOfWeekTitle: "Weekly activity",
    topSessionsTitle: "Session ranking",
    trendsDailyTitle: "Token usage mix",
    supportNormalized: "Complete data",
    supportNoData: "No data",
    supportDegraded: "Partial data",
    supportUnsupported: "Unsupported",
    supportInsufficient: "Not enough models",
    supportLowSample: "Small sample",
    activityTitle: "Activity",
    toolsTitle: "Tool usage",
    optimizeTitle: "Optimize",
    compareTitle: "Compare",
    explorerMetric: "Metric",
    explorerGroupBy: "Group by",
    explorerGranularity: "Granularity",
    explorerLimit: "Limit",
    explorerSession: "Session",
    explorerToolName: "Tool",
    explorerToolKind: "Tool kind",
    explorerTokenType: "Token type",
    explorerIncludeOther: "Include other",
    explorerIncludeNonTool: "Include non-tool",
    explorerAll: "All",
    sortTokens: "Token usage",
    sortDuration: "Active duration",
    sortCost: "Estimated cost",
    weekdayMon: "Mon",
    weekdayTue: "Tue",
    weekdayWed: "Wed",
    weekdayThu: "Thu",
    weekdayFri: "Fri",
    weekdaySat: "Sat",
    weekdaySun: "Sun",
    inputTokens: "Input",
    cacheReadTokens: "Cache-read",
    cacheCreationTokens: "Cache-creation",
    outputTokens: "Output",
    otherTokens: "Other",
    optimizeScore: "Score",
    optimizeSavings: "Potential savings",
    compareMetric: "Metric",
    metricCost: "Attributed cost",
    metricCalls: "Calls",
    metricTurns: "Turns",
    metricSessions: "Sessions",
    metricTokens: "Total tokens",
    groupSource: "Source",
    groupModel: "Model",
    groupProject: "Project",
    groupSession: "Session",
    groupTool: "Tool",
    groupToolKind: "Tool kind",
    groupIsTool: "Tool / non-tool",
    groupTokenType: "Token type",
    granularityTotal: "Total",
    granularityDay: "Day",
    granularityWeek: "Week",
    granularityMonth: "Month",
  },
};

export type NavItem = {
  id: string;
  group: "overview" | "distribution" | "ops";
  labelKey: keyof Copy;
};

export const NAV_ITEMS: NavItem[] = [
  { id: "overview", group: "overview", labelKey: "navUsage" },
  { id: "trends", group: "overview", labelKey: "navTrend" },
  { id: "models", group: "distribution", labelKey: "navModels" },
  { id: "sources", group: "distribution", labelKey: "navSources" },
  { id: "projects", group: "distribution", labelKey: "navProjects" },
  { id: "behavior", group: "distribution", labelKey: "navBehavior" },
  { id: "explorer", group: "distribution", labelKey: "navExplorer" },
  { id: "cost", group: "ops", labelKey: "navCost" },
  { id: "status", group: "ops", labelKey: "navStatus" },
  { id: "logs", group: "ops", labelKey: "navLogs" },
  { id: "quota", group: "ops", labelKey: "navQuota" },
];
