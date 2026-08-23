import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const RANGES = ['1d', '7d', '30d', 'all'];
const SORTS = ['tokens', 'duration', 'cost'];
const FILTER_KEYS = ['source', 'model', 'project', 'host'];
const ROOT_KEYS = [
  'schema_version',
  'index_name',
  'commit',
  'binary_sha256',
  'warmups_per_case',
  'iterations',
  'samples',
  'summaries',
];
const SAMPLE_KEYS = [
  'shape',
  'sort',
  'status',
  'supported',
  'support_level',
  'wall_ms',
  'query_ms',
  'payload_bytes',
];
const SUMMARY_KEYS = [
  'shape',
  'sort',
  'sample_count',
  'successful_samples',
  'supported_samples',
  'p95_wall_ms',
  'p95_query_ms',
  'max_payload_bytes',
];

function takeValue(argv, index, flag) {
  const value = argv[index + 1];
  if (value === undefined || value.startsWith('--')) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

export function parseArgs(argv) {
  const options = {
    url: 'http://127.0.0.1:37422',
    iterations: 5,
    output: null,
    indexName: 'idx_usage_event_top_sessions_cover',
    filters: {},
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--url') options.url = takeValue(argv, index++, arg);
    else if (arg === '--iterations') options.iterations = Number(takeValue(argv, index++, arg));
    else if (arg === '--output') options.output = takeValue(argv, index++, arg);
    else if (arg === '--schema-version') options.schemaVersion = takeValue(argv, index++, arg);
    else if (arg === '--index-name') options.indexName = takeValue(argv, index++, arg);
    else if (arg === '--commit') options.commit = takeValue(argv, index++, arg);
    else if (arg === '--binary-sha256') options.binarySha256 = takeValue(argv, index++, arg);
    else if (arg === '--source') options.filters.source = takeValue(argv, index++, arg);
    else if (arg === '--model') options.filters.model = takeValue(argv, index++, arg);
    else if (arg === '--project') options.filters.project = takeValue(argv, index++, arg);
    else if (arg === '--host') options.filters.host = takeValue(argv, index++, arg);
    else if (arg === '--help' || arg === '-h') options.help = true;
    else throw new Error(`Unknown argument: ${arg}`);
  }
  if (options.help) return options;
  if (!Number.isInteger(options.iterations) || options.iterations < 1) {
    throw new Error('--iterations must be a positive integer');
  }
  if (!options.schemaVersion) throw new Error('--schema-version is required');
  if (!options.commit) throw new Error('--commit is required');
  if (!options.binarySha256) throw new Error('--binary-sha256 is required');
  for (const key of FILTER_KEYS) {
    if (!options.filters[key]) throw new Error(`--${key} is required`);
  }
  return options;
}

export function usage() {
  return [
    'Usage: node scripts/benchmark-top-sessions.mjs [options]',
    '',
    '  --url <url>              Running loopback llmusage dashboard URL',
    '  --iterations <count>     Samples per case after one warm-up (default: 5)',
    '  --output <path>          Optional sanitized JSON result path',
    '  --schema-version <value> Database schema version recorded in output',
    '  --index-name <value>     Index name recorded in output',
    '  --commit <value>         Binary/source commit recorded in output',
    '  --binary-sha256 <value>  Exact benchmarked binary hash',
    '  --source <value>         Exact source filter (never written to output)',
    '  --model <value>          Exact model filter (never written to output)',
    '  --project <value>        Exact project hash (never written to output)',
    '  --host <value>           Exact host id (never written to output)',
  ].join('\n');
}

export function buildCases(filters) {
  const shapes = RANGES.map((range) => ({ shape: range, params: { range } }));
  for (const key of FILTER_KEYS) {
    shapes.push({ shape: key, params: { range: 'all', [key]: filters[key] } });
  }
  return shapes.flatMap(({ shape, params }) =>
    SORTS.map((sort) => ({ shape, sort, params: { ...params, sort, limit: '50' } })),
  );
}

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)];
}

export function parseQueryTiming(raw) {
  const match = raw?.match(/(?:^|,)\s*sessions-query;dur=([0-9]+(?:\.[0-9]+)?)(?:\s|,|$)/i);
  if (!match) throw new Error('/api/sessions response is missing sessions-query Server-Timing');
  const value = Number(match[1]);
  if (!Number.isFinite(value) || value < 0) throw new Error(`invalid sessions query timing: ${match[1]}`);
  return value;
}

async function requestSample(baseUrl, benchmarkCase, fetchImpl) {
  const url = new URL('/api/sessions', baseUrl);
  for (const [key, value] of Object.entries(benchmarkCase.params)) {
    url.searchParams.set(key, value);
  }
  url.searchParams.set('benchmark', `${Date.now()}-${Math.random()}`);
  const started = performance.now();
  const response = await fetchImpl(url, { cache: 'no-store' });
  const body = new Uint8Array(await response.arrayBuffer());
  const wallMs = performance.now() - started;
  let payload;
  try {
    payload = JSON.parse(new TextDecoder().decode(body));
  } catch (error) {
    throw new Error(`${url.pathname} returned invalid JSON: ${error.message}`);
  }
  const support = payload?.support;
  return {
    shape: benchmarkCase.shape,
    sort: benchmarkCase.sort,
    status: response.status,
    supported: support?.supported === true,
    support_level: typeof support?.level === 'string' ? support.level : 'missing',
    wall_ms: Number(wallMs.toFixed(2)),
    query_ms: parseQueryTiming(response.headers.get('server-timing')),
    payload_bytes: body.byteLength,
  };
}

function summarizeCase(benchmarkCase, samples) {
  return {
    shape: benchmarkCase.shape,
    sort: benchmarkCase.sort,
    sample_count: samples.length,
    successful_samples: samples.filter((sample) => sample.status === 200).length,
    supported_samples: samples.filter((sample) => sample.supported).length,
    p95_wall_ms: percentile(samples.map((sample) => sample.wall_ms), 0.95),
    p95_query_ms: percentile(samples.map((sample) => sample.query_ms), 0.95),
    max_payload_bytes: Math.max(...samples.map((sample) => sample.payload_bytes)),
  };
}

function assertExactKeys(value, expected, label) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw new Error(`${label} has non-allowlisted keys: ${actual.join(', ')}`);
  }
}

export function assertSanitizedResult(result) {
  assertExactKeys(result, ROOT_KEYS, 'benchmark result');
  result.samples.forEach((sample, index) => assertExactKeys(sample, SAMPLE_KEYS, `sample ${index}`));
  result.summaries.forEach((summary, index) => assertExactKeys(summary, SUMMARY_KEYS, `summary ${index}`));
}

export async function benchmarkTopSessions(options, fetchImpl = fetch) {
  const cases = buildCases(options.filters);
  const samples = [];
  const summaries = [];
  for (const benchmarkCase of cases) {
    await requestSample(options.url, benchmarkCase, fetchImpl);
    const caseSamples = [];
    for (let index = 0; index < options.iterations; index += 1) {
      caseSamples.push(await requestSample(options.url, benchmarkCase, fetchImpl));
    }
    samples.push(...caseSamples);
    summaries.push(summarizeCase(benchmarkCase, caseSamples));
  }
  const result = {
    schema_version: options.schemaVersion,
    index_name: options.indexName,
    commit: options.commit,
    binary_sha256: options.binarySha256,
    warmups_per_case: 1,
    iterations: options.iterations,
    samples,
    summaries,
  };
  assertSanitizedResult(result);
  return result;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(usage());
    return;
  }
  const result = await benchmarkTopSessions(options);
  const json = `${JSON.stringify(result, null, 2)}\n`;
  if (options.output) {
    await mkdir(dirname(resolve(options.output)), { recursive: true });
    await writeFile(options.output, json, 'utf8');
  }
  console.log(json);
}

const isMain = process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href;
if (isMain) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
