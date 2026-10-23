import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, sep } from 'node:path';
import { createRecording, recordedTransition } from './recording.mjs';

const capabilities = { permission_envelope: { information_ceiling: 'public_only' } };
const candidate = { domain: 'strategic', action: { kind: 'choose_route_node', x: 5, y: 0 } };
const before = { capabilities, position: { actions: [
  { semantic_action: 'go 5,0', candidate },
  { semantic_action: 'play:Strike+0#1@0', candidate: { domain: 'combat_atomic' } },
] } };

test('capture retains the actual act input and canonical source command', () => {
  const input = { command: 'act', actions: ['play:Strike#1@0'], advance: 'one_public_action' };
  const row = recordedTransition(before, input);
  assert.equal(row.input, input);
  assert.equal(row.action, 'play:Strike+0#1@0');
  assert.deepEqual(row.actions, input.actions);
});

test('source-bound choices match typed candidates, never their ordinal or label', () => {
  const input = { command: 'choose', selection: { source: { head_revision: 0 }, choice: { ordinal_path: [91] } } };
  assert.equal(recordedTransition(before, input, structuredClone(candidate)).action, 'go 5,0');
  assert.equal(recordedTransition(before, input, { ...candidate, action: { ...candidate.action, x: 6 } }).action, null);
  assert.equal(recordedTransition(before, input).action, null);
});

test('grouped execution retains its input without inventing intermediate actions', () => {
  assert.equal(recordedTransition(before, { command: 'act', actions: ['end', 'end'] }).action, null);
  assert.equal(recordedTransition(before, { command: 'choose', following_rewards: [{}] }, candidate).action, null);
});

test('observer rejects privileged capture before execution and preserves committed boundaries', async t => {
  const base = await realpath(tmpdir());
  const root = await mkdtemp(join(base, 'spiresim-recording-'));
  t.after(async () => {
    const target = await realpath(root);
    assert.ok(target.startsWith(base + sep) && target.includes('spiresim-recording-'));
    await rm(target, { recursive: true });
  });
  const recording = await createRecording({ directory: join(root, 'capture'), metadata: { id: 'test', source_revision: 'test-revision', operator: 'developer' } });
  const observer = recording.observer({}, () => ({ call: { step: 4 } }));
  const input = { command: 'choose', selection: { choice: { ordinal_path: [0] } } };
  await assert.rejects(observer({ phase: 'before', before: { capabilities: { permission_envelope: { information_ceiling: 'privileged_exact' } } }, input }), /public-only/);
  const context = await observer({ phase: 'before', before, input, candidate });
  const after = { position: {} };
  await observer({ phase: 'committed', before, after, input, context, candidate });
  const row = JSON.parse(await readFile(join(root, 'capture/actions.jsonl'), 'utf8'));
  assert.equal(row.action, 'go 5,0'); assert.deepEqual(row.input, input);
  assert.deepEqual(row.before, before); assert.deepEqual(row.after, after);
  assert.equal(row.call.step, 4); assert.equal(row.operator, 'developer');
});
