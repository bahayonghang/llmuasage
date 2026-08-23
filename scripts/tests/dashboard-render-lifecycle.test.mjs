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
      hidden: false,
      dataset: {},
      mutations: [],
      addEventListener() {},
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
    el.hidden = false;
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
const hero = await import('../../src/web/assets/render/hero.js');
const behavior = await import('../../src/web/assets/render/behavior.js');
const explorer = await import('../../src/web/assets/render/explorer.js');
const insights = await import('../../src/web/assets/render/insights.js');
const syncCommandCenter = await import('../../src/web/assets/render/sync-command-center.js');
const summaryCards = await import('../../src/web/assets/render/summary-cards.js');
const calendarHeatmap = await import('../../src/web/assets/render/calendar-heatmap.js');
const trendsDaily = await import('../../src/web/assets/render/trends-daily.js');
const topSessions = await import('../../src/web/assets/render/top-sessions.js');
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
    hosts: [{ host_id: 'local', label: 'local', total_tokens: 100, last_event_at: '2026-07-21T00:00:00Z' }],
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

function syncCenterPayload(overrides = {}) {
  return {
    mode: 'live',
    tone: 'warn',
    headline_key: 'syncCenter.headline.rebuildRisk',
    reason_key: 'syncCenter.reason.rebuildRisk',
    generated_at: '2026-08-19T12:39:53Z',
    current_job: null,
    last_run: {
      status: 'success',
      command: 'sync',
      started_at: '2026-08-19T08:58:20Z',
      finished_at: '2026-08-19T08:58:21Z',
    },
    safety: {
      ordinary_sync_safe: true,
      worker_lock: 'available',
      lossy_rebuild_risk: true,
      risk_sources: ['claude'],
      risk_details: [{ source: 'claude', missing_file_count: 728, protected_event_count: 36495 }],
      recent_failures: 0,
    },
    metrics: { events_seen: 127, inserted_delta: 84, stored_events: 249020, sources_ready: 9, sources_total: 9 },
    sources: [{
      source: 'claude',
      status: 'ok',
      tone: 'good',
      events_seen: 67,
      events_inserted: 31,
      stored_events: 36495,
      share: 0.5,
      lossy_rebuild_risk: true,
    }],
    actions: [{ id: 'sync', label_key: 'syncCenter.action.sync', primary: true, disabled: false }],
    ...overrides,
  };
}

function completedSnapshot(finishedAt = '2026-08-19T13:00:01Z') {
  return {
    job_id: 'job-1',
    status: 'completed',
    last_event: { type: 'Finished' },
    started_at: '2026-08-19T12:59:00Z',
    finished_at: finishedAt,
  };
}

test('hosts panel hides unless more than one host is present', async () => {
  const hosts = await import('../../src/web/assets/render/hosts.js');
  resetMutations();
  hosts.renderHosts({
    totals: { total_tokens: 100 },
    panels: { hosts: [{ host_id: 'local', label: 'local', total_tokens: 100, last_event_at: '2026-07-21T00:00:00Z' }] },
  });
  assert.equal(getElement('hosts').hidden, true);

  hosts.renderHosts({
    totals: { total_tokens: 150 },
    panels: {
      hosts: [
        { host_id: 'local', label: 'local', total_tokens: 100, last_event_at: '2026-07-21T00:00:00Z' },
        { host_id: 'devbox', label: 'devbox', total_tokens: 50, last_event_at: '2026-07-21T01:00:00Z' },
      ],
    },
  });
  assert.equal(getElement('hosts').hidden, false);
  assert.match(getElement('hosts-rows').innerHTML, /devbox/);
});

test('live module graph avoids filter-sensitive asset URLs', () => {
  assert.ok(liveModuleAssetUrls.includes('/assets/data/render-key.js'));
  assert.deepEqual(
    liveModuleAssetUrls.filter((url) => url.toLowerCase().includes('fingerprint')),
    [],
  );
});

test('agent badge catalog preserves registry order and renders non-interactive identity badges', () => {
  const rawCatalog = JSON.stringify([
    { id: 'codex', display_name: 'Codex', logo_url: 'assets/agent-logos/codex.svg' },
    { id: 'claude', display_name: 'Claude', logo_url: 'assets/agent-logos/claude.svg' },
    { id: 'opencode', display_name: 'OpenCode', logo_url: 'assets/agent-logos/opencode.svg' },
    { id: 'antigravity', display_name: 'Antigravity', logo_url: 'assets/agent-logos/antigravity.svg' },
    { id: 'kimi_code', display_name: 'Kimi Code', logo_url: 'assets/agent-logos/kimi_code.svg' },
    { id: 'pi', display_name: 'Pi', logo_url: 'assets/agent-logos/pi.svg' },
    { id: 'omp', display_name: 'OMP', logo_url: 'assets/agent-logos/omp.svg' },
    { id: 'grok', display_name: 'Grok', logo_url: 'assets/agent-logos/grok.svg' },
    { id: 'zcode', display_name: 'ZCode', logo_url: 'assets/agent-logos/zcode.svg' },
    { id: 'deepseek_harness', display_name: 'DeepSeek Harness', logo_url: 'assets/agent-logos/deepseek_harness.svg' },
  ]);
  const catalog = hero.parseSourceBadgeCatalog(rawCatalog, 'codex,claude,opencode');

  assert.deepEqual(catalog.map((entry) => entry.id), [
    'codex',
    'claude',
    'opencode',
    'antigravity',
    'kimi_code',
    'pi',
    'omp',
    'grok',
    'zcode',
    'deepseek_harness',
  ]);

  const markup = hero.renderSourceBadgeList(catalog);
  assert.match(markup, /<ul class="agent-badge-list" role="list">/);
  assert.equal((markup.match(/<li class="agent-badge"/g) || []).length, 10);
  assert.match(markup, /data-source="antigravity"/);
  assert.match(markup, /<img[^>]+alt="" aria-hidden="true"/);
  assert.doesNotMatch(markup, /<(?:button|a)\b|tabindex=|role="button"|onclick=/i);
});

test('agent badge catalog recovers safely from malformed, partial, and hostile input', () => {
  assert.deepEqual(
    hero.parseSourceBadgeCatalog('{bad json', 'codex,deepseek_harness').map((entry) => entry),
    [
      { id: 'codex', display_name: 'Codex', logo_url: 'assets/agent-logos/fallback.svg' },
      { id: 'deepseek_harness', display_name: 'Deepseek Harness', logo_url: 'assets/agent-logos/fallback.svg' },
    ],
  );

  const partial = hero.parseSourceBadgeCatalog([
    {
      id: 'future_agent',
      display_name: '<b>Future & "Very Long" Agent</b>',
      logo_url: 'https://example.com/remote.svg',
    },
    {
      id: 'future_agent',
      display_name: 'Duplicate must not replace the first entry',
      logo_url: 'assets/agent-logos/codex.svg',
    },
    { id: 'INVALID ID', display_name: 'Ignored', logo_url: 'assets/agent-logos/codex.svg' },
  ], ['future_agent', 'zcode']);
  assert.deepEqual(partial.map((entry) => entry.id), ['future_agent', 'zcode']);
  assert.equal(partial[0].display_name, '<b>Future & "Very Long" Agent</b>');
  assert.equal(partial[0].logo_url, 'assets/agent-logos/fallback.svg');
  assert.equal(partial[1].display_name, 'Zcode');

  const markup = hero.renderSourceBadgeList(partial);
  assert.match(markup, /&lt;b&gt;Future &amp; &quot;Very Long&quot; Agent&lt;\/b&gt;/);
  assert.doesNotMatch(markup, /https:\/\/example\.com|<b>/);
  assert.equal(hero.renderSourceBadgeList([]), '<ul class="agent-badge-list" role="list"></ul>');
});

test('agent badge source summaries use locale copy and stable counts', () => {
  copy.setLocale('zh');
  assert.equal(
    hero.formatSourceSummary(copy.UI_COPY.hero.sourceSummary, 8, 10),
    '当前筛选有数据 8 / 已支持 10',
  );
  copy.setLocale('en');
  assert.equal(
    hero.formatSourceSummary(copy.UI_COPY.hero.sourceSummary, 8, 10),
    'Data in current filter 8 / 10 supported',
  );
  copy.setLocale('zh');
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
  const rawStatus = {
    ...minimalRaw(),
    sync_command_center: {
      ...rawA.sync_command_center,
      tone: 'warn',
      headline_key: 'syncCenter.headline.failed',
    },
  };

  // activity 面板不消费 trends：trends 变化不影响 activity 指纹（section 独立性）
  assert.equal(
    fingerprint.panelFingerprint('activity', rawA, { locale: 'zh' }),
    fingerprint.panelFingerprint('activity', rawB, { locale: 'zh' }),
  );
  assert.notEqual(
    fingerprint.panelFingerprint('trends', rawA, { locale: 'zh' }),
    fingerprint.panelFingerprint('trends', rawB, { locale: 'zh' }),
  );
  assert.notEqual(
    fingerprint.panelFingerprint('hero', rawA, { locale: 'zh' }),
    fingerprint.panelFingerprint('hero', rawStatus, { locale: 'zh' }),
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

test('sync completed overlay stays ready across payload reload and clear', () => {
  const oldPayload = syncCenterPayload();
  const finishedAt = '2026-08-19T13:00:01Z';
  const refreshedPayload = syncCenterPayload({
    tone: 'good',
    headline_key: 'syncCenter.headline.ready',
    reason_key: 'syncCenter.reason.ready',
    generated_at: '2026-08-19T13:00:02Z',
    last_run: {
      status: 'success',
      command: 'sync',
      started_at: '2026-08-19T12:59:00Z',
      finished_at: finishedAt,
    },
  });
  const render = (payload, activeJobSnapshot) => {
    resetMutations();
    syncCommandCenter.renderSyncCommandCenter(
      { syncCommandCenter: payload },
      { activeJobSnapshot },
    );
    return getElement('sync-command-center').innerHTML;
  };

  const overlayHtml = render(oldPayload, completedSnapshot(finishedAt));
  assert.match(overlayHtml, /data-tone="good"/);
  assert.match(overlayHtml, /同步状态就绪/);
  assert.match(overlayHtml, new RegExp(finishedAt));
  assert.doesNotMatch(overlayHtml, /检测到重建风险/);
  assert.match(overlayHtml, /missing=728/);

  const reloadedHtml = render(refreshedPayload, completedSnapshot(finishedAt));
  assert.match(reloadedHtml, /data-tone="good"/);
  assert.match(reloadedHtml, new RegExp(finishedAt));
  assert.doesNotMatch(reloadedHtml, /2026-08-19T08:58:21Z/);

  const clearedHtml = render(refreshedPayload, null);
  assert.match(clearedHtml, /data-tone="good"/);
  assert.match(clearedHtml, new RegExp(finishedAt));
  assert.doesNotMatch(clearedHtml, /检测到重建风险/);
});

test('rebuild protection insight is neutral and keeps its counts', () => {
  const raw = minimalRaw();
  raw.diagnostics = {
    by_source: [{
      source: 'claude',
      missing_file_count: 728,
      protected_event_count: 36495,
      lossy_rebuild_risk: true,
    }],
    recent_failures: [],
  };
  const context = derive.buildContext(raw);
  const insight = context.insights.find((row) => row.id === 'lossy_rebuild');
  assert.ok(insight);
  assert.equal(insight.tone, 'neutral');
  assert.deepEqual(insight.params, {
    source: 'claude',
    missingCount: '728',
    protectedCount: '36,495',
  });
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
        { date: '2026-02-01', input_tokens: 10, cache_read_tokens: 20, cache_creation_tokens: 30, output_tokens: 40, total_tokens: 110, cost_with_cache_usd: 1 },
        { date: '2026-02-02', input_tokens: 20, cache_read_tokens: 30, cache_creation_tokens: 40, output_tokens: 50, total_tokens: 150, cost_with_cache_usd: 2 },
        { date: '2026-02-03', input_tokens: 30, cache_read_tokens: 40, cache_creation_tokens: 50, output_tokens: 60, total_tokens: 190, cost_with_cache_usd: 3 },
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
  calendarHeatmap.renderCalendarHeatmap(context, { rangePreset: '1d', filters: {} });
  assert.deepEqual(mutatedIds(), ['calendar-heatmap']);
  assert.equal(getElement('calendar-heatmap').hidden, true);
  assert.equal(getElement('calendar-heatmap').innerHTML, '');

  resetMutations();
  calendarHeatmap.renderCalendarHeatmap(context, state);
  assert.deepEqual(mutatedIds(), ['calendar-heatmap']);
  assert.equal(getElement('calendar-heatmap').hidden, false);
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('heatmap-day-strip'));
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('周一'));
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('aria-label='));
  assert.ok(!getElement('calendar-heatmap').innerHTML.includes('calendar-heatmap-svg'));

  context.panels.heatmap = Array.from({ length: 30 }, (_value, index) => {
    const date = new Date(Date.UTC(2026, 0, index + 1)).toISOString().slice(0, 10);
    return { date, event_count: 1, total_tokens: index + 1 };
  });
  calendarHeatmap.renderCalendarHeatmap(context, { rangePreset: '30d', filters: {} });
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('heatmap-day-strip'));
  assert.ok(!getElement('calendar-heatmap').innerHTML.includes('calendar-heatmap-svg'));

  context.panels.heatmap = Array.from({ length: 32 }, (_value, index) => {
    const date = new Date(Date.UTC(2026, 0, index + 1)).toISOString().slice(0, 10);
    return { date, event_count: 1, total_tokens: index + 1 };
  });
  calendarHeatmap.renderCalendarHeatmap(context, {
    rangePreset: 'custom',
    filters: { since: '2026-01-01', until: '2026-02-01' },
  });
  assert.ok(getElement('calendar-heatmap').innerHTML.includes('calendar-heatmap-svg'));
  assert.ok(!getElement('calendar-heatmap').innerHTML.includes('heatmap-day-strip'));

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
  assert.ok(getElement('trends-daily').innerHTML.includes('daily-series-4'));
  assert.ok(getElement('trends-daily').innerHTML.includes('2026-02-03'));
});

test('token composition derives authoritative totals, residuals, and invalid states', () => {
  const exact = trendsDaily.deriveDailyComposition({
    date: '2026-08-23',
    input_tokens: 40,
    cache_read_tokens: 20,
    cache_creation_tokens: 10,
    output_tokens: 30,
    total_tokens: 100,
  });
  assert.equal(exact.status, 'ok');
  assert.equal(exact.segments[4].value, 0);

  const totalOnly = trendsDaily.deriveDailyComposition({ date: '2026-08-23', total_tokens: 120 });
  assert.equal(totalOnly.segments[4].value, 120);
  assert.equal(totalOnly.total, 120);

  const residual = trendsDaily.deriveDailyComposition({
    date: '2026-08-23', input_tokens: 40, cache_read_tokens: 20,
    cache_creation_tokens: 10, output_tokens: 30, total_tokens: 135,
    reasoning_output_tokens: 35,
  });
  assert.equal(residual.segments[4].value, 35);
  assert.equal(residual.total, 135);

  const aggregate = trendsDaily.aggregateDailyComposition([
    { date: '2026-08-22', input_tokens: 10, output_tokens: 20, total_tokens: 40 },
    { date: '2026-08-23', input_tokens: 15, output_tokens: 25, total_tokens: 60 },
  ]);
  assert.equal(aggregate.total, 100);
  assert.equal(aggregate.segments[0].value, 25);
  assert.equal(aggregate.segments[4].value, 30);

  assert.equal(trendsDaily.deriveDailyComposition({ total_tokens: 0 }).status, 'no_data');
  assert.equal(trendsDaily.deriveDailyComposition({ input_tokens: 2, total_tokens: 1 }).reason, 'known_exceeds_total');
  assert.equal(trendsDaily.deriveDailyComposition({ input_tokens: -1, total_tokens: 1 }).reason, 'invalid_channel');
  assert.equal(trendsDaily.deriveDailyComposition({ input_tokens: Number.NaN, total_tokens: 1 }).reason, 'invalid_channel');
  assert.equal(trendsDaily.deriveDailyComposition({ total_tokens: Number.POSITIVE_INFINITY }).reason, 'invalid_total');
});

test('one-day and multi-day token composition render data, bilingual copy, and quality states', () => {
  const context = {
    panels: {
      trends_daily: [
        { date: '2026-08-22', input_tokens: 10, cache_read_tokens: 20, cache_creation_tokens: 0, output_tokens: 30, total_tokens: 70, cost_with_cache_usd: 0.4 },
        { date: '2026-08-23', input_tokens: 20, cache_read_tokens: 10, cache_creation_tokens: 5, output_tokens: 35, total_tokens: 90, cost_with_cache_usd: 0.6 },
      ],
      trends_daily_support: null,
    },
  };

  copy.setLocale('zh');
  trendsDaily.renderTrendsDaily(context, { rangePreset: '1d', filters: {} });
  let html = getElement('trends-daily').innerHTML;
  assert.match(html, /daily-composition-strip/);
  assert.match(html, /其他 \/ 未细分/);
  assert.match(html, /160/);
  assert.doesNotMatch(html, /近 7 天开始显示/);

  trendsDaily.renderTrendsDaily(context, { rangePreset: '7d', filters: {} });
  html = getElement('trends-daily').innerHTML;
  assert.match(html, /daily-series-4/);
  assert.match(html, /权威总量 90/);
  assert.match(html, /tabindex="0"/);
  assert.match(html, /<svg[^>]+role="group"/);

  trendsDaily.renderTrendsDaily({
    panels: {
      trends_daily: [{ date: '2026-08-23', input_tokens: 11, total_tokens: 10 }],
      trends_daily_support: null,
    },
  }, { rangePreset: '1d', filters: {} });
  assert.match(getElement('trends-daily').innerHTML, /已知通道合计 11 大于权威总量 10/);

  trendsDaily.renderTrendsDaily({
    panels: { trends_daily: [], trends_daily_support: { level: 'degraded', reason: 'query timeout' } },
  }, { rangePreset: '1d', filters: {} });
  assert.match(getElement('trends-daily').innerHTML, /query timeout/);

  trendsDaily.renderTrendsDaily({
    panels: { trends_daily: [], trends_daily_support: { level: 'loading' } },
  }, { rangePreset: '1d', filters: {} });
  assert.match(getElement('trends-daily').innerHTML, /正在加载 Token 用量构成/);

  trendsDaily.renderTrendsDaily({
    panels: { trends_daily: [{ date: '2026-08-23', total_tokens: 0 }], trends_daily_support: null },
  }, { rangePreset: '1d', filters: {} });
  assert.match(getElement('trends-daily').innerHTML, /当前筛选范围暂无 Token 用量/);
  assert.doesNotMatch(getElement('trends-daily').innerHTML, /daily-composition-strip/);

  copy.setLocale('en');
  trendsDaily.renderTrendsDaily(context, { rangePreset: '1d', filters: {} });
  assert.match(getElement('trends-daily').innerHTML, /Other \/ unclassified/);
  assert.match(getElement('trends-daily').innerHTML, /Authoritative total/);
  copy.setLocale('zh');
});

test('session ranking uses project, agent, and time without exposing technical labels', () => {
  copy.setLocale('zh');
  const rows = [
    {
      session_id: 'codex:sess_01234567-89ab-cdef',
      session_label: 'sess_01234567-89ab-cdef',
      project_label: '很长的中文项目名称'.repeat(12),
      source: 'codex',
      first_event_at: '2026-08-23T00:00:00Z',
      last_event_at: '2026-08-23T00:30:00Z',
      total_tokens: 1_000_000_000,
      active_minutes: 30,
      cost_usd: 12.5,
      event_count: 4,
    },
    {
      session_id: 'kimi_code:01a02a4b-6901',
      session_label: '01a02a4b-6901',
      project_label: '',
      source: 'kimi_code',
      total_tokens: 250_000_000,
      active_minutes: 0,
      cost_usd: 0,
      event_count: 1,
    },
    {
      session_id: 'unknown:hash',
      session_label: 'hash',
      project_label: '重复项目',
      source: 'future_agent',
      total_tokens: 0,
      active_minutes: 0,
      cost_usd: 0,
      event_count: 0,
    },
  ];

  const prepared = topSessions.prepareSessionRows(rows, 'tokens', {
    copy: copy.UI_COPY.sessionAnalytics.topSessions,
    timeZone: 'UTC',
  });
  assert.deepEqual(prepared.map((row) => row.ratio), [1, 0.25, 0]);
  assert.match(prepared[0].subtitle, /Codex · 2026-08-23 00:00–2026-08-23 00:30/);
  assert.equal(prepared[1].title, 'Kimi Code 会话');
  assert.match(prepared[1].subtitle, /1 个事件/);
  for (const row of prepared) {
    assert.doesNotMatch(`${row.title}${row.subtitle}${row.accessibleName}`, /sess_|01a02a4b|unknown:hash/);
  }
  assert.equal(topSessions.sessionMetricRatio(0, 0), 0);
  assert.equal(topSessions.sessionMetricRatio(Number.MAX_SAFE_INTEGER, 1), 1);
  assert.deepEqual(
    topSessions.prepareSessionRows(rows, 'duration', {
      copy: copy.UI_COPY.sessionAnalytics.topSessions,
      timeZone: 'UTC',
    }).map((row) => row.ratio),
    [1, 0, 0],
  );
  assert.deepEqual(
    topSessions.prepareSessionRows(rows, 'cost', {
      copy: copy.UI_COPY.sessionAnalytics.topSessions,
      timeZone: 'UTC',
    }).map((row) => row.ratio),
    [1, 0, 0],
  );
  assert.equal(topSessions.formatSessionMetric(rows[0], 'duration'), '30m');
  assert.equal(topSessions.formatSessionMetric(rows[0], 'cost'), '$12.50');
});

test('session ranking keeps canonical ids internal and fences sort refreshes', async () => {
  class InteractiveButton {
    constructor(dataset) {
      this.dataset = dataset;
      this.listeners = new Map();
    }
    addEventListener(type, listener) { this.listeners.set(type, listener); }
    click() { this.listeners.get('click')?.(); }
  }
  const root = {
    _innerHTML: '',
    sortButtons: [],
    sessionButtons: [],
    get innerHTML() { return this._innerHTML; },
    set innerHTML(value) {
      this._innerHTML = String(value);
      this.sortButtons = [...this._innerHTML.matchAll(/data-session-sort="([^"]+)"/g)]
        .map((match) => new InteractiveButton({ sessionSort: match[1] }));
      this.sessionButtons = [...this._innerHTML.matchAll(/data-session-id="([^"]*)"/g)]
        .map((match) => new InteractiveButton({ sessionId: match[1] }));
    },
    querySelectorAll(selector) {
      if (selector === '[data-session-sort]') return this.sortButtons;
      if (selector === '[data-session-id]') return this.sessionButtons;
      return [];
    },
  };
  elementRegistry.set('top-sessions', root);
  const selected = [];
  window.dispatchEvent = (event) => selected.push(event.detail.session);
  globalThis.CustomEvent ||= class CustomEvent {
    constructor(type, init) { this.type = type; this.detail = init?.detail; }
  };

  const technicalId = 'codex:sess_private-technical-id';
  const row = {
    session_id: technicalId,
    session_label: 'sess_private-technical-id',
    project_label: '项目 Alpha',
    source: 'codex',
    first_event_at: '2026-08-23T00:00:00Z',
    last_event_at: '2026-08-23T00:05:00Z',
    total_tokens: 100,
    active_minutes: 5,
    cost_usd: 1,
    event_count: 2,
  };
  const rangeReloadController = new AbortController();
  const state = {
    topSessionsSort: 'tokens',
    reloadGeneration: 7,
    rangeReloadController,
    filters: { timezone: 'UTC' },
    rawData: { top_sessions: [row] },
  };
  topSessions.renderTopSessions({ panels: { top_sessions: [row] } }, state);
  assert.match(root.innerHTML, /data-session-id="codex:sess_private-technical-id"/);
  const visibleAndAccessible = [
    ...root.innerHTML.matchAll(/<(?:strong|small)>(.*?)<\/(?:strong|small)>/g),
    ...root.innerHTML.matchAll(/aria-label="([^"]*)"/g),
  ].map((match) => match[1]).join(' ');
  assert.doesNotMatch(visibleAndAccessible, /sess_private-technical-id/);
  assert.doesNotMatch(root.innerHTML, /title="[^"]*sess_private-technical-id/);
  root.sessionButtons[0].click();
  assert.deepEqual(selected, [technicalId]);
  assert.equal(window.location.hash, '#logs');

  let resolveDuration;
  const durationPayload = [{ ...row, total_tokens: 90, active_minutes: 20 }];
  let durationOptions;
  const durationLoad = (_state, options) => new Promise((resolve) => {
    durationOptions = options;
    resolveDuration = resolve;
  });
  const durationRequest = topSessions.changeTopSessionsSort('duration', state, durationLoad);
  assert.equal(state.topSessionsLoading, true);
  assert.equal(state.topSessionsSort, 'duration');
  assert.match(root.innerHTML, /aria-busy="true"/);
  assert.match(root.innerHTML, /data-session-sort="duration"[^>]+aria-disabled="true" disabled/);
  assert.match(root.innerHTML, /正在按“活跃时长”刷新/);
  assert.match(root.innerHTML, /top-session-value">100</);
  assert.doesNotMatch(root.innerHTML, /top-session-value">5m</);
  assert.equal(durationOptions.signal, rangeReloadController.signal);
  resolveDuration(durationPayload);
  await durationRequest;
  assert.equal(state.topSessionsLoading, false);
  assert.equal(state.topSessionsAppliedSort, 'duration');
  assert.equal(state.rawData.top_sessions, durationPayload);
  assert.match(root.innerHTML, /top-session-value">20m</);

  await topSessions.changeTopSessionsSort('cost', state, async () => { throw new Error('offline'); });
  assert.equal(state.topSessionsSort, 'duration');
  assert.equal(state.topSessionsAppliedSort, 'duration');
  assert.equal(state.rawData.top_sessions, durationPayload);
  assert.match(root.innerHTML, /已保留上一次结果/);

  let resolveStale;
  let resolveLatest;
  state.topSessionsError = null;
  state.topSessionsSort = 'tokens';
  const staleRequest = topSessions.changeTopSessionsSort(
    'duration',
    state,
    () => new Promise((resolve) => { resolveStale = resolve; }),
  );
  const latestPayload = [{ ...row, cost_usd: 99 }];
  const latestRequest = topSessions.changeTopSessionsSort(
    'cost',
    state,
    () => new Promise((resolve) => { resolveLatest = resolve; }),
  );
  resolveLatest(latestPayload);
  await latestRequest;
  resolveStale([{ ...row, active_minutes: 999 }]);
  await staleRequest;
  assert.equal(state.topSessionsSort, 'cost');
  assert.equal(state.rawData.top_sessions, latestPayload);

  let resolveSuperseded;
  const supersededRequest = topSessions.changeTopSessionsSort(
    'tokens',
    state,
    () => new Promise((resolve) => { resolveSuperseded = resolve; }),
  );
  assert.equal(state.topSessionsLoading, true);
  state.reloadGeneration += 1;
  resolveSuperseded([{ ...row, total_tokens: 999 }]);
  await supersededRequest;
  assert.equal(state.topSessionsLoading, false);
  assert.equal(state.rawData.top_sessions, latestPayload);

  state.secondaryRefreshing = true;
  topSessions.renderTopSessions({ panels: { top_sessions: latestPayload } }, state);
  assert.equal((root.innerHTML.match(/aria-disabled="true" disabled/g) || []).length, 3);
  state.secondaryRefreshing = false;
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
