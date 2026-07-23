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
  assert.equal(getElement('activity-support').textContent, 'normalized');
  assert.ok(getElement('activity-table').innerHTML.includes('coding'));

  resetMutations();
  behavior.renderTools(context);
  assert.deepEqual(mutatedIds().sort(), ['tools-bars', 'tools-support', 'tools-table']);
  assert.equal(getElement('tools-support').textContent, 'no_data');
  assert.ok(getElement('tools-table').innerHTML.includes('Read'));

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
  assert.equal(getElement('activity-support').textContent, 'refreshing');
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
