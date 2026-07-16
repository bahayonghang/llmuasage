import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { existsSync } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

const RANGE_WINDOWS = { '1d': 'day', '7d': 'week', '30d': 'month', all: 'all' };

function parseArgs(argv) {
  const options = { url: 'http://127.0.0.1:37422', iterations: 5, output: null, chrome: null };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--url') options.url = argv[++index];
    else if (arg === '--iterations') options.iterations = Number(argv[++index]);
    else if (arg === '--output') options.output = argv[++index];
    else if (arg === '--chrome') options.chrome = argv[++index];
    else if (arg === '--help' || arg === '-h') options.help = true;
    else throw new Error(`Unknown argument: ${arg}`);
  }
  if (!Number.isInteger(options.iterations) || options.iterations < 1) {
    throw new Error('--iterations must be a positive integer');
  }
  return options;
}

function usage() {
  return [
    'Usage: node scripts/benchmark-dashboard-range.mjs [options]',
    '',
    '  --url <url>            Running llmusage dashboard URL',
    '  --iterations <count>   HTTP samples per range after one warm-up (default: 5)',
    '  --output <path>        Optional JSON result path',
    '  --chrome <path>        Chrome/Edge executable override',
  ].join('\n');
}

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)];
}

function summarize(samples) {
  const durations = samples.map((sample) => sample.duration_ms);
  const bytes = samples.map((sample) => sample.bytes);
  return {
    median_ms: percentile(durations, 0.5),
    p95_ms: percentile(durations, 0.95),
    median_bytes: percentile(bytes, 0.5),
    max_bytes: Math.max(...bytes),
    samples,
  };
}

async function requestSample(baseUrl, range) {
  const url = new URL('/api/dashboard', baseUrl);
  url.searchParams.set('scope', 'interactive');
  url.searchParams.set('range', range);
  url.searchParams.set('window', RANGE_WINDOWS[range]);
  url.searchParams.set('benchmark', `${Date.now()}-${Math.random()}`);
  const started = performance.now();
  const response = await fetch(url, { cache: 'no-store' });
  const body = new Uint8Array(await response.arrayBuffer());
  if (!response.ok) {
    throw new Error(`${url.pathname} returned ${response.status}: ${new TextDecoder().decode(body)}`);
  }
  return {
    duration_ms: Number((performance.now() - started).toFixed(2)),
    bytes: body.byteLength,
  };
}

async function benchmarkHttp(baseUrl, iterations) {
  const results = {};
  for (const range of Object.keys(RANGE_WINDOWS)) {
    await requestSample(baseUrl, range);
    const samples = [];
    for (let index = 0; index < iterations; index += 1) {
      samples.push(await requestSample(baseUrl, range));
    }
    results[range] = summarize(samples);
  }
  return results;
}

function findChrome(override) {
  const candidates = [
    override,
    process.env.CHROME_PATH,
    'C:/Program Files/Google/Chrome/Application/chrome.exe',
    'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
    'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
    'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  ].filter(Boolean);
  const executable = candidates.find(existsSync);
  if (!executable) {
    throw new Error('Chrome or Edge was not found; pass --chrome or set CHROME_PATH');
  }
  return executable;
}

class CdpClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.events = new Map();
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(message.error.message));
        else pending.resolve(message.result);
        return;
      }
      const listeners = this.events.get(message.method) || [];
      listeners.forEach((listener) => listener(message.params));
    });
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  once(method) {
    return new Promise((resolve) => {
      const listener = (params) => {
        const listeners = this.events.get(method) || [];
        this.events.set(method, listeners.filter((entry) => entry !== listener));
        resolve(params);
      };
      this.events.set(method, [...(this.events.get(method) || []), listener]);
    });
  }
}

async function connectCdp(webSocketUrl) {
  const socket = new WebSocket(webSocketUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  return new CdpClient(socket);
}

async function launchBrowser(chromePath) {
  const profile = await mkdtemp(join(tmpdir(), 'llmusage-range-benchmark-'));
  const child = spawn(chromePath, [
    '--headless=new',
    '--disable-gpu',
    '--disable-background-networking',
    '--no-first-run',
    '--no-default-browser-check',
    '--remote-debugging-port=0',
    `--user-data-dir=${profile}`,
    'about:blank',
  ], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
  const webSocketUrl = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Timed out waiting for Chrome DevTools')), 10000);
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => {
      const match = chunk.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    child.once('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`Chrome exited before DevTools was ready (code ${code})`));
    });
  });
  return { child, profile, webSocketUrl };
}

async function pageTarget(browserWebSocketUrl) {
  const endpoint = new URL(browserWebSocketUrl);
  const listUrl = `http://${endpoint.host}/json/list`;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const targets = await fetch(listUrl).then((response) => response.json());
    const page = targets.find((target) => target.type === 'page');
    if (page) return page.webSocketDebuggerUrl;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error('Chrome did not expose a page target');
}

function browserClickBenchmarkExpression() {
  return `
    (async () => {
      const presets = ['1d', '7d', '30d', 'all'];
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (read, timeoutMs, label) => {
        const started = performance.now();
        while (performance.now() - started < timeoutMs) {
          const value = read();
          if (value) return value;
          await sleep(16);
        }
        throw new Error('Timed out waiting for ' + label);
      };
      const apiEntries = (started) => performance.getEntriesByType('resource')
        .filter((entry) => entry.startTime >= started && entry.name.includes('/api/'));
      const longTasks = [];
      let longTaskObserver = null;
      if (PerformanceObserver.supportedEntryTypes?.includes('longtask')) {
        longTaskObserver = new PerformanceObserver((list) => {
          for (const entry of list.getEntries()) {
            longTasks.push({ start_ms: entry.startTime, duration_ms: entry.duration });
          }
        });
        longTaskObserver.observe({ type: 'longtask', buffered: true });
      }

      await waitFor(
        () => document.querySelector('[data-range-preset="1d"]') && !document.querySelector('.stale-refresh-notice'),
        20000,
        'initial dashboard render',
      );

      const clicks = [];
      for (const preset of presets) {
        performance.clearResourceTimings();
        const button = document.querySelector('[data-range-preset="' + preset + '"]');
        const trendHost = document.getElementById('trends-table');
        let renderAt = null;
        const mutation = new MutationObserver(() => {
          if (renderAt === null) renderAt = performance.now();
        });
        mutation.observe(trendHost, { childList: true, subtree: true, characterData: true });
        const started = performance.now();
        button.click();
        const feedbackAt = await waitFor(
          () => button.getAttribute('aria-pressed') === 'true' && performance.now(),
          1000,
          preset + ' click feedback',
        );
        const interactive = await waitFor(
          () => apiEntries(started).find((entry) => entry.name.includes('scope=interactive') && entry.responseEnd > 0),
          15000,
          preset + ' interactive response',
        );
        await waitFor(() => renderAt, 5000, preset + ' critical render');
        const secondaryAt = await waitFor(
          () => !document.querySelector('.stale-refresh-notice') && performance.now(),
          20000,
          preset + ' secondary completion',
        );
        mutation.disconnect();
        const resources = apiEntries(started);
        clicks.push({
          preset,
          feedback_ms: Number((feedbackAt - started).toFixed(2)),
          interactive_response_ms: Number((interactive.responseEnd - started).toFixed(2)),
          critical_render_ms: Number((renderAt - started).toFixed(2)),
          secondary_complete_ms: Number((secondaryAt - started).toFixed(2)),
          request_count: resources.length,
          payload_bytes: resources.reduce((sum, entry) => sum + (entry.encodedBodySize || entry.decodedBodySize || 0), 0),
        });
      }

      longTaskObserver?.disconnect();
      return {
        clicks,
        long_tasks: longTasks,
      };
    })()
  `;
}

function rapidBrowserBenchmarkExpression() {
  return `
    (async () => {
      const presets = ['1d', '7d', '30d', 'all'];
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (read, timeoutMs, label) => {
        const started = performance.now();
        while (performance.now() - started < timeoutMs) {
          const value = read();
          if (value) return value;
          await sleep(16);
        }
        throw new Error('Timed out waiting for ' + label);
      };
      await waitFor(
        () => document.querySelector('[data-range-preset="1d"]') && !document.querySelector('.stale-refresh-notice'),
        20000,
        'initial dashboard render',
      );
      performance.clearResourceTimings();
      const started = performance.now();
      let finalClickAt = started;
      for (const preset of presets) {
        finalClickAt = performance.now();
        document.querySelector('[data-range-preset="' + preset + '"]').click();
        if (preset !== 'all') await sleep(40);
      }
      const feedbackAt = await waitFor(
        () => document.querySelector('[data-range-preset="all"]').getAttribute('aria-pressed') === 'true' && performance.now(),
        1000,
        'rapid latest-wins feedback',
      );
      const latestResponse = await waitFor(
        () => performance.getEntriesByType('resource').find((entry) =>
          entry.name.includes('/api/dashboard') &&
          entry.name.includes('scope=interactive') &&
          entry.name.includes('range=all') &&
          entry.responseEnd > 0
        ),
        15000,
        'rapid latest interactive response',
      );
      const completeAt = await waitFor(
        () => !document.querySelector('.stale-refresh-notice') && performance.now(),
        20000,
        'rapid secondary completion',
      );
      const resources = performance.getEntriesByType('resource')
        .filter((entry) => entry.startTime >= started && entry.name.includes('/api/'));
      return {
        latest_feedback_ms: Number((feedbackAt - finalClickAt).toFixed(2)),
        latest_response_ms: Number((latestResponse.responseEnd - finalClickAt).toFixed(2)),
        complete_ms: Number((completeAt - finalClickAt).toFixed(2)),
        active_preset: document.querySelector('[data-range-preset][aria-pressed="true"]')?.dataset?.rangePreset || null,
        request_count: resources.length,
        interactive_requests: resources.filter((entry) => entry.name.includes('scope=interactive')).length,
        payload_bytes: resources.reduce((sum, entry) => sum + (entry.encodedBodySize || entry.decodedBodySize || 0), 0),
      };
    })()
  `;
}

async function benchmarkBrowser(baseUrl, chromeOverride) {
  const chromePath = findChrome(chromeOverride);
  const browser = await launchBrowser(chromePath);
  let client = null;
  try {
    const targetUrl = await pageTarget(browser.webSocketUrl);
    client = await connectCdp(targetUrl);
    await Promise.all([
      client.send('Page.enable'),
      client.send('Runtime.enable'),
      client.send('Network.enable'),
    ]);
    const loaded = client.once('Page.loadEventFired');
    await client.send('Page.navigate', { url: baseUrl });
    await loaded;
    const evaluated = await client.send('Runtime.evaluate', {
      expression: browserClickBenchmarkExpression(),
      awaitPromise: true,
      returnByValue: true,
    });
    if (evaluated.exceptionDetails) {
      throw new Error(evaluated.exceptionDetails.exception?.description || 'Browser benchmark failed');
    }
    const reloaded = client.once('Page.loadEventFired');
    await client.send('Page.reload', { ignoreCache: true });
    await reloaded;
    const rapid = await client.send('Runtime.evaluate', {
      expression: rapidBrowserBenchmarkExpression(),
      awaitPromise: true,
      returnByValue: true,
    });
    if (rapid.exceptionDetails) {
      throw new Error(rapid.exceptionDetails.exception?.description || 'Rapid benchmark failed');
    }
    return {
      chrome: chromePath,
      ...evaluated.result.value,
      rapid_switch: rapid.result.value,
    };
  } finally {
    await client?.send('Browser.close').catch(() => {});
    if (browser.child.exitCode === null) {
      browser.child.kill();
      await Promise.race([
        once(browser.child, 'exit'),
        new Promise((resolve) => setTimeout(resolve, 3000)),
      ]);
    }
    await rm(browser.profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
}

const options = parseArgs(process.argv.slice(2));
if (options.help) {
  console.log(usage());
  process.exit(0);
}

const result = {
  generated_at: new Date().toISOString(),
  url: options.url,
  budgets: { click_feedback_p95_ms: 100, interactive_api_p95_ms: 400, interactive_bytes: 128 * 1024 },
  http: await benchmarkHttp(options.url, options.iterations),
  browser: await benchmarkBrowser(options.url, options.chrome),
};

const json = `${JSON.stringify(result, null, 2)}\n`;
if (options.output) {
  await mkdir(dirname(options.output), { recursive: true });
  await writeFile(options.output, json, 'utf8');
}
console.log(json);
