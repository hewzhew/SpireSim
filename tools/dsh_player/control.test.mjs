import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, sep } from 'node:path';
import { controlPlayer } from './launch.mjs';
import { proposePlayerCommand, bindCoachedPlayer, readPlayerReview } from './supervision.mjs';

async function fixture(t, { failExecution = false, failClose = false, terminal = false } = {}) {
  const base = await realpath(tmpdir());
  const session = await mkdtemp(join(base, 'sts-player-control-'));
  t.after(async () => {
    const target = await realpath(session);
    assert.ok(target.startsWith(base + sep) && target.split(/[\\/]/).at(-1).startsWith('sts-player-control-'));
    await rm(target, { recursive: true });
  });
  const state = { schema: 'sts-run-play-agent-observation-v3', session_id: 'game-control', head_revision: 1, state_revision: 1,
    position: { kind: 'decision', observation: { domain: 'strategic', observation: {
      run: { act: 1, floor: 5 }, context: { kind: 'shop' },
    } } } };
  const mutations = [], starts = [], order = [];
  let connected = false;
  const game = {
    observe: async () => structuredClone(state),
    async execute(command) {
      if (command === 'read, inventory' || command === 'read') return { text: `Actual reading at revision ${state.state_revision}` };
      mutations.push(command); order.push('execute'); state.head_revision++; state.state_revision++;
      if (failExecution) throw new Error('Applied purchase; later command failed');
      if (terminal) state.position = { kind: 'terminal', outcome: { kind: 'heart_victory' } };
      return { text: 'Actual successor' };
    },
    async close() { connected = false; order.push('close'); if (failClose) throw new Error('connection close failed'); },
  };
  await bindCoachedPlayer(session, { id: 'player-control', home: session, model: 'deepseek-flash', reasoning: 'low',
    maxTokens: 16384, running: false, pid: process.pid });
  await proposePlayerCommand(session, game, 'buy:Inflame#0, buy:ShrugItOff#0');
  const dependencies = {
    async openWorkbench() { assert.equal(connected, false); connected = true; order.push('open'); return game; },
    async startPlayer(options) {
      assert.equal(connected, false, 'release the adopter connection before starting the model');
      order.push('start'); starts.push(options);
      await proposePlayerCommand(session, game, 'leave', 'Purchases completed.');
      return { outcome: 'needs-review' };
    },
  };
  const control = parameters => controlPlayer({ repository: session, session, parameters }, () => {}, dependencies);
  return { control, state, mutations, starts, order, session };
}

test('accept defaults to execution and pause; explicit continuation starts once at the successor', async t => {
  const f = await fixture(t);
  assert.equal((await f.control(['accept'])).outcome, 'completed');
  assert.deepEqual(f.mutations, ['buy:Inflame#0, buy:ShrugItOff#0']);
  assert.equal(f.starts.length, 0);
  assert.equal((await f.control(['next'])).outcome, 'needs-review');
  assert.equal(f.starts.length, 1);
  assert.equal((await readPlayerReview(f.session)).source.state_revision, 2);
});

test('accept --continue executes the approved commands once, releases the connection, and resumes the same player', async t => {
  const f = await fixture(t);
  assert.equal((await f.control(['accept', '--continue'])).outcome, 'needs-review');
  assert.deepEqual(f.mutations, ['buy:Inflame#0, buy:ShrugItOff#0']);
  assert.deepEqual(f.order, ['open', 'execute', 'close', 'start']);
  assert.equal(f.starts[0].resume, 'player-control');
  assert.equal(f.starts[0].supervised, true);
  assert.equal(f.starts[0].maxMs, 90000);
  assert.match(f.starts[0].task, /^Continue the bound run/);
  assert.doesNotMatch(f.starts[0].task, /Investigate the current decision and submit one proposal/);
  const pending = await readPlayerReview(f.session);
  assert.equal(pending.command, 'leave');
  assert.equal(pending.source.state_revision, 2);
  await assert.rejects(f.control(['next']), /outstanding proposal/);
  assert.equal(f.starts.length, 1, 'another continuation cannot bypass the new review point');
});

test('replace --continue executes only the replacement before resuming', async t => {
  const f = await fixture(t);
  await f.control(['replace', '--continue', 'buy:Inflame#0']);
  assert.deepEqual(f.mutations, ['buy:Inflame#0']);
  assert.equal(f.starts.length, 1);
});

test('partial failure, stale approval, or a failed close cannot start continuation', async t => {
  for (const failure of ['execution', 'stale', 'close']) {
    const f = await fixture(t, { failExecution: failure === 'execution', failClose: failure === 'close' });
    if (failure === 'stale') { f.state.head_revision++; f.state.state_revision++; }
    await assert.rejects(f.control(['accept', '--continue']), /later command failed|source changed|close failed/);
    assert.equal(f.starts.length, 0, failure);
    const review = await readPlayerReview(f.session);
    assert.equal(review.status, failure === 'execution' ? 'execution-error' : failure === 'stale' ? 'pending' : 'adopted');
    assert.equal(f.mutations.length, failure === 'stale' ? 0 : 1);
  }
});

test('completion at a terminal position does not call the model again', async t => {
  const f = await fixture(t, { terminal: true });
  assert.equal((await f.control(['accept', '--continue'])).outcome, 'terminal');
  assert.equal(f.starts.length, 0);
  assert.equal(f.mutations.length, 1);
  assert.equal((await readPlayerReview(f.session)).status, 'adopted');
});

test('malformed continuation options fail before connecting or adopting', async () => {
  const dependencies = { openWorkbench() { assert.fail('invalid options must not open a game'); } };
  for (const parameters of [['accept', '--continue', 'extra'], ['accept', '--continue', '--continue'], ['replace', '--continue']]) {
    await assert.rejects(controlPlayer({ parameters }, () => {}, dependencies), /takes no arguments|requires a replacement/);
  }
});
