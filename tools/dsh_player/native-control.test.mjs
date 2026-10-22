import test from 'node:test';
import assert from 'node:assert/strict';
import { Context } from '@deepseek-ai/cordis';
import LlmRuntime, { LlmAdapter, createUserMessage } from '@deepseek-ai/dsh-llm';
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session';
import ProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import AgentRegistry from '@deepseek-ai/dsh-agent';
import AgentLoop from '@deepseek-ai/dsh-agent-loop';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import ToolRuntime, { defineTool } from '@deepseek-ai/dsh-tools';
import CommandRuntime from '@deepseek-ai/dsh-commands';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import * as LlmRetry from '@deepseek-ai/dsh-llm-retry';
import { NativeGameControl, nativeGameControl, workspaceFacts, registerNativeCommands } from './native.mjs';
import { StsObservationView, observationTickProjection } from './observation-view.mjs';
import { mapTickProjection } from './map-view.mjs';
import { requestFromBoundary } from './inspection.mjs';

const message = text => createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text }] });
const gate = () => Promise.withResolvers();
const identity = revision => ({ head_revision: revision, state_revision: revision, archive_node_id: 'n' + revision, public_snapshot_id: 's' + revision });
const point = (revision, origins = []) => ({ identity: identity(revision), origins,
  location: { kind: 'strategic', act: 1, floor: revision, site: 'map' }, resources: { hp: 70, max_hp: 75, gold: revision * 10, potions: [null, null] } });

async function fixture(t, { inFlight = false, noGame = false, usage, actOnFirst = true,
  transientFailures = 0, partialFailure = false, agentLocale = 'en', manual = false } = {}) {
  const ctx = new Context(), started = gate(), release = gate(), calls = [], requests = [];
  for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime, CommandRuntime]) await ctx.plugin(plugin);
  await ctx.plugin(TokenMeter);
  await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
  await ctx.plugin(AgentLoop, { agents: [] });
  if (transientFailures) await ctx.plugin(LlmRetry);
  const handle = await ctx.agents.create({ sessionId: SessionId('native-controls'),
    agentOptions: { provider: 'fixture', model: 'fixture' } });
  let version = 2, previous = null, opens = 0, failRead = false, active = 0;
  const binding = { agent: String(handle.agent.id), session: 'fixture-game', repository: 'fixture', seed: '1', agentLocale, manual };
  const source = () => ({ session_path: binding.session, session_id: 'fixture', identity: identity(version) });
  const points = () => ({ schema: 'run-play-return-points-v2', source: source(), current: point(version),
    targets: [point(1, [{ kind: 'floor', floor: 1 }]), ...(previous === null ? [] : [point(previous, [{ kind: 'previous_endpoint' }])])] });
  const workbench = {
    observe: async () => ({ head_revision: version, state_revision: version, position: { public_snapshot_id: 's' + version } }),
    returnPoints: async () => points(),
    async execute(command) {
      assert.equal(active++, 0, 'all model and operator reads/actions use one queue');
      try {
        calls.push(command);
        if (command === 'advance') {
          started.resolve(); if (inFlight) await release.promise;
          version++;
        }
        if (failRead) throw new Error('fixture read failed');
        return { text: `Actual game state ${version}\nCommands: act-current-${version}` };
      } finally { active--; }
    },
    async restorePoint(request) {
      assert.equal(active, 0);
      calls.push('restore');
      if (request.source.session_path !== binding.session || request.source.identity.head_revision !== version) throw new Error('stale source');
      const from = identity(version); previous = version; version = request.target.state_revision;
      return { receipt: { schema: 'run-play-return-receipt-v1', from, to: identity(version), previous_endpoint: from }, points: points() };
    },
    close: async () => calls.push('close'),
  };
  let bound = !noGame;
  const deps = { commands: ctx.commands, binding: async (_agent, _config, options) => {
    if (!bound && !options.create) return null;
    bound = true; return binding;
  },
    openWorkbench: async () => { opens++; return workbench; } };
  // The model's first tool call should be the only action before cancellation.
  let reply = 0;
  class Adapter extends LlmAdapter {
    async *stream(request) {
      requests.push(request);
      if (requests.length <= transientFailures) {
        if (partialFailure) {
          yield { type: 'block-start', index: 0, blockType: 'text' };
          yield { type: 'text-delta', index: 0, text: 'Partial response' };
        }
        yield { type: 'finish', reason: { kind: 'error', failure: { code: 'SERVER', status: 503, message: 'Fixture service busy' } } };
        return;
      }
      const first = ++reply === 1 && actOnFirst;
      const block = first ? { type: 'tool-call', id: 'advance-1', name: 'sts', arguments: '{"command":"advance"}' }
        : { type: 'text', text: 'Finished.' };
      yield { type: 'block-start', index: 0, blockType: block.type };
      yield first ? { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments }
        : { type: 'text-delta', index: 0, text: block.text };
      yield { type: 'block-end', index: 0, block };
      if (usage) yield { type: 'usage', usage };
      yield { type: 'finish', reason: { kind: first ? 'tool-calls' : 'stop' } };
    }
  }
  ctx.llm.registerAdapter(['fixture'], new Adapter());
  const config = { agentLocale, manual };
  let control = new NativeGameControl(handle.agent, config, deps);
  registerNativeCommands(ctx);
  ctx.tools.register(defineTool({ name: 'sts', description: 'fixture', parameters: { command: { type: 'string', required: true } },
    output: { schema: { type: 'string' }, render: (_args, value) => [{ type: 'text', text: value }] },
    execute: async ({ command }, exec) => (await control.execute(command, exec.signal)).text }));
  // Control opens existing game, model later reuses exactly that workbench.
  if (!noGame) await control.points();
  t.after(async () => { release.resolve(); await handle.dispose(); await control.close(); await ctx.fiber.dispose(); });
  return { ctx, handle, started, release, calls, requests, points, binding, workbench,
    get control() { return control; }, get opens() { return opens; }, get version() { return version; },
    failRead(value) { failRead = value; },
    async reload() { await control.close(); control = new NativeGameControl(handle.agent, config, deps); },
  };
}

test('Chinese manual resume keeps actual reading and return receipt verbatim', async t => {
  const f = await fixture(t, { agentLocale: 'zh-CN', manual: true, actOnFirst: false });
  const listed = await f.control.points();
  const restored = await f.control.restore({ schema: 'run-play-return-request-v1', source: listed.points.source, target: listed.points.targets[0].identity });
  assert.equal(restored.status, 'ready');
  await f.control.resume();
  await f.handle.agent.whenIdle();
  assert.equal(f.requests.length, 1);
  const strings = value => typeof value === 'string' ? [value]
    : value && typeof value === 'object' ? Object.values(value).flatMap(strings) : [];
  const context = strings(f.requests[0]).find(text => text.startsWith('用户已明确通过工作台继续此对局'));
  assert.ok(context);
  assert.match(context, /当前是依据公开信息手动出牌/);
  assert.match(context, /Actual game state 1/);
  assert.match(context, /Commands: act-current-1/);
  assert.ok(context.includes(JSON.stringify(restored.receipt)), 'the actual receipt is inserted unchanged');
  assert.deepEqual(f.calls.filter(command => command === 'advance'), []);
});

test('Chinese request limits pause at the same next-request boundary with one actual action', async t => {
  const f = await fixture(t, { agentLocale: 'zh-CN' });
  const configured = f.control._budget({ maxRequests: 1, maxTokensPerRequest: 8192 });
  assert.match(configured.message, /实验预算已配置/);
  assert.equal(f.control.paused, false, 'budget configuration leaves the pause state unchanged');
  f.handle.agent.send(message('Play'), 'next-turn', true);
  await f.handle.agent.whenIdle();
  await f.control.budgetPause;
  assert.equal(f.requests.length, 1);
  assert.equal(f.version, 3);
  assert.equal(f.calls.filter(command => command === 'advance').length, 1);
  assert.equal(f.control.paused, true);
  assert.equal(f.control.status().budgetStop.reason, '已达到请求次数上限');
  await assert.rejects(f.control.execute('advance', new AbortController().signal), /游戏已在工作台暂停/);
});

test('native experiment stops before a second model request and preserves pause across control reattachment', async t => {
  const f = await fixture(t);
  const budget = await f.ctx.commands.execute(f.handle.agent, '/sts-budget {"maxRequests":1,"maxTokensPerRequest":1234}', [], new AbortController().signal);
  assert.equal(budget.result.kind, 'success');
  f.handle.agent.send(message('Play'), 'next-turn', true);
  await f.handle.agent.whenIdle();
  await f.control.budgetPause;
  assert.equal(f.requests.length, 1);
  assert.equal(f.requests[0].maxTokens, 1234);
  assert.equal(f.control.paused, true);
  assert.equal(f.control.budgetStop.reason, 'request limit reached');
  assert.ok(workspaceFacts(f.handle.agent.session.snapshotEvents()).some(fact => fact.kind === 'paused'));
  await f.control.resume(); await f.handle.agent.whenIdle(); await f.control.budgetPause;
  assert.equal(f.requests.length, 1, 'resume does not reset an exhausted request budget');
  assert.equal(f.control.paused, true);
  await f.reload();
  assert.equal(f.control.paused, true);
  assert.equal(f.control.budget.maxRequests, 1);
  await f.ctx.commands.execute(f.handle.agent, '/sts-budget null', [], new AbortController().signal);
  assert.equal(f.control.budget, null);
  assert.equal(f.control.paused, true, 'clearing a budget never resumes a player');
});

test('native controls capture exact SDK inputs without provider delivery events or duplicate message snapshots', async t => {
  const f = await fixture(t);
  f.handle.agent.followup(message('Play once.'));
  await f.handle.agent.whenIdle();
  const log = { meta: f.handle.agent.session.header, events: f.handle.agent.session.snapshotEvents(), inheritedEventCount: 0 };
  const boundaries = log.events.filter(event => event.type === 'plugin:sts/request-boundary');
  assert.equal(boundaries.length, f.requests.length);
  assert.equal(boundaries.length, 2);
  assert.equal(log.events.some(event => event.type === 'session-log-deepseek/delivery-accepted'), false);
  boundaries.forEach((event, index) => {
    assert.equal(event.ignorable, true);
    assert.equal(event.data.snapshot, undefined);
    const { signal, ...expected } = f.requests[index];
    assert.deepEqual(requestFromBoundary(log, event.data), JSON.parse(JSON.stringify(expected)));
  });
  assert.throws(() => requestFromBoundary(log, { ...boundaries[0].data, requestHash: 'wrong' }), /input digest/);
  assert.deepEqual(f.calls.filter(call => call === 'advance'), ['advance']);
});

test('reported usage threshold rejects the next request, using disjoint official usage buckets', async t => {
  const f = await fixture(t, { usage: { inputTokens: 4, outputTokens: 8, cacheReadTokens: 1000, cacheWriteTokens: 0 } });
  await f.ctx.commands.execute(f.handle.agent, '/sts-budget {"maxRequests":10,"maxReportedTokens":12}', [], new AbortController().signal);
  f.handle.agent.send(message('Play'), 'next-turn', true);
  await f.handle.agent.whenIdle(); await f.control.budgetPause;
  assert.equal(f.requests.length, 1);
  assert.equal(f.control.status().budget.reportedTokens, 12);
  assert.equal(f.control.budgetStop.reason, 'reported uncached input plus output threshold reached');
});

test('raising the output allowance replaces the earlier persisted experiment limit', async t => {
  const f = await fixture(t, { actOnFirst: false });
  for (const limit of [1234, 8192]) {
    await f.ctx.commands.execute(f.handle.agent,
      '/sts-budget ' + JSON.stringify({ maxRequests: 3, maxTokensPerRequest: limit }), [], new AbortController().signal);
    f.handle.agent.send(message('One reply.'), 'next-turn', true);
    await f.handle.agent.whenIdle();
    assert.equal(f.requests.at(-1).maxTokens, limit);
  }
  assert.equal(f.requests.length, 2);
});

test('missing provider usage stops a token-limited experiment instead of treating missing as zero', async t => {
  const f = await fixture(t);
  await f.ctx.commands.execute(f.handle.agent, '/sts-budget {"maxReportedTokens":100}', [], new AbortController().signal);
  f.handle.agent.send(message('Play'), 'next-turn', true);
  await f.handle.agent.whenIdle(); await f.control.budgetPause;
  assert.equal(f.requests.length, 1);
  assert.match(f.control.budgetStop.reason, /usage unavailable/);
});

test('a pre-response HTTP error permits the official retry and still consumes a request admission', { timeout: 10000 }, async t => {
  const f = await fixture(t, { transientFailures: 1, actOnFirst: false,
    usage: { inputTokens: 4, outputTokens: 8, cacheReadTokens: 1000, cacheWriteTokens: 0 } });
  await f.ctx.commands.execute(f.handle.agent, '/sts-budget {"maxRequests":3,"maxReportedTokens":100}', [], new AbortController().signal);
  f.handle.agent.send(message('Play'), 'next-turn', true);
  await f.handle.agent.whenIdle();
  assert.equal(f.requests.length, 2);
  assert.equal(f.control.paused, false);
  assert.equal(f.control.status().budget.admittedRequests, 2);
  assert.equal(f.control.status().budget.reportedTokens, 12);
  assert.equal(f.control.status().budget.unmeteredProviderErrors, 1);
  assert.equal(f.control.status().budget.missingUsageResponses, 0);
  assert.equal(f.version, 2);
});

test('request admission limit also stops an official retry after a pre-response HTTP error', { timeout: 10000 }, async t => {
  const f = await fixture(t, { transientFailures: 2, actOnFirst: false });
  await f.ctx.commands.execute(f.handle.agent, '/sts-budget {"maxRequests":1,"maxReportedTokens":100}', [], new AbortController().signal);
  f.handle.agent.send(message('Play'), 'next-turn', true);
  await f.handle.agent.whenIdle(); await f.control.budgetPause;
  assert.equal(f.requests.length, 1);
  assert.equal(f.control.paused, true);
  assert.equal(f.control.budgetStop.reason, 'request limit reached');
  assert.equal(f.control.status().budget.unmeteredProviderErrors, 1);
});

test('a partial response without usage stops before retry even when followed by HTTP503', { timeout: 10000 }, async t => {
  const f = await fixture(t, { transientFailures: 1, partialFailure: true, actOnFirst: false });
  await f.ctx.commands.execute(f.handle.agent, '/sts-budget {"maxRequests":3,"maxReportedTokens":100}', [], new AbortController().signal);
  f.handle.agent.send(message('Play'), 'next-turn', true);
  await f.handle.agent.whenIdle(); await f.control.budgetPause;
  assert.equal(f.requests.length, 1);
  assert.equal(f.control.paused, true);
  assert.match(f.control.budgetStop.reason, /usage unavailable/);
  assert.equal(f.control.status().budget.unmeteredProviderErrors, 0);
});

test('experiment deadline pauses even an idle chat without creating a game', async t => {
  const f = await fixture(t, { noGame: true });
  const deadlineAt = new Date(Date.now() + 25).toISOString();
  await f.ctx.commands.execute(f.handle.agent, '/sts-budget ' + JSON.stringify({ deadlineAt }), [], new AbortController().signal);
  await new Promise(resolve => setTimeout(resolve, 65));
  await f.control.budgetPause;
  assert.equal(f.control.paused, true);
  assert.equal(f.control.budgetStop.reason, 'deadline reached');
  assert.equal(f.requests.length, 0);
  assert.equal(f.opens, 0);
});

test('invalid or running budget configuration leaves the previous budget unchanged', async t => {
  const f = await fixture(t, { inFlight: true });
  await f.ctx.commands.execute(f.handle.agent, '/sts-budget {"maxRequests":2}', [], new AbortController().signal);
  let reply = await f.ctx.commands.execute(f.handle.agent, '/sts-budget {"deadlineAt":"tomorrow"}', [], new AbortController().signal);
  assert.equal(reply.result.kind, 'error');
  assert.equal(f.control.budget.maxRequests, 2);
  f.handle.agent.send(message('Play'), 'next-turn', true); await f.started.promise;
  reply = await f.ctx.commands.execute(f.handle.agent, '/sts-budget null', [], new AbortController().signal);
  assert.equal(reply.result.kind, 'error');
  assert.equal(f.control.budget.maxRequests, 2);
  f.release.resolve(); await f.handle.agent.whenIdle();
});

test('pause waits for an in-flight action, parks waking messages without requests, and resumes with the actual returned state', { timeout: 10000 }, async t => {
  const f = await fixture(t, { inFlight: true });
  f.handle.agent.followup(message('Run once.'));
  await f.started.promise;
  const waiting = f.control.pause();
  f.handle.agent.followup(message('Keep this queued correction.'));
  let stopped = false; waiting.then(() => { stopped = true; });
  await Promise.resolve(); assert.equal(stopped, false, 'in-flight game action is still settling');
  f.release.resolve();
  const paused = await waiting;
  assert.equal(paused.control.paused, true); assert.equal(f.version, 3);
  assert.equal(f.requests.length, 1); assert.equal(paused.control.queued, 1);
  assert.equal(f.handle.agent.status, 'idle');
  f.handle.agent.followup(message('Another queued instruction.'));
  await f.handle.agent.whenIdle();
  assert.equal(f.requests.length, 1); assert.equal(f.control.status().queued, 2);
  const listed = await f.control.points();
  const restored = await f.control.restore({ schema: 'run-play-return-request-v1', source: listed.points.source, target: listed.points.targets[0].identity });
  assert.equal(restored.status, 'ready'); assert.equal(f.version, 1); assert.equal(f.requests.length, 1);
  assert.equal(restored.points.targets.at(-1).origins[0].kind, 'previous_endpoint');
  await f.control.resume(); await f.handle.agent.whenIdle();
  assert.equal(f.control.status().paused, false); assert.ok(f.opens > 1, 'resume reconnects to the same durable game after paused connections were released');
  const delivered = JSON.stringify(f.requests.slice(1));
  assert.match(delivered, /Keep this queued correction/); assert.match(delivered, /Another queued instruction/);
  assert.match(delivered, /Actual game state 1/); assert.match(delivered, /Latest new return receipt/);
  assert.match(delivered, /Reuse options and candidates only when bound to this current state/);
  assert.doesNotMatch(delivered, /Earlier option objects, candidate IDs and predicted continuations are stale/);
  assert.deepEqual(f.calls.filter(call => call === 'advance'), ['advance']);
  assert.ok(workspaceFacts(f.handle.agent.session.snapshotEvents()).some(fact => fact.kind === 'returned'));
});

test('pure pause and resume supplies the checked state without declaring unchanged saved wins stale', async t => {
  const f = await fixture(t, { actOnFirst: false });
  await f.control.pause();
  await f.control.resume(); await f.handle.agent.whenIdle();
  const delivered = JSON.stringify(f.requests[0]);
  assert.match(delivered, /Actual game state 2/);
  assert.match(delivered, /checked current-state read/);
  assert.match(delivered, /Pausing alone does not invalidate unchanged-root saved wins/);
  assert.doesNotMatch(delivered, /return receipt|are stale/);
  assert.equal(f.version, 2);
  assert.equal(f.calls.filter(call => call === 'advance').length, 0);
});

test('a return receipt is supplied once, surviving owner replacement and allowing a later new return', async t => {
  const f = await fixture(t, { actOnFirst: false });
  await f.control.pause();
  await f.control.restore({ source: f.points().source, target: identity(1) });
  await f.reload();
  await f.control.resume(); await f.handle.agent.whenIdle();
  assert.match(JSON.stringify(f.requests.at(-1)), /Latest new return receipt/);
  assert.match(JSON.stringify(f.requests.at(-1)), /Actual game state 1/);
  await f.control.pause(); await f.reload();
  await f.control.resume(); await f.handle.agent.whenIdle();
  const secondRequest = f.requests.at(-1);
  const second = secondRequest.messages.findLast(item => item.source?.kind === 'sts-workspace').content[0].text;
  assert.doesNotMatch(second, /Latest new return receipt/);
  assert.match(second, /Actual game state 1/);
  assert.equal((JSON.stringify(secondRequest).match(/Latest new return receipt/g) ?? []).length, 1,
    'the earlier delivered receipt remains in conversation history without a fresh duplicate');
  await f.control.pause();
  const listed = f.points(), previous = listed.targets.find(item => item.origins.some(origin => origin.kind === 'previous_endpoint'));
  await f.control.restore({ source: listed.source, target: previous.identity });
  await f.control.resume(); await f.handle.agent.whenIdle();
  const third = f.requests.at(-1).messages.findLast(item => item.source?.kind === 'sts-workspace').content[0].text;
  assert.match(third, /Latest new return receipt/);
  assert.match(third, /Actual game state 2/);
  assert.equal(f.requests.length, 3);
  assert.equal(f.calls.filter(call => call === 'advance').length, 0);
});

test('a committed return with failed read is still supplied after a failed resume enqueue and retry', async t => {
  const f = await fixture(t, { actOnFirst: false });
  await f.control.pause();
  f.failRead(true);
  const returned = await f.control.restore({ source: f.points().source, target: identity(1) });
  assert.equal(returned.status, 'error'); assert.equal(returned.receipt.to.state_revision, 1);
  f.failRead(false);
  const send = f.handle.agent.send;
  f.handle.agent.send = () => { throw new Error('fixture enqueue failed'); };
  const failed = await f.control.resume();
  assert.equal(failed.status, 'error'); assert.match(failed.message, /enqueue failed/);
  assert.equal(f.control.paused, true); assert.equal(f.requests.length, 0);
  f.handle.agent.send = send;
  await f.reload();
  await f.control.resume(); await f.handle.agent.whenIdle();
  assert.match(JSON.stringify(f.requests[0]), /Latest new return receipt/);
  assert.match(JSON.stringify(f.requests[0]), /Actual game state 1/);
  assert.equal(f.control.paused, false); assert.equal(f.version, 1);
});

test('paused game reads release the executable and reconnect without advancing the saved game', async t => {
  const f = await fixture(t);
  const first = await f.control.pause();
  assert.equal(first.status, 'ready');
  assert.equal(f.control.workbench, undefined);
  const opens = f.opens, version = f.version;
  assert.match(await f.control.live(), /Actual game state 2/);
  assert.equal(f.control.workbench, undefined);
  assert.equal(f.opens, opens + 1);
  assert.equal(f.version, version);
  assert.equal(f.requests.length, 0);
});

test('pausing and observing a terminal game report its outcome without querying a missing inventory', async t => {
  const f = await fixture(t);
  f.workbench.observe = async () => ({ head_revision: 2, state_revision: 2,
    position: { kind: 'terminal', outcome: { kind: 'failure' } } });
  const execute = f.workbench.execute;
  f.workbench.execute = async command => {
    assert.equal(command, 'read', 'terminal positions have no current inventory decision');
    await execute(command);
    return { text: 'Terminal | Failure { reason: CommittedDefeat, game_outcome: Some(Defeat) }' };
  };
  const paused = await f.control.pause();
  assert.equal(paused.status, 'ready'); assert.equal(paused.control.paused, true);
  assert.match(paused.text, /CommittedDefeat/);
  assert.match(await f.control.live(), /CommittedDefeat/);
  assert.deepEqual(f.calls, ['read', 'close', 'read', 'close']); assert.equal(f.requests.length, 0);
});

test('human pause before the first game call is reversible without creating a run or blocking future play', { timeout: 10000 }, async t => {
  const f = await fixture(t, { noGame: true });
  assert.equal((await f.control.pause()).status, 'ready');
  assert.equal(f.opens, 0); assert.equal(f.control.paused, true); assert.equal(f.requests.length, 0);
  assert.equal((await f.control.resume()).status, 'ready');
  await f.handle.agent.whenIdle();
  assert.equal(f.opens, 1); assert.equal(f.control.paused, false); assert.equal(f.version, 3);
  assert.match(JSON.stringify(f.requests[0]), /No game has been created.*Begin with sts read/);
});

test('paused state survives capability replacement; reads stay lazy and failed return preserves the confirmed receipt', { timeout: 10000 }, async t => {
  const f = await fixture(t);
  await f.control.pause(); await f.reload();
  assert.equal(nativeGameControl(f.handle.agent), f.control); assert.equal(f.control.paused, true);
  assert.equal(f.opens, 1, 'attaching a paused capability does not open another game');
  f.handle.agent.followup(message('Do not lose this while paused.')); await f.handle.agent.whenIdle();
  assert.equal(f.requests.length, 0); assert.equal(f.control.status().queued, 1);
  const listed = await f.control.points();
  const rejected = await f.control.restore({ source: { ...listed.points.source, session_path: 'foreign' }, target: identity(1) });
  assert.equal(rejected.status, 'error'); assert.equal(f.version, 2); assert.equal(rejected.receipt, undefined);
  f.failRead(true);
  const partial = await f.control.restore({ source: listed.points.source, target: identity(1) });
  assert.equal(partial.status, 'error'); assert.equal(partial.receipt.to.state_revision, 1);
  assert.equal(partial.control.paused, true); assert.equal(f.requests.length, 0);
  f.failRead(false);
  const now = await f.control.points(), endpoint = now.points.targets.find(item => item.origins.some(origin => origin.kind === 'previous_endpoint'));
  assert.equal((await f.control.restore({ source: now.points.source, target: endpoint.identity })).status, 'ready');
  assert.equal(f.version, 2); assert.equal(f.requests.length, 0);
  await assert.rejects(f.control.execute('advance', new AbortController().signal), /paused/);
});

test('return-point observation does not resolve cold Agents, and mutations reach the retained owner', { timeout: 10000 }, async t => {
  const f = await fixture(t);
  // journal() identity binding is exercised by native-inspection.test; here
  // isolate control routing and the Remote write contract from disk location.
  let resolves = 0, coldQueries = 0;
  const view = new StsObservationView(f.ctx, { resolveAgent: async id => { resolves++; return { agent: f.handle.agent }; },
    execute: async () => { coldQueries++; return { stdout: JSON.stringify(f.points()) }; } });
  view.journal = async id => ({ binding: f.binding, agent: { id, session: { header: {} } }, log: { events: [] } });
  const warm = await view.points(String(f.handle.agent.id));
  assert.equal(warm.status, 'ready'); assert.equal(resolves, 0); assert.equal(coldQueries, 0);
  assert.equal((await view.live(String(f.handle.agent.id))).status, 'ready'); assert.equal(resolves, 0); assert.equal(f.opens, 1);
  await view.pause(String(f.handle.agent.id)); assert.equal(resolves, 1); assert.equal(f.control.paused, true);
  const result = await view.restore(String(f.handle.agent.id), { schema: 'run-play-return-request-v1', source: f.points().source, target: identity(1) });
  assert.equal(result.receipt.to.state_revision, 1); assert.equal(result.control.paused, true); assert.ok(result.throughSeq > 0);
  await assert.rejects(view.pause('another-chat'), /does not match/);
  await assert.rejects(view.pause('another-chat', 'zh-CN'), /不一致/);
  assert.equal(f.requests.length, 0);
  for (const projection of [observationTickProjection, mapTickProjection]) {
    const event = { type: 'command/run', seq: 99, data: { name: 'sts-return', commandId: '99' } };
    const applied = projection.apply(projection.init(), event);
    assert.equal(typeof applied === 'number' ? applied : applied.seq, 99);
  }
});
