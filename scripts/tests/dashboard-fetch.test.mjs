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
  await t.test('adds the browser IANA timezone unless an explicit timezone is set', async () => {
    dashboardFetch.clearLiveRequestCache();
    const originalIntl = globalThis.Intl;
    globalThis.Intl = {
      ...originalIntl,
      DateTimeFormat: () => ({ resolvedOptions: () => ({ timeZone: 'America/New_York' }) }),
    };
    const paths = [];
    globalThis.fetch = async (path) => {
      paths.push(path);
      return response({});
    };

    try {
      const state = { mode: 'live', rangePreset: '7d', trendWindow: 'week', filters: {} };
      await dashboardFetch.loadDashboardInteractiveSnapshot(state);
      assert.equal(
        new URL(paths[0], window.location.origin).searchParams.get('timezone'),
        'America/New_York',
      );

      dashboardFetch.clearLiveRequestCache();
      await dashboardFetch.loadDashboardInteractiveSnapshot({
        ...state,
        filters: { timezone: 'UTC+8' },
      });
      assert.equal(
        new URL(paths[1], window.location.origin).searchParams.get('timezone'),
        'UTC+8',
      );
    } finally {
      globalThis.Intl = originalIntl;
    }
  });
  await t.test('interactive bootstrap failure does not fan out to legacy endpoints', async () => {
    dashboardFetch.clearLiveRequestCache();
    const paths = [];
    globalThis.fetch = async (path) => {
      paths.push(path);
      throw new TypeError('connection refused');
    };

    const state = { mode: 'live', rangePreset: '1d', trendWindow: 'day', filters: {} };
    await assert.rejects(
      dashboardFetch.loadDashboardInteractiveSnapshot(state, { legacyFallback: false }),
      /connection refused/,
    );
    assert.equal(paths.length, 1);
    assert.match(paths[0], /^\/api\/dashboard\?/);
    assert.match(paths[0], /scope=interactive/);
  });

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


test('ready-widget fetchers preserve shared filtering and snapshot compatibility', async (t) => {
  await t.test('home overview, heatmap, and daily trends use filtered live requests', async () => {
    dashboardFetch.clearLiveRequestCache();
    const paths = [];
    globalThis.fetch = async (path, { signal } = {}) => {
      paths.push({ path, signal });
      return response(path.includes('home_overview') ? { summary: {} } : []);
    };
    const state = {
      mode: 'live',
      rangePreset: '7d',
      trendWindow: 'week',
      filters: { source: 'codex', timezone: 'Asia/Shanghai' },
      topSessionsSort: 'duration',
    };
    const controller = new AbortController();

    await dashboardFetch.fetchHomeOverview(state, { signal: controller.signal });
    await dashboardFetch.fetchHeatmap(state, { signal: controller.signal });
    await dashboardFetch.fetchTrendsDaily(state, { signal: controller.signal });
    await dashboardFetch.fetchTopSessions(state, { signal: controller.signal });
    await dashboardFetch.fetchHourOfWeek(state, { signal: controller.signal });

    assert.deepEqual(paths.map(({ path }) => new URL(path, window.location.origin).pathname), [
      '/api/home_overview',
      '/api/heatmap',
      '/api/trends_daily',
      '/api/sessions',
      '/api/hour_of_week',
    ]);
    for (const { path, signal } of paths) {
      const params = new URL(path, window.location.origin).searchParams;
      assert.equal(params.get('source'), 'codex');
      assert.equal(params.get('timezone'), 'Asia/Shanghai');
      assert.ok(signal instanceof AbortSignal);
    }
    const homeOverviewParams = new URL(paths[0].path, window.location.origin).searchParams;
    assert.equal(homeOverviewParams.get('compact'), 'true');
    assert.equal(new URL(paths[1].path, window.location.origin).searchParams.get('compact'), null);
    assert.equal(new URL(paths[1].path, window.location.origin).searchParams.get('days'), '7');
    assert.equal(new URL(paths[3].path, window.location.origin).searchParams.get('sort'), 'duration');
  });

  await t.test('old snapshots without ready-widget keys return empty states', async () => {
    let requests = 0;
    globalThis.fetch = async () => {
      requests += 1;
      return response({});
    };
    const state = { mode: 'snapshot', snapshot: {} };
    assert.equal(await dashboardFetch.fetchHomeOverview(state), null);
    assert.deepEqual(await dashboardFetch.fetchHeatmap(state), []);
    assert.deepEqual(await dashboardFetch.fetchTrendsDaily(state), []);
    assert.deepEqual(await dashboardFetch.fetchTopSessions(state), []);
    assert.deepEqual(await dashboardFetch.fetchHourOfWeek(state), []);
    const startup = await dashboardFetch.loadDashboardSnapshot(state);
    assert.equal(startup.home_overview, null);
    assert.deepEqual(startup.heatmap, []);
    assert.deepEqual(startup.trends_daily, []);
    assert.deepEqual(startup.top_sessions, []);
    assert.deepEqual(startup.hour_of_week, []);
    assert.equal(requests, 0);
  });
});

test('event log fetching keeps the compact page contract', async (t) => {
  await t.test('first live page requests 20 records with shared filters', async () => {
    dashboardFetch.clearLiveRequestCache();
    assert.equal(dashboardFetch.LOGS_PAGE_SIZE, 20);
    const paths = [];
    globalThis.fetch = async (path) => {
      paths.push(path);
      return response({ records: [], next_cursor: null });
    };
    const state = {
      mode: 'live',
      rangePreset: '7d',
      trendWindow: 'week',
      filters: { source: 'codex', timezone: 'Asia/Shanghai' },
    };

    await dashboardFetch.fetchLogs(state, { cache: false });

    const url = new URL(paths[0], window.location.origin);
    assert.equal(url.pathname, '/api/logs');
    assert.equal(url.searchParams.get('page_size'), '20');
    assert.equal(url.searchParams.get('source'), 'codex');
    assert.equal(url.searchParams.get('range'), '7d');
    assert.equal(url.searchParams.get('timezone'), 'Asia/Shanghai');
  });

  await t.test('session, cursor, and event-key params survive pagination', async () => {
    dashboardFetch.clearLiveRequestCache();
    const paths = [];
    globalThis.fetch = async (path) => {
      paths.push(path);
      return response({ records: [], next_cursor: null });
    };
    const state = { mode: 'live', rangePreset: '30d', trendWindow: 'month', filters: {} };

    await dashboardFetch.fetchLogs(state, { session: 'sess-1', cursor: 'CURSOR123', cache: false });
    await dashboardFetch.fetchLogs(state, { eventKey: 'codex:abc-123', cache: false });

    const paged = new URL(paths[0], window.location.origin).searchParams;
    assert.equal(paged.get('page_size'), '20');
    assert.equal(paged.get('session'), 'sess-1');
    assert.equal(paged.get('cursor'), 'CURSOR123');
    assert.equal(paged.get('event_key'), null);

    const detail = new URL(paths[1], window.location.origin).searchParams;
    assert.equal(detail.get('event_key'), 'codex:abc-123');
    assert.equal(detail.get('cursor'), null);
  });

  await t.test('snapshot mode returns empty logs without network calls', async () => {
    let requests = 0;
    globalThis.fetch = async () => {
      requests += 1;
      return response({});
    };
    const state = { mode: 'snapshot', snapshot: {} };
    assert.deepEqual(await dashboardFetch.fetchLogs(state), { records: [], next_cursor: null });
    assert.equal(requests, 0);
  });
});
