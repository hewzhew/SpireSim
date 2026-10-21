import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, sep } from 'node:path';
import { needsMaintainerReview, proposePlayerCommand, adoptPlayerProposal,
  capturePlayerFrame, readPlayerReview, renderPlayerReview, bindCoachedPlayer,
  setPlayerGuidance, currentPlayerGuidance, currentPlayerAdoption } from './supervision.mjs';

async function fixture(t) {
  const base = await realpath(tmpdir());
  const session = await mkdtemp(join(base, 'sts-coached-'));
  t.after(async () => {
    const target = await realpath(session);
    assert.ok(target.startsWith(base + sep) && target.split(/[\\/]/).at(-1).startsWith('sts-coached-'));
    await rm(target, { recursive: true });
  });
  const observation = { schema: 'sts-run-play-agent-observation-v1', session_id: 'game-1', head_revision: 1, state_revision: 1,
    position: { kind: 'decision', observation: { domain: 'strategic', observation: {
      run: { act: 1, floor: 5 }, context: { kind: 'shop' },
    } } } };
  const adopted = [];
  const game = { observe: async () => structuredClone(observation), execute: async command => {
    if (command === 'read, inventory') return { text: 'Act 1 F5 | HP 71/75 | Shop\nDeck: 16 cards' };
    adopted.push(command); observation.head_revision++; observation.state_revision++;
    return { text: 'Actual successor' };
  } };
  return { session, observation, adopted, game };
}

test('queries and gold/page navigation stay available; every consequential command needs review', () => {
  for (const command of ['read', 'research', 'preview', 'plans', 'review', 'info', 'map', 'inventory', 'open-card#0', 'claim:Gold#0']) {
    assert.equal(needsMaintainerReview([[command, []]]), false, command);
  }
  for (const command of ['take:Pummel#0', 'skip-card#0', 'buy:Inflame#0', 'leave', 'go', 'proceed', 'commit', 'act', '0', 'loot', 'back']) {
    assert.equal(needsMaintainerReview([[command, []]]), true, command);
  }
  assert.equal(needsMaintainerReview([['read', []], ['buy:Inflame#0', []]]), true);
  assert.equal(needsMaintainerReview([['back', ['list']]]), false);
});

test('proposing stops before execution and adoption is source-bound and happens once', async t => {
  const f = await fixture(t);
  const proposal = await proposePlayerCommand(f.session, f.game, 'buy:Inflame#0', 'Support existing multi-hit attacks.');
  assert.equal(proposal.status, 'pending'); assert.equal(f.observation.head_revision, 1);
  assert.deepEqual(f.adopted, []);
  await assert.rejects(proposePlayerCommand(f.session, f.game, 'leave'), /prior proposal/);
  const accepted = await adoptPlayerProposal(f.session, f.game);
  assert.equal(accepted.review.status, 'adopted'); assert.equal(accepted.frame.source.head_revision, 2);
  assert.deepEqual(f.adopted, ['buy:Inflame#0']);
  assert.match(currentPlayerAdoption(accepted.review, accepted.frame), /executed this choice.*buy:Inflame#0.*do not repeat/);
  f.observation.head_revision++;
  assert.equal(currentPlayerAdoption(accepted.review, await capturePlayerFrame(f.game)), '');
  await assert.rejects(adoptPlayerProposal(f.session, f.game), /No pending/);
});

test('maintainer replacement executes the replacement; stale approval cannot execute anything', async t => {
  const f = await fixture(t);
  await proposePlayerCommand(f.session, f.game, 'buy:Rampage#0');
  await setPlayerGuidance(f.session, await capturePlayerFrame(f.game), 'Support existing multi-hit attacks.');
  const replaced = await adoptPlayerProposal(f.session, f.game, { command: 'buy:Inflame#0' });
  assert.deepEqual(f.adopted, ['buy:Inflame#0']);
  const handoff = [currentPlayerAdoption(replaced.review, replaced.frame), currentPlayerGuidance(replaced.review, replaced.frame)].join('\n');
  assert.match(handoff, /proposed command was buy:Rampage#0.*replaced/);
  assert.equal(handoff.split('Support existing multi-hit attacks.').length - 1, 1);
  const differentReason = { ...replaced.review, maintainerNote: 'Verified affordable purchase.' };
  assert.match(currentPlayerAdoption(differentReason, replaced.frame), /Maintainer reason: Verified affordable purchase/);
  await proposePlayerCommand(f.session, f.game, 'buy:ShrugItOff#0');
  f.observation.head_revision++; f.observation.state_revision++;
  await assert.rejects(adoptPlayerProposal(f.session, f.game), /source changed/);
  assert.deepEqual(f.adopted, ['buy:Inflame#0']);
  assert.match(renderPlayerReview(await capturePlayerFrame(f.game), await readPlayerReview(f.session)), /stale/);
});

test('partial or uncertain execution never becomes a retryable pending proposal', async t => {
  const f = await fixture(t);
  await proposePlayerCommand(f.session, f.game, 'buy:Inflame#0, leave');
  f.game.execute = async command => { f.adopted.push(command); f.observation.head_revision++; throw new Error('Applied prefix; connection failed'); };
  await assert.rejects(adoptPlayerProposal(f.session, f.game), /connection failed/);
  assert.equal((await readPlayerReview(f.session)).status, 'execution-error');
  await assert.rejects(proposePlayerCommand(f.session, f.game, 'leave'), /prior proposal/);
  await assert.rejects(adoptPlayerProposal(f.session, f.game), /No pending/);
  assert.equal(f.adopted.length, 1);
  const frame = await capturePlayerFrame({ ...f.game, execute: async () => ({ text: 'Actual state after partial execution' }) });
  await setPlayerGuidance(f.session, frame, 'The prefix already happened; inspect the remaining decision.', { reject: true });
  assert.equal((await readPlayerReview(f.session)).status, 'rejected');
  assert.deepEqual((await readPlayerReview(f.session)).rejectedAtSource, frame.source);
});

test('a running model blocks maintainer adoption, and a changing read cannot create a proposal', async t => {
  const f = await fixture(t);
  await proposePlayerCommand(f.session, f.game, 'leave');
  await bindCoachedPlayer(f.session, { id: 'player-1', home: f.session, pid: process.pid, running: true });
  await assert.rejects(adoptPlayerProposal(f.session, f.game), /player to stop/);
  assert.deepEqual(f.adopted, []);
  f.game.execute = async () => { f.observation.head_revision++; return { text: 'Reading changed' }; };
  await assert.rejects(capturePlayerFrame(f.game), /changed while reading/);
});

test('guidance survives purchases in its task and expires on departure', async t => {
  const f = await fixture(t);
  const frame = await capturePlayerFrame(f.game);
  const review = await setPlayerGuidance(f.session, frame, 'Support multi-hit attacks with lasting Strength.');
  f.observation.head_revision++; f.observation.state_revision++;
  assert.match(currentPlayerGuidance(review, await capturePlayerFrame(f.game)), /lasting Strength/);
  f.observation.position.observation.observation.context.kind = 'map';
  assert.equal(currentPlayerGuidance(review, await capturePlayerFrame(f.game)), '');
});

test('interrupted executing state needs explicit source-recorded rejection, not approval or a new proposal', async t => {
  const f = await fixture(t);
  const proposal = await proposePlayerCommand(f.session, f.game, 'leave');
  await writeFile(join(f.session, 'dsh-review.json'), JSON.stringify({ ...proposal, status: 'executing' }));
  const frame = await capturePlayerFrame(f.game);
  await assert.rejects(setPlayerGuidance(f.session, frame, 'Continue'), /explicitly reject/);
  await assert.rejects(proposePlayerCommand(f.session, f.game, 'leave'), /prior proposal/);
  await assert.rejects(adoptPlayerProposal(f.session, f.game), /No pending/);
  await setPlayerGuidance(f.session, frame, 'Checked the actual state after interruption; make a new proposal.', { reject: true });
  assert.deepEqual((await readPlayerReview(f.session)).rejectedAtSource, frame.source);
  assert.deepEqual(f.adopted, []);
});

test('a completed combat hands off its actual reward state without ambiguous old plan indices or combat guidance', async t => {
  const f = await fixture(t);
  const rewards = await capturePlayerFrame(f.game);
  rewards.task.family = 'rewards';
  const combat = { ...rewards.task, family: 'combat', detail: 'ThreeSentries' };
  const review = { status: 'adopted', command: 'commit 3', selectedCommand: 'commit 1',
    sourceAfter: rewards.source, frame: { task: combat }, maintainerNote: 'Use Heart of Iron in this combat.' };
  const handoff = currentPlayerAdoption(review, rewards);
  assert.match(handoff, /completed the previous combat decision.*actual successor.*rewards choices are still available/);
  assert.doesNotMatch(handoff, /commit [13]|Heart of Iron/);
  assert.equal(currentPlayerAdoption(review, { ...rewards, source: { ...rewards.source, state_revision: 2 } }), '');
});
