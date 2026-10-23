import test from 'node:test';
import assert from 'node:assert/strict';
import { recordedClient } from './recording.mjs';

const publicCapabilities = { permission_envelope: { information_ceiling: 'public_only' } };
test('a frozen client retains the actual input and the corresponding source command', async () => {
  const before = { capabilities: publicCapabilities, position: { actions: [{ semantic_action: 'play:Strike+0#0@0' }, { semantic_action: 'play:Strike+0#1@0' }] } }, after = { head_revision: 2 };
  const calls = [], records = [];
  const original = Object.freeze({ observe: async () => before, act: async (...args) => { calls.push(args); return { raw: { observation: after } }; } });
  const options = { advance: 'one_public_action' };
  const result = await recordedClient(original, row => records.push(row), () => ({ callId: 'actual-call' })).act(['play:Strike#1@0'], options);
  assert.deepEqual(calls, [[['play:Strike#1@0'], options]]);
  assert.equal(records[0].action, 'play:Strike+0#1@0');
  assert.equal(records[0].before, before); assert.equal(records[0].after, result.raw.observation);
});

test('a grouped execution is retained without inventing intermediate states', async () => {
  const original = Object.freeze({ observe: async () => ({ capabilities: publicCapabilities, position: { actions: [] } }), act: async () => ({ raw: { observation: { head_revision: 3 } } }) });
  const records = [];
  await recordedClient(original, row => records.push(row), () => ({})).act(['end', 'end']);
  assert.equal(records[0].action, null); assert.equal(records[0].actions.length, 2);
});

test('privileged capture is rejected before advancing the game', async () => {
  let acted = false;
  const game = { observe: async () => ({ capabilities: { permission_envelope: { information_ceiling: 'privileged_exact' } } }), act: async () => { acted = true; } };
  await assert.rejects(recordedClient(game, () => {}, () => ({})).act(['end']), /public-only/);
  assert.equal(acted, false);
});
