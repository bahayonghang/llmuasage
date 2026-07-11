import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

globalThis.window = {
  console: { info() {}, warn() {}, error() {} },
  location: { origin: 'http://127.0.0.1' },
};

const source = await readFile(new URL('../../src/web/assets/data/fetch.js', import.meta.url), 'utf8');
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`;
const dashboardFetch = await import(moduleUrl);

function response(payload) {
  return {
    ok: true,
    async json() { return payload; },
  };
}

function abortError() {
  return new DOMException('The operation was aborted.', 'AbortError');
}

test('live dashboard request lifecycle', async (t) => {
  await t.test('coalesces normalized in-flight requests', async () => {
    dashboardFetch.clearLiveRequestCache();
    let calls = 0;
    let release;
    globalThis.fetch = (_path, { signal } = {}) => {
      calls += 1;
      return new Promise((resolve, reject) => {
        release = () => resolve(response({ overview: { calls } }));
        signal?.addEventListener('abort', () => reject(abortError()), { once: true });
      });
    };

    const state = { mode: 'live', rangePreset: '7d', trendWindow: 'week', filters: {} };
    const first = dashboardFetch.loadDashboardInteractiveSnapshot(state);
    const second = dashboardFetch.loadDashboardInteractiveSnapshot(state);
    assert.equal(calls, 1);
    release();
    assert.deepEqual(await first, await second);
  });

  await t.test('propagates AbortSignal and cache invalidation aborts in-flight work', async () => {
    dashboardFetch.clearLiveRequestCache();
    let aborts = 0;
    globalThis.fetch = (_path, { signal } = {}) => new Promise((_resolve, reject) => {
      signal?.addEventListener('abort', () => {
        aborts += 1;
        reject(abortError());
      }, { once: true });
    });

    const state = { mode: 'live', rangePreset: '30d', trendWindow: 'month', filters: {} };
    const controller = new AbortController();
    const signalled = dashboardFetch.loadDashboardInteractiveSnapshot(state, { signal: controller.signal });
    controller.abort();
    await assert.rejects(signalled, { name: 'AbortError' });

    const invalidated = dashboardFetch.loadDashboardInteractiveSnapshot({ ...state, rangePreset: 'all' });
    dashboardFetch.clearLiveRequestCache();
    await assert.rejects(invalidated, { name: 'AbortError' });
    assert.equal(aborts, 2);
  });

  await t.test('bounds the live response cache to 32 entries', async () => {
    dashboardFetch.clearLiveRequestCache();
    let calls = 0;
    globalThis.fetch = async (path) => {
      calls += 1;
      return response({ path, calls });
    };
    const state = { mode: 'live', filters: {} };
    for (let index = 0; index < 33; index += 1) {
      await dashboardFetch.loadSection(state, `section-${index}`, `/api/test/${index}`);
    }
    await dashboardFetch.loadSection(state, 'section-0', '/api/test/0');
    assert.equal(calls, 34, 'the oldest entry should be evicted after the 33rd unique request');
  });
});
