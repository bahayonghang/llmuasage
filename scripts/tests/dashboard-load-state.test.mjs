import assert from 'node:assert/strict';
import test from 'node:test';

import {
  SECONDARY_SECTIONS,
  armDashboardCoreDeadline,
  classifyDashboardError,
  createDashboardLoadState,
  dashboardLocaleRenderMode,
  isDegradedSectionPayload,
  reduceDashboardLoadState,
  runLoadersWithConcurrency,
} from '../../src/web/assets/load-state.js';

test('core deadline enters slow at 2s and aborts at 6s', () => {
  const timers = [];
  const events = [];
  const cancel = armDashboardCoreDeadline({
    setTimer(callback, delay) {
      const timer = { callback, delay, cleared: false };
      timers.push(timer);
      return timer;
    },
    clearTimer(timer) { timer.cleared = true; },
    onSlow() { events.push('slow'); },
    onTimeout() { events.push('timeout'); },
  });
  assert.deepEqual(timers.map((timer) => timer.delay), [2000, 6000]);
  timers[0].callback();
  assert.deepEqual(events, ['slow']);
  timers[1].callback();
  assert.deepEqual(events, ['slow', 'timeout']);
  cancel();
  assert.ok(timers.every((timer) => timer.cleared));
});

test('dashboard load reducer tracks slow core and exact secondary progress', () => {
  let state = createDashboardLoadState(7, 100);
  state = reduceDashboardLoadState(state, { type: 'slow', generation: 7 });
  assert.equal(state.phase, 'core_pending');
  assert.equal(state.slow, true);

  state = reduceDashboardLoadState(state, { type: 'core_succeeded', generation: 7 });
  assert.equal(state.phase, 'secondary_loading');
  assert.equal(state.secondarySettled, 0);

  for (const [index, section] of SECONDARY_SECTIONS.entries()) {
    state = reduceDashboardLoadState(state, {
      type: 'secondary_settled',
      generation: 7,
      section,
      degraded: section === 'optimize',
    });
    assert.equal(state.secondarySettled, index + 1);
  }
  assert.equal(state.phase, 'complete');
  assert.equal(state.secondaryDegraded, 1);
  assert.deepEqual(state.degradedSections, ['optimize']);

  const duplicate = reduceDashboardLoadState(state, {
    type: 'secondary_settled',
    generation: 7,
    section: 'optimize',
    degraded: true,
  });
  assert.equal(duplicate, state, 'a section settles at most once');
});

test('five secondary loaders use concurrency two and all settle after one rejects', async () => {
  let active = 0;
  let maxActive = 0;
  const seen = [];
  const loaders = Object.fromEntries(SECONDARY_SECTIONS.map((section) => [section, async () => {
    active += 1;
    maxActive = Math.max(maxActive, active);
    await new Promise((resolve) => setImmediate(resolve));
    active -= 1;
    if (section === 'tools') throw new Error('tools failed');
    return { section };
  }]));

  await runLoadersWithConcurrency(loaders, 2, (section, payload, error) => {
    seen.push({ section, payload, error });
  });
  assert.equal(maxActive, 2);
  assert.deepEqual(seen.map((entry) => entry.section).sort(), [...SECONDARY_SECTIONS].sort());
  assert.equal(seen.find((entry) => entry.section === 'tools').error.message, 'tools failed');
});

test('secondary result callback failures are not retried as loader failures', async () => {
  let callbackCalls = 0;
  await assert.rejects(
    runLoadersWithConcurrency(
      { activity: async () => ({ support: { level: 'ready' } }) },
      2,
      async () => {
        callbackCalls += 1;
        throw new Error('render failed');
      },
    ),
    /render failed/,
  );
  assert.equal(callbackCalls, 1);
});

test('stale generations cannot overwrite a retry', () => {
  const initial = createDashboardLoadState(1);
  const retry = reduceDashboardLoadState(initial, { type: 'retry', generation: 2 });
  const stale = reduceDashboardLoadState(retry, { type: 'failed', generation: 1, errorKind: 'http' });
  assert.equal(stale, retry);
  assert.equal(stale.phase, 'core_pending');
});

test('locale rendering follows current retry state instead of a captured initial error', () => {
  const state = {
    rawData: null,
    loadState: reduceDashboardLoadState(createDashboardLoadState(1), {
      type: 'failed',
      generation: 1,
      errorKind: 'network',
    }),
  };
  assert.equal(dashboardLocaleRenderMode(state), 'error');

  state.loadState = reduceDashboardLoadState(state.loadState, { type: 'retry', generation: 2 });
  state.loadState = reduceDashboardLoadState(state.loadState, { type: 'core_succeeded', generation: 2 });
  state.rawData = { overview: {} };
  assert.equal(dashboardLocaleRenderMode(state), 'data');
});

test('error classification and degraded detection are explicit', () => {
  assert.equal(classifyDashboardError(new Error('offline')), 'network');
  assert.equal(classifyDashboardError(Object.assign(new Error('bad'), { status: 503 })), 'http');
  assert.equal(classifyDashboardError(new SyntaxError('json')), 'parse');
  assert.equal(classifyDashboardError(new DOMException('aborted', 'AbortError')), 'cancelled');
  assert.equal(classifyDashboardError(new Error('late'), true), 'timeout');
  assert.equal(isDegradedSectionPayload({ support: { level: 'degraded' } }), true);
  assert.equal(isDegradedSectionPayload({ support: { level: 'no_data', supported: false } }), false);
});

test('duplicate reload reuses the active promise without aborting its controller', async () => {
  globalThis.window = {
    console: { info() {}, warn() {}, error() {} },
    location: { origin: 'http://127.0.0.1', search: '', pathname: '/', hash: '' },
    localStorage: null,
    matchMedia: () => ({ matches: false }),
    __LLMUSAGE_BOOTSTRAP__: { claim() {} },
  };
  globalThis.document = {
    readyState: 'loading',
    body: { dataset: { mode: 'live', appVersion: 'test' } },
    documentElement: {
      lang: 'en',
      dataset: { locale: 'en' },
      getAttribute() { return 'light'; },
      setAttribute() {},
    },
    addEventListener() {},
    getElementById() { return null; },
    querySelectorAll() { return []; },
  };

  const app = await import(`../../src/web/assets/app.js?reload-race=${Date.now()}`);
  let aborts = 0;
  const state = {
    reloadPromise: Promise.resolve('existing'),
    rangeReloadController: { abort() { aborts += 1; } },
  };
  assert.equal(await app.reloadDashboard(state), 'existing');
  assert.equal(aborts, 0);
});
