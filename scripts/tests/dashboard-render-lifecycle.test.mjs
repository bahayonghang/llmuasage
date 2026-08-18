import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const ASSET_ROOT = new URL('../../src/web/assets/', import.meta.url);

async function collectLocalModuleAssetUrls(entryUrl) {
  const pending = [entryUrl];
  const seen = new Set();
  while (pending.length > 0) {
    const moduleUrl = pending.pop();
    if (seen.has(moduleUrl.href)) continue;
    seen.add(moduleUrl.href);

    const source = await readFile(moduleUrl, 'utf8');
    const importPattern = /(?:\bfrom\s+|\bimport\s*(?:\(\s*)?)["']([^"']+)["']/g;
    for (const match of source.matchAll(importPattern)) {
      const specifier = match[1];
      if (!specifier.startsWith('.')) continue;
      const importedUrl = new URL(specifier, moduleUrl);
      if (importedUrl.href.startsWith(ASSET_ROOT.href)) pending.push(importedUrl);
    }
  }

  return [...seen]
    .map((href) => `/assets/${decodeURIComponent(new URL(href).pathname.slice(ASSET_ROOT.pathname.length))}`)
    .sort();
}

const liveModuleAssetUrls = await collectLocalModuleAssetUrls(new URL('app.js', ASSET_ROOT));

// window/document stub 必须先于模块 import（各模块顶层有 `const logger = window.console`）。
globalThis.window = {
  console: { info() {}, warn() {}, error() {} },
  location: { origin: 'http://127.0.0.1' },
  localStorage: null,
};

const elementRegistry = new Map();

function getElement(id) {
  if (!elementRegistry.has(id)) {
    const el = {
      id,
      dataset: {},
      mutations: [],
      insertAdjacentHTML(position, html) {
        this.mutations.push(['insertAdjacentHTML', position]);
        this._innerHTML = position === 'afterbegin' ? html + this._innerHTML : this._innerHTML + html;
      },
      querySelector() { return { hidden: true, style: {}, textContent: '' }; },
      querySelectorAll() { return []; },
    };
    Object.defineProperty(el, 'textContent', {
      get() { return this._textContent ?? ''; },
      set(value) { this.mutations.push(['textContent', String(value)]); this._textContent = String(value); },
    });
    Object.defineProperty(el, 'innerHTML', {
      get() { return this._innerHTML ?? ''; },
      set(value) { this.mutations.push(['innerHTML', value]); this._innerHTML = String(value); },
    });
    elementRegistry.set(id, el);
  }
  return elementRegistry.get(id);
}

function resetMutations() {
  for (const el of elementRegistry.values()) {
    el.mutations = [];
  }
}

function mutatedIds() {
  return [...elementRegistry.values()].filter((el) => el.mutations.length > 0).map((el) => el.id);
}

globalThis.document = {
  getElementById: getElement,
};

const fingerprint = await import('../../src/web/assets/data/render-key.js');
const format = await import('../../src/web/assets/data/format.js');
const derive = await import('../../src/web/assets/data/derive.js');
const copy = await import('../../src/web/assets/copy.js');
const behavior = await import('../../src/web/assets/render/behavior.js');
const explorer = await import('../../src/web/assets/render/explorer.js');
const insights = await import('../../src/web/assets/render/insights.js');
const summaryCards = await import('../../src/web/assets/render/summary-cards.js');
const calendarHeatmap = await import('../../src/web/assets/render/calendar-heatmap.js');
const trendsDaily = await import('../../src/web/assets/render/trends-daily.js');
const loadState = await import('../../src/web/assets/load-state.js');

function minimalRaw() {
  return {
    overview: {
      generated_at: '2026-07-22T00:00:00Z',
      total: { total_tokens: 100 },
      last_24h: { total_tokens: 10 },
      source_count: 1,
      cache_efficiency: 0.5,
      last_sync_at: '2026-07-21T23:00:00Z',
    },
    trends: [{ label: '2026-07-21', total_tokens: 50 }],
    models: [{ model: 'm1', total_tokens: 100, input_tokens: 40, output_tokens: 60, cache_read_tokens: 0, cost_with_cache_usd: 0.1, cache_savings_usd: 0 }],
    sources: [{ source: 'codex', total_tokens: 100, last_event_at: '2026-07-21T00:00:00Z' }],
    projects: [{ project_hash: 'p1', project_label: 'proj', total_tokens: 100 }],
    costs: [{ source: 'codex', model: 'm1', estimated_cost_usd: 0.1, event_count: 2, total_tokens: 100 }],
    activity: { support: { supported: true, level: 'normalized' }, breakdown: [] },
    tools: { support: { supported: true, level: 'normalized' }, breakdown: [] },
    optimize: { support: { supported: true }, findings: [] },
    compare: { support: { supported: true }, metrics: [] },
    health: { integrations: [], cursors: [], cursor_count: 0, recent_failures: [] },
    diagnostics: { by_source: [], recent_failures: [] },
    sync_command_center: { generated_at: '2026-07-22T00:00:00Z', sources: [], metrics: {} },
  };
}

function behaviorContext(secondaryRefreshing = false) {
  return {
    panels: {
      activity: [{ category: 'coding', turns: 10, edit_turns: 2, one_shot_rate: 0.5, estimated_cost_usd: 1.23 }],
      activity_support: { supported: true, level: 'normalized' },
      tools: [{ tool_name: 'Read', tool_kind: 'read', calls: 5, call_share: 0.5, estimated_cost_usd: 0.1 }],
      tools_support: { supported: false, level: 'no_data' },
      optimize: {
        support: { supported: true },
        findings: [{ severity: 'low', title: 'f1', evidence: 'e', recommendation: 'r' }],
        grade: 'A',
        score: 95,
        estimated_savings_tokens: 100,
        estimated_savings_usd: 0.5,
      },
      compare: {
        support: { supported: true },
        metrics: [{ id: 'cost', label: 'Cost', model_a_value: 1, model_b_value: 2 }],
        working_style: [],
        model_a: { model: 'a' },
        model_b: { model: 'b' },
      },
      secondary_refreshing: secondaryRefreshing,
    },
  };
}

test('live module graph avoids filter-sensitive asset URLs', () => {
  assert.ok(liveModuleAssetUrls.includes('/assets/data/render-key.js'));
  assert.deepEqual(
    liveModuleAssetUrls.filter((url) => url.toLowerCase().includes('fingerprint')),
    [],
  );
});

test('fingerprint strips volatile per-query fields', async (t) => {
  await t.test('overview/sync_command_center generated_at never changes the fingerprint', () => {
    const a = minimalRaw();
    const b = {
      ...minimalRaw(),
      overview: { ...a.overview, generated_at: '2026-07-22T00:00:30Z' },
      sync_command_center: { ...a.sync_command_center, generated_at: '2026-07-22T00:00:31Z' },
    };
    assert.equal(fingerprint.dashboardFingerprint(a), fingerprint.dashboardFingerprint(b));
    assert.equal(
      fingerprint.panelFingerprint('hero', a, { locale: 'zh' }),
      fingerprint.panelFingerprint('hero', b, { locale: 'zh' }),
    );
  });

  await t.test('real data changes still change the fingerprint', () => {
    const a = minimalRaw();
    const c = { ...minimalRaw(), trends: [{ label: '2026-07-22', total_tokens: 60 }] };
    assert.notEqual(fingerprint.dashboardFingerprint(a), fingerprint.dashboardFingerprint(c));
  });

  await t.test('_meta render-state does not enter the dashboard fingerprint', () => {
    const a = minimalRaw();
    const d = { ...minimalRaw(), _meta: { secondary_refreshing: true } };
    assert.equal(fingerprint.dashboardFingerprint(a), fingerprint.dashboardFingerprint(d));
  });

  await t.test('stripVolatileFields never mutates its input', () => {
    const a = minimalRaw();
    fingerprint.stripVolatileFields(a);
    assert.equal(a.overview.generated_at, '2026-07-22T00:00:00Z');
    assert.equal(a.sync_command_center.generated_at, '2026-07-22T00:00:00Z');
  });

  await t.test('stableSerialize is key-order independent', () => {
    assert.equal(
      fingerprint.stableSerialize({ b: 1, a: { d: [1, 2], c: 'x' } }),
      fingerprint.stableSerialize({ a: { c: 'x', d: [1, 2] }, b: 1 }),
    );
  });
});

test('panel fingerprints isolate subsets, locale, and extra state', () => {
  const rawA = minimalRaw();
  const rawB = { ...minimalRaw(), trends: [] };

  // activity 面板不消费 trends：trends 变化不影响 activity 指纹（section 独立性）
  assert.equal(
    fingerprint.panelFingerprint('activity', rawA, { locale: 'zh' }),
    fingerprint.panelFingerprint('activity', rawB, { locale: 'zh' }),
  );
  assert.notEqual(
    fingerprint.panelFingerprint('trends', rawA, { locale: 'zh' }),
    fingerprint.panelFingerprint('trends', rawB, { locale: 'zh' }),
  );

  // locale 是指纹 key 的一部分：locale 变化指纹自然失效（locale 切换重渲文案）
  assert.notEqual(
    fingerprint.panelFingerprint('hero', rawA, { locale: 'zh' }),
    fingerprint.panelFingerprint('hero', rawA, { locale: 'en' }),
  );

  // 面板级 extra（展开状态）变化令该面板指纹失效
  assert.notEqual(
    fingerprint.panelFingerprint('models', rawA, { locale: 'zh', extra: { expanded: false } }),
    fingerprint.panelFingerprint('models', rawA, { locale: 'zh', extra: { expanded: true } }),
  );

  // secondary refreshing 元数据经 extra 进入指纹：settle 后 notice 必然消失
  assert.notEqual(
    fingerprint.panelFingerprint('activity', rawA, { locale: 'zh', extra: { refreshing: true } }),
    fingerprint.panelFingerprint('activity', rawA, { locale: 'zh', extra: { refreshing: false } }),
  );
});

test('buildContext memoizes by rawData reference', () => {
  const raw = minimalRaw();
  derive.buildContext(raw);
  const before = derive.buildContextStats();

  // 同一引用重复渲染（面板展开/折叠、locale 切换路径）零重算
  const ctx1 = derive.buildContext(raw);
  const ctx2 = derive.buildContext(raw);
  const afterHits = derive.buildContextStats();
  assert.equal(ctx1, ctx2);
  assert.equal(afterHits.computes, before.computes);
  assert.ok(afterHits.memoHits >= before.memoHits + 2);

  // 新引用（替换式更新）才重新派生
  derive.buildContext({ ...raw });
  const afterNewRef = derive.buildContextStats();
  assert.equal(afterNewRef.computes, before.computes + 1);
});

test('locale switch does not recompute buildContext but invalidates fingerprints', () => {
  const raw = minimalRaw();
  derive.buildContext(raw);
  const fpZh = fingerprint.panelFingerprint('hero', raw, { locale: copy.getLocale() });

  const before = derive.buildContextStats();
  copy.setLocale(copy.getLocale() === 'zh' ? 'en' : 'zh');
  try {
    // 模拟 app.js onLocaleChange → renderDashboard(state.rawData)：同引用，memo 命中
    derive.buildContext(raw);
    const after = derive.buildContextStats();
    assert.equal(after.computes, before.computes, 'locale 切换不应触发 buildContext 重算');
    assert.ok(after.memoHits > before.memoHits);

    const fpSwitched = fingerprint.panelFingerprint('hero', raw, { locale: copy.getLocale() });
    assert.notEqual(fpZh, fpSwitched, 'locale 变化必须令面板指纹失效');
  } finally {
    copy.setLocale('zh');
  }
});

test('UTC instants render in the requested timezone', () => {
  assert.equal(format.formatClock('2026-08-18T12:00:00Z', 'Asia/Shanghai'), '20:00');
  assert.equal(format.formatClock('2026-08-18T12:26:54Z', 'Asia/Shanghai'), '20:26');
  assert.equal(format.formatClock('2026-08-18T12:00:00Z', 'UTC'), '12:00');
  assert.equal(format.formatDateTime('2026-08-18T12:00:00Z', 'Asia/Shanghai'), '2026-08-18 20:00');
  assert.equal(format.formatDateTime('2026-08-18T12:26:54Z', 'Asia/Shanghai'), '2026-08-18 20:26:54');
  assert.equal(format.formatClock('2026-08-18', 'Asia/Shanghai'), '2026-08-18');
  assert.equal(format.formatDateTime('2026-08', 'UTC'), '2026-08');
  assert.equal(format.formatDateTime('', 'UTC'), '--');
});

test('Intl.NumberFormat construction is bounded', () => {
  for (let i = 0; i < 500; i += 1) {
    format.formatNumber(i);
    format.formatCompact(i * 1000);
    format.formatTokenAmount(i);
    format.formatCompactCurrency(i * 1000);
  }
  const warmed = format.numberFormatterStats();
  assert.ok(warmed.constructed <= 4, `构造次数应有界，实际 ${warmed.constructed}`);
  assert.ok(warmed.cached <= 4);

  // 格式化结果不受缓存影响
  assert.equal(format.formatNumber(1234567), '1,234,567');

  for (let i = 0; i < 500; i += 1) {
    format.formatNumber(i);
    format.formatCompact(i * 1000);
    format.formatCompactCurrency(i * 1000);
  }
  const after = format.numberFormatterStats();
  assert.equal(after.constructed, warmed.constructed, '预热后不得再构造新 formatter');
});

test('behavior sections render only their own containers', () => {
  const context = behaviorContext(false);

  resetMutations();
  behavior.renderActivity(context);
  assert.deepEqual(mutatedIds().sort(), ['activity-bars', 'activity-support', 'activity-table']);
  assert.equal(getElement('activity-support').textContent, '数据完整');
  assert.ok(getElement('activity-table').innerHTML.includes('编码'));

  resetMutations();
  behavior.renderTools(context);
  assert.deepEqual(mutatedIds().sort(), ['tools-bars', 'tools-support', 'tools-table']);
  assert.equal(getElement('tools-support').textContent, '暂无数据');
  assert.ok(getElement('tools-table').innerHTML.includes('Read'));
  assert.ok(getElement('tools-table').innerHTML.includes('读取'));

  resetMutations();
  behavior.renderOptimize(context);
  assert.deepEqual(mutatedIds().sort(), ['optimize-findings', 'optimize-summary']);
  assert.ok(getElement('optimize-findings').innerHTML.includes('f1'));

  resetMutations();
  behavior.renderCompare(context);
  assert.deepEqual(mutatedIds().sort(), ['compare-panel']);
  assert.ok(getElement('compare-panel').innerHTML.includes('Cost'));
});

test('stale refresh notice follows secondary_refreshing and locale', () => {
  const refreshing = behaviorContext(true);

  resetMutations();
  behavior.renderActivity(refreshing);
  assert.equal(getElement('activity-support').textContent, '刷新中');
  assert.ok(getElement('activity-table').innerHTML.includes('stale-refresh-notice'));
  assert.ok(getElement('activity-table').innerHTML.includes('正在刷新当前时间范围'));

  resetMutations();
  behavior.renderOptimize(refreshing);
  assert.ok(getElement('optimize-findings').innerHTML.includes('stale-refresh-notice'));

  resetMutations();
  behavior.renderCompare(refreshing);
  assert.ok(getElement('compare-panel').innerHTML.includes('stale-refresh-notice'));

  // settle（refreshing=false）后 notice 消失
  resetMutations();
  behavior.renderActivity(behaviorContext(false));
  assert.ok(!getElement('activity-table').innerHTML.includes('stale-refresh-notice'));

  // locale 切换后 notice 文案切换
  copy.setLocale('en');
  try {
    resetMutations();
    behavior.renderActivity(behaviorContext(true));
    assert.ok(
      getElement('activity-table').innerHTML.includes('temporarily showing the previous result'),
    );
  } finally {
    copy.setLocale('zh');
  }
});

test('dynamic analysis terminology follows the selected locale', () => {
  const explorerContext = {
    panels: {
      explorer: {
        support: { supported: false, level: 'no_data', reason: 'No usage events match this filter.' },
        metric: 'total_tokens',
        group_by: 'model',
        granularity: 'day',
        totals: { value: 0 },
        rows: [],
        series: [],
      },
    },
  };
  const insightContext = {
    insights: [
      { id: 'top_model', tone: 'neutral', params: { model: 'gpt-5', tokens: '1.2K' } },
      { id: 'sync_failure', tone: 'warn', params: { count: 1, command: 'sync' } },
    ],
  };

  explorer.renderExplorer(explorerContext);
  insights.renderInsights(insightContext);
  assert.equal(getElement('explorer-support').textContent, '暂无数据');
  assert.ok(getElement('explorer-summary').innerHTML.includes('分组维度'));
  assert.ok(getElement('explorer-rows').innerHTML.includes('当前筛选范围没有用量事件'));
  assert.ok(getElement('insights-card').innerHTML.includes('当前筛选范围的主要模型'));
  assert.ok(getElement('insights-card').innerHTML.includes('1.2K Token'));

  copy.setLocale('en');
  try {
    explorer.renderExplorer(explorerContext);
    insights.renderInsights(insightContext);
    assert.equal(getElement('explorer-support').textContent, 'No data');
    assert.ok(getElement('explorer-summary').innerHTML.includes('Group by'));
    assert.ok(getElement('insights-card').innerHTML.includes('Primary model in this filter range'));
    assert.ok(getElement('insights-card').innerHTML.includes('1.2K tokens'));
    assert.ok(getElement('insights-card').innerHTML.includes('Failure records: 1'));
  } finally {
    copy.setLocale('zh');
  }
});


test('ready-widget pure derivations preserve approved mappings and boundaries', () => {
  const cards = derive.buildSummaryCards({
    summary: {
      total_sessions: 2,
      total_requests: 5,
      total_tokens: 1200,
      total_cost_usd: 1.25,
      cache_efficiency: 0.75,
      active_days: 3,
      platforms: 2,
    },
    by_platform: {
      codex: { tokens: 900 },
      claude: { tokens: 300 },
    },
  });
  assert.equal(cards.length, 6);
  assert.equal(cards[0].label, '会话数');
  assert.equal(cards[0].sub, '2 个来源');
  assert.equal(cards[1].sub, '2.5 次 / 会话');
  assert.equal(cards[2].featured, true);
  assert.equal(cards[2].label, 'Token 用量');
  assert.equal(cards[2].sub, '用量最高来源: codex');
  assert.match(cards[2].sub, /codex/);
  assert.equal(cards[5].label, '缓存读取占比');
  assert.equal(cards[5].value, '75.0%');

  assert.deepEqual(derive.heatmapLevels([0, 0, 0]), [0, 0, 0]);
  assert.deepEqual(derive.heatmapLevels([0, 7]), [0, 4]);
  assert.deepEqual(derive.heatmapLevels([0, 1, 2, 3, 4]), [0, 1, 2, 3, 4]);
  assert.equal(trendsDaily.niceScale(0), 1);
  assert.equal(trendsDaily.niceScale(187), 200);
  assert.equal(trendsDaily.niceScale(501), 1000);
});

test('ready-widget renderers mutate only their section containers', () => {
  const context = {
    panels: {
      home_overview: {
        summary: {
          total_sessions: 2,
          total_requests: 5,
          total_tokens: 1200,
          total_cost_usd: 1.25,
          cache_efficiency: 0.75,
          active_days: 3,
          platforms: 2,
        },
        by_platform: { codex: { tokens: 1200 } },
      },
      heatmap: [
        { date: '2026-01-31', event_count: 1, total_tokens: 10 },
        { date: '2026-02-01', event_count: 2, total_tokens: 20 },
        { date: '2026-02-02', event_count: 3, total_tokens: 30 },
      ],
      heatmap_support: null,
      trends_daily: [
        { date: '2026-02-01', input_tokens: 10, cache_read_tokens: 20, cache_creation_tokens: 30, output_tokens: 40, cost_with_cache_usd: 1 },
        { date: '2026-02-02', input_tokens: 20, cache_read_tokens: 30, cache_creation_tokens: 40, output_tokens: 50, cost_with_cache_usd: 2 },
        { date: '2026-02-03', input_tokens: 30, cache_read_tokens: 40, cache_creation_tokens: 50, output_tokens: 60, cost_with_cache_usd: 3 },
      ],
      trends_daily_support: null,
    },
  };
  const state = { rangePreset: '7d', filters: {} };

  resetMutations();
  summaryCards.renderSummaryCards(context, state);
  assert.deepEqual(mutatedIds(), ['summary-cards']);
  assert.ok(getElement('summary-cards').innerHTML.includes('summary-card featured'));

  resetMutations();
  calendarHeatmap.renderCalendarHeatmap(context, state);
  assert.deepEqual(mutatedIds(), ['calendar-heatmap']);
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('viewBox="0 0'));
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('周一'));
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('aria-label='));

  context.panels.heatmap = Array.from({ length: 365 }, (_value, index) => {
    const date = new Date(Date.UTC(2025, 0, index + 1)).toISOString().slice(0, 10);
    return { date, event_count: 1, total_tokens: index + 1 };
  });
  calendarHeatmap.renderCalendarHeatmap(context, { rangePreset: 'all', filters: {} });
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('calendar-heatmap-svg is-long-range'));

  resetMutations();
  trendsDaily.renderTrendsDaily(context, state);
  assert.deepEqual(mutatedIds(), ['trends-daily']);
  assert.ok(getElement('trends-daily').innerHTML.includes('daily-series-3'));
  assert.ok(getElement('trends-daily').innerHTML.includes('2026-02-03'));
});

test('ready-widget render lifecycle rejects stale results and waits for every section', () => {
  for (const target of ['home_overview', 'heatmap', 'trends_daily']) {
    let state = loadState.reduceDashboardLoadState(loadState.createDashboardLoadState(4), {
      type: 'core_succeeded',
      generation: 4,
    });
    for (const section of loadState.SECONDARY_SECTIONS.filter((section) => section !== target)) {
      state = loadState.reduceDashboardLoadState(state, {
        type: 'secondary_settled', generation: 4, section, degraded: false,
      });
    }
    assert.equal(state.phase, 'secondary_loading');
    const stale = loadState.reduceDashboardLoadState(state, {
      type: 'secondary_settled', generation: 3, section: target, degraded: false,
    });
    assert.equal(stale, state);
    const complete = loadState.reduceDashboardLoadState(state, {
      type: 'secondary_settled', generation: 4, section: target, degraded: false,
    });
    assert.equal(complete.phase, 'complete');
  }
});
