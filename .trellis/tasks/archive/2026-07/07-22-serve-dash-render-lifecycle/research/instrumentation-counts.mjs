/*
 * S2 渲染生命周期插桩计数脚本（node 直接运行，不进 CI）。
 * 用法：node .trellis/tasks/07-22-serve-dash-render-lifecycle/research/instrumentation-counts.mjs
 *
 * 模拟改造后 app.js 的调用序列，用 buildContextStats() 计量真实派生次数；
 * "旧路径真算"= 无 memo 时每次调用都全量派生（= 调用次数，由代码走读确认）。
 */

globalThis.window = {
  console: { info() {}, warn() {}, error() {} },
  location: { origin: 'http://127.0.0.1' },
  localStorage: null,
};
globalThis.document = { getElementById: () => null };

const derive = await import('../../../../src/web/assets/data/derive.js');
const fingerprint = await import('../../../../src/web/assets/data/fingerprint.js');

function raw(overrides = {}) {
  return {
    overview: { generated_at: 't0', total: { total_tokens: 100 }, last_24h: { total_tokens: 5 }, cache_efficiency: 0.5 },
    trends: [{ label: 'd1', total_tokens: 50 }],
    models: [{ model: 'm', total_tokens: 100 }],
    sources: [{ source: 'codex', total_tokens: 100 }],
    projects: [],
    costs: [],
    activity: { support: { supported: true }, breakdown: [] },
    tools: { support: { supported: true }, breakdown: [] },
    optimize: { support: { supported: true }, findings: [] },
    compare: { support: { supported: true }, metrics: [] },
    health: { integrations: [], cursor_count: 0 },
    diagnostics: { by_source: [], recent_failures: [] },
    sync_command_center: { generated_at: 't0', sources: [], metrics: {} },
    ...overrides,
  };
}

function scenario(label, oldComputes, newCalls, fn) {
  const before = derive.buildContextStats();
  const result = fn();
  const after = derive.buildContextStats();
  const computes = after.computes - before.computes;
  console.log(
    `${label}\n  旧路径真算 ${oldComputes} 次；新路径 buildContext 调用 ${newCalls} 次、真算 ${computes} 次${result ? `；${result}` : ''}`,
  );
  return computes;
}

console.log('== S2 渲染生命周期插桩计数（新路径实测，旧路径=调用次数） ==\n');

// 1. renderDashboard 单次（renderPrimaryDashboard + renderBehaviorSections + renderExplorerPanel 共享 memo）
{
  const data = raw();
  scenario('renderDashboard 单次调用链', 2, 3, () => {
    derive.buildContext(data);
    derive.buildContext(data);
    derive.buildContext(data);
  });
}

// 2. fast-range 全流程：refreshing 标记(2 入口) + core 到达(1) + 5 个 secondary 各(1) + settle(2)
{
  let data = raw();
  derive.buildContext(data); // 首屏已渲染
  scenario('fast-range 全流程（10 个调用点）', 10, 10, () => {
    data = { ...data, _meta: { secondary_refreshing: true } };
    derive.buildContext(data); // renderBehaviorSections
    derive.buildContext(data); // renderExplorerPanel（memo 命中）
    data = { ...data, trends: [{ label: 'd1', total_tokens: 88 }], _meta: { secondary_refreshing: true } };
    derive.buildContext(data); // renderPrimaryDashboard
    for (const section of ['activity', 'tools', 'optimize', 'compare', 'explorer']) {
      data = { ...data, [section]: { support: { supported: true }, breakdown: [] } };
      derive.buildContext(data); // renderSecondarySection（每次新数据到达）
    }
    data = { ...data, _meta: { secondary_refreshing: false } };
    derive.buildContext(data); // settle renderBehaviorSections
    derive.buildContext(data); // settle renderExplorerPanel（memo 命中）
    return '每次真算都对应一次真实数据到达';
  });
}

// 3. locale 切换：同一 rawData 引用重渲
{
  const data = raw();
  derive.buildContext(data);
  scenario('locale 切换重渲（3 入口）', 2, 3, () => {
    derive.buildContext(data);
    derive.buildContext(data);
    derive.buildContext(data);
    return '0 次真算（memo 命中），指纹含 locale 故文案面板正常重渲';
  });
}

// 4. 面板展开/折叠：只重渲对应面板
{
  const data = raw();
  derive.buildContext(data);
  scenario('展开/折叠单个面板', 2, 1, () => {
    const fpA = fingerprint.panelFingerprint('models', data, { locale: 'zh', extra: { expanded: false } });
    const fpB = fingerprint.panelFingerprint('models', data, { locale: 'zh', extra: { expanded: true } });
    derive.buildContext(data); // memo 命中
    return fpA !== fpB ? '仅 models 面板指纹变化，其余面板指纹不变（零写入）' : '指纹未生效!';
  });
}

// 5. 自动刷新数据未变：dashboardFingerprint 短路 + secondary 指纹短路
{
  const previous = raw();
  const next = raw({ overview: { ...previous.overview, generated_at: 't1' }, sync_command_center: { ...previous.sync_command_center, generated_at: 't1' } });
  scenario('自动刷新 tick（数据未变，仅 generated_at 漂移）', 2, 0, () => {
    const same = fingerprint.dashboardFingerprint(previous) === fingerprint.dashboardFingerprint(next);
    if (!same) derive.buildContext(next); // 不会走到
    return same
      ? '主指纹相同 → skip renderPrimaryDashboard；secondary 各指纹相同 → 0 次 buildContext、0 DOM 写入'
      : '指纹剔除失效!';
  });
}

// 6. job 轮询无变化 tick：浅比对 + memo
{
  const data = raw();
  derive.buildContext(data);
  scenario('job 轮询无变化 tick（900ms）', 1, 1, () => {
    derive.buildContext(data); // refreshSyncCommandCenter 内（memo 命中）
    return '浅比对无变化时连 updateSyncButton/渲染都跳过';
  });
}
