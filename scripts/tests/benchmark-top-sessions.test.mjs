import assert from 'node:assert/strict';
import test from 'node:test';

import {
  benchmarkTopSessions,
  buildCases,
  parseArgs,
  parseQueryTiming,
} from '../benchmark-top-sessions.mjs';

const FILTERS = {
  source: 'private-source-value',
  model: 'private-model-value',
  project: 'private-project-hash',
  host: 'private-host-id',
};

test('builds the required 24-case range, filter, and sort matrix', () => {
  const cases = buildCases(FILTERS);
  assert.equal(cases.length, 24);
  assert.deepEqual(
    [...new Set(cases.map(({ shape }) => shape))],
    ['1d', '7d', '30d', 'all', 'source', 'model', 'project', 'host'],
  );
  for (const shape of ['1d', '7d', '30d', 'all', 'source', 'model', 'project', 'host']) {
    assert.deepEqual(
      cases.filter((entry) => entry.shape === shape).map(({ sort }) => sort),
      ['tokens', 'duration', 'cost'],
    );
  }
});

test('requires metadata and every private filter without echoing values', () => {
  assert.throws(() => parseArgs([]), /--schema-version is required/);
  assert.throws(
    () => parseArgs(['--schema-version', '24', '--commit', 'abc']),
    /--binary-sha256 is required/,
  );
  const parsed = parseArgs([
    '--schema-version', '24',
    '--commit', 'abc',
    '--binary-sha256', 'def',
    '--source', FILTERS.source,
    '--model', FILTERS.model,
    '--project', FILTERS.project,
    '--host', FILTERS.host,
  ]);
  assert.deepEqual(parsed.filters, FILTERS);
});

test('records per-sample wall, server query, status, support, and payload evidence', async () => {
  const requested = [];
  const fetchImpl = async (url) => {
    requested.push(new URL(url));
    return new Response(
      JSON.stringify({
        support: { supported: true, level: 'supported', reason: null },
        rows: [{ session_id: 'must-not-be-copied' }],
      }),
      {
        status: 200,
        headers: {
          'content-type': 'application/json',
          'server-timing': 'sessions-query;dur=12.34',
        },
      },
    );
  };
  const result = await benchmarkTopSessions({
    url: 'http://127.0.0.1:39000',
    iterations: 2,
    schemaVersion: '24',
    indexName: 'idx_usage_event_top_sessions_cover',
    commit: 'abc123',
    binarySha256: 'def456',
    filters: FILTERS,
  }, fetchImpl);

  assert.equal(requested.length, 24 * 3);
  assert.equal(result.samples.length, 24 * 2);
  assert.equal(result.summaries.length, 24);
  assert.ok(result.samples.every((sample) => sample.status === 200));
  assert.ok(result.samples.every((sample) => sample.supported));
  assert.ok(result.samples.every((sample) => sample.query_ms === 12.34));
  assert.ok(result.summaries.every((summary) => summary.sample_count === 2));

  const serialized = JSON.stringify(result);
  for (const secret of [...Object.values(FILTERS), 'must-not-be-copied', '39000']) {
    assert.equal(serialized.includes(secret), false, `sanitized output leaked ${secret}`);
  }
});

test('rejects responses without auditable server query timing', () => {
  assert.equal(parseQueryTiming('cache;desc=miss, sessions-query;dur=0.75'), 0.75);
  assert.throws(() => parseQueryTiming(null), /missing sessions-query Server-Timing/);
});
