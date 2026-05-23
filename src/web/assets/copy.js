const logger = window.console;

export const LOCALES = Object.freeze(['zh', 'en']);
export const DEFAULT_LOCALE = 'zh';
export const LOCALE_STORAGE_KEY = 'llmusage:locale';

/*
 * ========================================================================
 * 步骤1：定义中文 UI_COPY
 * ========================================================================
 * 目标：
 * 1) 保留原有结构，render/* 模块的导入路径无需改动
 * 2) 给 hero / runtime 错误占位补齐显式的字段，避免散落的硬编码
 */
const UI_COPY_ZH = Object.freeze({
  hero: Object.freeze({
    summaryKicker: '概览',
    summaryTitle: '运行概览',
    statusEyebrow: '运行概览',
    statusTitle: '系统健康',
    statusStable: '正常',
    statusOk: '正常',
    statusWarn: '有失败',
    statusUnknown: '未知',
    rows: Object.freeze({
      generated_at: '生成时间',
      last_sync_at: '最近同步',
      last_export_at: '最近导出',
      sourceCount: '来源数',
      failure_count: '失败记录',
      topModel: '用量最高模型',
    }),
    cell: Object.freeze({
      integrations: '集成',
      cursors: '游标数',
      failures: '失败',
    }),
    metrics: Object.freeze({
      total: Object.freeze({
        label: '总用量',
        body: '累计 Token',
      }),
      last24h: Object.freeze({
        label: '近 24 小时',
        body: '最近 24 小时总量',
      }),
      sources: Object.freeze({
        label: '来源数',
        body: '已记录来源',
      }),
      cost: Object.freeze({
        label: '估算成本',
        body: '累计成本',
      }),
    }),
    error: Object.freeze({
      title: '数据加载失败',
      detail: 'detail',
      heroMeta: '数据读取',
      heroMetaState: '失败',
      pill: '异常',
      generic: '读取本地数据失败',
    }),
  }),
  actions: Object.freeze({
    exportDone: '已导出',
  }),
  sections: Object.freeze({
    trend: Object.freeze({
      kicker: '趋势',
      title: '用量趋势',
      copy: '主图展示当前窗口内最近 10 条记录，完整明细可展开查看。',
      detailKicker: '明细',
      detailCopy: '当前窗口的完整时间序列默认折叠，避免首屏过长。',
      expandLabel: '展开完整明细',
      collapseLabel: '收起完整明细',
      totalLabel: '时间窗口总量',
      peakLabel: '最高单段用量',
      averageLabel: '平均每段用量',
      rawPrefix: '原始值',
      tableTime: '时间',
      tableTokens: '总用量',
      emptyChart: '暂无趋势数据。',
      tableEmpty: '暂无趋势明细。',
      chartAria: '用量趋势柱状图',
    }),
    models: Object.freeze({
      kicker: '模型',
      title: '模型用量分布',
      copy: '先看用量最高的模型，再按需展开完整排行。',
      chartCaption: '用量最高的 8 个模型',
      expandedChartCaption: '全部模型',
      chartAria: '模型用量横向柱状图',
      emptyChart: '暂无模型统计。',
      emptyTable: '暂无模型对比数据。',
      expandLabel: '展开完整排行',
      collapseLabel: '收起完整排行',
      headers: Object.freeze({
        model: '模型',
        total_tokens: '总用量',
        input_share: '输入占比',
        output_share: '输出占比',
        cached_share: '缓存占比',
      }),
    }),
    sources: Object.freeze({
      kicker: '来源',
      title: '来源分布',
      chartCaption: '用量最高的 4 个来源',
      expandedChartCaption: '全部来源',
      chartAria: '来源用量横向柱状图',
      emptyChart: '暂无来源统计。',
      emptyTable: '暂无来源明细。',
      expandLabel: '展开全部来源',
      collapseLabel: '收起全部来源',
      headers: Object.freeze({
        source: '来源',
        last_event_at: '最近记录',
      }),
    }),
    projects: Object.freeze({
      kicker: '项目',
      title: '项目排行',
      emptyTable: '暂无项目数据。',
      expandLabel: '展开全部项目',
      collapseLabel: '收起全部项目',
      headers: Object.freeze({
        project: '项目',
        ref: '项目标识',
        tokens: '总用量',
      }),
    }),
    costs: Object.freeze({
      kicker: '成本',
      title: '成本估算',
      chartCaption: '成本最高的 5 个来源 / 模型组合',
      expandedChartCaption: '全部来源 / 模型成本项',
      chartAria: '成本估算横向柱状图',
      emptyChart: '暂无成本数据。',
      emptyTable: '暂无成本明细。',
      expandLabel: '展开全部成本项',
      collapseLabel: '收起全部成本项',
      headers: Object.freeze({
        model: '模型',
        source: '来源',
        estimated_cost_usd: '估算成本',
      }),
    }),
    health: Object.freeze({
      kicker: '状态',
      title: '运行状态',
      chips: Object.freeze({
        integrations: '集成就绪',
        cursors: '游标数',
        failures: '失败记录',
      }),
      failuresTitle: '最近失败',
      failuresEmpty: '当前没有失败记录。',
      integrationsTitle: '集成状态',
      integrationsEmpty: '暂无集成状态。',
    }),
    insights: Object.freeze({
      kicker: '洞察',
      title: '诊断线索',
      copy: '这些信号用于定位下一步排查入口，不是最终诊断或账单结论。',
      disclaimer: 'Reading Dashboard：以下是基于本地数据的信号和可能下一步。',
      emptyTitle: '暂无需要关注的信号',
      emptyBody: '当前窗口未发现失败、定价缺口或源文件保留风险。',
      defaultLabel: '信号',
      defaultAction: '结合具体会话和同步日志继续确认。',
    }),
  }),
});

/*
 * ========================================================================
 * 步骤2：定义英文 UI_COPY（结构与中文严格一致）
 * ========================================================================
 */
const UI_COPY_EN = Object.freeze({
  hero: Object.freeze({
    summaryKicker: 'Overview',
    summaryTitle: 'Run summary',
    statusEyebrow: 'Run summary',
    statusTitle: 'System health',
    statusStable: 'Healthy',
    statusOk: 'Healthy',
    statusWarn: 'Has failures',
    statusUnknown: 'Unknown',
    rows: Object.freeze({
      generated_at: 'Generated',
      last_sync_at: 'Last sync',
      last_export_at: 'Last export',
      sourceCount: 'Sources',
      failure_count: 'Failures',
      topModel: 'Top model',
    }),
    cell: Object.freeze({
      integrations: 'Integrations',
      cursors: 'Cursors',
      failures: 'Failures',
    }),
    metrics: Object.freeze({
      total: Object.freeze({
        label: 'Total',
        body: 'Cumulative tokens',
      }),
      last24h: Object.freeze({
        label: 'Last 24h',
        body: 'Tokens in last 24 hours',
      }),
      sources: Object.freeze({
        label: 'Sources',
        body: 'Recorded sources',
      }),
      cost: Object.freeze({
        label: 'Est. cost',
        body: 'Cumulative cost',
      }),
    }),
    error: Object.freeze({
      title: 'Failed to load data',
      detail: 'detail',
      heroMeta: 'Data read',
      heroMetaState: 'failed',
      pill: 'Error',
      generic: 'Failed to read local data',
    }),
  }),
  actions: Object.freeze({
    exportDone: 'Exported',
  }),
  sections: Object.freeze({
    trend: Object.freeze({
      kicker: 'Trends',
      title: 'Usage trends',
      copy: 'The chart shows the most recent 10 buckets in the current window. Expand for the full table.',
      detailKicker: 'Detail',
      detailCopy: 'The full time series for the current window is collapsed by default to keep the first screen tight.',
      expandLabel: 'Expand full detail',
      collapseLabel: 'Collapse detail',
      totalLabel: 'Window total',
      peakLabel: 'Peak bucket',
      averageLabel: 'Average per bucket',
      rawPrefix: 'Raw',
      tableTime: 'Time',
      tableTokens: 'Tokens',
      emptyChart: 'No trend data.',
      tableEmpty: 'No trend detail.',
      chartAria: 'Usage trend bar chart',
    }),
    models: Object.freeze({
      kicker: 'Models',
      title: 'Model usage',
      copy: 'Top models first; expand for the full ranking on demand.',
      chartCaption: 'Top 8 models by tokens',
      expandedChartCaption: 'All models',
      chartAria: 'Model usage horizontal bar chart',
      emptyChart: 'No model data.',
      emptyTable: 'No model comparison data.',
      expandLabel: 'Expand full ranking',
      collapseLabel: 'Collapse ranking',
      headers: Object.freeze({
        model: 'Model',
        total_tokens: 'Tokens',
        input_share: 'Input %',
        output_share: 'Output %',
        cached_share: 'Cached %',
      }),
    }),
    sources: Object.freeze({
      kicker: 'Sources',
      title: 'Sources',
      chartCaption: 'Top 4 sources by tokens',
      expandedChartCaption: 'All sources',
      chartAria: 'Source usage horizontal bar chart',
      emptyChart: 'No source data.',
      emptyTable: 'No source detail.',
      expandLabel: 'Expand all sources',
      collapseLabel: 'Collapse sources',
      headers: Object.freeze({
        source: 'Source',
        last_event_at: 'Last seen',
      }),
    }),
    projects: Object.freeze({
      kicker: 'Projects',
      title: 'Projects',
      emptyTable: 'No project data.',
      expandLabel: 'Expand all projects',
      collapseLabel: 'Collapse projects',
      headers: Object.freeze({
        project: 'Project',
        ref: 'Reference',
        tokens: 'Tokens',
      }),
    }),
    costs: Object.freeze({
      kicker: 'Cost',
      title: 'Cost estimate',
      chartCaption: 'Top 5 source / model combinations',
      expandedChartCaption: 'All source / model entries',
      chartAria: 'Cost estimate horizontal bar chart',
      emptyChart: 'No cost data.',
      emptyTable: 'No cost detail.',
      expandLabel: 'Expand all cost entries',
      collapseLabel: 'Collapse cost entries',
      headers: Object.freeze({
        model: 'Model',
        source: 'Source',
        estimated_cost_usd: 'Est. cost',
      }),
    }),
    health: Object.freeze({
      kicker: 'Status',
      title: 'Health',
      chips: Object.freeze({
        integrations: 'Integrations ready',
        cursors: 'Cursors',
        failures: 'Failures',
      }),
      failuresTitle: 'Recent failures',
      failuresEmpty: 'No failures recorded.',
      integrationsTitle: 'Integrations',
      integrationsEmpty: 'No integration status.',
    }),
    insights: Object.freeze({
      kicker: 'Insights',
      title: 'Diagnostic signals',
      copy: 'Signals point to the next investigation step; they are not final diagnoses or billing truth.',
      disclaimer: 'Reading Dashboard: local-data signals and possible next steps.',
      emptyTitle: 'No attention signals',
      emptyBody: 'This window has no failures, pricing gaps, or source retention risks.',
      defaultLabel: 'Signal',
      defaultAction: 'Confirm with session details and sync logs.',
    }),
  }),
});

/*
 * ========================================================================
 * 步骤3：扁平化 SHELL_COPY，专供 [data-i18n] 走 DOM 替换
 * ========================================================================
 * 目标：
 * 1) 服务端模板里的中文短语全部走 key 查表
 * 2) 默认值仍写在 HTML 中（保持 mod.rs 现有断言）
 * 3) 只覆盖切到英文需要替换的部分
 */
const SHELL_COPY_ZH = Object.freeze({
  'shell.crumb.dashboard': 'dashboard',
  'shell.crumb.local': '本地用量概览',
  'shell.tag.local': '仅本地',
  'shell.tag.snapshot': '离线文件',
  'shell.btn.export': '导出 JSON',
  'shell.btn.sync': '同步',
  'shell.sync.idle': '待同步',
  'shell.sync.running': '同步中',
  'shell.sync.completed': '同步完成',
  'shell.sync.cancelled': '已取消',
  'shell.sync.failed': '同步失败',
  'shell.sync.cancel': '取消同步',
  'shell.sync.snapshotDisabled': '离线快照不可启动同步',
  'shell.refresh.label': '刷新',
  'shell.refresh.off': '关闭',
  'shell.refresh.aria': '自动刷新间隔',
  'shell.refresh.failed': '刷新失败',
  'shell.refresh.snapshotDisabled': '离线快照不可自动刷新',
  'shell.brand.sub': 'local',
  'shell.nav.label.overview': '概览',
  'shell.nav.label.distribution': '分布',
  'shell.nav.label.ops': '运营',
  'shell.nav.item.usage': '用量概览',
  'shell.nav.item.trend': '用量趋势',
  'shell.nav.item.models': '模型分布',
  'shell.nav.item.sources': '来源分布',
  'shell.nav.item.projects': '项目排行',
  'shell.nav.item.behavior': '行为分析',
  'shell.nav.item.cost': '成本估算',
  'shell.nav.item.status': '运行状态',
  'shell.endpoint.lastSync': '最近同步',
  'shell.filters.source': '来源',
  'shell.filters.allSources': '全部来源',
  'shell.filters.model': '模型',
  'shell.filters.modelPlaceholder': 'all models',
  'shell.filters.range': '时间范围',
  'shell.filters.rangeAria': '快捷时间范围',
  'shell.filters.range.1d': '近 1 天',
  'shell.filters.range.7d': '近 7 天',
  'shell.filters.range.30d': '近 30 天',
  'shell.filters.range.all': '全部',
  'shell.filters.since': '起始日期',
  'shell.filters.until': '结束日期',
  'shell.filters.datePlaceholder': 'YYYY-MM-DD',
  'shell.filters.apply': '应用筛选',
  'shell.filters.reset': '重置',
  'shell.filters.snapshotDisabled': '离线快照使用导出时的固定筛选',
  'shell.hero.eyebrow': 'DASHBOARD',
  'shell.hero.title.html': '本地用量<span class="accent">概览</span>',
  'shell.hero.desc':
    '本地查看近期用量、成本估算和运行状态。所有数据存放在本机 SQLite 中，不依赖任何外部接口、不上报任何遥测，可放心断网使用。',
  'shell.trends.eyebrow': 'TRENDS',
  'shell.trends.title': '用量趋势',
  'shell.trends.sub': '主图展示当前窗口内最近 10 条记录，完整明细可展开查看。',
  'shell.trends.legend.tokens': '用量 (Token)',
  'shell.trends.chart.recent10': '最近 10 个时段',
  'shell.models.eyebrow': 'MODELS',
  'shell.models.title': '模型用量分布',
  'shell.models.sub': '先看用量最高的模型，再按需展开完整排行。',
  'shell.models.panelTitle': '用量最高的 8 个模型',
  'shell.models.panelSub': '单位：Token，按累计计算',
  'shell.models.expand': '展开完整排行 →',
  'shell.models.collapse': '收起完整排行 ↑',
  'shell.sources.eyebrow': 'SOURCES',
  'shell.sources.title': '来源分布',
  'shell.sources.sub': '用量最高的 4 个来源',
  'shell.projects.eyebrow': 'PROJECTS',
  'shell.projects.title': '项目排行',
  'shell.projects.sub': '按累计 Token 排序',
  'shell.projects.expand': '展开全部项目 →',
  'shell.projects.collapse': '收起全部项目 ↑',
  'shell.behavior.eyebrow': 'BEHAVIOR',
  'shell.behavior.title': '行为分析',
  'shell.behavior.sub': '基于同步阶段提取的 normalized turn/tool facts；低样本或未支持来源会显式显示降级状态。',
  'shell.behavior.activity.title': 'Activity',
  'shell.behavior.activity.sub': '按 turn category 聚合 turns、one-shot 与 retry',
  'shell.behavior.tools.title': 'Tools',
  'shell.behavior.tools.sub': 'Core tools / shell / MCP / agent actions',
  'shell.behavior.optimize.title': 'Optimize',
  'shell.behavior.optimize.sub': '只读浪费检测；不会自动执行删除、归档或重写。',
  'shell.behavior.compare.title': 'Compare',
  'shell.behavior.compare.sub': '按模型对比成本、one-shot、retry 与工作风格；低样本显式提示。',
  'shell.cost.eyebrow': 'COST',
  'shell.cost.title': '成本估算',
  'shell.cost.sub': '基于公开计价表的本地估算。仅供参考，与账单存在差异。',
  'shell.cost.panelTitle': '成本最高的 5 个 来源 / 模型 组合',
  'shell.cost.panelSub': '单位：USD',
  'shell.cost.expand': '展开全部成本项 →',
  'shell.cost.collapse': '收起全部成本项 ↑',
  'shell.failures.eyebrow': 'FAILURES',
  'shell.failures.title': '最近失败',
  'shell.integrations.eyebrow': 'INTEGRATIONS',
  'shell.integrations.title': '集成状态',
  'shell.insights.eyebrow': 'INSIGHTS',
  'shell.insights.title': '诊断线索',
  'shell.insights.sub': '信号只表示可能的下一步，不代表最终诊断。',
  'shell.footer.build': 'llmusage · local build',
  'shell.footer.backToTop': '回到顶部 ↑',
  'toolbar.theme.toLight': '浅色',
  'toolbar.theme.toDark': '深色',
  'toolbar.lang.label.zh': '中',
  'toolbar.lang.label.en': 'EN',
  'toolbar.theme.aria': '切换主题',
  'toolbar.lang.aria': '切换语言',
  'toolbar.group.aria': '偏好',
  'shell.window.title': 'llmusage · 本地用量概览',
  'seg.all': '全部',
  'shell.date.weekdays': '日|一|二|三|四|五|六',
  'shell.date.clear': '清除',
  'shell.date.today': '今天',
  'shell.date.prevMonth': '上个月',
  'shell.date.nextMonth': '下个月',
});

const SHELL_COPY_EN = Object.freeze({
  'shell.crumb.dashboard': 'dashboard',
  'shell.crumb.local': 'Local usage',
  'shell.tag.local': 'Local-only',
  'shell.tag.snapshot': 'Snapshot',
  'shell.btn.export': 'Export JSON',
  'shell.btn.sync': 'Sync',
  'shell.sync.idle': 'Idle',
  'shell.sync.running': 'Syncing',
  'shell.sync.completed': 'Sync complete',
  'shell.sync.cancelled': 'Cancelled',
  'shell.sync.failed': 'Sync failed',
  'shell.sync.cancel': 'Cancel sync',
  'shell.sync.snapshotDisabled': 'Offline snapshots cannot start sync jobs',
  'shell.refresh.label': 'Refresh',
  'shell.refresh.off': 'Off',
  'shell.refresh.aria': 'Auto refresh interval',
  'shell.refresh.failed': 'Refresh failed',
  'shell.refresh.snapshotDisabled': 'Offline snapshots cannot auto-refresh',
  'shell.brand.sub': 'local',
  'shell.nav.label.overview': 'Overview',
  'shell.nav.label.distribution': 'Distribution',
  'shell.nav.label.ops': 'Operations',
  'shell.nav.item.usage': 'Usage',
  'shell.nav.item.trend': 'Trends',
  'shell.nav.item.models': 'Models',
  'shell.nav.item.sources': 'Sources',
  'shell.nav.item.projects': 'Projects',
  'shell.nav.item.behavior': 'Behavior',
  'shell.nav.item.cost': 'Cost',
  'shell.nav.item.status': 'Status',
  'shell.endpoint.lastSync': 'Last sync',
  'shell.filters.source': 'Source',
  'shell.filters.allSources': 'All sources',
  'shell.filters.model': 'Model',
  'shell.filters.modelPlaceholder': 'All models',
  'shell.filters.range': 'Range',
  'shell.filters.rangeAria': 'Quick date range',
  'shell.filters.range.1d': 'Last 1d',
  'shell.filters.range.7d': 'Last 7d',
  'shell.filters.range.30d': 'Last 30d',
  'shell.filters.range.all': 'All',
  'shell.filters.since': 'Since',
  'shell.filters.until': 'Until',
  'shell.filters.datePlaceholder': 'YYYY-MM-DD',
  'shell.filters.apply': 'Apply filters',
  'shell.filters.reset': 'Reset',
  'shell.filters.snapshotDisabled': 'Offline snapshots use the filters captured at export time',
  'shell.hero.eyebrow': 'DASHBOARD',
  'shell.hero.title.html': 'Local <span class="accent">usage</span>',
  'shell.hero.desc':
    'View recent local usage, cost estimates and runtime status. All data stays in a local SQLite file with no external calls and no telemetry — safe to use offline.',
  'shell.trends.eyebrow': 'TRENDS',
  'shell.trends.title': 'Usage trends',
  'shell.trends.sub': 'The chart shows the most recent 10 buckets in the current window; expand for the full table.',
  'shell.trends.legend.tokens': 'Usage (tokens)',
  'shell.trends.chart.recent10': 'Recent 10 buckets',
  'shell.models.eyebrow': 'MODELS',
  'shell.models.title': 'Model usage',
  'shell.models.sub': 'Top models first; expand for the full ranking on demand.',
  'shell.models.panelTitle': 'Top 8 models by tokens',
  'shell.models.panelSub': 'Unit: tokens, cumulative',
  'shell.models.expand': 'Expand full ranking →',
  'shell.models.collapse': 'Collapse ranking ↑',
  'shell.sources.eyebrow': 'SOURCES',
  'shell.sources.title': 'Sources',
  'shell.sources.sub': 'Top 4 sources by tokens',
  'shell.projects.eyebrow': 'PROJECTS',
  'shell.projects.title': 'Projects',
  'shell.projects.sub': 'Sorted by cumulative tokens',
  'shell.projects.expand': 'Expand all projects →',
  'shell.projects.collapse': 'Collapse projects ↑',
  'shell.behavior.eyebrow': 'BEHAVIOR',
  'shell.behavior.title': 'Behavior analytics',
  'shell.behavior.sub': 'Powered by normalized turn/tool facts extracted during sync; low-sample or unsupported sources degrade explicitly.',
  'shell.behavior.activity.title': 'Activity',
  'shell.behavior.activity.sub': 'Turns, one-shot and retry signals grouped by turn category',
  'shell.behavior.tools.title': 'Tools',
  'shell.behavior.tools.sub': 'Core tools / shell / MCP / agent actions',
  'shell.behavior.optimize.title': 'Optimize',
  'shell.behavior.optimize.sub': 'Read-only waste detection; llmusage never deletes, archives or rewrites automatically.',
  'shell.behavior.compare.title': 'Compare',
  'shell.behavior.compare.sub': 'Compare model cost, one-shot, retry and working-style signals with explicit low-sample warnings.',
  'shell.cost.eyebrow': 'COST',
  'shell.cost.title': 'Cost estimate',
  'shell.cost.sub': 'Local estimate from public pricing tables. For reference only — may differ from your bill.',
  'shell.cost.panelTitle': 'Top 5 source / model combinations',
  'shell.cost.panelSub': 'Unit: USD',
  'shell.cost.expand': 'Expand all cost entries →',
  'shell.cost.collapse': 'Collapse cost entries ↑',
  'shell.failures.eyebrow': 'FAILURES',
  'shell.failures.title': 'Recent failures',
  'shell.integrations.eyebrow': 'INTEGRATIONS',
  'shell.integrations.title': 'Integrations',
  'shell.insights.eyebrow': 'INSIGHTS',
  'shell.insights.title': 'Diagnostic signals',
  'shell.insights.sub': 'Signals suggest next steps; they are not final diagnoses.',
  'shell.footer.build': 'llmusage · local build',
  'shell.footer.backToTop': 'Back to top ↑',
  'toolbar.theme.toLight': 'Light',
  'toolbar.theme.toDark': 'Dark',
  'toolbar.lang.label.zh': '中',
  'toolbar.lang.label.en': 'EN',
  'toolbar.theme.aria': 'Toggle theme',
  'toolbar.lang.aria': 'Toggle language',
  'toolbar.group.aria': 'Preferences',
  'shell.window.title': 'llmusage · Local Usage',
  'seg.all': 'All',
  'shell.date.weekdays': 'Su|Mo|Tu|We|Th|Fr|Sa',
  'shell.date.clear': 'Clear',
  'shell.date.today': 'Today',
  'shell.date.prevMonth': 'Previous month',
  'shell.date.nextMonth': 'Next month',
});

const STATUS_LABEL_ZH = Object.freeze({
  ready: '正常',
  success: '成功',
  running: '运行中',
  failed: '失败',
  warn: '警告',
  ok: '正常',
  missing: '缺失',
  drifted: '配置漂移',
  disabled: '已禁用',
  stale: '已过期',
  'missing-db': '数据库缺失',
});

const STATUS_LABEL_EN = Object.freeze({
  ready: 'Ready',
  success: 'Success',
  running: 'Running',
  failed: 'Failed',
  warn: 'Warning',
  ok: 'Healthy',
  missing: 'Missing',
  drifted: 'Drifted',
  disabled: 'Disabled',
  stale: 'Stale',
  'missing-db': 'Missing DB',
});

/*
 * ========================================================================
 * 步骤4：locale 状态 + 订阅器
 * ========================================================================
 * 目标：
 * 1) 默认按 localStorage 决定首屏语言
 * 2) setLocale 保存、切换 UI_COPY 引用、广播事件
 * 3) onLocaleChange 让 toggle 触发后所有渲染层都能 rerender
 */
let currentLocale = readStoredLocale();
const localeListeners = new Set();

function readStoredLocale() {
  try {
    const stored = window.localStorage?.getItem(LOCALE_STORAGE_KEY);
    return LOCALES.includes(stored) ? stored : DEFAULT_LOCALE;
  } catch (_err) {
    return DEFAULT_LOCALE;
  }
}

function uiCopyFor(locale) {
  return locale === 'en' ? UI_COPY_EN : UI_COPY_ZH;
}

function shellCopyFor(locale) {
  return locale === 'en' ? SHELL_COPY_EN : SHELL_COPY_ZH;
}

function statusMappingFor(locale) {
  return locale === 'en' ? STATUS_LABEL_EN : STATUS_LABEL_ZH;
}

export let UI_COPY = uiCopyFor(currentLocale);

export function getLocale() {
  return currentLocale;
}

export function setLocale(locale) {
  logger.info('开始切换 locale');

  // 4.1 标准化输入；不识别的回退默认
  const next = LOCALES.includes(locale) ? locale : DEFAULT_LOCALE;
  if (next === currentLocale) {
    logger.info('locale 未变化，跳过');
    return next;
  }

  // 4.2 更新内部状态并写存储
  currentLocale = next;
  UI_COPY = uiCopyFor(next);
  try {
    window.localStorage?.setItem(LOCALE_STORAGE_KEY, next);
  } catch (_err) {
    /* 忽略隐私模式下的写失败 */
  }

  // 4.3 通知订阅者
  for (const cb of localeListeners) {
    try {
      cb(next);
    } catch (err) {
      logger.error('locale 监听器抛错', err);
    }
  }

  logger.info('完成 locale 切换');
  return next;
}

export function onLocaleChange(callback) {
  if (typeof callback !== 'function') return () => {};
  localeListeners.add(callback);
  return () => localeListeners.delete(callback);
}

/*
 * ========================================================================
 * 步骤5：扁平 key 查表
 * ========================================================================
 */
export function getShellCopy(key) {
  const map = shellCopyFor(currentLocale);
  if (Object.prototype.hasOwnProperty.call(map, key)) {
    return map[key];
  }
  // 未配置 key 时回退中文默认，避免空字符串
  const fallback = SHELL_COPY_ZH[key];
  return fallback ?? key;
}

export function getShellCopyMap() {
  return shellCopyFor(currentLocale);
}

/*
 * ========================================================================
 * 步骤6：解析趋势窗口文案（按 locale 切换）
 * ========================================================================
 */
const TREND_WINDOW_COPY_ZH = Object.freeze({
  day: Object.freeze({
    peakFootLabel: '最高时段',
    activeFootSuffix: '个有记录时段',
    chartCaption: '最近 10 个时段',
    compareCaption: '最近时段对比',
  }),
  week: Object.freeze({
    peakFootLabel: '最高单日',
    activeFootSuffix: '个有记录日期',
    chartCaption: '最近 10 个日期',
    compareCaption: '最近日期对比',
  }),
  month: Object.freeze({
    peakFootLabel: '最高单日',
    activeFootSuffix: '个有记录日期',
    chartCaption: '最近 10 个日期',
    compareCaption: '最近日期对比',
  }),
  all: Object.freeze({
    peakFootLabel: '最高单月',
    activeFootSuffix: '个有记录月份',
    chartCaption: '最近 10 个月',
    compareCaption: '最近月份对比',
  }),
});

const TREND_WINDOW_COPY_EN = Object.freeze({
  day: Object.freeze({
    peakFootLabel: 'Peak bucket',
    activeFootSuffix: ' active buckets',
    chartCaption: 'Recent 10 buckets',
    compareCaption: 'Recent buckets compared',
  }),
  week: Object.freeze({
    peakFootLabel: 'Peak day',
    activeFootSuffix: ' active days',
    chartCaption: 'Recent 10 days',
    compareCaption: 'Recent days compared',
  }),
  month: Object.freeze({
    peakFootLabel: 'Peak day',
    activeFootSuffix: ' active days',
    chartCaption: 'Recent 10 days',
    compareCaption: 'Recent days compared',
  }),
  all: Object.freeze({
    peakFootLabel: 'Peak month',
    activeFootSuffix: ' active months',
    chartCaption: 'Recent 10 months',
    compareCaption: 'Recent months compared',
  }),
});

export function resolveTrendWindowCopy(windowName) {
  logger.info('开始解析趋势窗口文案');

  // 6.1 按当前 locale 选表，未识别窗口回退 day
  const table = currentLocale === 'en' ? TREND_WINDOW_COPY_EN : TREND_WINDOW_COPY_ZH;
  const resolved = table[windowName] || table.day;

  logger.info('完成趋势窗口文案解析');
  return resolved;
}

/*
 * ========================================================================
 * 步骤7：翻译状态文案（按 locale 切换）
 * ========================================================================
 */
export function translateStatusLabel(status) {
  logger.info('开始翻译状态文案');

  // 7.1 命中常用状态后按 locale 输出，未知状态原样回退
  const normalized = String(status || '').toLowerCase();
  const mapping = statusMappingFor(currentLocale);
  const fallbackLabel = currentLocale === 'en' ? 'Unknown' : '未知';
  const resolved = mapping[normalized] || String(status || fallbackLabel);

  logger.info('完成状态文案翻译');
  return resolved;
}
