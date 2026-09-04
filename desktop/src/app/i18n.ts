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
    heroDesc: "核心块来自 dashboard_interactive。次级六卡稍后由 secondary-ui 加载。",
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
    heroDesc: "Core blocks come from dashboard_interactive. The six summary cards load later in secondary-ui.",
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
