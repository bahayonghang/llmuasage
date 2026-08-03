import test from 'node:test';
import assert from 'node:assert/strict';
globalThis.window = { console, localStorage: { getItem: () => null, setItem: () => {} }, addEventListener: () => {}, location: { hash: '', origin: 'http://localhost' } };
globalThis.document = {};
const { logsGenerationIsCurrent } = await import('../../src/web/assets/render/logs-viewer.js');
test('logs viewer rejects stale generations and filter signatures', () => {
  assert.equal(logsGenerationIsCurrent(3, 3, 'a', 'a'), true);
  assert.equal(logsGenerationIsCurrent(4, 3, 'a', 'a'), false);
  assert.equal(logsGenerationIsCurrent(3, 3, 'b', 'a'), false);
});
