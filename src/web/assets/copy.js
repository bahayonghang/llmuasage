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
    statusEyebrow: '同步状态',
    statusTitle: '数据状态',
    statusStable: '正常',
    statusOk: '正常',
    statusWarn: '存在失败',
    statusUnknown: '未知',
    rows: Object.freeze({
      generated_at: '生成时间',
      last_sync_at: '最近同步',
      last_export_at: '最近导出',
      sourceCount: '数据来源',
      failure_count: '最近失败',
      topModel: '用量最高模型',
    }),
    cell: Object.freeze({
      cursors: '同步游标',
      failures: '最近失败',
    }),
    metrics: Object.freeze({
      total: Object.freeze({
        label: '总用量',
        body: '累计 Token',
        footRawLabel: '累计 Token',
        footLeaderLabel: '用量最高模型',
      }),
      last24h: Object.freeze({
        label: '近 24 小时',
        body: '最近 24 小时总量',
        footRawLabel: '原始值',
        footAverageLabel: '平均每段',
        bucketUnit: '段',
      }),
      sources: Object.freeze({
        label: '来源数',
        body: '已记录来源',
        footPrimaryLabel: '主要来源',
        footLastLabel: '最近记录',
      }),
      cost: Object.freeze({
        label: '估算成本',
        body: '累计成本',
        footRawLabel: '累计成本',
        footTopLabel: '最高',
      }),
    }),
    error: Object.freeze({
      title: '数据加载失败',
      detail: '详情',
      heroMeta: '数据读取',
      heroMetaState: '失败',
      pill: '异常',
      generic: '读取本地数据失败',
    }),
  }),
  actions: Object.freeze({
    exportDone: '已导出',
  }),
  readyWidgets: Object.freeze({
    summary: Object.freeze({
      sessions: '会话数', requests: '请求数', tokens: 'Token 用量', cost: '估算成本', activeDays: '活跃天数',
      cacheEfficiency: '缓存读取占比', platforms: '个来源', perSession: '次 / 会话', topPlatform: '用量最高来源',
      currentRange: '当前筛选范围', cacheHint: '缓存读取 Token 占输入侧 Token 的比例', empty: '当前筛选范围暂无汇总数据。',
      loading: '正在加载汇总数据…',
    }),
    heatmap: Object.freeze({
      title: '每日活跃度', sub: '按本地日期汇总当前筛选范围内的用量。', tokens: 'Token 用量', events: '请求数',
      less: '少', more: '多', empty: '当前筛选范围暂无每日用量。', loading: '正在加载每日活跃度…',
      recentYear: '全部范围仅展示最近一年', eventCount: '请求', tokenCount: 'Token',
      metricAria: '每日活跃度指标', weekdays: Object.freeze(['', '周一', '', '周三', '', '周五', '']),
      weekdaysFull: Object.freeze(['周日', '周一', '周二', '周三', '周四', '周五', '周六']),
    }),
    trendsDaily: Object.freeze({
      title: '每日 Token 用量构成', sub: '按日查看输入、缓存读取、缓存写入与输出 Token。',
      input: '输入 Token', cacheRead: '缓存读取 Token', cacheCreation: '缓存写入 Token', output: '输出 Token', cost: '估算成本',
      empty: '当前筛选范围暂无每日用量趋势。', loading: '正在加载每日用量趋势…',
      oneDay: '近 24 小时请查看短时趋势；每日趋势从近 7 天开始显示。',
    }),
  }),
  sessionAnalytics: Object.freeze({
    topSessions: Object.freeze({ title: '高用量会话', sub: '按当前排序指标显示筛选范围内的会话。', loading: '正在加载会话排行…', empty: '当前筛选范围暂无会话。', untitled: '未命名会话', noProject: '无项目', sortAria: '会话排序指标', sort: Object.freeze({ tokens: 'Token 用量', duration: '活跃时长', cost: '估算成本' }) }),
    hourOfWeek: Object.freeze({ title: '每周活跃时段', sub: '按浏览器时区汇总每周各小时的 Token 用量。', loading: '正在加载每周活跃时段…', empty: '当前筛选范围暂无分时用量。', tokens: 'Token', events: '请求', weekdays: Object.freeze(['周一','周二','周三','周四','周五','周六','周日']) }),
    logs: Object.freeze({ liveOnly: '事件日志仅在实时看板中可用。', session: '会话', clear: '清除', time: '时间', source: '来源', model: '模型', tokens: 'Token 用量', cost: '估算成本', project: '项目', loading: '正在加载事件日志…', empty: '当前筛选暂无事件。', more: '加载更多', rawLoading: '正在读取原始记录…', rawUnavailable: '未保留原始记录。' }),
  }),
  behavior: Object.freeze({
    support: Object.freeze({ normalized: '数据完整', no_data: '暂无数据', degraded: '部分数据', low_sample: '样本较少', insufficient_models: '模型不足', missing_model: '模型无数据', refreshing: '刷新中' }),
    reasons: Object.freeze({ noFacts: '当前筛选范围没有可用的行为明细，请先同步支持行为解析的数据来源。', insufficientModels: '至少需要两个有本地用量的模型才能进行对比。', lowSample: '当前样本较少；请在每个模型积累更多请求和编辑轮次后再作结论。', missingModel: '所选模型之一在当前筛选范围内没有数据。' }),
    activity: Object.freeze({ empty: '当前筛选范围暂无活动类型数据。', category: '类型', turns: '轮次', editTurns: '编辑轮次', oneShot: '一次完成率', cost: '估算成本', turnsUnit: '轮次', categories: Object.freeze({ coding: '编码', debugging: '调试', feature: '功能设计', refactoring: '重构', testing: '测试', exploration: '探索', planning: '规划', delegation: '委派', git: '版本控制', build_deploy: '构建与部署', conversation: '对话', brainstorming: '头脑风暴', general: '其他' }) }),
    tools: Object.freeze({ empty: '当前筛选范围暂无工具调用数据。', tool: '工具', type: '类型', calls: '调用次数', share: '占比', cost: '估算成本', callsUnit: '次调用', kinds: Object.freeze({ core: '内置工具', mcp: 'MCP 工具', bash: '命令行', skill: '技能', agent: '子代理', planning: '规划', read: '读取', edit: '编辑', search: '搜索', other: '其他', '(non-tool)': '非工具调用' }) }),
    optimize: Object.freeze({ score: '优化评分', potential: '可优化空间', estimated: '估算', mode: '模式', readOnly: '只读分析', empty: '当前没有可用的优化建议；建议仅基于已解析的行为明细生成，llmusage 不会自动清理或改写数据。', severity: Object.freeze({ high: '高', medium: '中', low: '低' }), findings: Object.freeze({ low_read_edit_ratio: Object.freeze({ title: '读取与编辑调用比例偏低', recommendation: '较大范围编辑前先检查相关文件；这只是只读信号，不会自动改写。', evidence: '当前读取/搜索调用相对编辑调用偏少。' }), duplicate_reads: Object.freeze({ title: '同一会话重复读取', recommendation: '可将关键事实记录到笔记，或缩小检查范围，避免重复读取同一目标。', evidence: '同一会话多次读取或搜索了相同目标。' }), junk_reads: Object.freeze({ title: '读取了生成文件或依赖目录', recommendation: '手动排查时优先查看源代码目录，并忽略生成文件和依赖目录。', evidence: '读取或搜索调用涉及疑似生成文件或依赖路径。' }), session_outlier: Object.freeze({ title: '单个会话占用量过高', recommendation: '全局优化前先检查该会话；长上下文或重复重试可能只出现在该会话中。', evidence: '单个会话占当前筛选范围内的大部分轮次 Token。' }) }) }),
    compare: Object.freeze({ empty: '至少需要两个模型才能显示对比结果；样本较少时会明确提示。', metric: '指标', metrics: Object.freeze({ one_shot_rate: '一次完成率', retry_rate: '重试率', cost_per_call: '单次请求成本', cost_per_edit_turn: '单次编辑成本', cache_efficiency: '缓存读取占比', delegation_rate: '委派率', planning_rate: '规划率', tools_per_turn: '每轮工具调用数' }) }),
  }),
  explorer: Object.freeze({
    support: Object.freeze({ normalized: '数据完整', no_data: '暂无数据', degraded: '部分数据', unsupported: '不支持', refreshing: '刷新中' }),
    summary: Object.freeze({ metric: '指标', groupBy: '分组维度', rows: '结果数', seriesPoints: '个序列数据点' }),
    table: Object.freeze({ dimension: '维度', key: '标识', share: '占比' }),
    values: Object.freeze({ tool: '工具调用', non_tool: '非工具调用', toolKinds: Object.freeze({ read: '读取', edit: '编辑', shell: '命令行', bash: '命令行', mcp: 'MCP 工具', agent: '子代理', '(non-tool)': '非工具调用' }), tokenTypes: Object.freeze({ input: '输入 Token', cache_read: '缓存读取 Token', cache_creation: '缓存写入 Token', output: '输出 Token', reasoning_output: '推理输出 Token' }) }),
    empty: '当前筛选范围暂无分析结果。',
    reasons: Object.freeze({ noUsage: '当前筛选范围没有用量事件。', noFacts: '当前来源有用量数据，但缺少可用于此分析的行为明细。', omittedFacts: '部分来源缺少行为明细，已从结果中排除。', tokenFilter: 'Token 类型筛选仅支持“Token 用量”指标。', tokenGroup: '按 Token 类型分组仅支持“Token 用量”指标。' }),
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
        cursors: '同步游标',
        failures: '最近失败',
      }),
      failuresTitle: '最近失败',
      failuresEmpty: '当前没有失败记录。',
    }),
    syncCenter: Object.freeze({
      eyebrow: '同步',
      generatedAt: '生成时间',
      workerLock: '同步执行锁',
      riskPrefix: '重建风险来源：',
      noRisk: '普通同步不会删除已导入历史。',
      riskFacts: '重建保护事实',
      sourcesEmpty: '暂无同步状态数据；等待下一次同步或刷新。',
      sourceShareAria: '同步来源占比',
      actions: Object.freeze({
        sync: '立即同步',
      }),
      details: '同步详情',
      detailsHint: '查看来源与最近运行',
      metrics: Object.freeze({
        eventsSeen: '扫描记录',
        insertedDelta: '新增记录',
        storedEvents: '已存记录',
        sourcesReady: '就绪来源',
      }),
      parseIssues: Object.freeze({
        malformed: '畸形行',
        oversized: '超大行',
        skipped: '跳过行',
        accounting: '记账异常',
      }),
      workerLockState: Object.freeze({
        available: '可用',
        busy: '占用中',
        unknown: '未知',
      }),
      sourceStatus: Object.freeze({
        ok: '正常',
        error: '错误',
        rebuild_risk: '重建风险',
        ready: '就绪',
        success: '成功',
        running: '运行中',
        failed: '失败',
        idle: '待同步',
        missing: '缺失',
        stale: '已过期',
      }),
      statusLabels: Object.freeze({
        currentStatus: '当前状态',
        jobId: '任务 ID',
        lastEvent: '最近事件',
        started: '开始时间',
        finished: '完成时间',
        error: '错误',
        lastCommand: '最近命令',
        lastStatus: '最近状态',
        lastFinished: '完成时间',
        lastError: '最近错误',
      }),
    }),
    insights: Object.freeze({
      kicker: '洞察',
      title: '诊断线索',
      copy: '这些信号用于定位下一步排查入口，不是最终诊断或账单结论。',
      disclaimer: '数据解读提示：以下是基于本地数据的信号和建议排查方向。',
      emptyTitle: '暂无需要关注的信号',
      emptyBody: '当前窗口未发现失败、定价缺口或源文件保留风险。',
      defaultLabel: '信号',
      defaultAction: '结合具体会话和同步日志继续确认。',
      items: Object.freeze({
        cache_low: Object.freeze({ label: '缓存线索', title: '缓存读取占比偏低', evidence: '当前筛选范围的缓存读取占比为 {percentage}%。', action: '可检查提示复用、长上下文缓存或模型缓存支持；这是线索，不是最终诊断。' }),
        pricing_gap: Object.freeze({ label: '定价可靠性', title: '存在定价不完整的成本项', evidence: '{count} 个模型汇总项定价不完整，例如 {model}（{status}）。', action: '成本估算可用于趋势判断；对账前请刷新定价快照或检查未匹配的模型。' }),
        sync_failure: Object.freeze({ label: '同步失败', title: '最近有同步失败', evidence: '共有 {count} 条失败记录，最近命令为 {command}。', action: '打开最近失败详情或重新运行同步；完成后看板会刷新当前筛选范围。' }),
        lossy_rebuild: Object.freeze({ label: '数据保留', title: '存在重建保护事实', evidence: '{source} 当前缺失 {missingCount} 个源文件，本地库保留 {protectedCount} 条事件。', action: '这是 sync --rebuild 的前置保护条件；普通同步仍保留已导入历史。' }),
        missing_source: Object.freeze({ label: '源文件状态', title: '存在源文件缺失记录', evidence: '{source} 当前记录 {missingCount} 个缺失文件。', action: '这通常只影响归档诊断；普通同步会保留已导入的用量历史。' }),
        stale_source: Object.freeze({ label: '来源时效', title: '部分来源近期没有新事件', evidence: '{source} 的最近事件时间为 {lastEvent}。', action: '如果仍在使用该来源，请确认本地产物存在，并重新运行同步。' }),
        top_cost: Object.freeze({ label: '成本主因', title: '当前筛选范围的主要成本来源', evidence: '{source} · {model} 约为 {cost}。', action: '可优先从这个来源与模型组合排查成本变化。' }),
        top_model: Object.freeze({ label: '用量主因', title: '当前筛选范围的主要模型', evidence: '{model} 使用了 {tokens} Token。', action: '没有可用成本时，可先按 Token 用量定位主要消耗。' }),
        top_project: Object.freeze({ label: '项目聚焦', title: '当前筛选范围的主要项目', evidence: '{project} 使用了 {tokens} Token。', action: '若要降低用量，可先检查该项目的会话模式和模型选择。' }),
      }),
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
    statusEyebrow: 'Sync status',
    statusTitle: 'Data status',
    statusStable: 'Healthy',
    statusOk: 'Healthy',
    statusWarn: 'Failures found',
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
      cursors: 'Sync cursors',
      failures: 'Recent failures',
    }),
    metrics: Object.freeze({
      total: Object.freeze({
        label: 'Total',
        body: 'Cumulative tokens',
        footRawLabel: 'Cumulative tokens',
        footLeaderLabel: 'Top model',
      }),
      last24h: Object.freeze({
        label: 'Last 24h',
        body: 'Tokens in last 24 hours',
        footRawLabel: 'Raw value',
        footAverageLabel: 'Avg per bucket',
        bucketUnit: 'buckets',
      }),
      sources: Object.freeze({
        label: 'Sources',
        body: 'Recorded sources',
        footPrimaryLabel: 'Top source',
        footLastLabel: 'Last seen',
      }),
      cost: Object.freeze({
        label: 'Est. cost',
        body: 'Cumulative cost',
        footRawLabel: 'Cumulative cost',
        footTopLabel: 'Top',
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
  readyWidgets: Object.freeze({
    summary: Object.freeze({
      sessions: 'Sessions', requests: 'Requests', tokens: 'Token usage', cost: 'Estimated cost', activeDays: 'Active days',
      cacheEfficiency: 'Cache-read share', platforms: 'sources', perSession: 'per session', topPlatform: 'Top source',
      currentRange: 'Current filter range', cacheHint: 'Share of input-side tokens served from cache', empty: 'No summary data in this filter range.',
      loading: 'Loading summary data…',
    }),
    heatmap: Object.freeze({
      title: 'Daily activity', sub: 'Usage grouped by local calendar date for the current filter range.', tokens: 'Token usage', events: 'Requests',
      less: 'Less', more: 'More', empty: 'No daily usage in this filter range.', loading: 'Loading daily activity…',
      recentYear: 'All-time range shows only the most recent year', eventCount: 'requests', tokenCount: 'tokens',
      metricAria: 'Daily activity metric', weekdays: Object.freeze(['', 'Mon', '', 'Wed', '', 'Fri', '']),
      weekdaysFull: Object.freeze(['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']),
    }),
    trendsDaily: Object.freeze({
      title: 'Daily token usage mix', sub: 'Daily input, cache read, cache creation, and output tokens.',
      input: 'Input tokens', cacheRead: 'Cache-read tokens', cacheCreation: 'Cache-creation tokens', output: 'Output tokens', cost: 'Estimated cost',
      empty: 'No daily usage trend in this filter range.', loading: 'Loading daily usage trends…',
      oneDay: 'Use the short-window trend for the last 24 hours; daily trends start at 7 days.',
    }),
  }),
  sessionAnalytics: Object.freeze({
    topSessions: Object.freeze({ title: 'Highest-usage sessions', sub: 'Sessions in the current filter range ranked by the selected metric.', loading: 'Loading session ranking…', empty: 'No sessions in this filter range.', untitled: 'Untitled session', noProject: 'No project', sortAria: 'Session ranking metric', sort: Object.freeze({ tokens: 'Token usage', duration: 'Active duration', cost: 'Estimated cost' }) }),
    hourOfWeek: Object.freeze({ title: 'Weekly activity', sub: 'Token usage by weekday and hour in the browser timezone.', loading: 'Loading weekly activity…', empty: 'No hourly usage in this filter range.', tokens: 'tokens', events: 'requests', weekdays: Object.freeze(['Mon','Tue','Wed','Thu','Fri','Sat','Sun']) }),
    logs: Object.freeze({ liveOnly: 'Event logs are available only in the live dashboard.', session: 'Session', clear: 'Clear', time: 'Time', source: 'Source', model: 'Model', tokens: 'Token usage', cost: 'Estimated cost', project: 'Project', loading: 'Loading events…', empty: 'No events match these filters.', more: 'Load more', rawLoading: 'Loading raw record…', rawUnavailable: 'Raw record was not retained.' }),
  }),
  behavior: Object.freeze({
    support: Object.freeze({ normalized: 'Complete data', no_data: 'No data', degraded: 'Partial data', low_sample: 'Small sample', insufficient_models: 'Not enough models', missing_model: 'Model has no data', refreshing: 'Refreshing' }),
    reasons: Object.freeze({ noFacts: 'No behavior details match these filters. Sync a source with behavior parsing support.', insufficientModels: 'At least two models with local usage are required for comparison.', lowSample: 'The sample is small; wait for more requests and edit turns per model before drawing conclusions.', missingModel: 'One selected model has no data in this filter range.' }),
    activity: Object.freeze({ empty: 'No activity-category data in this filter range.', category: 'Category', turns: 'Turns', editTurns: 'Edit turns', oneShot: 'One-shot rate', cost: 'Estimated cost', turnsUnit: 'turns', categories: Object.freeze({ coding: 'Coding', debugging: 'Debugging', feature: 'Feature design', refactoring: 'Refactoring', testing: 'Testing', exploration: 'Exploration', planning: 'Planning', delegation: 'Delegation', git: 'Version control', build_deploy: 'Build and deploy', conversation: 'Conversation', brainstorming: 'Brainstorming', general: 'Other' }) }),
    tools: Object.freeze({ empty: 'No tool-call data in this filter range.', tool: 'Tool', type: 'Type', calls: 'Calls', share: 'Share', cost: 'Estimated cost', callsUnit: 'calls', kinds: Object.freeze({ core: 'Built-in tool', mcp: 'MCP tool', bash: 'Shell', skill: 'Skill', agent: 'Sub-agent', planning: 'Planning', read: 'Read', edit: 'Edit', search: 'Search', other: 'Other', '(non-tool)': 'Non-tool call' }) }),
    optimize: Object.freeze({ score: 'Optimization score', potential: 'Potential savings', estimated: 'estimated', mode: 'Mode', readOnly: 'Read-only', empty: 'No optimization findings are available. Recommendations use parsed behavior details only, and llmusage never cleans up or rewrites data automatically.', severity: Object.freeze({ high: 'High', medium: 'Medium', low: 'Low' }), findings: Object.freeze({ low_read_edit_ratio: Object.freeze({ title: 'Low read-to-edit ratio', recommendation: 'Review the relevant files before larger edit runs. This is a read-only signal and does not rewrite anything automatically.', evidence: 'Read and search calls are low relative to edit calls.' }), duplicate_reads: Object.freeze({ title: 'Repeated reads in one session', recommendation: 'Record key facts in notes or narrow the inspection range to avoid rereading the same target.', evidence: 'One session read or searched the same target repeatedly.' }), junk_reads: Object.freeze({ title: 'Generated or dependency files were read', recommendation: 'Prioritize source directories and ignore generated or dependency folders during manual investigation.', evidence: 'Read or search calls touched likely generated or dependency paths.' }), session_outlier: Object.freeze({ title: 'One session dominates usage', recommendation: 'Inspect this session before optimizing globally; long context or repeated retries may be local to it.', evidence: 'One session accounts for most turn tokens in this filter range.' }) }) }),
    compare: Object.freeze({ empty: 'At least two models are required for comparison. Small samples are called out explicitly.', metric: 'Metric', metrics: Object.freeze({ one_shot_rate: 'One-shot rate', retry_rate: 'Retry rate', cost_per_call: 'Cost per request', cost_per_edit_turn: 'Cost per edit turn', cache_efficiency: 'Cache-read share', delegation_rate: 'Delegation rate', planning_rate: 'Planning rate', tools_per_turn: 'Tool calls per turn' }) }),
  }),
  explorer: Object.freeze({
    support: Object.freeze({ normalized: 'Complete data', no_data: 'No data', degraded: 'Partial data', unsupported: 'Unsupported', refreshing: 'Refreshing' }),
    summary: Object.freeze({ metric: 'Metric', groupBy: 'Group by', rows: 'Results', seriesPoints: 'series points' }),
    table: Object.freeze({ dimension: 'Dimension', key: 'Key', share: 'Share' }),
    values: Object.freeze({ tool: 'Tool call', non_tool: 'Non-tool call', toolKinds: Object.freeze({ read: 'Read', edit: 'Edit', shell: 'Shell', bash: 'Shell', mcp: 'MCP tool', agent: 'Sub-agent', '(non-tool)': 'Non-tool call' }), tokenTypes: Object.freeze({ input: 'Input tokens', cache_read: 'Cache-read tokens', cache_creation: 'Cache-creation tokens', output: 'Output tokens', reasoning_output: 'Reasoning-output tokens' }) }),
    empty: 'No analysis results in this filter range.',
    reasons: Object.freeze({ noUsage: 'No usage events match these filters.', noFacts: 'The current sources have usage data but no behavior details for this analysis.', omittedFacts: 'Sources without behavior details were omitted from the results.', tokenFilter: 'Token-type filters only support the Token usage metric.', tokenGroup: 'Grouping by token type only supports the Token usage metric.' }),
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
        cursors: 'Sync cursors',
        failures: 'Recent failures',
      }),
      failuresTitle: 'Recent failures',
      failuresEmpty: 'No failures recorded.',
    }),
    syncCenter: Object.freeze({
      eyebrow: 'Sync',
      generatedAt: 'Generated',
      workerLock: 'Worker lock',
      riskPrefix: 'Rebuild-risk sources:',
      noRisk: 'Ordinary sync keeps imported history intact.',
      riskFacts: 'Rebuild protection facts',
      sourcesEmpty: 'No sync status data yet; sync or refresh to populate it.',
      sourceShareAria: 'Sync source share',
      actions: Object.freeze({
        sync: 'Sync now',
      }),
      details: 'Sync details',
      detailsHint: 'View sources and latest run',
      metrics: Object.freeze({
        eventsSeen: 'Events seen',
        insertedDelta: 'Inserted delta',
        storedEvents: 'Stored events',
        sourcesReady: 'Ready sources',
      }),
      parseIssues: Object.freeze({
        malformed: 'Malformed',
        oversized: 'Oversized',
        skipped: 'Skipped',
        accounting: 'Accounting',
      }),
      workerLockState: Object.freeze({
        available: 'Available',
        busy: 'Busy',
        unknown: 'Unknown',
      }),
      sourceStatus: Object.freeze({
        ok: 'OK',
        error: 'Error',
        rebuild_risk: 'Rebuild risk',
        ready: 'Ready',
        success: 'Success',
        running: 'Running',
        failed: 'Failed',
        idle: 'Idle',
        missing: 'Missing',
        stale: 'Stale',
      }),
      statusLabels: Object.freeze({
        currentStatus: 'Current status',
        jobId: 'Job ID',
        lastEvent: 'Last event',
        started: 'Started',
        finished: 'Finished',
        error: 'Error',
        lastCommand: 'Last command',
        lastStatus: 'Last status',
        lastFinished: 'Last finished',
        lastError: 'Last error',
      }),
    }),
    insights: Object.freeze({
      kicker: 'Insights',
      title: 'Diagnostic signals',
      copy: 'Signals point to the next investigation step; they are not final diagnoses or billing truth.',
      disclaimer: 'Data interpretation: local signals and suggested investigation paths.',
      emptyTitle: 'No attention signals',
      emptyBody: 'This window has no failures, pricing gaps, or source retention risks.',
      defaultLabel: 'Signal',
      defaultAction: 'Confirm with session details and sync logs.',
      items: Object.freeze({
        cache_low: Object.freeze({ label: 'Cache signal', title: 'Cache-read share is low', evidence: 'Cache-read share is {percentage}% in the current filter range.', action: 'Check prompt reuse, long-context caching, or model cache support. This is a signal, not a final diagnosis.' }),
        pricing_gap: Object.freeze({ label: 'Pricing reliability', title: 'Some cost entries have incomplete pricing', evidence: '{count} model summaries have incomplete pricing, such as {model} ({status}).', action: 'Use cost estimates for trends; refresh the pricing snapshot or inspect unmatched models before reconciliation.' }),
        sync_failure: Object.freeze({ label: 'Sync failure', title: 'A recent sync failed', evidence: 'Failure records: {count}; latest command: {command}.', action: 'Open the latest failure details or run sync again. The dashboard refreshes this filter range afterward.' }),
        lossy_rebuild: Object.freeze({ label: 'Data retention', title: 'Rebuild protection facts', evidence: '{source} currently lacks {missingCount} source files; the local store retains {protectedCount} events.', action: 'This is a precondition for sync --rebuild; ordinary sync keeps imported history.' }),
        missing_source: Object.freeze({ label: 'Source files', title: 'Some source files are missing', evidence: '{source} currently records {missingCount} missing files.', action: 'This normally affects archive diagnostics only; ordinary sync keeps imported usage history.' }),
        stale_source: Object.freeze({ label: 'Source freshness', title: 'Some sources have no recent events', evidence: 'The latest event for {source} was {lastEvent}.', action: 'If the source is still in use, confirm its local artifacts exist and run sync again.' }),
        top_cost: Object.freeze({ label: 'Cost driver', title: 'Primary cost source in this filter range', evidence: '{source} · {model} is approximately {cost}.', action: 'Start with this source and model combination when investigating cost changes.' }),
        top_model: Object.freeze({ label: 'Usage driver', title: 'Primary model in this filter range', evidence: '{model} used {tokens} tokens.', action: 'When cost data is unavailable, use token usage to locate the main consumption.' }),
        top_project: Object.freeze({ label: 'Project focus', title: 'Primary project in this filter range', evidence: '{project} used {tokens} tokens.', action: 'To reduce usage, start with this project’s session patterns and model choices.' }),
      }),
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
  'shell.crumb.dashboard': '看板',
  'shell.crumb.local': '本地用量概览',
  'shell.tag.local': '仅本地',
  'shell.tag.snapshot': '离线文件',
  'shell.btn.export': '导出 CSV',
  'shell.btn.sync': '同步',
  'shell.sync.idle': '待同步',
  'shell.sync.running': '同步中',
  'shell.sync.cancelling': '正在取消',
  'shell.sync.completed': '同步完成',
  'shell.sync.cancelled': '已取消',
  'shell.sync.failed': '同步失败',
  'shell.sync.cancel': '取消同步',
  'shell.sync.snapshotDisabled': '离线快照不可启动同步',
  'shell.syncCenter.eyebrow': '同步',
  'shell.syncCenter.loading': '正在读取同步状态…',
  'shell.load.core': '正在加载核心看板',
  'shell.load.coreDetail': '正在读取总览、趋势、模型、来源、项目和成本。',
  'shell.load.slow': '核心数据耗时较长',
  'shell.load.slowDetail': '查询仍在进行；若本地服务中断，本页会自动停止等待。',
  'shell.load.secondary': '正在加载分析区',
  'shell.load.secondaryDetail': '已完成 {settled}/{total} 个分析区。',
  'shell.load.degraded': '其中 {count} 个分析区已降级，但核心看板仍可使用。',
  'shell.load.timeout': '核心看板加载超时',
  'shell.load.network': '无法连接本地服务',
  'shell.load.http': '本地服务返回错误',
  'shell.load.parse': '看板响应无法解析',
  'shell.load.errorDetail': '请重试看板数据；此操作不会启动同步。',
  'shell.load.retry': '重试看板',
  'shell.load.segment': '分析区 {index}，{state}',
  'shell.load.segmentPending': '等待中',
  'shell.load.segmentReady': '已完成',
  'shell.load.segmentDegraded': '已降级',
  'syncCenter.headline.empty': '等待同步状态',
  'syncCenter.headline.ready': '同步状态就绪',
  'syncCenter.headline.running': '同步正在运行',
  'syncCenter.headline.busy': '同步执行器忙碌',
  'syncCenter.headline.failed': '最近同步存在失败',
  'syncCenter.headline.cancelled': '同步已取消',
  'syncCenter.headline.rebuildRisk': '检测到重建风险',
  'syncCenter.reason.empty': '当前快照没有可用的同步命令中心数据。',
  'syncCenter.reason.ready': '可按需触发普通同步；危险重建路径仍由后端保护。',
  'syncCenter.reason.running': '正在使用结构化进度刷新当前同步状态。',
  'syncCenter.reason.cancelling': '已请求取消；后台任务正在释放同步执行锁并收尾。',
  'syncCenter.reason.failedJob': '最近一次前台同步任务失败；请查看同步日志后重试。',
  'syncCenter.reason.jobFailed': '同步任务失败；详细信息保留在本地日志中。',
  'syncCenter.reason.lastRunFailed': '最近一次同步失败；详细信息保留在本地日志中。',
  'syncCenter.reason.sourceError': '该来源最近同步出现错误；详细信息保留在本地日志中。',
  'syncCenter.reason.cancelled': '同步已取消，已导入数据保持不变。',
  'syncCenter.reason.rebuildRisk': '普通同步安全；重建前需要先处理风险来源。',
  'syncCenter.action.sync': '立即同步',
  'syncCenter.action.busy': '已有同步任务持有同步执行锁。',
  'shell.refresh.label': '刷新',
  'shell.refresh.off': '关闭',
  'shell.refresh.aria': '自动刷新间隔',
  'shell.refresh.failed': '刷新失败',
  'shell.refresh.secondaryStale': '正在刷新当前时间范围；这里暂时显示上一轮结果。',
  'shell.refresh.snapshotDisabled': '离线快照不可自动刷新',
  'shell.brand.sub': '本地',
  'shell.nav.label.overview': '概览',
  'shell.nav.label.distribution': '分布',
  'shell.nav.label.ops': '运行',
  'shell.nav.item.usage': '用量概览',
  'shell.nav.item.trend': '用量趋势',
  'shell.nav.item.models': '模型分布',
  'shell.nav.item.sources': '来源分布',
  'shell.nav.item.projects': '项目排行',
  'shell.nav.item.behavior': '行为分析',
  'shell.nav.item.explorer': '用量分析',
  'shell.nav.item.cost': '成本估算',
  'shell.nav.item.status': '运行状态',
  'shell.nav.item.logs': '事件日志',
  'shell.logs.title': '事件日志',
  'shell.logs.sub': '分页查看本地用量事件；展开后可按需读取原始记录。',
  'shell.endpoint.lastSync': '最近同步',
  'shell.filters.source': '来源',
  'shell.filters.aria': '看板筛选条件',
  'shell.filters.allSources': '全部来源',
  'shell.filters.model': '模型',
  'shell.filters.modelPlaceholder': '全部模型',
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
  'shell.hero.eyebrow': '概览',
  'shell.hero.title.html': '本地用量<span class="accent">概览</span>',
  'shell.hero.desc':
    '本地查看近期用量、成本估算和运行状态。所有数据存放在本机 SQLite 中，不依赖任何外部接口、不上报任何遥测，可放心断网使用。',
  'shell.trends.eyebrow': '趋势',
  'shell.trends.title': '用量趋势',
  'shell.trends.sub': '主图展示当前窗口内最近 10 条记录，完整明细可展开查看。',
  'shell.trends.legend.tokens': '用量 (Token)',
  'shell.trends.chart.recent10': '最近 10 个时段',
  'shell.trends.windowAria': '趋势时间窗口',
  'shell.models.eyebrow': '模型',
  'shell.models.title': '模型用量分布',
  'shell.models.sub': '先看用量最高的模型，再按需展开完整排行。',
  'shell.models.panelTitle': '用量最高的 8 个模型',
  'shell.models.panelSub': '单位：Token，按累计计算',
  'shell.models.expand': '展开完整排行 →',
  'shell.models.collapse': '收起完整排行 ↑',
  'shell.sources.eyebrow': '来源',
  'shell.sources.title': '来源分布',
  'shell.sources.sub': '用量最高的 4 个来源',
  'shell.projects.eyebrow': '项目',
  'shell.projects.title': '项目排行',
  'shell.projects.sub': '按累计 Token 排序',
  'shell.projects.expand': '展开全部项目 →',
  'shell.projects.collapse': '收起全部项目 ↑',
  'shell.behavior.eyebrow': '行为',
  'shell.behavior.title': '行为分析',
  'shell.behavior.sub': '基于同步阶段提取的标准化交互与工具调用数据；样本不足或来源不支持时会明确提示。',
  'shell.behavior.activity.title': '活动类型',
  'shell.behavior.activity.sub': '按活动类型汇总轮次、编辑轮次和一次完成率。',
  'shell.behavior.tools.title': '工具使用',
  'shell.behavior.tools.sub': '汇总内置工具、命令行、MCP 与子代理操作。',
  'shell.behavior.optimize.title': '优化建议',
  'shell.behavior.optimize.sub': '只读分析潜在浪费；不会自动删除、归档或改写数据。',
  'shell.behavior.compare.title': '模型对比',
  'shell.behavior.compare.sub': '按模型对比成本、单轮完成率、重试率与使用模式；样本不足时会明确提示。',
  'shell.explorer.eyebrow': '分析',
  'shell.explorer.title': '用量分析',
  'shell.explorer.sub': '按时间粒度、指标、维度与工具筛选本地用量；结果由后端聚合，不在浏览器中处理原始记录。',
  'shell.explorer.metric': '指标',
  'shell.explorer.metric.cost': '归因成本',
  'shell.explorer.metric.calls': '调用数',
  'shell.explorer.metric.turns': '轮次',
  'shell.explorer.metric.sessions': '会话数',
  'shell.explorer.metric.tokens': '总 Token',
  'shell.explorer.groupBy': '分组维度',
  'shell.explorer.group.source': '来源',
  'shell.explorer.group.model': '模型',
  'shell.explorer.group.project': '项目',
  'shell.explorer.group.session': '会话',
  'shell.explorer.group.tool': '工具',
  'shell.explorer.group.toolKind': '工具类型',
  'shell.explorer.group.isTool': '工具/非工具',
  'shell.explorer.group.tokenType': 'Token 类型',
  'shell.explorer.granularity': '时间粒度',
  'shell.explorer.granularity.total': '总计',
  'shell.explorer.granularity.day': '按日',
  'shell.explorer.granularity.week': '按周',
  'shell.explorer.granularity.month': '按月',
  'shell.explorer.limit': '最多显示',
  'shell.explorer.session': '会话过滤',
  'shell.explorer.sessionPlaceholder': '会话 ID',
  'shell.explorer.tool': '工具过滤',
  'shell.explorer.toolPlaceholder': 'Read / Bash / Edit',
  'shell.explorer.toolKind': '工具类型',
  'shell.explorer.toolKind.read': '读取',
  'shell.explorer.toolKind.edit': '编辑',
  'shell.explorer.toolKind.shell': '命令行',
  'shell.explorer.toolKind.mcp': 'MCP 工具',
  'shell.explorer.toolKind.agent': '子代理',
  'shell.explorer.toolKind.nonTool': '非工具调用',
  'shell.explorer.tokenType': 'Token 类型',
  'shell.explorer.tokenType.input': '输入 Token',
  'shell.explorer.tokenType.cacheRead': '缓存读取 Token',
  'shell.explorer.tokenType.cacheCreation': '缓存写入 Token',
  'shell.explorer.tokenType.output': '输出 Token',
  'shell.explorer.tokenType.reasoningOutput': '推理输出 Token',
  'shell.explorer.all': '全部',
  'shell.explorer.includeOther': '合并其他项',
  'shell.explorer.includeNonTool': '包含非工具',
  'shell.explorer.apply': '运行分析',
  'shell.explorer.reset': '重置',
  'shell.explorer.snapshotDisabled': '离线快照使用导出时保存的分析查询',
  'shell.explorer.rowsTitle': '维度排行',
  'shell.explorer.rowsSub': '按当前指标排序，超出显示上限的结果可合并为其他项。',
  'shell.explorer.seriesTitle': '时间序列',
  'shell.explorer.seriesSub': '前 5 个维度使用独立刻度展示，完整区间可用于离线查看。',
  'shell.explorer.seriesScope': '显示总量前 {shown} 个维度；其余维度见明细。',
  'shell.explorer.seriesScopeAll': '显示全部 {shown} 个维度。',
  'shell.explorer.seriesIndependentScale': '各维度独立刻度',
  'shell.explorer.seriesPeak': '峰值',
  'shell.explorer.seriesAria': '{label} 时间趋势，{range}，峰值 {value}',
  'shell.explorer.seriesDetails': '时间序列明细',
  'shell.explorer.seriesDetailsMeta': '{count} 个数据点 · {range}',
  'shell.explorer.seriesTruncated': '已显示最近 {shown} / 共 {total} 条；趋势图仍覆盖完整区间。',
  'shell.explorer.seriesTotalEmpty': '当前粒度为总计，不返回时间序列。',
  'shell.explorer.seriesEmpty': '暂无分析时间序列。',
  'shell.explorer.table.time': '时间',
  'shell.explorer.table.dimension': '维度',
  'shell.cost.eyebrow': '成本',
  'shell.cost.title': '成本估算',
  'shell.cost.sub': '基于公开计价表的本地估算。仅供参考，与账单存在差异。',
  'shell.cost.panelTitle': '成本最高的 5 个来源 / 模型组合',
  'shell.cost.panelSub': '单位：USD',
  'shell.cost.expand': '展开全部成本项 →',
  'shell.cost.collapse': '收起全部成本项 ↑',
  'shell.failures.eyebrow': '失败',
  'shell.failures.title': '最近失败',
  'shell.insights.eyebrow': '洞察',
  'shell.insights.title': '诊断线索',
  'shell.insights.sub': '信号只表示可能的下一步，不代表最终诊断。',
  'shell.footer.build': 'llmusage · 本地构建',
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
  'shell.btn.export': 'Export CSV',
  'shell.btn.sync': 'Sync',
  'shell.sync.idle': 'Idle',
  'shell.sync.running': 'Syncing',
  'shell.sync.cancelling': 'Cancelling',
  'shell.sync.completed': 'Sync complete',
  'shell.sync.cancelled': 'Cancelled',
  'shell.sync.failed': 'Sync failed',
  'shell.sync.cancel': 'Cancel sync',
  'shell.sync.snapshotDisabled': 'Offline snapshots cannot start sync jobs',
  'shell.syncCenter.eyebrow': 'SYNC',
  'shell.syncCenter.loading': 'Reading sync status…',
  'shell.load.core': 'Loading the core dashboard',
  'shell.load.coreDetail': 'Reading overview, trends, models, sources, projects, and costs.',
  'shell.load.slow': 'Core data is taking longer',
  'shell.load.slowDetail': 'The query is still running. This page will stop waiting if the local service disconnects.',
  'shell.load.secondary': 'Loading analysis sections',
  'shell.load.secondaryDetail': '{settled} of {total} analysis sections settled.',
  'shell.load.degraded': '{count} analysis sections degraded; the core dashboard remains available.',
  'shell.load.timeout': 'Core dashboard timed out',
  'shell.load.network': 'Cannot reach the local service',
  'shell.load.http': 'The local service returned an error',
  'shell.load.parse': 'The dashboard response could not be parsed',
  'shell.load.errorDetail': 'Retry dashboard data. This action will not start sync.',
  'shell.load.retry': 'Retry dashboard',
  'shell.load.segment': 'Analysis section {index}, {state}',
  'shell.load.segmentPending': 'pending',
  'shell.load.segmentReady': 'complete',
  'shell.load.segmentDegraded': 'degraded',
  'syncCenter.headline.empty': 'Waiting for sync status',
  'syncCenter.headline.ready': 'Sync status is ready',
  'syncCenter.headline.running': 'Sync is running',
  'syncCenter.headline.busy': 'Sync worker is busy',
  'syncCenter.headline.failed': 'Recent sync failed',
  'syncCenter.headline.cancelled': 'Sync cancelled',
  'syncCenter.headline.rebuildRisk': 'Rebuild risk detected',
  'syncCenter.reason.empty': 'This snapshot has no sync command center data yet.',
  'syncCenter.reason.ready': 'You can run ordinary sync; destructive rebuild paths remain guarded by the backend.',
  'syncCenter.reason.running': 'Structured progress is updating the current sync state.',
  'syncCenter.reason.cancelling': 'Cancellation was requested; the worker is releasing the lock and winding down.',
  'syncCenter.reason.failedJob': 'The latest foreground sync job failed; review sync logs before retrying.',
  'syncCenter.reason.jobFailed': 'The sync job failed; details remain in local logs.',
  'syncCenter.reason.lastRunFailed': 'The latest sync run failed; details remain in local logs.',
  'syncCenter.reason.sourceError': 'This source has a recent sync error; details remain in local logs.',
  'syncCenter.reason.cancelled': 'Sync was cancelled; imported data remains unchanged.',
  'syncCenter.reason.rebuildRisk': 'Ordinary sync is safe; resolve risk sources before rebuilding.',
  'syncCenter.action.sync': 'Sync now',
  'syncCenter.action.busy': 'Another sync job holds the worker lock.',
  'shell.refresh.label': 'Refresh',
  'shell.refresh.off': 'Off',
  'shell.refresh.aria': 'Auto refresh interval',
  'shell.refresh.failed': 'Refresh failed',
  'shell.refresh.secondaryStale': 'Refreshing this range; this panel is temporarily showing the previous result.',
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
  'shell.nav.item.explorer': 'Usage analysis',
  'shell.nav.item.cost': 'Cost',
  'shell.nav.item.status': 'Status',
  'shell.nav.item.logs': 'Event logs',
  'shell.logs.title': 'Event logs',
  'shell.logs.sub': 'Browse local usage events page by page, with raw records loaded on demand.',
  'shell.endpoint.lastSync': 'Last sync',
  'shell.filters.source': 'Source',
  'shell.filters.aria': 'Dashboard filters',
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
    'View recent local usage, cost estimates and runtime status. All data stays in a local SQLite file with no external calls and no telemetry. Safe to use offline.',
  'shell.trends.eyebrow': 'TRENDS',
  'shell.trends.title': 'Usage trends',
  'shell.trends.sub': 'The chart shows the most recent 10 buckets in the current window; expand for the full table.',
  'shell.trends.legend.tokens': 'Usage (tokens)',
  'shell.trends.chart.recent10': 'Recent 10 buckets',
  'shell.trends.windowAria': 'Trends time window',
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
  'shell.behavior.sub': 'Uses interaction and tool-call data extracted during sync; low sample sizes and unsupported sources are called out explicitly.',
  'shell.behavior.activity.title': 'Activity categories',
  'shell.behavior.activity.sub': 'Turns, edit turns, and one-shot rate by activity category.',
  'shell.behavior.tools.title': 'Tool usage',
  'shell.behavior.tools.sub': 'Built-in tools, shell commands, MCP, and sub-agent actions.',
  'shell.behavior.optimize.title': 'Optimization hints',
  'shell.behavior.optimize.sub': 'Read-only waste detection; llmusage never deletes, archives or rewrites automatically.',
  'shell.behavior.compare.title': 'Model comparison',
  'shell.behavior.compare.sub': 'Compare estimated cost, single-turn completion, retry, and working-pattern signals with explicit low-sample warnings.',
  'shell.explorer.eyebrow': 'EXPLORER',
  'shell.explorer.title': 'Usage analysis',
  'shell.explorer.sub': 'Analyze local usage by time granularity, metric, dimension, and tool filters. Results come from backend aggregation, not browser-side raw-record processing.',
  'shell.explorer.metric': 'Metric',
  'shell.explorer.metric.cost': 'Attributed cost',
  'shell.explorer.metric.calls': 'Calls',
  'shell.explorer.metric.turns': 'Turns',
  'shell.explorer.metric.sessions': 'Sessions',
  'shell.explorer.metric.tokens': 'Total tokens',
  'shell.explorer.groupBy': 'Group by',
  'shell.explorer.group.source': 'Source',
  'shell.explorer.group.model': 'Model',
  'shell.explorer.group.project': 'Project',
  'shell.explorer.group.session': 'Session',
  'shell.explorer.group.tool': 'Tool',
  'shell.explorer.group.toolKind': 'Tool kind',
  'shell.explorer.group.isTool': 'Tool / non-tool',
  'shell.explorer.group.tokenType': 'Token type',
  'shell.explorer.granularity': 'Time granularity',
  'shell.explorer.granularity.total': 'Total',
  'shell.explorer.granularity.day': 'Daily',
  'shell.explorer.granularity.week': 'Weekly',
  'shell.explorer.granularity.month': 'Monthly',
  'shell.explorer.limit': 'Maximum results',
  'shell.explorer.session': 'Session filter',
  'shell.explorer.sessionPlaceholder': 'session id',
  'shell.explorer.tool': 'Tool filter',
  'shell.explorer.toolPlaceholder': 'Read / Bash / Edit',
  'shell.explorer.toolKind': 'Tool kind',
  'shell.explorer.toolKind.read': 'Read',
  'shell.explorer.toolKind.edit': 'Edit',
  'shell.explorer.toolKind.shell': 'Shell',
  'shell.explorer.toolKind.mcp': 'MCP tool',
  'shell.explorer.toolKind.agent': 'Sub-agent',
  'shell.explorer.toolKind.nonTool': 'Non-tool call',
  'shell.explorer.tokenType': 'Token type',
  'shell.explorer.tokenType.input': 'Input tokens',
  'shell.explorer.tokenType.cacheRead': 'Cache-read tokens',
  'shell.explorer.tokenType.cacheCreation': 'Cache-creation tokens',
  'shell.explorer.tokenType.output': 'Output tokens',
  'shell.explorer.tokenType.reasoningOutput': 'Reasoning-output tokens',
  'shell.explorer.all': 'All',
  'shell.explorer.includeOther': 'Merge Other',
  'shell.explorer.includeNonTool': 'Include non-tool',
  'shell.explorer.apply': 'Run analysis',
  'shell.explorer.reset': 'Reset',
  'shell.explorer.snapshotDisabled': 'Offline snapshots use the analysis query captured during export',
  'shell.explorer.rowsTitle': 'Dimension ranking',
  'shell.explorer.rowsSub': 'Sorted by the selected metric; results beyond the display limit can be merged into Other.',
  'shell.explorer.seriesTitle': 'Time series',
  'shell.explorer.seriesSub': 'The top 5 dimensions use independent scales across the full offline-ready range.',
  'shell.explorer.seriesScope': 'Showing the top {shown} dimensions by total; find the rest in details.',
  'shell.explorer.seriesScopeAll': 'Showing all {shown} dimensions.',
  'shell.explorer.seriesIndependentScale': 'Independent scale per dimension',
  'shell.explorer.seriesPeak': 'Peak',
  'shell.explorer.seriesAria': '{label} time trend, {range}, peak {value}',
  'shell.explorer.seriesDetails': 'Time-series details',
  'shell.explorer.seriesDetailsMeta': '{count} data points · {range}',
  'shell.explorer.seriesTruncated': 'Showing the latest {shown} of {total} rows; the chart still covers the full range.',
  'shell.explorer.seriesTotalEmpty': 'Total granularity does not return a time series.',
  'shell.explorer.seriesEmpty': 'No analysis time-series data.',
  'shell.explorer.table.time': 'Time',
  'shell.explorer.table.dimension': 'Dimension',
  'shell.cost.eyebrow': 'COST',
  'shell.cost.title': 'Cost estimate',
  'shell.cost.sub': 'Local estimate from public pricing tables. For reference only. May differ from your bill.',
  'shell.cost.panelTitle': 'Top 5 source / model combinations',
  'shell.cost.panelSub': 'Unit: USD',
  'shell.cost.expand': 'Expand all cost entries →',
  'shell.cost.collapse': 'Collapse cost entries ↑',
  'shell.failures.eyebrow': 'FAILURES',
  'shell.failures.title': 'Recent failures',
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
 * 4) document.documentElement.lang 随 locale 同步（zh-CN / en）
 */
let currentLocale = readStoredLocale();
const localeListeners = new Set();

function syncDocumentLang(locale) {
  if (typeof document === 'undefined' || !document.documentElement) return;
  document.documentElement.lang = locale === 'zh' ? 'zh-CN' : 'en';
}

syncDocumentLang(currentLocale);

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
  syncDocumentLang(next);
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
