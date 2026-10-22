import test from 'node:test';
import assert from 'node:assert/strict';
import { copyFile, mkdtemp, realpath, rm } from 'node:fs/promises';
import { dirname, join, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import { Context } from '@deepseek-ai/cordis';
import LlmRuntime, { createUserMessage, createAssistantMessage, createMessage, ToolCallId } from '@deepseek-ai/dsh-llm';
import SessionStore, { Session } from '@deepseek-ai/dsh-session';
import ProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import GameContextCompaction, { gameCheckpoint, requestBranchReset,
  gameContextSource, sameGameContextSource, gameDecisionTask, sameGameDecisionTask,
  requestDecisionReset, decisionContextRange, contextQueryCommand, decisionContextSource, decisionQueryReferences,
  recordedGameReading } from './context.mjs';
import { runPlayer } from './launch.mjs';
import { commandTokens, commandGroups } from '../run_play/session.mjs';

const normalizeContextQuery = command => {
  try {
    const tokens = commandTokens(command);
    return contextQueryCommand(commandGroups(tokens[0], tokens.slice(1)));
  } catch { /* Invalid input is not eligible. */ }
};

async function fixture(t, { unfinished = false, fresh = false, experiments = [], restore = true } = {}) {
  const ctx = new Context();
  t.after(() => ctx.fiber.dispose());
  for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, TokenMeter, GameContextCompaction]) await ctx.plugin(plugin);
  const session = ctx.sessions.create();
  const text = value => [{ type: 'text', text: value }];
  const assistant = content => createAssistantMessage({ content, source: { provider: 'fixture', model: 'fixture' } });
  session.append('turn/start', { turn: 1 });
  session.append('step/start', { turn: 1, step: 1 });
  session.append('user/message', createUserMessage({ content: text('Complete A20H autonomously using Flash.'), source: { kind: 'user' } }), { surfaceOp: 'append' });
  if (!fresh) session.append('assistant/message', { turn: 1, step: 1,
    message: assistant(text(('Old branch owns Centennial Puzzle and has a winning plan.\n').repeat(120))), stream: [] }, { surfaceOp: 'append' });
  session.append('step/end', { turn: 1, step: 1 });
  if (fresh) return { ctx, session, agent: { session, options: { provider: 'fixture', model: 'fixture' } } };
  let step = 2;
  for (const [index, experiment] of experiments.entries()) {
    const id = ToolCallId(`experiment-${index}`);
    session.append('step/start', { turn: 1, step });
    session.append('assistant/message', { turn: 1, step, stream: [],
      message: assistant([{ type: 'tool-call', id, name: 'sts', arguments: JSON.stringify({ command: experiment.command }) }]) }, { surfaceOp: 'append' });
    session.append('tool/call', { turn: 1, step, callId: id, name: 'sts', arguments: JSON.stringify({ command: experiment.command }) });
    session.append('tool/result', { turn: 1, step, message: { ...createMessage({ role: 'tool', toolCallId: id,
      source: { kind: 'tool', callId: id }, content: text(experiment.text) }), isError: Boolean(experiment.error) },
      ...(experiment.meta ? { meta: experiment.meta } : {}) }, { surfaceOp: 'append' });
    session.append('step/end', { turn: 1, step });
    step++;
  }
  if (!restore) return { ctx, session, agent: { session, options: { provider: 'fixture', model: 'fixture' } } };
  session.append('step/start', { turn: 1, step });
  const id = ToolCallId('restore-call');
  session.append('assistant/message', { turn: 1, step,
    message: assistant([{ type: 'tool-call', id, name: 'sts', arguments: '{"command":"back floor 36"}' }]), stream: [] }, { surfaceOp: 'append' });
  session.append('tool/call', { turn: 1, step, callId: id, name: 'sts', arguments: '{"command":"back floor 36"}' });
  if (!unfinished) session.append('tool/result', { turn: 1, step, message: createMessage({ role: 'tool', toolCallId: id,
    source: { kind: 'tool', callId: id }, content: text('Completed: back floor 36\nCurrent relics: Burning Blood.\nCurrent HP: 53/72.') }) }, { surfaceOp: 'append' });
  if (!unfinished) session.append('step/end', { turn: 1, step });
  return { ctx, session, agent: { session, options: { provider: 'fixture', model: 'fixture' } } };
}

test('official restoration replaces all prior reasoning with fresh state and keeps the durable journal', async t => {
  const { ctx, session, agent } = await fixture(t);
  requestBranchReset(agent);
  requestDecisionReset(agent, 'Act 3 F38 | HP 53/72\nCurrent relics: Burning Blood.');
  const result = await ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
  assert.ok(result);
  const visible = JSON.stringify(session.deriveMessages());
  assert.doesNotMatch(visible, /Old branch owns Centennial Puzzle|restore-call/);
  assert.match(visible, /Complete A20H autonomously using Flash/);
  assert.match(visible, /Current relics: Burning Blood|HP 53\/72|Completed: back floor 36/);
  assert.ok(session.snapshotEvents().some(e => e.type === 'assistant/message' && JSON.stringify(e.data).includes('Old branch owns Centennial Puzzle')));
  assert.equal(session.snapshotEvents().find(e => e.type === 'user/message' && e.surfaceOp?.op === 'replace').data.source.kind, 'sts-decision');
  // A remounted plugin gets the human instruction from durable events, even
  // though it is now hidden behind a prior checkpoint on the model surface.
  const resumed = { ...agent };
  requestBranchReset(resumed);
  requestDecisionReset(resumed, 'Act 3 F38 | HP 53/72\nCurrent relics: Burning Blood.');
  // Add one completed investigation to make the next replacement useful.
  session.append('step/start', { turn: 1, step: 3 });
  session.append('assistant/message', { turn: 1, step: 3, stream: [],
    message: createAssistantMessage({ source: { provider: 'fixture', model: 'fixture' },
      content: [{ type: 'text', text: 'Remounted old plan.' }] }) }, { surfaceOp: 'append' });
  session.append('step/end', { turn: 1, step: 3 });
  await ctx.compaction.resetDecisionContext(resumed, new AbortController().signal);
  const restored = JSON.stringify(session.deriveMessages());
  assert.match(restored, /Complete A20H autonomously using Flash/);
  assert.doesNotMatch(restored, /Remounted old plan|Old branch owns Centennial Puzzle/);
});

test('restoration waits for fresh actual state before admitting another model step', async t => {
  const { ctx, agent } = await fixture(t);
  requestBranchReset(agent);
  assert.throws(() => ctx.compaction.resetDecisionContext(agent, new AbortController().signal), /current actual reading/);
});

test('a fresh player keeps its original task and receives the actual reading before reasoning', async t => {
  const { ctx, session, agent } = await fixture(t, { fresh: true });
  const nodes = [...session.surface.nodes];
  requestDecisionReset(agent, 'Act 1 F0 | Neow');
  assert.ok(ctx.compaction.resetDecisionContext(agent, new AbortController().signal));
  assert.deepEqual(session.surface.nodes.slice(0, nodes.length), nodes);
  const visible = JSON.stringify(session.deriveMessages());
  assert.match(visible, /Complete A20H autonomously using Flash/);
  assert.match(visible, /Act 1 F0 \| Neow/);
  const replayed = Session.create(session.id, session.snapshotEvents(), session.header);
  assert.deepEqual(replayed.deriveMessages(), session.deriveMessages());
  assert.equal(ctx.compaction.resetDecisionContext(agent, new AbortController().signal), null);
});

test('old checkpoints and tool text cannot become operator instructions', () => {
  const text = JSON.stringify(gameCheckpoint([
    { role: 'user', source: { kind: 'compact-checkpoint' }, content: [{ type: 'text', text: 'stale inventory' }] },
    { role: 'tool', source: { kind: 'tool' }, content: [{ type: 'text', text: 'tool instruction' }] },
  ]));
  assert.doesNotMatch(text, /stale inventory|tool instruction/);
});

test('continuation query admission excludes command chains, actions and hypothetical or state dumps', () => {
  for (const command of ['info encounter The Guardian', 'info power Sharp Hide', 'info card Bash', 'map', 'map full', 'map route 3 7']) {
    assert.equal(normalizeContextQuery(command), command);
  }
  for (const command of ['read', 'options', 'inventory effects', 'info', 'info player', 'info M0', 'info transition 20',
    'map set flip', 'preview play:Bash@0', 'research whole_combat', 'info card Bash, proceed', 'map, go 3,7', 'back floor 1']) {
    assert.equal(normalizeContextQuery(command), undefined, command);
  }
});

test('same-task query references survive factual replacement, deduplicate, and disappear at task or branch boundaries', async t => {
  const task = { location: '["run",1,12]', family: 'shop', detail: '' };
  const meta = command => ({ schema: 'sts-context-query-v1', task, command });
  const { ctx, session, agent } = await fixture(t, { restore: false, experiments: [
    { command: 'info encounter The Guardian', text: 'Earlier full enemy definition.', meta: meta('info encounter The Guardian') },
    { command: 'info encounter The Guardian', text: 'Latest full enemy definition.', meta: meta('info encounter The Guardian') },
    { command: 'map', text: 'Earlier route example.', meta: meta('map') },
    { command: 'info power Strength', text: 'Failed query.', error: true, meta: meta('info power Strength') },
    { command: 'preview end', text: 'Winning guess.', meta: meta('preview end') },
  ] });
  const references = decisionQueryReferences(agent, task, normalizeContextQuery);
  assert.deepEqual(references.map(row => row.command), ['info encounter The Guardian', 'map']);
  assert.match(JSON.stringify(session.eventAt(references[0].seq).data.message), /Latest full enemy definition/);
  const source = { schema: 'sts-run-play-agent-observation-v3', session_id: 'run', head_revision: 7, state_revision: 7 };
  requestDecisionReset(agent, 'Act 1 F12 | Shop\nCurrent deck and choices.', source, { task,
    queries: references.map(row => ({ command: row.command, text: `Refreshed ${row.command}.` })), queryRefs: references.map(row => row.seq) });
  ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
  const messages = session.deriveMessages();
  assert.match(JSON.stringify(messages), /Refreshed info encounter|Refreshed map/);
  assert.doesNotMatch(JSON.stringify(messages), /Earlier full|Latest full|Earlier route|Failed query|Winning guess|Old branch owns/);
  assert.deepEqual(decisionContextSource(session).task, task);
  const remounted = { ...agent };
  assert.deepEqual(decisionQueryReferences(remounted, task, normalizeContextQuery), references);
  assert.deepEqual(decisionQueryReferences(remounted, task), [], 'missing parser fails closed');
  assert.deepEqual(Session.create(session.id, session.snapshotEvents(), session.header).deriveMessages(), messages);
  assert.deepEqual(decisionQueryReferences(remounted, { ...task, family: 'map' }, normalizeContextQuery), []);
  requestBranchReset(remounted);
  assert.deepEqual(decisionQueryReferences(remounted, task, normalizeContextQuery), []);
});

test('context backend loads with installed dependencies and no adjacent run_play directory', async t => {
  const root = await realpath(dirname(fileURLToPath(import.meta.url)));
  const directory = await mkdtemp(join(root, 'context-package-'));
  t.after(async () => {
    const target = await realpath(directory);
    assert.ok(target.startsWith(root + sep) && target.split(/[\\/]/).at(-1).startsWith('context-package-'));
    await rm(target, { recursive: true });
  });
  const path = join(directory, 'context.mjs');
  await copyFile(join(root, 'context.mjs'), path);
  execFileSync(process.execPath, ['--input-type=module', '-e', `await import(${JSON.stringify(pathToFileURL(path).href)})`], {
    cwd: directory, windowsHide: true,
  });
});

test('pressure checkpoints retain source-recorded actual text in either language, without parsing commands or headings', () => {
  const call = (id, command) => ({ role: 'assistant', content: [
    { type: 'reasoning', text: 'Confirmed: Heavy Blade+ uses Strength x3. This deck blocks 30 per turn.' },
    { type: 'tool-call', id, name: 'sts', arguments: JSON.stringify({ command }) },
  ] });
  const result = (toolCallId, text, isError = false) => ({ id: `result-${toolCallId}`, role: 'tool', toolCallId, isError,
    content: [{ type: 'text', text }] });
  const actual = '第三幕 F38 | 生命 40/75 | 商店\nDeck: 29 cards\nRelics: Runic Pyramid\nCard Heavy Blade+: Strength contributes 5 times.\n';
  const messages = [
    { role: 'user', source: { kind: 'user' }, content: [{ type: 'text', text: 'Finish the bound Heart run.' }] },
    call('buy', 'buy:HeavyBlade+#0, buy:FeelNoPain#0'),
    result('buy', `Completed: buy Heavy Blade+\nAct 3 F38 | HP 40/75 | Shop\nDeck: 28 cards\n${actual}`),
    call('preview', 'preview end'),
    result('preview', 'Actual position before adopting actions:\nAct 3 F38 | HP 40/75\nHypothetical successor of preview;\nTerminal | Failure'),
    call('lab', 'deck-lab TheHeart scenarios 3'),
    result('lab', 'Act 4 F56 | HP 75/75 | diagnostic root'),
    result('unpaired', 'Act 3 F38 | HP 2/75 | unpaired data'),
    { role: 'user', source: { kind: 'compact-checkpoint' }, content: [{ type: 'text', text: 'Old summary: remove all defense.' }] },
  ];
  const recorded = [{ message: messages[2], meta: { currentReading: {
    control: 'strategic', summary: '第三幕 F38 | 商店\n', text: actual,
  } } }];
  const checkpoint = JSON.stringify(gameCheckpoint(messages, recorded));
  assert.match(checkpoint, /Finish the bound Heart run/);
  assert.match(checkpoint, /Deck: 29 cards/);
  assert.match(checkpoint, /第三幕 F38/);
  assert.match(checkpoint, /Strength contributes 5 times/);
  assert.doesNotMatch(checkpoint, /Deck: 28 cards|Confirmed:|blocks 30|Strength x3|Failure|75\/75|1\/75|2\/75|remove all defense/);
  assert.doesNotMatch(checkpoint, /branch was restored/);
  assert.doesNotMatch(JSON.stringify(gameCheckpoint(messages)), /Deck: 29 cards|Deck: 28 cards/,
    'English-looking text without a recorded boundary is not a current reading');
  assert.doesNotMatch(JSON.stringify(gameCheckpoint([...messages, call('error', 'act end'),
    result('error', 'Act 3 F38 | HP 1/75 | incomplete call', true)], recorded)), /Deck: 29 cards/);
  assert.doesNotMatch(JSON.stringify(gameCheckpoint([...messages, call('pending', 'act end')], recorded)), /Deck: 29 cards/);
  assert.doesNotMatch(JSON.stringify(gameCheckpoint(messages, [{ ...recorded[0],
    message: { ...messages[2], id: 'another-result' } }])), /Deck: 29 cards/);
  assert.throws(() => recordedGameReading({ message: messages[2], meta: { currentReading: {
    control: 'strategic', summary: '假数据', text: 'not in the actual result',
  } } }), /does not match/);
});

test('official pressure compaction makes a factual game checkpoint without a summarization model request', async t => {
  const { ctx, session, agent } = await fixture(t, { experiments: [
    { command: 'read', text: '第三幕 F38 | 生命 40/75 | 商店\nDeck: 29 cards\nRelics: Runic Pyramid',
      meta: { currentReading: { control: 'strategic', summary: '第三幕 F38 | 商店\n',
        text: '第三幕 F38 | 生命 40/75 | 商店\nDeck: 29 cards\nRelics: Runic Pyramid' } } },
    { command: 'preview end', text: 'Act 3 F38 | HP 0/75 | hypothetical only' },
  ] });
  const range = { start: session.surface.nodes[0], end: session.surface.nodes.at(-3) };
  const originalEvents = structuredClone(session.snapshotEvents());
  await ctx.compaction.compactRegion(range.start, range.end, agent, new AbortController().signal);
  const visible = JSON.stringify(session.deriveMessages());
  assert.match(visible, /Game history checkpoint/);
  assert.match(visible, /Complete A20H autonomously using Flash/);
  assert.match(visible, /Deck: 29 cards/);
  assert.match(visible, /Current HP: 53\/72/); // latest tool unit remains verbatim
  assert.doesNotMatch(visible, /Old branch owns Centennial Puzzle|hypothetical only/);
  const summary = session.snapshotEvents().find(e => e.type === 'compaction/summary');
  assert.equal(summary.data.model, 'game-checkpoint');
  assert.equal(summary.data.llmStreamCall, undefined);
  assert.deepEqual(session.snapshotEvents().slice(0, originalEvents.length), originalEvents);
  assert.deepEqual(Session.create(session.id, session.snapshotEvents(), session.header).deriveMessages(), session.deriveMessages());
});

test('v4-pro is rejected before resolving paths or creating a player', async () => {
  await assert.rejects(runPlayer({ model: 'deepseek-v4-pro' }), /prohibited.*deepseek-flash/);
});

test('context continuity requires the authoritative game source, not matching readings', () => {
  const source = { schema: 'sts-run-play-agent-observation-v3', session_id: 'same-run',
    head_revision: 9, state_revision: 8 };
  assert.deepEqual(gameContextSource({ ...source, position: { hp: 72 } }), source);
  assert.ok(sameGameContextSource(source, { ...source, position: { hp: 5 } }));
  for (const changed of [{ session_id: 'other-run' }, { head_revision: 10 },
    { state_revision: 9 }, { schema: 'sts-run-play-agent-observation-v4' }]) {
    assert.equal(sameGameContextSource(source, { ...source, ...changed }), false);
  }
  assert.equal(sameGameContextSource(undefined, source), false);
  assert.equal(sameGameContextSource({ hp: 72, hand: ['Strike'] }, source), false);
  assert.equal(sameGameContextSource({ ...source, state_revision: -1 }, source), false);
});

test('decision tasks keep nested selections and investigations, and cut completed tasks', () => {
  const observation = (kind, floor = 24, detail = {}) => ({
    schema: 'sts-run-play-agent-observation-v3', session_id: 'run', head_revision: 10, state_revision: 9,
    position: { kind: 'decision', observation: {
      domain: kind === 'combat' ? 'combat' : 'strategic',
      run_context: { act: 2, floor, encounter_id: 'Slavers' },
      observation: { run: { act: 2, floor }, context: { kind, ...detail } },
    } },
  });
  const shop = gameDecisionTask(observation('shop'));
  const selected = gameDecisionTask(observation('run_choice', 24, { source: 'Shop' }), shop);
  assert.ok(sameGameDecisionTask(shop, selected));
  assert.ok(sameGameDecisionTask(selected, gameDecisionTask(observation('shop'), selected)));
  const event = gameDecisionTask(observation('event', 24, { event: 'WomanInBlue', screen: 0 }));
  assert.ok(sameGameDecisionTask(event, gameDecisionTask(observation('event', 24, { event: 'WomanInBlue', screen: 1 }), event)));
  assert.ok(sameGameDecisionTask(event, gameDecisionTask(observation('reward'), event)));
  const rewards = gameDecisionTask(observation('reward'));
  assert.ok(sameGameDecisionTask(rewards, gameDecisionTask(observation('card_reward'), rewards)));
  for (const kind of ['campfire', 'treasure', 'boss_relic']) {
    const task = gameDecisionTask(observation(kind));
    assert.ok(sameGameDecisionTask(task, gameDecisionTask(observation('run_choice'), task)));
    assert.equal(sameGameDecisionTask(task, gameDecisionTask(observation('map'), task)), false);
  }
  const combat = gameDecisionTask(observation('combat'));
  assert.ok(sameGameDecisionTask(combat, gameDecisionTask({ ...observation('combat'), head_revision: 99, state_revision: 98 }, combat)));
  assert.equal(sameGameDecisionTask(combat, gameDecisionTask(observation('reward'), combat)), false);
  assert.equal(sameGameDecisionTask(shop, gameDecisionTask(observation('shop', 25), shop)), false);
  assert.equal(sameGameDecisionTask(shop, gameDecisionTask(observation('map'), shop)), false);
  assert.throws(() => gameDecisionTask({ ...observation('shop'), position: {} }), /typed run position/);
});

test('a fresh decision discards even the last assistant reasoning and retains actual state and receipts', async t => {
  const { ctx, session, agent } = await fixture(t);
  const id = ToolCallId('leave-shop');
  session.append('step/start', { turn: 1, step: 3 });
  session.append('assistant/message', { turn: 1, step: 3, stream: [], message: createAssistantMessage({
    source: { provider: 'fixture', model: 'fixture' }, content: [
      { type: 'reasoning', text: 'Old shop guess: buy all attacks and keep this plan forever.' },
      { type: 'tool-call', id, name: 'sts', arguments: '{"command":"buy:ShrugItOff#0, leave"}' },
    ],
  }) }, { surfaceOp: 'append' });
  session.append('tool/call', { turn: 1, step: 3, callId: id, name: 'sts', arguments: '{"command":"buy:ShrugItOff#0, leave"}' });
  session.append('tool/result', { turn: 1, step: 3, message: createMessage({ role: 'tool', toolCallId: id,
    source: { kind: 'tool', callId: id }, content: [{ type: 'text', text: 'Completed: buy:ShrugItOff#0\nOld shop inventory\nCompleted: leave\nOld partial reading' }] }) }, { surfaceOp: 'append' });
  session.append('step/end', { turn: 1, step: 3 });
  requestDecisionReset(agent, 'Act 2 F24 | HP 81/82 | Map\nDeck: 20 cards\nDisarm: lower enemy Strength by 2.');
  await ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
  const visible = JSON.stringify(session.deriveMessages());
  assert.doesNotMatch(visible, /Old shop|Old branch|53\/72|Centennial|winning plan|restore-call|leave-shop/);
  assert.match(visible, /Complete A20H autonomously using Flash/);
  assert.match(visible, /Completed: buy:ShrugItOff#0/);
  assert.match(visible, /Completed: leave/);
  assert.match(visible, /HP 81\/82|Deck: 20 cards|lower enemy Strength by 2/);
  assert.equal(session.snapshotEvents().filter(e => e.type === 'user/message' && e.surfaceOp?.op === 'replace').at(-1).data.source.kind, 'sts-decision');
  assert.ok(session.snapshotEvents().some(e => e.type === 'assistant/message' && JSON.stringify(e.data).includes('Old shop guess')));
});

test('decision replacement requires a complete tool result even when fresh state is available', async t => {
  const { ctx, session, agent } = await fixture(t, { unfinished: true });
  requestDecisionReset(agent, 'Act 2 F24 | Map');
  assert.throws(() => decisionContextRange(session), /unfinished tool pair/);
  assert.throws(() => ctx.compaction.resetDecisionContext(agent, new AbortController().signal), /unfinished tool pair/);
});

test('decision refresh identifies the player command that already ran, even after a query', async t => {
  const { ctx, session, agent } = await fixture(t, { restore: false, experiments: [
    { command: 'smith:DemonForm#0', text: 'Completed: smith Demon Form@deck[15]\nMap\nDemon Form+' },
    { command: 'options', text: 'Map · completed node (1,14) Campfire\ngo 0,15' },
  ] });
  requestDecisionReset(agent, 'Map · completed node (1,14) Campfire\nDeck: Demon Form+');
  ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
  const visible = JSON.stringify(session.deriveMessages());
  assert.match(visible, /Continue the operator task from its current progress/);
  assert.match(visible, /game execution by this player/);
  assert.match(visible, /Submitted command: smith:DemonForm#0/);
  assert.match(visible, /Completed: smith Demon Form@deck\[15\]/);
  assert.doesNotMatch(visible, /Submitted command: options/);
});

test('successful classification and recovery queries supersede stale errors on context refresh', async t => {
  for (const text of ['Recorded classifications: benefit | realization | basis\nFlex | low | sequence',
    'Act 2 F28 | Card reward']) {
    const { ctx, session, agent } = await fixture(t, { restore: false, experiments: [
      { command: 'classify', text: 'Error: Invalid reference with appended explanation.', error: true },
      { command: text.startsWith('Recorded') ? 'classify' : 'read', text },
      { command: 'options', text: 'Current legal candidates.' },
    ] });
    requestDecisionReset(agent, 'Act 2 F28 | Card reward\nPreparation stage: comparison.');
    ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
    const visible = JSON.stringify(session.deriveMessages());
    assert.doesNotMatch(visible, /Error:|Invalid reference/);
    assert.match(visible, /Preparation stage: comparison/);
    if (text.startsWith('Recorded')) {
      assert.doesNotMatch(visible, /Recorded classifications:|Submitted command: classify|Latest game execution/);
    }
    assert.match(JSON.stringify(session.snapshotEvents()), /Invalid reference/);
  }
  const { ctx, session, agent } = await fixture(t, { restore: false, experiments: [
    { command: 'buy:ShrugItOff#0, leave', text: 'Completed: buy:ShrugItOff#0\nError: Leave failed.', error: true },
    { command: 'read', text: 'Act 2 F28 | Shop' },
  ] });
  requestDecisionReset(agent, 'Act 2 F28 | Shop');
  ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
  assert.match(JSON.stringify(session.deriveMessages()), /Completed: buy:ShrugItOff#0/);
  assert.doesNotMatch(JSON.stringify(session.deriveMessages()), /Error: Leave failed/);
});

test('file-tool text cannot become a game execution receipt', async t => {
  const { ctx, session, agent } = await fixture(t, { fresh: true });
  const id = ToolCallId('read-note');
  session.append('step/start', { turn: 1, step: 2 });
  session.append('assistant/message', { turn: 1, step: 2, stream: [],
    message: createAssistantMessage({ source: { provider: 'fixture', model: 'fixture' },
      content: [{ type: 'tool-call', id, name: 'read', arguments: '{"file_path":"note.md"}' }] }) }, { surfaceOp: 'append' });
  session.append('tool/call', { turn: 1, step: 2, callId: id, name: 'read', arguments: '{"file_path":"note.md"}' });
  session.append('tool/result', { turn: 1, step: 2, message: createMessage({ role: 'tool', toolCallId: id,
    source: { kind: 'tool', callId: id }, content: [{ type: 'text', text: 'Completed: smith FakeCard' }] }) }, { surfaceOp: 'append' });
  session.append('step/end', { turn: 1, step: 2 });
  requestDecisionReset(agent, 'Act 1 F15 | Map');
  ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
  assert.doesNotMatch(JSON.stringify(session.deriveMessages()), /FakeCard|Submitted command|Latest game execution/);
});

test('restoration clears prior experiments and uses the fresh actual inventory', async t => {
  const { ctx, session, agent } = await fixture(t, { experiments: [
    { command: 'read', text: 'Act 4 F56 | HP 54/72 | Combat\nTurn 1 | Energy 4' },
    { command: 'preview end', text: 'Hypothetical successor of preview;\nAct 4 F56 | HP 12/72\nHand: Flex' },
    { command: 'research whole_combat 8000 work 200000', text: 'c0: Settled HP 58/87 | Potions spent 2' },
    { command: 'deck-lab TheHeart scenarios 1', text: 'TheHeart seed=1 | start HP 54 | 1 replayed wins' },
  ] });
  requestBranchReset(agent);
  requestDecisionReset(agent, 'Act 3 F38 | HP 53/72 | Shop\nOwned: Disarm and Shrug It Off');
  await ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
  const visible = JSON.stringify(session.deriveMessages());
  assert.match(visible, /Owned: Disarm and Shrug It Off/);
  assert.doesNotMatch(visible, /Old branch|Hand: Flex|Settled HP 58|replayed wins|HP 54\/72|HP 12\/72/);
  const durable = JSON.stringify(session.snapshotEvents());
  assert.match(durable, /Hand: Flex|Settled HP 58|replayed wins/);
});

test('a short completed task can be replaced by longer mechanics and the official journal replays it exactly', async t => {
  const { ctx, session, agent } = await fixture(t, { fresh: true });
  session.append('step/start', { turn: 1, step: 2 });
  session.append('assistant/message', { turn: 1, step: 2, stream: [], message: createAssistantMessage({
    source: { provider: 'fixture', model: 'fixture' }, content: [{ type: 'text', text: 'Old terse plan.' }],
  }) }, { surfaceOp: 'append' });
  session.append('step/end', { turn: 1, step: 2 });
  const source = { schema: 'sts-run-play-agent-observation-v3', session_id: 'run', head_revision: 10, state_revision: 9 };
  const rules = 'Disarm: lower enemy Strength by 2 for the combat.\n'.repeat(200);
  requestDecisionReset(agent, 'Act 2 F24 | Combat\n' + rules, source);
  ctx.compaction.resetDecisionContext(agent, new AbortController().signal);
  const visible = session.deriveMessages();
  assert.doesNotMatch(JSON.stringify(visible), /Old terse plan/);
  assert.equal(visible.at(-1).source.kind, 'sts-decision');
  assert.deepEqual(visible.at(-1).source.gameSource, source);
  assert.ok(visible.at(-1).content[0].text.endsWith(rules));
  const replayed = Session.create(session.id, session.snapshotEvents(), session.header);
  assert.deepEqual(replayed.deriveMessages(), visible);
  assert.deepEqual(replayed.surface.nodes, session.surface.nodes);
  assert.ok(replayed.snapshotEvents().some(e => e.type === 'assistant/message'));
});
