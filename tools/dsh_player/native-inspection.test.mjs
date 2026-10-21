import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm, realpath, cp, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { Context } from '@deepseek-ai/cordis';
import LlmRuntime, { LlmAdapter, createUserMessage } from '@deepseek-ai/dsh-llm';
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session';
import ProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import AgentRegistry from '@deepseek-ai/dsh-agent';
import AgentLoop from '@deepseek-ai/dsh-agent-loop';
import ToolRuntime, { defineTool } from '@deepseek-ai/dsh-tools';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import * as SessionPersistence from '@deepseek-ai/dsh-session-persistence-jsonl';
import { readPlayerJournal } from './inspection.mjs';
import { parseNativeObservation, observeNative, nativeJournalSummary, renderNativeObservation, readLiveGame } from './native-inspection.mjs';
import { StsObservationView, observationTickProjection } from './observation-view.mjs';

async function fixture(t) {
  const base = await realpath(tmpdir()), repository = await mkdtemp(join(base, 'sts-native-observe-'));
  const home = join(repository, 'dsh'), id = 'session-observe-fixture';
  const session = join(repository, '.oracle-lab/run-play', `dsh-native-${createHash('sha256').update(id).digest('hex').slice(0, 24)}`);
  const binding = { schema: 'sts-dsh-native-binding-v1', repository, session, agent: id, seed: '1' };
  await mkdir(session, { recursive: true });
  await writeFile(join(session, 'dsh-native.json'), JSON.stringify(binding));
  const focus = join(repository, '.oracle-lab/run-play-records/current-session');
  await mkdir(dirname(focus), { recursive: true }); await writeFile(focus, 'unrelated-game');
  const ctx = new Context(); let handle;
  t.after(async () => {
    await handle?.dispose(); await ctx.fiber.dispose();
    const target = await realpath(repository);
    assert.ok(dirname(target) === base && target.startsWith(base + sep) && target.split(/[\\/]/).at(-1).startsWith('sts-native-observe-'));
    await rm(target, { recursive: true });
  });
  for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime, TokenMeter]) await ctx.plugin(plugin);
  await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
  await ctx.plugin(SessionPersistence, { root: join(home, 'sessions'), compression: 'zstd' });
  await ctx.plugin(AgentLoop, { agents: [], maxParallelToolCalls: 1 });
  ctx.systemPrompt.section({ name: 'fixture', order: 300, text: 'Use the supplied game state.' });
  const executed = [], requests = [];
  ctx.tools.register(defineTool({ name: 'sts', description: 'Fixture game tool',
    parameters: { command: { type: 'string', required: true } },
    output: { schema: { type: 'string' }, render: (_args, text) => [{ type: 'text', text }],
      presentationMeta: () => ({ schema: 'sts-native-game-v1', agent: id, session }) },
    execute({ command }) {
      executed.push(command);
      if (command === 'broken') throw new Error('Applied before error: discard potion\nError: invalid choice');
      return `Act 1 F3 | HP 46/75 | Gold 133 | Shop\nDeck: 11 cards\n${command} result`;
    },
  }));
  let response = 0;
  class Adapter extends LlmAdapter {
    async *stream() {
      response++;
      const command = ['read', 'broken', null, 'inventory', null][response - 1];
      const block = command ? { type: 'tool-call', id: `call-${response}`, name: 'sts', arguments: JSON.stringify({ command }) }
        : { type: 'text', text: `Finished turn at response ${response}.` };
      yield { type: 'block-start', index: 0, blockType: block.type };
      yield command ? { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments }
        : { type: 'text-delta', index: 0, text: block.text };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'usage', usage: { inputTokens: 10, cacheReadTokens: 100, cacheWriteTokens: 0, outputTokens: 5 } };
      yield { type: 'finish', reason: { kind: command ? 'tool-calls' : 'stop' } };
    }
  }
  ctx.llm.registerAdapter(['fixture'], new Adapter());
  ctx.on('llm/stream', (request, next) => {
    const { signal, ...plain } = request; requests.push(JSON.parse(JSON.stringify(plain)));
    const owner = handle.agent.session;
    owner.append('session-log-deepseek/delivery-accepted', { sessionId: id, sessionFormatVersion: owner.header.version,
      throughSeq: owner.snapshotEvents().length - 1 });
    return next();
  });
  handle = await ctx.agents.create({ sessionId: SessionId(id), meta: { cwd: repository }, agentOptions: { provider: 'fixture', model: 'fixture' } });
  for (const task of ['First task.', 'Second task.']) {
    handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: task }] }));
    await handle.agent.whenIdle(); await ctx.sessions.flush(handle.agent.session);
  }
  return { ctx, owner: handle.agent.session, repository, home, binding, id, session, focus, executed, requests,
    log: await readPlayerJournal(home, id, { compression: 'zstd' }) };
}

test('native observer reads official compressed logs, scopes turns and leaves gameplay and history untouched', async t => {
  const f = await fixture(t), neverOpen = () => assert.fail('log inspection must not connect to Rust');
  const args = ['--home', f.home, '--chat', 'observe-fixture', '--turn', '1'];
  const first = await observeNative({ repository: f.repository, parameters: args }, { openWorkbench: neverOpen });
  assert.equal(first.report.turn, 1); assert.equal(first.report.status, 'completed');
  assert.equal(first.report.steps, 3); assert.equal(first.report.gameCalls, 2);
  assert.equal(first.report.errors.length, 1); assert.equal(first.report.errors[0].step, 2);
  assert.deepEqual(first.report.accounting.totals, { uncachedInputTokens: 30, cacheReadTokens: 300, cacheWriteTokens: 0, outputTokens: 15 });
  assert.match(first.text, /345 tokens/); assert.match(first.text, /recorded page may no longer/);
  const remembered = await observeNative({ repository: f.repository }, { openWorkbench: neverOpen });
  assert.equal(remembered.report.turn, 2); assert.equal(remembered.report.errors.length, 0);
  assert.equal(remembered.report.accounting.totals.outputTokens, 25);
  const selected = await observeNative({ repository: f.repository, parameters: ['2', '--turn', '1'] }, { openWorkbench: neverOpen });
  assert.match(selected.text, /sts broken/); assert.match(selected.text, /invalid choice/);
  const input = await observeNative({ repository: f.repository, parameters: ['2', 'input', '--turn', '1'] }, { openWorkbench: neverOpen });
  assert.deepEqual(input.report.inputs[0].request, f.requests[1]);
  assert.match(input.text, /no SDK checksum/); assert.doesNotMatch(JSON.stringify(input.report.inputs), /Second task/);
  assert.equal(await readFile(f.focus, 'utf8'), 'unrelated-game');
  assert.deepEqual((await readPlayerJournal(f.home, f.id, { compression: 'zstd' })).events, f.log.events);
  assert.deepEqual(f.executed, ['read', 'broken', 'inventory'], 'observations made no additional tool or model calls');
});

test('selected observation pairs a result with only the immediate next model step in the same turn', async t => {
  const f = await fixture(t);
  const first = await nativeJournalSummary(f.log, f.binding, { turn: 1, step: 1 });
  assert.equal(first.following.number, 2);
  assert.match(renderNativeObservation(first), /Next recorded model step 2:\nCommand: sts broken/);
  assert.doesNotMatch(renderNativeObservation(first), /Finished turn|Applied before error/);
  const second = await nativeJournalSummary(f.log, f.binding, { turn: 1, step: 2 });
  assert.match(renderNativeObservation(second), /Next recorded model step 3:\nFinished turn at response 3/);
  const last = await nativeJournalSummary(f.log, f.binding, { turn: 1, step: 3 });
  assert.equal(last.following, null);
  assert.match(renderNativeObservation(last), /No following model step recorded in this turn/);
  assert.doesNotMatch(renderNativeObservation(last), /Command: sts inventory/);

  const partial = structuredClone(f.log);
  partial.events = partial.events.slice(0, first.following.seq + 1);
  const pending = await nativeJournalSummary(partial, f.binding, { turn: 1, step: 1 });
  assert.match(renderNativeObservation(pending), /No model output or command recorded yet/);
  assert.deepEqual(f.executed, ['read', 'broken', 'inventory']);
  assert.deepEqual((await readPlayerJournal(f.home, f.id, { compression: 'zstd' })).events, f.log.events);
});

test('native observation Remote shares the journal interpreter, loads input only on demand and makes no game or model calls', async t => {
  const f = await fixture(t);
  const inspectSession = async id => {
    const owner = f.ctx.sessions.get(SessionId(id));
    if (!owner) throw new Error('fixture: session not found');
    if (id !== f.id) return { meta: owner.header, events: [], inheritedEventCount: 0 };
    await f.ctx.sessions.flush(owner);
    return readPlayerJournal(f.home, id, { compression: 'zstd' });
  };
  const view = new StsObservationView(f.ctx, { inspectSession, openWorkbench: () => assert.fail('journal reads must not connect to Rust') });
  const latest = await view.read(f.id);
  assert.equal(latest.turn, 2); assert.equal(latest.timeline[0].calls[0].command, 'inventory');
  assert.equal(latest.timeline[0].outcome, 'completed'); assert.deepEqual(latest.timeline.map(item => item.step), [1, 2]);
  assert.equal(latest.timeline[1].outcome, 'model');
  assert.equal(latest.timeline[0].usage.inputTokens, 10); assert.ok(latest.timeline[0].durationMs >= 0);
  assert.equal(latest.accounting.totals.outputTokens, 25);
  assert.equal('inputs' in latest, false);
  const previous = await view.read(f.id, 1);
  assert.equal(previous.errors, 1); assert.equal(previous.timeline[1].outcome, 'error');
  assert.equal(previous.timeline[1].step, 2); assert.equal(previous.timeline[1].calls[0].command, 'broken');
  assert.equal(previous.recordedMayBeStale, true);
  const selected = await view.step(f.id, 1, 2);
  assert.equal(selected.calls[0].command, 'broken'); assert.match(selected.calls[0].text, /Applied before error/);
  assert.equal('inputs' in selected, false);
  const input = await view.step(f.id, 1, 2, true);
  assert.deepEqual(input.inputs[0].request, f.requests[1]);
  assert.deepEqual((await readPlayerJournal(f.home, f.id, { compression: 'zstd' })).events, f.log.events);
  assert.deepEqual(f.executed, ['read', 'broken', 'inventory']);
  assert.equal(await readFile(f.focus, 'utf8'), 'unrelated-game');
  await assert.rejects(view.read('foreign-chat'), /not found/);
  await assert.rejects(view.step(f.id, 1, 99), /No step/);
  await assert.rejects(view.step(f.id, 0, 1), /有效/);
  await assert.rejects(view.read(f.id, -1), /正整数/);
  const empty = f.ctx.sessions.create('empty-chat', { meta: { cwd: f.repository } });
  assert.equal((await view.read(empty.id)).status, 'not-started');
  assert.equal((await view.live(empty.id)).status, 'not-started');
  const digest = createHash('sha256').update(empty.id).digest('hex').slice(0, 24);
  await assert.rejects(readFile(join(f.repository, '.oracle-lab/run-play', `dsh-native-${digest}`, 'dsh-native.json')), { code: 'ENOENT' });
});

test('native observation freezes the selected journal cut across asynchronous flush and closes optional live reads', async t => {
  const f = await fixture(t), cut = f.owner.seq;
  const flush = f.ctx.sessions.flush.bind(f.ctx.sessions);
  let append = true;
  f.ctx.sessions.flush = async session => {
    await flush(session);
    if (append) {
      append = false;
      session.append('user/message', f.log.events.find(event => event.type === 'user/message').data, { surfaceOp: 'append' });
      await flush(session);
    }
    return true;
  };
  const calls = [], snapshot = { head_revision: 40, state_revision: 40, position: { public_snapshot_id: 'same' } };
  const view = new StsObservationView(f.ctx, { openWorkbench: async options => {
    assert.equal(options.session, f.session);
    return { observe: async () => snapshot, execute: async command => { calls.push(command); return { text: 'Actual inventory' }; },
      close: async () => calls.push('close') };
  }, inspectSession: async () => {
    const captured = f.owner.seq;
    await f.ctx.sessions.flush(f.owner);
    const log = await readPlayerJournal(f.home, f.id, { compression: 'zstd' });
    return { ...log, events: log.events.slice(0, captured) };
  } });
  assert.equal((await view.read(f.id)).throughSeq, cut - 1);
  assert.equal((await view.read(f.id)).throughSeq, cut);
  await mkdir(join(f.repository, 'target/dev-opt'), { recursive: true });
  await writeFile(join(f.repository, 'target/dev-opt/run_play.exe'), 'fixture');
  const live = await view.live(f.id);
  assert.match(live.text, /Actual inventory/); assert.deepEqual(calls, ['read, inventory', 'close']);
  let state = observationTickProjection.init();
  state = observationTickProjection.apply(state, { type: 'tool/call', seq: 12 });
  assert.equal(state, 12);
  assert.equal(observationTickProjection.apply(state, { type: 'assistant/message', seq: 13 }), 13);
  assert.equal(observationTickProjection.apply(state, { type: 'irrelevant', seq: 14 }), 12);
});

test('observation can inspect a cold stored chat without attaching an Agent or requiring a game binary', async t => {
  const f = await fixture(t), ctx = new Context();
  t.after(() => ctx.fiber.dispose());
  const view = new StsObservationView(ctx, { inspectSession: id => readPlayerJournal(f.home, id, { compression: 'zstd' }) });
  assert.equal((await view.read(f.id)).gameCalls, 1);
  assert.deepEqual(f.executed, ['read', 'broken', 'inventory']);
  const foreign = new Context(); t.after(() => foreign.fiber.dispose());
  await assert.rejects(new StsObservationView(foreign, { inspectSession: async () => ({ ...f.log, meta: { ...f.log.meta, id: 'wrong' } }) }).read(f.id), /不一致/);
});

test('missing delivery evidence stays unavailable and invalid or foreign evidence is rejected', async t => {
  const f = await fixture(t);
  const missing = structuredClone(f.log);
  missing.events.find(event => event.type === 'session-log-deepseek/delivery-accepted').type = 'fixture/no-boundary';
  const report = await nativeJournalSummary(missing, f.binding, { turn: 1, step: 1, input: true });
  assert.equal(report.inputs.length, 0); assert.match(renderNativeObservation(report), /original input is unavailable/);
  const invalid = structuredClone(f.log);
  invalid.events.find(event => event.type === 'session-log-deepseek/delivery-accepted').data.throughSeq = 999999;
  await assert.rejects(nativeJournalSummary(invalid, f.binding, { turn: 1, step: 1, input: true }), /Invalid delivery boundary/);
  await assert.rejects(nativeJournalSummary(f.log, { ...f.binding, agent: 'foreign' }), /different chats/);
  const foreign = structuredClone(f.log);
  foreign.events.find(event => event.type === 'tool/result').data.meta.session = join(f.repository, 'other-game');
  await assert.rejects(nativeJournalSummary(foreign, f.binding), /another game/);
  await writeFile(join(f.session, 'dsh-native.json'), JSON.stringify({ schema: 'sts-dsh-native-binding-v1', agent: f.id }));
  await assert.rejects(observeNative({ repository: f.repository, parameters: ['--home', f.home, '--chat', f.id] }), /Invalid native chat binding/);
});

test('cold return-point inspection queries only Rust and manual returns make old log views stale', async t => {
  const f = await fixture(t), ctx = new Context(); t.after(() => ctx.fiber.dispose());
  await mkdir(join(f.repository, 'target/dev-opt'), { recursive: true });
  await writeFile(join(f.repository, 'target/dev-opt/run_play.exe'), 'fixture');
  const captured = [];
  const view = new StsObservationView(ctx, { inspectSession: () => Promise.resolve(f.log),
    resolveAgent: () => assert.fail('read-only return points must not activate an Agent'),
    execute: async (_binary, args) => { captured.push(args); return { stdout: '{"schema":"run-play-return-points-v1","targets":[]}' }; } });
  assert.equal((await view.points(f.id)).status, 'ready');
  assert.deepEqual(captured, [['back', '--session', f.session, '--list', '--json']]);
  assert.equal(f.requests.length, 5);
  assert.equal((await view.points(f.id)).control.paused, false);
  const events = [...f.log.events,
    { seq: f.log.events.length, time: Date.now(), type: 'command/run', data: { commandId: 'return-1', name: 'sts-return', source: { kind: 'user' } } },
    { seq: f.log.events.length + 1, time: Date.now(), type: 'command/done', data: { commandId: 'return-1', kind: 'success',
      text: JSON.stringify({ schema: 'sts-native-workspace-v1', kind: 'returned', paused: true,
        result: { receipt: { from: { state_revision: 4 }, to: { state_revision: 1 } } } }) } }];
  view.inspectSession = async () => ({ ...f.log, events });
  const report = await view.read(f.id);
  assert.equal(report.control.paused, true); assert.equal(report.recordedMayBeStale, true);
  assert.equal(report.interventions.length, 2); assert.equal(report.interventions[1].kind, 'returned');
  assert.deepEqual(f.executed, ['read', 'broken', 'inventory']);
});

test('native live observation makes only queries, closes the connection and rejects concurrent game changes', async t => {
  const f = await fixture(t), calls = [], snapshot = { head_revision: 40, state_revision: 40, position: { public_snapshot_id: 'same' } };
  let changed = false, read = 0;
  const openWorkbench = async options => {
    assert.equal(options.session, f.session);
    return {
      observe: async () => { calls.push('observe'); return changed && ++read === 2 ? { ...snapshot, state_revision: 41 } : snapshot; },
      execute: async command => { calls.push(command); return { text: 'Read-only current game and deck.' }; },
      close: async () => calls.push('close'),
    };
  };
  const args = ['live', '--home', f.home, '--chat', f.id];
  const value = await observeNative({ repository: f.repository, parameters: args }, { openWorkbench });
  assert.match(value.text, /Live game read at h40, state 40/);
  assert.deepEqual(calls, ['observe', 'read, inventory', 'observe', 'close']);
  changed = true; calls.length = 0;
  await assert.rejects(observeNative({ repository: f.repository, parameters: ['live'] }, { openWorkbench }), /changed during/);
  assert.equal(calls.at(-1), 'close');
});

test('cold live reads preserve terminal outcomes and do not request an unavailable public inventory', async () => {
  for (const outcome of ['Defeat', 'Victory']) {
    const calls = [], snapshot = { head_revision: 40, state_revision: 40, position: { kind: 'terminal' } };
    const live = await readLiveGame({ session: 'terminal-game' }, 'repository', async () => ({
      observe: async () => { calls.push('observe'); return snapshot; },
      execute: async command => {
        calls.push(command); assert.equal(command, 'read');
        return { text: 'Terminal | ' + outcome };
      },
      close: async () => calls.push('close'),
    }));
    assert.equal(live.text, 'Terminal | ' + outcome);
    assert.equal(live.publicSnapshotId, null);
    assert.deepEqual(calls, ['observe', 'read', 'observe', 'close']);
  }
});

test('native list skips bindings absent from this home, and CLI inspection needs no game binary or gameplay focus', async t => {
  const f = await fixture(t);
  const otherId = 'absent-chat', otherSession = join(f.repository, '.oracle-lab/run-play', `dsh-native-${createHash('sha256').update(otherId).digest('hex').slice(0, 24)}`);
  await mkdir(otherSession); await writeFile(join(otherSession, 'dsh-native.json'), JSON.stringify({ ...f.binding, agent: otherId, session: otherSession }));
  const list = await observeNative({ repository: f.repository, parameters: ['list', '--home', f.home] });
  assert.match(list.text, /session-observe-fixture/); assert.doesNotMatch(list.text, /absent-chat/);
  const packageDir = dirname(fileURLToPath(import.meta.url));
  await cp(join(packageDir, '../run_play'), join(f.repository, 'tools/run_play'), { recursive: true });
  await symlink(packageDir, join(f.repository, 'tools/dsh_player'), process.platform === 'win32' ? 'junction' : 'dir');
  await rm(f.focus);
  const text = execFileSync(process.execPath, [join(f.repository, 'tools/run_play/play.mjs'), 'observe', '--home', f.home, '--chat', f.id], { encoding: 'utf8', windowsHide: true });
  assert.match(text, /Turn 2: completed/);
});

test('observer rejects ambiguous arguments and unavailable turn or step instead of selecting another boundary', async t => {
  for (const args of [['0'], ['1', 'extra'], ['live', '--turn', '1'], ['--chat'], ['--home', 'a', '--home', 'b']]) {
    assert.throws(() => parseNativeObservation(args));
  }
  const f = await fixture(t);
  await assert.rejects(nativeJournalSummary(f.log, f.binding, { turn: 99 }), /No recorded turn/);
  await assert.rejects(nativeJournalSummary(f.log, f.binding, { turn: 1, step: 99 }), /No step/);
  const pending = structuredClone(f.log);
  const tail = pending.events.findLast(event => event.type === 'step/start');
  pending.events = pending.events.slice(0, tail.seq + 1);
  pending.events.push({ ...structuredClone(f.log.events.find(event => event.type === 'tool/call')), seq: pending.events.length,
    data: { turn: 2, step: tail.data.step, name: 'sts', callId: 'unconfirmed', arguments: '{"command":"leave"}' } });
  const report = await nativeJournalSummary(pending, f.binding);
  assert.equal(report.pending.length, 1); assert.match(renderNativeObservation(report), /execution|Unconfirmed/);
});
