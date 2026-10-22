import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, sep } from 'node:path';
import { Context } from '@deepseek-ai/cordis';
import LlmRuntime, { LlmAdapter, LlmError, createUserMessage } from '@deepseek-ai/dsh-llm';
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session';
import ProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import AgentRegistry from '@deepseek-ai/dsh-agent';
import AgentLoop from '@deepseek-ai/dsh-agent-loop';
import ToolRuntime, { defineTool } from '@deepseek-ai/dsh-tools';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import * as SessionPersistence from '@deepseek-ai/dsh-session-persistence-jsonl';
import * as LlmRetry from '@deepseek-ai/dsh-llm-retry';
import GameContextCompaction, { requestDecisionReset } from './context.mjs';
import { controlPlayer } from './launch.mjs';
import { bindCoachedPlayer } from './supervision.mjs';
import { recordPlayerRequests, readPlayerJournal, inspectPlayer, inspectJournal, renderInspection, parseInspectionArgs } from './inspection.mjs';

async function fixture(t) {
  const base = await realpath(tmpdir());
  const repository = await mkdtemp(join(base, 'sts-player-inspection-'));
  const session = join(repository, 'game'), home = join(repository, 'dsh');
  await mkdir(session); await mkdir(join(home, 'runs'), { recursive: true });
  const ctx = new Context();
  t.after(async () => {
    await ctx.fiber.dispose();
    const target = await realpath(repository);
    assert.ok(target.startsWith(base + sep) && target.split(/[\\/]/).at(-1).startsWith('sts-player-inspection-'));
    await rm(target, { recursive: true });
  });
  for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime, TokenMeter]) await ctx.plugin(plugin);
  await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
  await ctx.plugin(SessionPersistence, { root: join(home, 'sessions'), compression: 'none' });
  await ctx.plugin(GameContextCompaction, { auto: false });
  await ctx.plugin(AgentLoop, { agents: [], maxParallelToolCalls: 1 });
  await ctx.plugin(LlmRetry);
  let guide = 'Earlier guide: source facts are supplied.', response = 0;
  ctx.systemPrompt.section({ name: 'fixture', order: 300, text: () => guide });
  const dispatched = [];
  ctx.on('llm/stream', (request, next) => {
    const { signal, ...plain } = request;
    dispatched.push(JSON.parse(JSON.stringify(plain)));
    return next();
  });
  const executed = [];
  ctx.tools.register(defineTool({ name: 'sts', description: 'A bound game tool', parameters: {
    command: { type: 'string', required: true },
  }, output: { schema: { type: 'string' }, render: (_args, value) => [{ type: 'text', text: value }] },
  execute({ command }, exec) {
    executed.push(command);
    if (command === 'proceed') { exec.concludeTurn(); return 'Pending proposal (not executed): proceed'; }
    return 'Act 1 F6 | HP 73/75 | Rewards complete\nproceed | Effect: leave rewards';
  } }));
  class Adapter extends LlmAdapter {
    providerRetryPolicy() { return { mode: 'normal', maxRetries: 2, retryableCodes: ['TRANSPORT'],
      initialDelayMs: 1, maxDelayMs: 1, jitterRatio: 0 }; }
    async *stream() {
      response++;
      if (response === 2) {
        yield { type: 'block-start', index: 0, blockType: 'text' };
        yield { type: 'text-delta', index: 0, text: 'Uncommitted failed output.' };
        throw new LlmError('Fixture transport failure', 'TRANSPORT');
      }
      if (response > 4) {
        yield { type: 'block-start', index: 0, blockType: 'text' };
        yield { type: 'text-delta', index: 0, text: 'New task completed.' };
        yield { type: 'block-end', index: 0, block: { type: 'text', text: 'New task completed.' } };
        yield { type: 'finish', reason: { kind: 'stop' } };
        return;
      }
      const command = response === 1 ? 'read' : response === 3 ? 'options' : 'proceed';
      const block = { type: 'tool-call', id: `call-${response}`, name: 'sts', arguments: JSON.stringify({ command }) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  ctx.llm.registerAdapter(['fixture'], new Adapter());
  const id = 'player-inspection';
  let readingForReset, sourceForReset;
  const handle = await ctx.agents.create({ sessionId: SessionId(id), meta: { cwd: repository },
    agentOptions: { provider: 'fixture', model: 'fixture' },
    setup: async scoped => {
      scoped.on('system-prompt/assemble', (_assembly, context, next) => {
        if (context.agent && readingForReset) {
          requestDecisionReset(context.agent, readingForReset, sourceForReset);
          readingForReset = undefined;
        }
        return next();
      });
    } });
  t.after(() => handle.dispose());
  const source = { schema: 'sts-run-play-agent-observation-v3', session_id: 'game', head_revision: 1, state_revision: 1 };
  const reading = 'Act 1 F6 | HP 73/75 | Rewards complete\nDeck: 19 cards\nproceed | Effect: leave rewards';
  const output = join(home, 'runs', `${id}-1000-first`); await mkdir(output);
  const eventStart = handle.agent.session.snapshotEvents().length;
  const close = recordPlayerRequests(ctx, { id, session: () => handle.agent.session, output });
  readingForReset = reading; sourceForReset = source;
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Earlier operator task.' }] }));
  await handle.agent.whenIdle(); close();
  const settled = handle.agent.session.snapshotEvents().filter(event => event.type === 'turn/end').at(-1).data.reason;
  assert.equal(settled.kind, 'completed', JSON.stringify(settled));
  await ctx.sessions.flush(handle.agent.session);
  const manifest = { id, repository, session, eventStart, task: 'Earlier operator task.' };
  const result = { outcome: 'needs-review', eventEnd: handle.agent.session.snapshotEvents().length };
  const finalObservation = { ...source, position: { kind: 'decision', observation: { domain: 'strategic', observation: {
    run: { act: 1, floor: 6, current_hp: 73, max_hp: 75, gold: 96 }, context: { kind: 'reward' },
  } } } };
  for (const [name, value] of Object.entries({ 'manifest.json': manifest, 'result.json': result, 'final-observation.json': finalObservation })) {
    await writeFile(join(output, name), JSON.stringify(value));
  }
  await bindCoachedPlayer(session, { id, home, running: false, lastRun: output });
  const firstRequests = dispatched.slice();
  guide = 'New guide: unrelated later task.';
  readingForReset = 'Act 1 F7 | Campfire\nNew inventory'; sourceForReset = { ...source, head_revision: 2, state_revision: 2 };
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'New operator task.' }] }));
  await handle.agent.whenIdle();
  await ctx.sessions.flush(handle.agent.session);
  return { ctx, repository, session, home, id, output, manifest, result, finalObservation, firstRequests, executed };
}

test('historical requests exactly match SDK dispatch across retry and later context replacement', async t => {
  const f = await fixture(t);
  const log = await readPlayerJournal(f.home, f.id);
  const boundaries = (await readFile(join(f.output, 'requests.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.equal(boundaries.length, 4);
  assert.ok(boundaries.every(row => !row.snapshot), 'ordinary boundaries do not duplicate the input');
  const report = inspectJournal(log, { directory: f.output, manifest: f.manifest, result: f.result,
    finalObservation: f.finalObservation }, boundaries);
  assert.equal(report.steps.length, 3);
  const requests = report.steps.flatMap(step => step.requests);
  assert.deepEqual(requests.map(entry => entry.request), f.firstRequests);
  assert.ok(requests.every(entry => entry.precision === 'sdk-verified'));
  assert.deepEqual(report.steps[1].requests.map(entry => entry.attempt), [1, 2]);
  assert.deepEqual(requests[1].request, requests[2].request);
  assert.doesNotMatch(JSON.stringify(requests), /Uncommitted failed output|New guide|New inventory|New operator/);
  assert.deepEqual(report.steps.flatMap(step => step.calls.map(call => call.name)), ['sts', 'sts', 'sts']);
  const text = renderInspection(report);
  assert.equal(text.split('Earlier operator task.').length - 1, 1, 'the default view does not reprint unchanged input');
  assert.match(text, /same as first supplied source/);
  assert.match(renderInspection(report, { step: 2, attempt: 2, input: true }), /Earlier guide/);
  assert.match(renderInspection(report, { step: 2 }), /Uncommitted failed output\./);
  const chinese = inspectJournal(log, { directory: f.output, manifest: f.manifest, result: f.result,
    finalObservation: f.finalObservation }, boundaries, 'zh-CN');
  assert.deepEqual(chinese, report, 'inspection language does not change the structured report or original requests');
  const chineseText = renderInspection(chinese, { locale: 'zh-CN' });
  assert.match(chineseText, /^玩家 player-inspection/);
  assert.match(chineseText, /已记录的运行事件范围/);
  assert.match(chineseText, /所供读数：\nAct 1 F6/);
  assert.match(chineseText, /与首次所供来源相同/);
  assert.ok(chineseText.includes('Earlier operator task.'));
  assert.match(renderInspection(chinese, { step: 2, locale: 'zh-CN' }), /Uncommitted failed output\./);
  const rawInput = renderInspection(chinese, { step: 1, input: true, locale: 'zh-CN' });
  assert.deepEqual(JSON.parse(rawInput.slice(rawInput.indexOf('\n{') + 1)), f.firstRequests[0]);
  const localized = structuredClone(report);
  for (const item of localized.steps) for (const entry of item.requests) {
    const checkpoint = entry.request?.messages.find(message => message.source?.kind === 'sts-decision');
    if (!checkpoint) continue;
    const prefix = '说明含 Act 99 F99 | 这不是局面起点\n';
    const reading = '第二幕 F24 | 生命 55/75 | 营火\n当前牌组 23 张\n';
    checkpoint.content = [{ type: 'text', text: prefix + reading + '查询附录' }];
    checkpoint.source.readingRange = { start: prefix.length, end: prefix.length + reading.length };
  }
  const translated = renderInspection(localized);
  assert.match(translated, /Supplied reading:\n第二幕 F24/);
  assert.doesNotMatch(translated, /这不是局面起点|查询附录/);
  const corruptedRange = localized.steps[0].requests[0].request.messages.find(message => message.source?.kind === 'sts-decision');
  corruptedRange.source.readingRange.end = Number.MAX_SAFE_INTEGER;
  assert.throws(() => renderInspection(localized), /reading range is invalid/);
});

test('inspection uses a read handle, scopes the selected run, and never connects a game or starts a model', async t => {
  const f = await fixture(t);
  const journalPath = await f.ctx.sessionPersistence.resolveCurrentLog(SessionId(f.id));
  const before = await readFile(journalPath);
  for (const locale of ['en', 'zh-CN']) {
    const output = [];
    const inspected = await controlPlayer({ repository: f.repository, session: f.session, parameters: ['inspect', '1'], locale }, text => output.push(text), {
      openWorkbench() { assert.fail('inspection must not open Rust'); }, startPlayer() { assert.fail('inspection must not call a model'); },
    });
    assert.equal(inspected.outcome, 'inspected');
    assert.equal(inspected.report.steps.length, 3, 'later conversation turns are outside the selected run');
    assert.match(output.join('\n'), locale === 'en' ? /^Player / : /^玩家 /);
    assert.deepEqual(await readFile(journalPath), before);
  }
  assert.deepEqual(f.executed, ['read', 'options', 'proceed']);
});

test('missing or damaged dispatch evidence stays explicit and old journals are not called verified', async t => {
  const f = await fixture(t);
  const log = await readPlayerJournal(f.home, f.id);
  const run = { directory: f.output, manifest: f.manifest, result: f.result, finalObservation: f.finalObservation };
  const older = inspectJournal(log, run, null);
  assert.ok(older.steps.flatMap(step => step.requests).every(entry => entry.precision === 'journal-only'));
  assert.match(renderInspection(older), /dispatch boundary was not recorded/);
  const rows = (await readFile(join(f.output, 'requests.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  rows[0].requestHash = '0'.repeat(64);
  const corrupted = inspectJournal(log, run, rows);
  assert.equal(corrupted.steps[0].requests[0].precision, 'unavailable');
  assert.match(renderInspection(corrupted), /hash does not match/);
  const chineseCorrupted = inspectJournal(log, run, rows, 'zh-CN');
  assert.equal(chineseCorrupted.steps[0].requests[0].precision, 'unavailable');
  assert.match(renderInspection(chineseCorrupted, { locale: 'zh-CN' }), /请求摘要与已保存的派发记录不匹配/);
  delete rows[0].requestHash;
  assert.equal(inspectJournal(log, run, rows).steps[0].requests[0].precision, 'unavailable');
  rows[0].throughSeq = log.events.length + 1;
  assert.equal(inspectJournal(log, run, rows).steps[0].requests[0].precision, 'unavailable');
  assert.throws(() => renderInspection(older, { step: 4 }), /No step 4/);
  assert.throws(() => renderInspection(older, { step: 2, attempt: 3 }), /No attempt 3/);
  const partial = inspectJournal(log, run, []);
  assert.match(renderInspection(partial), /No dispatched request recorded/);
  const snapshotRows = (await readFile(join(f.output, 'requests.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  snapshotRows[0].snapshot = f.firstRequests[0];
  snapshotRows[0].throughSeq = log.events.find(event => event.type === 'step/start').seq;
  const explicit = inspectJournal(log, run, snapshotRows);
  assert.equal(explicit.steps[0].requests[0].precision, 'sdk-snapshot');
  assert.deepEqual(explicit.steps[0].requests[0].request, f.firstRequests[0]);
  snapshotRows[0].throughSeq += 0.5;
  assert.equal(inspectJournal(log, run, snapshotRows).steps[0].requests[0].precision, 'unavailable');

  const failedLog = structuredClone(log);
  const failed = failedLog.events.find(event => event.type === 'assistant/attempt');
  failed.data.stream.push({ type: 'chunk', time: failed.time, chunk: { type: 'block-end', index: 1,
    block: { type: 'tool-call', id: 'call-3', name: 'sts', arguments: '{"command":"options"}' } } });
  const failedStep = inspectJournal(failedLog, run, null).steps[1];
  assert.equal(failedStep.calls.length, 2);
  assert.equal(failedStep.calls[0].started, false, 'a failed stream cannot borrow a retried call execution');
  assert.equal(failedStep.calls[0].result, undefined);
  assert.equal(failedStep.calls[1].started, true);
  assert.ok(failedStep.calls[1].result);
});

test('inspection scope remains typed when there is no recorded turn', () => {
  const report = inspectJournal({ events: [] }, { directory: 'empty-run', manifest: { id: 'empty-player' } }, []);
  assert.equal(report.scope, 'noRecordedTurn');
  assert.match(renderInspection(report), /No recorded model turn/);
  assert.match(renderInspection(report, { locale: 'zh-CN' }), /没有已记录的模型轮次/);
});

test('inspection rejects invalid selectors and foreign run identities', async t => {
  for (const args of [['0'], ['1.0'], ['1', 'output'], ['input'], ['--run', '../run'], ['--run'], ['1', 'input', 'extra']]) {
    assert.throws(() => parseInspectionArgs(args), /player inspect/);
  }
  assert.deepEqual(parseInspectionArgs(['2.1', 'input', '--run', 'known-run']), { step: 2, attempt: 1, input: true, run: 'known-run' });
  const f = await fixture(t);
  const binding = { id: f.id, home: f.home, lastRun: f.output };
  await writeFile(join(f.output, 'manifest.json'), JSON.stringify({ ...f.manifest, session: join(f.repository, 'other-game') }));
  await assert.rejects(inspectPlayer({ repository: f.repository, session: f.session, binding }), /does not belong/);
});
