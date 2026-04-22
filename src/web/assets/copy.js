const logger = window.console;

export const UI_COPY = Object.freeze({
  hero: Object.freeze({
    summaryKicker: '概览',
    summaryTitle: '运行概览',
    statusStable: '正常',
    statusWarn: '有失败',
    rows: Object.freeze({
      generatedAt: '生成时间',
      lastSyncAt: '最近同步',
      lastExportAt: '最近导出',
      sourceCount: '来源数',
      failureCount: '失败记录',
      topModel: '用量最高模型',
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
        totalTokens: '总用量',
        inputShare: '输入占比',
        outputShare: '输出占比',
        cachedShare: '缓存占比',
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
        lastEventAt: '最近记录',
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
        estimatedCostUsd: '估算成本',
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
  }),
});

const TREND_WINDOW_COPY = Object.freeze({
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

/*
 * ========================================================================
 * 步骤1：解析趋势窗口文案
 * ========================================================================
 * 目标：
 * 1) 按时间窗口返回对应的单位和标题
 * 2) 避免在渲染层散落 day / week / month / all 判断
 * 3) 让趋势区标题始终贴合真实聚合粒度
 */
export function resolveTrendWindowCopy(windowName) {
  logger.info('开始解析趋势窗口文案');

  // 1.1 根据窗口名选择对应的时间单位文案
  const resolved = TREND_WINDOW_COPY[windowName] || TREND_WINDOW_COPY.day;

  logger.info('完成趋势窗口文案解析');
  return resolved;
}

/*
 * ========================================================================
 * 步骤2：翻译状态文案
 * ========================================================================
 * 目标：
 * 1) 把后端状态码转换成用户可读中文
 * 2) 保持未知状态原样回退，避免吞掉真实值
 * 3) 让运行状态区不再直接暴露英文状态码
 */
export function translateStatusLabel(status) {
  logger.info('开始翻译状态文案');

  // 2.1 优先命中常见状态，再回退到原始状态文本
  const normalized = String(status || '').toLowerCase();
  const mapping = {
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
  };
  const resolved = mapping[normalized] || String(status || '未知');

  logger.info('完成状态文案翻译');
  return resolved;
}
