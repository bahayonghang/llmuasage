import test from 'node:test';
import assert from 'node:assert/strict';
globalThis.window = { console, localStorage: { getItem: () => null, setItem: () => {} }, addEventListener: () => {}, location: { hash: '', origin: 'http://localhost' } };
globalThis.document = {};
const viewer = await import('../../src/web/assets/render/logs-viewer.js');
const { logsGenerationIsCurrent, logsTimeContent, logsTextContent, isLogsRowActivationKey } = viewer;
test('logs viewer rejects stale generations and filter signatures', () => {
  assert.equal(logsGenerationIsCurrent(3, 3, 'a', 'a'), true);
  assert.equal(logsGenerationIsCurrent(4, 3, 'a', 'a'), false);
  assert.equal(logsGenerationIsCurrent(3, 3, 'b', 'a'), false);
});
test('time cells show a compact local value and keep the raw timestamp as title', () => {
  const content = logsTimeContent('2026-08-28T10:04:05Z');
  assert.equal(content.title, '2026-08-28T10:04:05Z');
  assert.match(content.text, /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}(:\d{2})?$/);
  assert.notEqual(content.text, content.title);
  assert.deepEqual(logsTimeContent(''), { text: '--', title: '' });
  assert.deepEqual(logsTimeContent(null), { text: '--', title: '' });
});
test('long text cells keep the full value available as title', () => {
  const longSession = 'session-identifier-'.repeat(4).slice(0, 64);
  assert.equal(longSession.length, 64);
  const content = logsTextContent(longSession);
  assert.equal(content.text, longSession);
  assert.equal(content.title, longSession);
  assert.deepEqual(logsTextContent('   '), { text: '--', title: '' });
  assert.deepEqual(logsTextContent(undefined), { text: '--', title: '' });
});
test('only Enter and Space activate a focusable log row', () => {
  assert.equal(isLogsRowActivationKey('Enter'), true);
  assert.equal(isLogsRowActivationKey(' '), true);
  assert.equal(isLogsRowActivationKey('Spacebar'), true);
  assert.equal(isLogsRowActivationKey('Tab'), false);
  assert.equal(isLogsRowActivationKey('Escape'), false);
  assert.equal(isLogsRowActivationKey('a'), false);
});
