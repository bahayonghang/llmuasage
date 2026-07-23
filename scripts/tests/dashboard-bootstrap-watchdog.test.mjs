import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../src/web/assets/bootstrap-watchdog.js', import.meta.url), 'utf8');

function harness({ mode = 'live', fetchImpl = async () => ({ ok: true }) } = {}) {
  let now = 0;
  let nextTimer = 1;
  let reloads = 0;
  const timers = new Map();
  const listeners = new Map();
  const elements = new Map([
    ['sync-command-center', { innerHTML: '' }],
    ['status-panel', { innerHTML: '' }],
  ]);
  const documentListeners = new Map();
  const document = {
    body: { dataset: { mode } },
    documentElement: { dataset: { locale: 'en' } },
    getElementById(id) { return elements.get(id) || null; },
    addEventListener(type, listener) { documentListeners.set(type, listener); },
  };
  const window = {
    document,
    fetch: fetchImpl,
    location: { reload() { reloads += 1; } },
    setTimeout(callback, delay) {
      const id = nextTimer++;
      timers.set(id, { at: now + delay, callback });
      return id;
    },
    clearTimeout(id) { timers.delete(id); },
    addEventListener(type, listener) {
      const entries = listeners.get(type) || [];
      entries.push(listener);
      listeners.set(type, entries);
    },
    removeEventListener(type, listener) {
      listeners.set(type, (listeners.get(type) || []).filter((entry) => entry !== listener));
    },
  };
  window.window = window;
  const context = vm.createContext({ window, document, AbortController, console, URL });
  vm.runInContext(source, context);

  return {
    window,
    elements,
    listeners,
    get reloads() { return reloads; },
    async advance(milliseconds) {
      const target = now + milliseconds;
      while (true) {
        const due = [...timers.entries()]
          .filter(([, timer]) => timer.at <= target)
          .sort((left, right) => left[1].at - right[1].at)[0];
        if (!due) break;
        timers.delete(due[0]);
        now = due[1].at;
        due[1].callback();
        await Promise.resolve();
        await Promise.resolve();
      }
      now = target;
      await Promise.resolve();
      await Promise.resolve();
    },
    dispatch(type, event) {
      for (const listener of listeners.get(type) || []) listener(event);
    },
    clickRetry() {
      documentListeners.get('click')?.({
        target: { closest(selector) { return selector === '[data-dashboard-retry]' ? this : null; } },
      });
    },
  };
}

test('unclaimed module graph reaches a terminal bootstrap error', async () => {
  const page = harness();
  await page.advance(3000);
  assert.match(page.elements.get('sync-command-center').innerHTML, /page application did not start/i);
  assert.match(page.elements.get('sync-command-center').innerHTML, /data-dashboard-retry/);
});

test('claim and ready cancel both watchdog deadlines', async () => {
  const page = harness();
  page.window.__LLMUSAGE_BOOTSTRAP__.claim();
  page.window.__LLMUSAGE_BOOTSTRAP__.ready();
  await page.advance(5000);
  assert.equal(page.window.__LLMUSAGE_BOOTSTRAP__.state(), 'ready');
  assert.equal(page.elements.get('sync-command-center').innerHTML, '');
});

test('claimed startup exception and ready timeout are terminal', async () => {
  const page = harness();
  page.window.__LLMUSAGE_BOOTSTRAP__.claim();
  page.dispatch('unhandledrejection', { reason: new Error('startup exploded') });
  await page.advance(0);
  assert.match(page.elements.get('sync-command-center').innerHTML, /startup exploded/);

  const timeout = harness();
  timeout.window.__LLMUSAGE_BOOTSTRAP__.claim();
  await timeout.advance(1000);
  assert.match(timeout.elements.get('sync-command-center').innerHTML, /page application did not start/i);
});

test('probe failure reports service unavailable while snapshot never probes', async () => {
  let calls = 0;
  const live = harness({ fetchImpl: async () => { calls += 1; throw new Error('refused'); } });
  await live.advance(3000);
  assert.equal(calls, 1);
  assert.match(live.elements.get('sync-command-center').innerHTML, /local service unavailable/i);

  const snapshot = harness({ mode: 'snapshot', fetchImpl: async () => { calls += 1; return { ok: true }; } });
  await snapshot.advance(3000);
  assert.equal(calls, 1, 'snapshot watchdog must not probe a local service');
  assert.match(snapshot.elements.get('sync-command-center').innerHTML, /offline page resources/i);
});

test('a hanging root probe times out and Retry only reloads the page', async () => {
  const page = harness({
    fetchImpl: async (_path, { signal }) => new Promise((_resolve, reject) => {
      signal.addEventListener('abort', () => reject(new DOMException('timed out', 'AbortError')), { once: true });
    }),
  });
  await page.advance(3000);
  assert.equal(page.elements.get('sync-command-center').innerHTML, '', 'probe is still pending');
  await page.advance(1500);
  assert.match(page.elements.get('sync-command-center').innerHTML, /local service unavailable/i);
  page.clickRetry();
  assert.equal(page.reloads, 1);
});

test('late module handoff can take ownership after watchdog output', async () => {
  const page = harness();
  await page.advance(3000);
  assert.equal(page.window.__LLMUSAGE_BOOTSTRAP__.state(), 'failed');
  page.window.__LLMUSAGE_BOOTSTRAP__.claim();
  page.window.__LLMUSAGE_BOOTSTRAP__.ready();
  assert.equal(page.window.__LLMUSAGE_BOOTSTRAP__.state(), 'ready');
});
