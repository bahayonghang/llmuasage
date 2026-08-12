import test from 'node:test';
import assert from 'node:assert/strict';
import { buildAnalyticsCsv, escapeCsvCell } from '../../src/web/assets/csv-export.js';

test('CSV escaping blocks formulas and follows RFC quoting', () => {
  for (const value of ['=cmd()', '+1', '-1', '@x', '\tformula', '\rformula', '\nformula']) assert.equal(escapeCsvCell(value).replace(/^"/, '').startsWith("'"), true, value);
  assert.equal(escapeCsvCell('a,"b"\nline'), '"a,""b""\nline"');
});

test('analytics CSV emits BOM, localized multi-section headers, and untrusted labels safely', () => {
  const data = { home_overview: { summary: { total_sessions: 1, total_requests: 2, total_tokens: 3, total_cost_usd: 4, active_days: 5, cache_efficiency: 0.25, platforms: 9 } }, projects: [{ project_label: '=cmd()', total_tokens: 3 }], models: [], sources: [], trends_daily: [], top_sessions: [{ session_label: '@x', total_tokens: 2, active_minutes: 1 }] };
  const zh = buildAnalyticsCsv(data, 'zh');
  const en = buildAnalyticsCsv(data, 'en');
  assert.equal(zh.charCodeAt(0), 0xfeff);
  assert.match(zh, /汇总\r\n指标,值/);
  assert.equal(zh.split('\r\n\r\n')[0].split('\r\n').length, 8, 'summary contains a title, header, and six cards');
  assert.doesNotMatch(zh, /platforms/);
  assert.match(en, /Summary\r\nMetric,Value/);
  assert.match(zh, /'=cmd\(\)/);
  assert.match(zh, /'@x/);
  assert.ok(zh.split('\r\n\r\n').length >= 6);
});
