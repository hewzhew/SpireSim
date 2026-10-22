import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { Context } from '@deepseek-ai/cordis';
import LlmRuntime, { LlmAdapter, createUserMessage } from '@deepseek-ai/dsh-llm';
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session';
import ProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import AgentRegistry from '@deepseek-ai/dsh-agent';
import AgentLoop from '@deepseek-ai/dsh-agent-loop';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import ToolRuntime, { defineTool } from '@deepseek-ai/dsh-tools';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import GameContextCompaction, { requestBranchReset, gameDecisionTask } from './context.mjs';
import { scopeOf } from '@deepseek-ai/dsh-scope';
import * as Player from './index.mjs';
import { preparationTaskRoot, preparationTaskProgress, restoredPreparationDecision } from './preparation.mjs';
import { runPreparationFlow } from './launch.mjs';
import { readPlayerReview, adoptPlayerProposal, setPlayerGuidance } from './supervision.mjs';

function decisionPosition(act = 1, boss = 'TheGuardian', domain = 'strategic', keys = [false, false, false]) {
  const context = { act, ascension_level: 20, run_goal: 'heart_victory',
    public_map: { boss }, keys };
  return { kind: 'decision', public_snapshot_id: 'fixture-position', observation: domain === 'strategic'
    ? { domain, observation: { run: context, run_goal: context.run_goal, public_map: context.public_map } }
    : { domain, run_context: { status: 'available', ...context }, observation: { player: { ascension_level: 20 } } } };
}

async function fixture(t, rules, fail = false, behavior = {}) {
  const repository = await mkdtemp(join(tmpdir(), 'sts-player-rules-'));
  const session = join(repository, 'session');
  await mkdir(join(repository, 'tools/run_play'), { recursive: true });
  await mkdir(session);
  const positionFile = join(repository, 'position.json');
  const setPosition = position => {
    if (position.kind === 'decision') position.public_snapshot_id ??= 'fixture-position';
    return writeFile(positionFile, JSON.stringify(position));
  };
  await setPosition(decisionPosition());
  await writeFile(join(repository, 'tools/run_play/session.mjs'),
    'export const freshRecordDirectory = repository => repository + "/records";\n'
    + `export { commandTokens, commandGroups } from ${JSON.stringify(new URL('../run_play/session.mjs', import.meta.url).href)};`);
  await writeFile(join(repository, 'tools/run_play/contract.mjs'),
    `export { conciseCardCommand, strategicOptionCommand } from ${JSON.stringify(new URL('../run_play/contract.mjs', import.meta.url).href)};`);
  await writeFile(join(repository, 'tools/run_play/tokens.mjs'),
    `export { normalizeActionTokens } from ${JSON.stringify(new URL('../run_play/tokens.mjs', import.meta.url).href)};`);
  await writeFile(join(repository, 'tools/run_play/workbench.mjs'), `
    import { appendFile, mkdir, readFile } from 'node:fs/promises';
    export async function openWorkbench({ recordDirectory }) {
      await mkdir(recordDirectory, { recursive: true });
      const log = value => appendFile(recordDirectory + '/calls.jsonl', JSON.stringify(value) + '\\n');
      const position = async () => JSON.parse(await readFile(${JSON.stringify(positionFile)}, 'utf8'));
      let failNextObservation = false, adoptionObservationChanged = false;
      if (recordDirectory.endsWith('forced-wire') && ${Boolean(behavior.forcedOpeningPosition)}) {
        const { writeFile } = await import('node:fs/promises');
        await writeFile(${JSON.stringify(positionFile)}, JSON.stringify(${JSON.stringify(behavior.forcedOpeningPosition ?? null)}));
      }
      return {
        async observe() {
          await log('observe');
          if (failNextObservation) { failNextObservation = false; throw new Error('injected observation failure after acknowledged action'); }
          if (${Boolean(behavior.adoptionObservationPosition)} && !adoptionObservationChanged) {
            const { existsSync } = await import('node:fs');
            if (existsSync(${JSON.stringify(join(repository, 'records/adoptions.jsonl'))})) {
              adoptionObservationChanged = true;
              const { writeFile } = await import('node:fs/promises');
              await writeFile(${JSON.stringify(positionFile)}, JSON.stringify(${JSON.stringify(behavior.adoptionObservationPosition ?? null)}));
            }
          }
          const current = await position();
          const observed = { schema: 'sts-run-play-agent-observation-v3', session_id: 'fixture',
            head_revision: current.revision ?? 1, state_revision: current.revision ?? 1,
            position: current, ...(current.decisionEvidence ? { decision_evidence: current.decisionEvidence } : {}),
            ...(current.deckEvidence ? { deck_evidence: current.deckEvidence } : {}) };
          if (current.observation?.domain === 'strategic') {
            const { preparationFixture } = await import(${JSON.stringify(new URL('./preparation-fixture.test.mjs', import.meta.url).href)});
            const state = current.observation.observation;
            observed.preparation_reading = preparationFixture(observed, {
              overview: 'Act ' + state.run.act + ' F' + (state.run.floor ?? 0) + ' | ' + (state.context?.kind ?? 'decision'),
              ...(current.preparationOptions ?? {}),
            });
          }
          return observed;
        },
        async preview(commands) {
          const endpoint = ${JSON.stringify(behavior.previews ?? {})}[commands.join(' / ')];
          if (!endpoint) throw new Error('Unexpected fixture preview ' + commands.join(' / '));
          await log({ preview: commands });
          return { preview: true, endpoint };
        },
        async execute(command) {
          if (Array.isArray(command)) command = command.join(' ');
          await log(command);
          if (${fail}) throw new Error('bound rules unavailable');
          const recorded = ${JSON.stringify(behavior.recordedReadings ?? {})}[command];
          if (recorded) return { text: recorded.text, currentReading: recorded };
          const current = await position();
          const action = ${JSON.stringify(behavior.actions ?? {})}[command];
          if (action) {
            const { writeFile } = await import('node:fs/promises');
            await writeFile(${JSON.stringify(positionFile)}, JSON.stringify(action));
            if (command === ${JSON.stringify(behavior.failAfter ?? '')}) throw new Error('injected failure after saved action');
            failNextObservation = ${Boolean(behavior.failObservationAfterAction)};
            return { text: 'Completed: ' + command + '\\n' + action.reading };
          }
          if (current.kind === 'terminal' && command !== 'read') throw new Error('no rules decision at terminal');
          if (${Boolean(behavior.readDrift)} && command === 'read, inventory effects') {
            current.revision = (current.revision ?? 1) + 1;
            const { writeFile } = await import('node:fs/promises');
            await writeFile(${JSON.stringify(positionFile)}, JSON.stringify(current));
          }
          if (command.startsWith('deck-lab ')) {
            return { text: 'Fixed diagnostic cases: ' + command };
          }
          if (command.startsWith('back ')) return { text: 'Restored actual state', branchChanged: true };
          if (current.queryErrors?.includes(command)) throw new Error('Earlier query unavailable in current content');
          if (current.queryResponses?.[command] !== undefined) return { text: current.queryResponses[command] };
          if (current.reading) return { text: command.startsWith('info rules') ? current.rules ?? 'Current run rules'
            : command === 'read, inventory effects' || command === 'read' ? current.reading
              : 'Completed: ' + command + '\\n' + current.reading };
          const observed = current.observation;
          const boss = (observed.domain === 'strategic' ? observed.observation : observed.run_context).public_map.boss;
          const rules = ${JSON.stringify(rules)};
          return { text: typeof rules === 'string' ? rules : rules[boss] };
        },
        close() { return log('close'); }
      };
    }`);
  const ctx = new Context();
  t.after(async () => {
    await ctx.fiber.dispose();
    assert.equal(dirname(resolve(repository)), resolve(tmpdir()));
    assert.ok(repository.split(/[\\/]/).at(-1).startsWith('sts-player-rules-'));
    await rm(repository, { recursive: true, force: true });
  });
  for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime, TokenMeter]) {
    await ctx.plugin(plugin);
  }
  await ctx.plugin(GameContextCompaction, { auto: false });
  await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
  await ctx.plugin(AgentLoop, { agents: [], maxParallelToolCalls: 1 });
  let agentContext;
  const create = (playerConfig = {}, seed) => ctx.agents.create({ sessionId: SessionId(`rules-${Date.now()}`), meta: {}, ...(seed ? { seed } : {}),
    agentOptions: { provider: 'fixture', model: 'fixture' },
    setup: async scoped => {
      agentContext = scoped;
      await scoped.plugin(Player, { repository, session, resetContextOnAttach: false, ...playerConfig });
    },
  });
  return { ctx, create, setPosition, repository, session, scope: () => agentContext,
    classifications: async () => (await readFile(join(repository, 'records/classifications.jsonl'), 'utf8'))
      .trim().split('\n').map(JSON.parse),
    submissions: async () => (await readFile(join(repository, 'records/classification-submissions.jsonl'), 'utf8'))
      .trim().split('\n').map(JSON.parse),
    adoptions: async () => (await readFile(join(repository, 'records/adoptions.jsonl'), 'utf8'))
      .trim().split('\n').map(JSON.parse),
    calls: async () => (await readFile(join(repository, 'records/game-0/calls.jsonl'), 'utf8'))
      .trim().split('\n').map(JSON.parse) };
}

function preparationPosition(act, floor, site = 'map', revision = 1) {
  // Act 4's known pair of encounters does not require a normal map Boss field.
  const current = decisionPosition(act, act === 2 ? 'Collector' : act === 3 ? 'AwakenedOne' : null);
  current.revision = revision;
  const state = current.observation.observation;
  Object.assign(state.run, { floor, current_hp: 65, max_hp: 80, potion_capacity: 3 });
  state.context = { kind: site };
  // Public observations group equivalent cards; count is the master-deck size.
  state.cards = [{ card: 'Defend', upgrades: 0, count: 3 }, { card: 'Bash', upgrades: 0, count: 1 }];
  state.potions = [{ potion: { potion: 'FairyPotion' } }, { potion: null }, { potion: null }];
  Object.assign(state.public_map, { current_x: 2, current_y: 4 });
  return current;
}

test('ordinary scripted players localize guides and tool descriptions per scope without changing schemas', async t => {
  const f = await fixture(t, 'Supplied rules {{opaque}}');
  const guides = [], schemas = [], handles = [];
  for (const agentLocale of ['en', 'zh-CN']) {
    const handle = await f.create({ agentLocale }); handles.push(handle);
    const scope = f.scope();
    const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(scope) });
    const guide = assembly.sections.find(section => section.name === 'sts:player-guide').text;
    guides.push(guide);
    const tools = f.ctx.tools.schemas(handle.agent).filter(tool => ['sts', 'sts_feedback'].includes(tool.name));
    schemas.push(JSON.parse(JSON.stringify(tools, (key, value) => key === 'description' ? undefined : value)));
    const tool = tools.find(tool => tool.name === 'sts');
    assert.match(guide, agentLocale === 'en' ? /Bare info expands the current options/ : /无参数 info 展开当前选项/);
    assert.match(tool.description, agentLocale === 'en' ? /Submit a displayed game command/ : /提交显示的游戏命令/);
    assert.ok(tool.description.includes('review <plan ID>'));
    const failed = await f.ctx.tools.execute({ name: 'sts_feedback', arguments: { message: ' ' }, agent: handle.agent,
      signal: new AbortController().signal, callId: 'language-validation-' + agentLocale });
    assert.equal(failed.isError, true);
    assert.match(failed.content[0].text, agentLocale === 'en' ? /concrete difficulty/ : /具体困难/);
  }
  assert.notEqual(guides[0], guides[1]);
  assert.deepEqual(schemas[0], schemas[1]);
  for (const handle of handles) await handle.dispose();
});

test('preparation scopes localize the actual guide and schema from their model language', async t => {
  const f = await fixture(t, 'Raw rules {{literal}}');
  const current = scopedPosition('campfire', 1), state = current.observation.observation;
  state.base_energy_per_turn = 3;
  state.cards = state.cards.map(card => ({ ...card, misc_value: 0, cost_modifier: 0, base_damage_override: null, base_block_override: null }));
  current.deckEvidence = { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: current.public_snapshot_id,
    cards: state.cards.map(card => ({ card, persistent_cost: 1, persistent_damage: 0, persistent_block: 0,
      mechanics: { card: card.card, upgrades: card.upgrades, card_type: card.card === 'Bash' ? 'Attack' : 'Skill', draw_cards: 0 } })) };
  await f.setPosition(current);
  const schemas = [];
  for (const agentLocale of ['en', 'zh-CN']) {
    const handle = await f.create({ taskScope: 'preparation', agentLocale });
    const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
    const guide = assembly.sections.find(section => section.name === 'sts:player-guide').text;
    const tool = f.ctx.tools.schemas(handle.agent).find(tool => tool.name === 'sts');
    assert.match(guide, agentLocale === 'en' ? /Current task: rate the candidates/ : /当前任务：评估候选/);
    assert.match(tool.description, agentLocale === 'en' ? /Submit the current preparation step/ : /提交当前准备步骤/);
    const comparison = assembly.sections.find(section => section.name === 'sts:classification').text;
    assert.match(comparison, agentLocale === 'en' ? /Whole-deck comparison baseline/ : /完整卡组比较基线/);
    schemas.push(JSON.parse(JSON.stringify(tool, (key, value) => key === 'description' ? undefined : value)));
    await handle.dispose();
  }
  assert.deepEqual(schemas[0], schemas[1]);
});

test('both languages record acknowledged decisions on a failed tool result before any recovery read', async t => {
  for (const agentLocale of ['en', 'zh-CN']) {
    const command = 'take:BurningPact#0';
    const f = await fixture(t, 'Current rules', false, { actions: { [command]: scopedPosition('map', 2) }, failObservationAfterAction: true });
    await f.setPosition(checkedCardPosition());
    let requests = 0;
    class Adapter extends LlmAdapter {
      async *stream() {
        if (requests++ === 0) {
          const block = { type: 'tool-call', id: `typed-recovery-${agentLocale}`, name: 'sts', arguments: JSON.stringify({ command: 'choose', choice: command }) };
          yield { type: 'block-start', index: 0, blockType: 'tool-call' };
          yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
          yield { type: 'block-end', index: 0, block };
          yield { type: 'finish', reason: { kind: 'tool-calls' } };
        } else yield { type: 'finish', reason: { kind: 'stop' } };
      }
    }
    f.ctx.llm.registerAdapter(['fixture'], new Adapter());
    const handle = await f.create({ taskScope: 'preparation', agentLocale });
    handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete the current reward choice.' }] }));
    await handle.agent.whenIdle();
    const failed = handle.agent.session.snapshotEvents().find(event => event.type === 'tool/result' && event.data.message.isError);
    assert.ok(failed);
    assert.equal(failed.data.meta.preparationDecision.command, command);
    assert.match(failed.data.meta.preparationDecision.basis, agentLocale === 'en' ? /^Selected / : /^选择 /);
    assert.equal(restoredPreparationDecision(handle.agent.session).command, command);
    const content = failed.data.message.content.map(block => block.text ?? '').join('\n');
    assert.match(content, agentLocale === 'en' ? /Adopted preparation command:/ : /已采用的准备命令：/);
    const recovered = await rawGameExec(f.ctx, handle.agent, 'read');
    assert.equal(Boolean(recovered.isError), false, resultText(recovered));
    assert.match(resultText(recovered), agentLocale === 'en' ? /Preparation task complete/ : /准备任务完成/);
    assert.equal((await f.calls()).filter(call => call === command).length, 1);
    await handle.dispose();
  }
});

test('Chinese preparation tool repairs classifications and returns its localized recorded table without executing a choice', async t => {
  const f = await fixture(t, 'Opaque {{rules}} 原文');
  await f.setPosition(scopedPosition('campfire', 1));
  const handle = await f.create({ taskScope: 'preparation', agentLocale: 'zh-CN' });
  t.after(() => handle.dispose());
  const partial = await rawGameExec(f.ctx, handle.agent, 'classify', 'buy:BottledFlame#0 | medium | direct | @foreign');
  assert.equal(Boolean(partial.isError), false, resultText(partial));
  assert.match(resultText(partial), /已记录分类行：.*修正：使用 buy:BottledFlame#0 所供的条件/s);
  const complete = await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis());
  assert.equal(Boolean(complete.isError), false, resultText(complete));
  assert.match(resultText(complete), /已记录分类：收益/);
  assert.equal((await f.classifications()).length, 1);
  assert.ok(!(await f.calls()).some(call => /^(buy:|leave$)/.test(typeof call === 'string' ? call : '')));
});

test('bound rules are loaded before play, remain scoped, and close with the player', async t => {
  const rules = 'Run rules | A20\nAct transition: heal 75% of missing HP. {{literal}}';
  const f = await fixture(t, rules);
  const handle = await f.create();
  const assemble = () => f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  const assembly = await assemble();
  const section = assembly.sections.find(x => x.name === 'sts:run-rules');
  assert.equal(section.text, rules);
  assert.equal(section.interpolate, false);
  assert.deepEqual(await f.calls(), ['observe', 'info rules preparation', 'observe']);
  assert.ok(!(await f.ctx.systemPrompt.assemble()).sections.some(x => x.name === 'sts:run-rules'));
  // Reassembly depends on the mounted scope rather than dialogue history.
  assert.equal((await assemble()).sections
    .find(x => x.name === 'sts:run-rules').text, rules);
  await handle.dispose();
  assert.deepEqual(await f.calls(), ['observe', 'info rules preparation', 'observe', 'observe', 'close']);
});

test('scripted tools retain current-reading metadata without adding it to model-visible text', async t => {
  const reading = { control: 'strategic', summary: '第一幕 | 商店\n', text: '第一幕 | 商店\n金币 99\n' };
  const f = await fixture(t, 'Current rules', false, { recordedReadings: { read: reading } });
  const handle = await f.create(); t.after(() => handle.dispose());
  const result = await rawGameExec(f.ctx, handle.agent, 'read');
  assert.equal(Boolean(result.isError), false, resultText(result));
  assert.equal(resultText(result), reading.text);
  assert.deepEqual(result.meta.currentReading, reading);
  assert.doesNotMatch(resultText(result), /currentReading|"summary"/);
});

test('coached native tool pauses before a purchase while leaving investigations available', async t => {
  const f = await fixture(t, 'Current rules');
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ supervised: true }); t.after(() => handle.dispose());
  const result = await rawGameExec(f.ctx, handle.agent, 'buy:Inflame#0', undefined, { reason: 'Lasting Strength for existing attacks.' });
  assert.equal(Boolean(result.isError), false);
  assert.equal(result.concludesTurn, true);
  assert.match(resultText(result), /Pending proposal \(not executed\)/);
  assert.ok(!(await f.calls()).includes('buy:Inflame#0'));
  assert.equal((await readPlayerReview(f.session)).command, 'buy:Inflame#0');
  const before = await f.calls();
  // The official loop may have more tool calls queued in the same batch.
  for (const command of ['claim:Gold#0', 'open-card#0', 'read', 'leave']) {
    const queued = await rawGameExec(f.ctx, handle.agent, command);
    assert.equal(queued.concludesTurn, true);
    assert.match(resultText(queued), /Pending proposal.*buy:Inflame#0/);
  }
  assert.deepEqual(await f.calls(), before, 'no command reaches the workbench after a pending proposal');
  const review = await readPlayerReview(f.session);
  await writeFile(join(f.session, 'dsh-review.json'), JSON.stringify({ ...review, status: 'execution-error' }));
  const blocked = await rawGameExec(f.ctx, handle.agent, 'buy:ShrugItOff#0');
  assert.equal(blocked.concludesTurn, true);
  assert.match(resultText(blocked), /review required: execution-error/);
  assert.deepEqual(await f.calls(), before);
  assert.equal((await readPlayerReview(f.session)).command, 'buy:Inflame#0');
});

test('ordinary preparation assembly requests the native preparation rules scope', async t => {
  const f = await fixture(t, 'Native preparation rules');
  await f.setPosition(scopedPosition('map', 1));
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  assert.equal(assembly.sections.find(row => row.name === 'sts:run-rules').text, 'Current run rules');
  assert.ok((await f.calls()).includes('info rules preparation'));
  assert.ok(!(await f.calls()).includes('info rules'));
});

test('pending Act entry refreshes rules within the same public Act and Boss', async t => {
  const f = await fixture(t, 'bound rules');
  const handle = await f.create();
  const current = decisionPosition();
  current.observation.observation.run.pending_act_entry = 2;
  await f.setPosition(current);
  await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  assert.equal((await f.calls()).filter(call => call === 'info rules preparation').length, 2);
  await handle.dispose();
});

test('pending Act recovery refreshes with surviving HP and Max HP, while ordinary HP changes preserve shared rules', async t => {
  const f = await fixture(t, 'bound rules');
  const current = preparationPosition(1, 17, 'reward', 1);
  const run = current.observation.observation.run;
  run.pending_act_entry = 2;
  current.reading = 'Act 1 F17 | Rewards';
  current.rules = 'Transition recovery gives 76/80 HP.';
  await f.setPosition(current);
  const handle = await f.create(); t.after(() => handle.dispose());
  const rules = async () => (await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) })).sections
    .find(section => section.name === 'sts:run-rules').text;
  assert.equal(await rules(), current.rules);
  run.current_hp = 40;
  current.rules = 'Transition recovery gives 70/80 HP.';
  await f.setPosition(current);
  assert.equal(await rules(), current.rules);
  run.max_hp = 84;
  current.rules = 'Transition recovery gives 73/84 HP.';
  await f.setPosition(current);
  assert.equal(await rules(), current.rules);
  assert.equal((await f.calls()).filter(call => call === 'info rules preparation').length, 3);
  delete run.pending_act_entry;
  current.rules = 'Current Act rules without an entry forecast.';
  await f.setPosition(current);
  await rules();
  run.current_hp = 35;
  await f.setPosition(current);
  await rules();
  assert.equal((await f.calls()).filter(call => call === 'info rules preparation').length, 4);
});

test('an unavailable bound rules query fails setup and releases its connection', async t => {
  const f = await fixture(t, '', true);
  await assert.rejects(f.create(), /bound rules unavailable/);
  assert.deepEqual(await f.calls(), ['observe', 'info rules preparation', 'close']);
});

test('the next assembly follows public Boss changes and rewinds without requerying ordinary changes', async t => {
  const definitions = { TheGuardian: 'Act 1 Guardian rules', Collector: 'Act 2 Collector rules' };
  const f = await fixture(t, definitions);
  await f.create();
  const assemble = () => f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  const rules = async () => (await assemble()).sections.filter(x => x.name === 'sts:run-rules');
  assert.deepEqual((await rules()).map(x => x.text), [definitions.TheGuardian]);
  // Combat and preparation carry the same public run context.
  await f.setPosition(decisionPosition(1, 'TheGuardian', 'combat'));
  assert.equal((await rules())[0].text, definitions.TheGuardian);
  await f.setPosition(decisionPosition(2, 'Collector'));
  assert.equal((await rules())[0].text, definitions.Collector);
  await f.setPosition(decisionPosition(2, 'Collector', 'combat'));
  assert.equal((await rules())[0].text, definitions.Collector);
  await f.setPosition(decisionPosition());
  assert.equal((await rules())[0].text, definitions.TheGuardian);
  assert.deepEqual((await f.calls()).filter(x => x === 'info rules preparation'), Array(3).fill('info rules preparation'));
  assert.ok(!(await f.ctx.systemPrompt.assemble()).sections.some(x => x.name === 'sts:run-rules'));
});

test('terminal reporting and terminal attachment work without querying an absent decision', async t => {
  const f = await fixture(t, 'current run rules');
  const handle = await f.create();
  await f.setPosition({ kind: 'terminal', outcome: { kind: 'combat_defeat' } });
  const assemble = () => f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  assert.equal((await assemble()).sections.find(x => x.name === 'sts:run-rules').text, '');
  await handle.dispose();
  const terminalHandle = await f.create();
  assert.equal((await assemble()).sections.find(x => x.name === 'sts:run-rules').text, '');
  assert.deepEqual((await f.calls()).filter(x => x === 'info rules preparation'), ['info rules preparation']);
  await terminalHandle.dispose();
});

test('key acquisition and rewind refresh rules across strategic and combat context', async t => {
  const f = await fixture(t, 'current run rules');
  const handle = await f.create();
  const assemble = () => f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  let queries = 1;
  for (const [domain, keys, changed] of [
    ['strategic', [true, true, false], true],
    ['combat', [true, true, false], false],
    ['combat', [true, true, true], true],
    ['strategic', [true, true, true], false],
    ['strategic', [true, true, false], true],
    ['combat', [false, false, false], true],
  ]) {
    await f.setPosition(decisionPosition(1, 'TheGuardian', domain, keys));
    await assemble();
    queries += Number(changed);
    assert.equal((await f.calls()).filter(x => x === 'info rules preparation').length, queries);
  }
  await handle.dispose();
});

test('preparation across Acts and shops supplies public facts without automatic combat samples', async t => {
  const f = await fixture(t, 'current rules');
  const handle = await f.create();
  const assemble = () => f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  for (const [act, floor, site, revision] of [[2, 21, 'shop', 1], [2, 22, 'card_reward', 2],
    [3, 38, 'shop', 3], [4, 53, 'campfire', 4], [4, 54, 'shop', 5], [2, 21, 'shop', 6]]) {
    await f.setPosition(preparationPosition(act, floor, site, revision));
    const assembly = await assemble();
    assert.ok(!assembly.sections.some(x => x.name === 'sts:preparation'));
  }
  assert.equal((await f.calls()).filter(x => x.startsWith('deck-lab')).length, 0);
  // A requested diagnostic still routes through the explicit game tool.
  const result = await f.ctx.tools.execute({ name: 'sts', arguments: { command: 'deck-lab TheHeart scenarios 3' },
    agent: handle.agent, signal: new AbortController().signal, callId: 'explicit-diagnostic' });
  assert.ok(!result.isError);
  assert.match(resultText(result), /Fixed diagnostic cases/);
  assert.equal((await f.calls()).filter(x => x.startsWith('deck-lab')).length, 1);
  await handle.dispose();
});

test('official model requests start fresh at decision boundaries and retain one complete investigation', async t => {
  const f = await fixture(t, 'current rules');
  const position = (kind, revision, text) => {
    const value = preparationPosition(1, 12, kind, revision);
    value.reading = text;
    if (kind === 'combat') {
      value.observation = { domain: 'combat', run_context: { status: 'available', act: 1, floor: 12,
        encounter_id: 'GremlinNob', public_map: { boss: 'TheGuardian' } },
      observation: { player: { ascension_level: 20 } } };
    }
    return value;
  };
  await f.setPosition(position('shop', 1, 'Act 1 F12 | Shop\nOwned: Bash and 4 Defend\nChoices: BottledFlame; purge'));
  const requests = [];
  const steps = [
    ['read', null, 'Shop investigation: compare purchases.'],
    ['buy:BottledFlame#0', position('run_choice', 2, 'Act 1 F12 | Select bottled attack\nChoices: Bash; Strike'), 'Choose Bash for the bottle.'],
    ['select:Bash#0', position('shop', 3, 'Act 1 F12 | Shop\nOwned: Bottled Flame; bottled Bash'), 'The bottle purchase completed.'],
    ['leave', position('map', 4, 'Act 1 F12 | Map\nOwned: bottled Bash\nChoices: go 2,5'), 'Stale final shop plan must leave the next request.'],
    ['go 2,5', position('combat', 5, 'Act 1 F12 | Combat Nob\nTurn 1 | HP 70/80\nHand: Bash\nEnrage: Skills increase enemy Strength.'), 'Map plan.'],
    ['research whole_combat 8000 work 200000', null, 'Combat investigation: compare early setup.'],
    ['preview play:Bash@0', null, 'Combat preview evidence.'],
    ['act play:Bash@0', position('combat', 6, 'Act 1 F12 | Combat Nob\nTurn 1 | HP 70/80\nHand: Defend'), 'Adopt the checked prefix.'],
    ['commit 0', position('reward', 7, 'Act 1 F12 | Reward\nHP 62/80\nChoices: open-card#0'), 'Stale final combat conclusion.'],
    ['open-card#0', position('card_reward', 8, 'Act 1 F12 | Card reward\nChoices: take:ShrugItOff#0; skip'), 'Reward investigation: compare draw with skip.'],
    ['take:ShrugItOff#0', position('map', 9, 'Act 1 F12 | Map\nDeck: added Shrug It Off\nChoices: go 3,6'), 'Stale reward conclusion.'],
    ['go 3,6', { kind: 'terminal', revision: 10, outcome: { kind: 'heart_victory' }, reading: 'Terminal | Heart victory' }, 'Last map thought.'],
  ];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      const index = requests.length;
      requests.push(JSON.stringify(options.messages));
      const step = steps[index];
      const reason = step?.[2] ?? 'Done.';
      yield { type: 'block-start', index: 0, blockType: 'text' };
      yield { type: 'text-delta', index: 0, text: reason };
      yield { type: 'block-end', index: 0, block: { type: 'text', text: reason } };
      if (step) {
        if (step[1]) await f.setPosition(step[1]);
        const block = { type: 'tool-call', id: `action-${index}`, name: 'sts', arguments: JSON.stringify({ command: step[0] }) };
        yield { type: 'block-start', index: 1, blockType: 'tool-call' };
        yield { type: 'tool-call-delta', index: 1, id: block.id, name: block.name, argumentsDelta: block.arguments };
        yield { type: 'block-end', index: 1, block };
      }
      yield { type: 'finish', reason: { kind: step ? 'tool-calls' : 'stop' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create();
  t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Finish this bound Heart run independently.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests.length, 13);
  // Check the request before the adapter's first read call can hide a missing
  // initial checkpoint. The actual inventory must already be supplied.
  assert.match(requests[0], /Act 1 F12 \| Shop/);
  assert.match(requests[0], /Owned: Bash and 4 Defend/);
  assert.match(requests[0], /Choices: BottledFlame; purge/);
  assert.equal(requests[0].split('Finish this bound Heart run independently.').length - 1, 1,
    'the supplied initial task is not duplicated inside its factual checkpoint');
  // A bottle's selection and return keep the shop investigation.
  assert.match(requests[2], /Shop investigation/);
  assert.match(requests[3], /Shop investigation|bottle purchase completed/);
  // The action that exits a task may itself contain stale reasoning. It goes too.
  assert.doesNotMatch(requests[4], /Shop investigation|Stale final shop|4 Defend/);
  assert.match(requests[4], /Choices: go 2,5|Owned: bottled Bash|Completed: leave/);
  assert.doesNotMatch(requests[5], /Map plan/);
  assert.match(requests[5], /Enrage: Skills increase enemy Strength/);
  assert.match(requests[8], /Combat investigation: compare early setup/);
  assert.match(requests[8], /Combat preview evidence/);
  assert.match(requests[8], /Adopt the checked prefix/);
  assert.doesNotMatch(requests[9], /Combat investigation: compare early setup|Combat preview evidence|Stale final combat/);
  assert.match(requests[9], /HP 62\/80|Completed: commit 0/);
  assert.match(requests[10], /Reward investigation/);
  assert.doesNotMatch(requests[11], /Reward investigation|Stale reward/);
  assert.match(requests[12], /Terminal \| Heart victory/);
  assert.doesNotMatch(requests[12], /Last map thought/);
  for (const request of requests) assert.match(request, /Finish this bound Heart run independently/);
  const events = handle.agent.session.snapshotEvents();
  assert.equal(events.filter(e => e.type === 'tool/call').length, steps.length);
  assert.equal(events.filter(e => e.type === 'tool/result').length, steps.length);
  assert.equal(events.filter(e => e.type === 'user/message' && e.surfaceOp?.op === 'replace').length, 5);
  assert.ok(events.some(e => e.type === 'assistant/message' && JSON.stringify(e.data).includes('Stale final shop')));
});

test('a newly admitted continuation replaces obsolete task guidance before the official model request', async t => {
  const f = await fixture(t, 'current rules');
  const shop = preparationPosition(1, 5, 'shop', 1);
  shop.reading = 'Act 1 F5 | Shop\nGold 166\nChoices: buy:Inflame#0; leave';
  await f.setPosition(shop);
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests.push(options.messages);
      yield { type: 'block-start', index: 0, blockType: 'text' };
      yield { type: 'text-delta', index: 0, text: 'Decision investigated.' };
      yield { type: 'block-end', index: 0, block: { type: 'text', text: 'Decision investigated.' } };
      yield { type: 'finish', reason: { kind: 'stop' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create(); t.after(() => handle.dispose());
  const followup = text => handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text }] }));
  followup('Obsolete instruction: reconnect by reading first.');
  await handle.agent.whenIdle();
  requestBranchReset(handle.agent);
  followup('Current instruction: use the supplied actual state and propose one choice.');
  await handle.agent.whenIdle();
  assert.equal(requests.length, 2);
  const current = JSON.stringify(requests[1]);
  assert.doesNotMatch(current, /Obsolete instruction/);
  assert.equal(current.split('Current instruction: use the supplied actual state and propose one choice.').length - 1, 1);
  assert.match(current, /Act 1 F5 \| Shop.*Gold 166/);
  assert.equal(handle.agent.session.snapshotEvents().filter(event => event.type === 'user/message'
    && JSON.stringify(event.data).includes('Obsolete instruction')).length, 1, 'the durable operator message is kept');
});

test('remounted coached tasks refresh prior queries through correction and nested purchase, then discard them on departure', async t => {
  const shop = preparationPosition(1, 12, 'shop', 1);
  shop.reading = 'Act 1 F12 | Shop\nGold 150\nChoices: buy:BottledFlame#0; leave';
  shop.queryResponses = { 'info encounter TheGuardian': 'Old Guardian query answer.', map: 'Old route query answer.' };
  const selected = preparationPosition(1, 12, 'run_choice', 2);
  selected.reading = 'Act 1 F12 | Select bottled attack\nChoices: act select:Bash#0';
  selected.queryResponses = { 'info encounter TheGuardian': 'Current Guardian query answer.', map: 'Current route query answer.' };
  selected.queryErrors = ['map'];
  const f = await fixture(t, 'current rules', false, { actions: { 'buy:BottledFlame#0': selected } });
  let phase = 0;
  const requests = [[], [], [], []];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      const step = requests[phase].length;
      requests[phase].push(options.messages);
      const command = phase === 0 ? ['info encounter TheGuardian', 'map', 'buy:BottledFlame#0'][step]
        : phase === 1 ? 'buy:BottledFlame#0' : phase === 2 ? 'act select:Bash#0' : 'go 3,7';
      const text = phase === 0 ? 'Discard this old model theory.' : 'Continue from the host facts.';
      yield { type: 'block-start', index: 0, blockType: 'text' };
      yield { type: 'text-delta', index: 0, text };
      yield { type: 'block-end', index: 0, block: { type: 'text', text } };
      const block = { type: 'tool-call', id: `phase-${phase}-${step}`, name: 'sts', arguments: JSON.stringify({ command }) };
      yield { type: 'block-start', index: 1, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 1, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 1, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const run = async (position, seed) => {
    await f.setPosition(position);
    const handle = await f.create({ supervised: true }, seed);
    try {
      handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Continue the bound run.' }] }));
      await handle.agent.whenIdle();
      return handle.agent.session.snapshotEvents();
    } finally { await handle.dispose(); }
  };
  let seed = await run(shop);
  let review = await readPlayerReview(f.session);
  await setPlayerGuidance(f.session, review.frame, 'Current correction; inspect the latest facts.', { reject: true });
  shop.queryResponses = { 'info encounter TheGuardian': 'Refreshed Guardian query answer.', map: 'Refreshed route query answer.' };
  shop.reading += '\nRefreshed route query answer.';
  phase = 1;
  seed = await run(shop, seed);
  let supplied = JSON.stringify(requests[1][0]);
  assert.match(supplied, /Refreshed Guardian query answer/);
  assert.match(supplied, /Refreshed route query answer/);
  assert.equal(supplied.split('Refreshed route query answer.').length - 1, 1, 'a query answer already in the actual reading is not duplicated');
  assert.match(supplied, /Query: map.*Included in the current reading above/);
  assert.match(supplied, /Current correction/);
  assert.doesNotMatch(supplied, /Old Guardian|Old route|Discard this old model theory/);
  assert.equal(supplied.split('Refreshed Guardian query answer.').length - 1, 1);
  const { openWorkbench } = await import(pathToFileURL(join(f.repository, 'tools/run_play/workbench.mjs')));
  const game = await openWorkbench({ recordDirectory: join(f.repository, 'adoption-wire') });
  try { await adoptPlayerProposal(f.session, game); } finally { await game.close(); }
  phase = 2;
  seed = await run(selected, seed);
  supplied = JSON.stringify(requests[2][0]);
  assert.match(supplied, /Current Guardian query answer/);
  assert.match(supplied, /Earlier query could not be refreshed.*Its old answer was not retained/);
  assert.doesNotMatch(supplied, /Current route query answer/);
  assert.equal(requests[2][0].find(message => message.source?.kind === 'sts-decision').source.queryRefs.length, 1);
  assert.match(supplied, /Select bottled attack|Current correction/);
  assert.doesNotMatch(supplied, /Gold 150|Choices: buy:BottledFlame|Old Guardian|Refreshed Guardian/);
  review = await readPlayerReview(f.session);
  assert.equal(review.frame.task.family, 'shop', 'the bottle target is still the shopping task');
  await setPlayerGuidance(f.session, review.frame, 'Finish this shop.', { reject: true });
  const map = preparationPosition(1, 12, 'map', 3);
  map.reading = 'Act 1 F12 | Map\nChoices: go 3,7';
  phase = 3;
  await run(map, seed);
  supplied = JSON.stringify(requests[3][0]);
  assert.doesNotMatch(supplied, /Guardian query answer|route query answer|Current correction|Finish this shop/);
  assert.equal((await f.calls()).filter(command => command === 'info encounter TheGuardian').length, 3);
  assert.equal((await f.calls()).filter(command => command === 'map').length, 3);
});

test('coached handoff follows the live source at model admission and guidance expires on task departure', async t => {
  const f = await fixture(t, 'current rules');
  const shop = preparationPosition(1, 5, 'shop', 1);
  shop.reading = 'Act 1 F5 | Shop\nGold 70\nOwned: Inflame\nChoices: leave';
  await f.setPosition(shop);
  const source = { schema: 'sts-run-play-agent-observation-v3', session_id: 'fixture', head_revision: 1, state_revision: 1 };
  const task = gameDecisionTask({ ...source, position: shop });
  await writeFile(join(f.session, 'dsh-review.json'), JSON.stringify({ schema: 'sts-dsh-review-v1', status: 'adopted',
    command: 'buy:Inflame#0', selectedCommand: 'buy:Inflame#0', sourceAfter: source, frame: { task },
    maintainerNote: 'Verified purchase marker.', guidance: { task, text: 'Prefer draw support in this shop.' } }));
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests.push(options.messages);
      yield { type: 'block-start', index: 0, blockType: 'text' };
      yield { type: 'text-delta', index: 0, text: 'Current decision investigated.' };
      yield { type: 'block-end', index: 0, block: { type: 'text', text: 'Current decision investigated.' } };
      yield { type: 'finish', reason: { kind: 'stop' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ supervised: true }); t.after(() => handle.dispose());
  const continuePlayer = async position => {
    await f.setPosition(position);
    requestBranchReset(handle.agent);
    handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Continue the current decision.' }] }));
    await handle.agent.whenIdle();
  };
  await continuePlayer(shop);
  const checkpoint = requests[0].find(message => message.source?.kind === 'sts-decision');
  assert.match(JSON.stringify(checkpoint), /Verified purchase marker.*Gold 70/);
  assert.match(JSON.stringify(checkpoint), /Prefer draw support/);
  assert.doesNotMatch(JSON.stringify(requests[0].filter(message => message.source?.kind === 'user')), /Verified purchase marker/);
  const moved = { ...shop, revision: 2, reading: 'Act 1 F5 | Shop\nGold 50\nChoices: leave' };
  await continuePlayer(moved);
  assert.doesNotMatch(JSON.stringify(requests[1]), /Verified purchase marker|already executed this choice/);
  assert.match(JSON.stringify(requests[1]), /Gold 50|Prefer draw support/);
  const map = preparationPosition(1, 5, 'map', 3);
  map.reading = 'Act 1 F5 | Map\nChoices: go 5,5';
  await continuePlayer(map);
  assert.doesNotMatch(JSON.stringify(requests[2]), /Prefer draw support|Verified purchase marker/);
  assert.match(JSON.stringify(requests[2]), /Act 1 F5 \| Map/);
});

test('a changing snapshot stops context rebuild before making a model request', async t => {
  const f = await fixture(t, 'current rules', false, { readDrift: true });
  const current = preparationPosition(1, 12, 'shop');
  current.reading = 'Act 1 F12 | Shop\nChoices: leave';
  await f.setPosition(current);
  let requests = 0;
  class Adapter extends LlmAdapter { async *stream() { requests++; throw new Error('unexpected model request'); } }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create();
  t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Finish the run.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests, 0);
  assert.match(JSON.stringify(handle.agent.session.snapshotEvents().at(-1)), /position changed while rebuilding/);
});

function scopedPosition(site, revision) {
  const position = preparationPosition(2, 21, site, revision);
  position.reading = `Act 2 F21 | ${site}\nOwned: Bash\nChoices: buy:BottledFlame#0; select:Bash#0; leave`;
  position.actions = site === 'run_choice' ? [{ semantic_action: null, candidate: { action: { kind: 'begin_run_card_selection' } },
    run_selection_family: { reason: 'Bottled', min_choices: 1, max_choices: 1, card_tokens: ['Bash+0#0'],
      selectable_cards: [{ ordinal: 0, card: 'Bash', upgrades: 0 }] } }]
    : ['buy:BottledFlame#0', 'buy:Flex#0', 'buy:Evolve#0', 'leave']
      .map(semantic_action => ({ semantic_action, candidate: { action: {} } }));
  return position;
}

const fixtureBasis = (references = '@effect,@resource-tradeoff') => ['buy:BottledFlame#0', 'buy:Flex#0', 'buy:Evolve#0', 'leave']
  .map(command => `${command} | medium | direct | ${references}`).join('\n');
const rawGameExec = (ctx, agent, command, basis, extra = {}) => ctx.tools.execute({ name: 'sts', arguments: { command, ...(basis !== undefined ? { basis } : {}), ...extra },
  agent, signal: new AbortController().signal, callId: `scoped-${Date.now()}-${command}` });
const gameExec = async (ctx, agent, command, basis = 'Prefer the offered benefit to keeping resources after comparing access and payment.') => {
  if (command === 'act select:Bash#0') {
    const classified = await rawGameExec(ctx, agent, 'classify', 'act select:Bash#0 | medium | direct | @effect,@resource-tradeoff');
    if (classified.isError) return classified;
    return rawGameExec(ctx, agent, command);
  }
  if (/^(buy:|leave$)/.test(command) && basis.trim()) {
    const classified = await rawGameExec(ctx, agent, 'classify', fixtureBasis());
    if (classified.isError) return classified;
    return rawGameExec(ctx, agent, command, `${command === 'leave' ? 'buy:BottledFlame#0' : 'leave'} | @function,@resources`);
  }
  return rawGameExec(ctx, agent, command, basis);
};
const resultText = result => result.content.filter(x => x.type === 'text').map(x => x.text).join('\n');

function routePosition() {
  const current = preparationPosition(2, 26, 'map', 1);
  current.observation.observation.run.current_hp = 21;
  current.observation.observation.public_map = { current: [3, 8], boss: 'Collector', nodes: [
    [2, 9, 'elite', false, []], [3, 9, 'monster', false, []], [4, 9, 'question_mark', false, []],
  ] };
  current.actions = [2, 3, 4].map(x => ({ semantic_action: `route:${x}.9`,
    candidate: { action: { kind: 'choose_route_node', x, y: 9 } } }));
  current.reading = 'Act 2 F26 | Map | HP 21/80\nChoices: go 2,9; go 3,9; go 4,9';
  return current;
}

const incompleteRouteBasis = 'go 3,9 | low | direct | @effect\ngo 4,9 | high | direct | @effect';
const correctedRouteRow = 'go 2,9 | low | direct | @effect';

test('official context replacement retains the full-table route choice while completing a missing row', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { 'go 4,9': preparationPosition(2, 27, 'event', 2) } });
  await f.setPosition(routePosition());
  const requests = [];
  const submissions = [
    { command: 'classify', basis: incompleteRouteBasis, choice: 'go 4,9' },
    { command: 'classify', basis: correctedRouteRow },
  ];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      const index = requests.length;
      requests.push(JSON.stringify(options.messages));
      assert.ok(index < submissions.length, 'both rows and saved choice should finish after correction');
      const block = { type: 'tool-call', id: `route-correction-${index}`, name: 'sts', arguments: JSON.stringify(submissions[index]) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete the current route choice.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests.length, 2);
  assert.match(requests[1], /Saved choice: go 4,9/);
  assert.match(requests[1], /Pending commands: go 2,9/);
  assert.doesNotMatch(requests[1], /Supply those rows and choice/);
  const calls = await f.calls();
  assert.equal(calls.filter(command => command === 'go 4,9').length, 1);
  assert.ok(!calls.includes('go 2,9'));
  const [adoption] = await f.adoptions();
  assert.equal(adoption.selected, 'go 4,9');
  assert.equal(adoption.adoptedChoice, 'go 4,9');
  assert.equal(adoption.source.state_revision, 1);
  const [classification] = await f.classifications();
  assert.equal(classification.pendingChoice, 'go 4,9');
  assert.equal(adoption.classificationAt, classification.at);
  assert.deepEqual(classification.rows.map(row => row.category), ['low', 'low', 'high']);
});

test('a correction cannot replace the saved route choice and a lower-rated route cannot pass adoption', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { 'go 4,9': preparationPosition(2, 27, 'event', 2) } });
  await f.setPosition(routePosition());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  const first = await rawGameExec(f.ctx, handle.agent, 'classify', incompleteRouteBasis, { choice: 'go 4,9' });
  assert.ok(!first.isError, resultText(first));
  const correction = await rawGameExec(f.ctx, handle.agent, 'classify', correctedRouteRow, { choice: 'go 2,9' });
  assert.ok(correction.isError, resultText(correction));
  assert.match(resultText(correction), /Saved choice: go 4,9.*submit command choose/s);
  assert.ok(!(await f.calls()).some(command => /^go /.test(command)));
  assert.equal((await f.classifications())[0].rows.length, 3, 'the valid correction row stays recorded');
  const inconsistent = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: 'go 2,9' });
  assert.ok(inconsistent.isError);
  assert.match(resultText(inconsistent), /rated low; higher recorded recommendations: go 4,9 \(high\)/);
  assert.ok(!(await f.calls()).includes('go 2,9'));
  const selected = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: 'go 4,9' });
  assert.ok(!selected.isError, resultText(selected));
  assert.equal((await f.adoptions()).length, 1);
});

test('ordinary preparation exposes only rows and choice while the host binds the recorded comparison', async t => {
  const f = await fixture(t, 'Current rules');
  const current = preparationPosition(2, 26, 'treasure', 1);
  current.actions = [
    { semantic_action: 'open-chest', candidate: { action: { kind: 'open_chest' } } },
    { semantic_action: 'proceed', candidate: { action: { kind: 'proceed' } } },
  ];
  await f.setPosition(current);
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const schema = f.ctx.tools.schemas(handle.agent).find(row => row.name === 'sts');
  assert.ok(!Object.hasOwn(schema.parameters.properties, 'comparison'));
  const adopted = await rawGameExec(f.ctx, handle.agent, 'classify',
    'open-chest | high | direct | @effect,@resource-tradeoff\nproceed | low | direct | @effect,@resource-tradeoff',
    { choice: 'open-chest' });
  assert.ok(!adopted.isError, resultText(adopted));
  const adoptions = await f.adoptions();
  assert.equal(adoptions[0].basisKind, 'host-comparison');
  assert.equal(adoptions[0].selected, 'open-chest');
});

test('preparation supplies checks before inference, hides file tools and rejects incomplete classifications before mutation', async t => {
  const f = await fixture(t, 'Current rules');
  await f.setPosition(scopedPosition('shop', 1));
  let fileCalls = 0;
  f.ctx.tools.register(defineTool({ name: 'read', description: 'Read fixture file', parameters: {},
    output: { schema: { type: 'string' } }, execute() { fileCalls++; return 'fixture'; } }));
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  assert.deepEqual(f.ctx.tools.schemas(handle.agent).map(tool => tool.name).sort(), ['sts', 'sts_feedback']);
  const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  assert.match(assembly.sections.find(section => section.name === 'sts:classification').text, /Current classification candidates:.*Applicable mechanism and preparation checks:.*buy:BottledFlame#0/s);
  const denied = await f.ctx.tools.execute({ name: 'read', arguments: {}, agent: handle.agent,
    signal: new AbortController().signal, callId: 'denied-file-read' });
  assert.ok(denied.isError);
  assert.equal(fileCalls, 0);
  const incomplete = await f.ctx.tools.execute({ name: 'sts', arguments: { command: 'classify', basis: 'leave | high | direct | @effect,@resource-tradeoff' },
    agent: handle.agent, signal: new AbortController().signal, callId: 'incomplete-classification' });
  assert.ok(!incomplete.isError);
  assert.match(resultText(incomplete), /Classify remaining candidates/);
  assert.ok(!(await f.calls()).includes('leave'));
});

test('choose adopts a recorded classification after a rejected comparison without resubmitting rows', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  const premature = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: 'leave' });
  assert.ok(premature.isError); assert.match(resultText(premature), /classification rows and choice/);
  assert.ok(!(await f.calls()).includes('leave'));
  const rejected = await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis(), {
    choice: 'leave', comparison: 'buy:Flex#0 | @resource-tradeoff',
  });
  assert.ok(rejected.isError); assert.match(resultText(rejected), /comparison references/);
  assert.ok(!(await f.calls()).includes('leave'));
  const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  assert.match(assembly.sections.find(section => section.name === 'sts:player-guide').text,
    /Classification complete.*Call sts with command choose and choice/s);
  const adopted = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: 'leave' });
  assert.ok(!adopted.isError, resultText(adopted));
  assert.equal((await f.calls()).filter(command => command === 'leave').length, 1);
  const adoptions = await f.adoptions();
  assert.equal(adoptions.length, 1); assert.equal(adoptions[0].selected, 'leave');
  assert.equal(adoptions[0].basisKind, 'host-comparison');
});

test('reward navigation supplies only pending work and guards a completed open command', async t => {
  const f = await fixture(t, 'Current rules');
  const current = preparationPosition(1, 12, 'reward', 7);
  current.reading = 'Rewards: open-card#0; claim:BlockPotion#0; proceed';
  current.actions = [
    { semantic_action: 'open-card#0', candidate: { action: { kind: 'open_card_reward', reward_item_index: 0 } } },
    { semantic_action: 'claim:BlockPotion#0', candidate: { action: { kind: 'claim_reward', reward_item_index: 1 } } },
    { semantic_action: 'proceed', candidate: { action: { kind: 'proceed' } } },
  ];
  await f.setPosition(current);
  const handle = await f.create({ taskScope: 'preparation', rewardProgress: {
    source: { schema: 'sts-run-play-agent-observation-v3', session_id: 'fixture', head_revision: 7, state_revision: 7 }, declined: [0],
  } });
  t.after(() => handle.dispose());
  const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  const text = assembly.sections.find(section => section.name === 'sts:classification').text;
  assert.match(text, /Completed card decisions.*reward items 1 were declined/);
  assert.doesNotMatch(text, /open-card#0/);
  const classify = await rawGameExec(f.ctx, handle.agent, 'classify',
    'claim:BlockPotion#0 | medium | direct | @effect,@resource-tradeoff\nproceed | low | direct | @effect,@resource-tradeoff');
  assert.ok(!classify.isError);
  const open = await rawGameExec(f.ctx, handle.agent, 'open-card#0', 'proceed | @function');
  assert.ok(open.isError);
  assert.ok(!(await f.calls()).includes('open-card#0'));
});

test('preparation scope owns experiments and stops a multi-command batch at its first complete purchase', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { 'buy:Flex#0': scopedPosition('shop', 2) } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const diagnosticsBefore = (await f.calls()).filter(x => x.startsWith('deck-lab '));
  assert.deepEqual(diagnosticsBefore, []);
  const probe = await gameExec(f.ctx, handle.agent, 'deck-lab TheHeart scenarios 9 work 900000');
  assert.ok(probe.isError);
  assert.match(resultText(probe), /one preparation/);
  assert.deepEqual((await f.calls()).filter(x => x.startsWith('deck-lab ')), diagnosticsBefore);
  const result = await gameExec(f.ctx, handle.agent, 'buy:Flex#0, buy:Evolve#0');
  assert.ok(result.isError);
  assert.match(resultText(result), /Completed: buy:Flex#0/);
  assert.match(resultText(result), /Preparation task complete/);
  assert.equal((await f.calls()).filter(x => x === 'buy:Flex#0').length, 1);
  assert.ok(!(await f.calls()).includes('buy:Evolve#0'));
});

test('preparation scope retains an attached selection then prevents leaving or replaying the preparation', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: {
    'buy:BottledFlame#0': scopedPosition('run_choice', 2),
    'act select:Bash#0': scopedPosition('shop', 3),
  } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const purchase = await gameExec(f.ctx, handle.agent, 'buy:BottledFlame#0');
  assert.match(resultText(purchase), /finish its attached choices/);
  assert.equal(purchase.concludesTurn, undefined);
  const unclassified = await rawGameExec(f.ctx, handle.agent, 'act select:Bash#0');
  assert.ok(unclassified.isError);
  assert.match(resultText(unclassified), /Call classify/);
  assert.ok(!(await f.calls()).includes('act select:Bash#0'));
  const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  assert.match(assembly.sections.find(section => section.name === 'sts:classification').text, /act select:Bash#0/);
  const selected = await gameExec(f.ctx, handle.agent, 'act select:Bash#0');
  assert.ok(!selected.isError, resultText(selected));
  assert.equal(selected.concludesTurn, true);
  assert.match(resultText(selected), /Preparation task complete/);
  assert.match(resultText(selected), /Adopted preparation command: buy:BottledFlame#0/);
  const classifications = await f.classifications();
  assert.deepEqual(classifications.map(row => row.stage), ['pending', 'selection']);
  assert.equal(classifications[1].taskRoot.source.state_revision, 1);
  const adoptions = await f.adoptions();
  assert.equal(adoptions[1].selected, 'act select:Bash#0');
  assert.equal(adoptions[1].classificationAt, classifications[1].at);
  for (const command of ['leave', 'buy:BottledFlame#0', 'mark Done']) {
    assert.ok((await gameExec(f.ctx, handle.agent, command)).isError);
  }
  const read = await gameExec(f.ctx, handle.agent, 'read');
  assert.ok(!read.isError);
  assert.match(resultText(read), /Preparation task complete/);
});

test('a saved action followed by transport failure still completes the preparation, including on resume', async t => {
  const initial = scopedPosition('shop', 1);
  const root = preparationTaskRoot({ schema: 'sts-run-play-agent-observation-v3', session_id: 'fixture',
    head_revision: 1, state_revision: 1, position: initial });
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) }, failAfter: 'leave' });
  await f.setPosition(initial);
  const handle = await f.create({ taskScope: 'preparation', preparationRoot: root });
  const failed = await gameExec(f.ctx, handle.agent, 'leave');
  assert.ok(failed.isError);
  assert.match(resultText(failed), /injected failure/);
  assert.match(resultText(await gameExec(f.ctx, handle.agent, 'go 4,4')), /Preparation task complete/);
  assert.match(resultText(await gameExec(f.ctx, handle.agent, 'read')), /Preparation task complete/);
  await handle.dispose();
  const resumed = await f.create({ taskScope: 'preparation', preparationRoot: root });
  t.after(() => resumed.dispose());
  const assembled = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  assert.match(assembled.sections.find(x => x.name === 'sts:task-progress').text, /Preparation task complete/);
  assert.ok((await gameExec(f.ctx, resumed.agent, 'go 4,4')).isError);
  assert.equal((await f.calls()).filter(x => x === 'leave').length, 1);
  assert.ok(!(await f.calls()).includes('go 4,4'));
});

test('acknowledged card and classified actions retain their decision when the following observation fails', async t => {
  for (const cardChoice of [false, true]) {
    const command = cardChoice ? 'take:BurningPact#0' : 'leave';
    const f = await fixture(t, 'Current rules', false, { actions: { [command]: scopedPosition('map', 2) },
      failObservationAfterAction: true });
    await f.setPosition(cardChoice ? checkedCardPosition() : scopedPosition('shop', 1));
    const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
    const failed = cardChoice
      ? await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: command })
      : await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis(), { choice: command });
    assert.ok(failed.isError, resultText(failed));
    const recovered = await rawGameExec(f.ctx, handle.agent, 'read');
    assert.ok(!recovered.isError, resultText(recovered));
    assert.match(resultText(recovered), new RegExp(`Adopted preparation command: ${command}`));
    assert.match(resultText(recovered), /Preparation task complete/);
    assert.match(resultText(failed), new RegExp(`Adopted preparation command: ${command}`));
    assert.equal((await f.calls()).filter(call => call === command).length, 1);
  }
});

test('official recovery keeps an acknowledged decision through a failed observation and excludes read as an adoption', async t => {
  const command = 'take:BurningPact#0';
  const f = await fixture(t, 'Current rules', false, { actions: { [command]: scopedPosition('map', 2) },
    failObservationAfterAction: true });
  await f.setPosition(checkedCardPosition());
  const requests = [], submissions = [{ command: 'choose', choice: command }, { command: 'read' }];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      const index = requests.length; requests.push(JSON.stringify(options.messages));
      assert.ok(index < submissions.length);
      const block = { type: 'tool-call', id: `acknowledged-recovery-${index}`, name: 'sts', arguments: JSON.stringify(submissions[index]) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete the current reward choice.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests.length, 2);
  const decision = restoredPreparationDecision(handle.agent.session);
  assert.equal(decision.command, command);
  assert.doesNotMatch(JSON.stringify(handle.agent.session.snapshotEvents()), /Adopted preparation command: read/);
  assert.equal((await f.calls()).filter(call => call === command).length, 1);
});

test('a combined target classification accepts the existing space-separated select spelling', async t => {
  const command = 'act select:Bash#0';
  const f = await fixture(t, 'Current rules', false, { actions: { [command]: scopedPosition('event', 2) } });
  await f.setPosition(scopedPosition('run_choice', 1));
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  const adopted = await rawGameExec(f.ctx, handle.agent, 'classify',
    'act select:Bash#0 | high | direct | @effect', { choice: 'act select Bash+0#0' });
  assert.ok(!adopted.isError, resultText(adopted));
  assert.equal((await f.calls()).filter(call => call === command).length, 1);
  assert.equal((await f.classifications())[0].pendingChoice, command);
});

test('classified adoption checks the current source after writing its intent and preserves an externally changed position', async t => {
  const changed = scopedPosition('shop', 2);
  const f = await fixture(t, 'Current rules', false, { adoptionObservationPosition: changed,
    actions: { leave: scopedPosition('map', 3) } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  const result = await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis(), { choice: 'leave' });
  assert.ok(result.isError);
  assert.match(resultText(result), /position changed before adopting/);
  assert.doesNotMatch(resultText(result), /Adopted preparation command/);
  assert.ok(!(await f.calls()).includes('leave'));
  assert.deepEqual(JSON.parse(await readFile(join(f.repository, 'position.json'), 'utf8')), changed);
  const recovery = await rawGameExec(f.ctx, handle.agent, 'read');
  assert.match(resultText(recovery), /Preparation task complete/);
  assert.doesNotMatch(resultText(recovery), /Adopted preparation command/);
});

test('a multi-target task adopts one complete selection with all member classifications', async t => {
  const initial = scopedPosition('run_choice', 1);
  const family = initial.actions[0].run_selection_family;
  Object.assign(family, { reason: 'Purge', min_choices: 2, max_choices: 2,
    card_tokens: ['Strike+0#0', 'Strike+0#1', 'Bash+0#0'], selectable_cards: [
      { ordinal: 0, card: 'Strike', upgrades: 0 }, { ordinal: 2, card: 'Strike', upgrades: 0 }, { ordinal: 5, card: 'Bash', upgrades: 0 },
    ] });
  const command = 'act select:Strike#1,Strike#0';
  const f = await fixture(t, 'Current rules', false, { actions: { [command]: scopedPosition('event', 2) } });
  await f.setPosition(initial);
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const classified = await rawGameExec(f.ctx, handle.agent, 'classify',
    'act select:Strike#0 | high | direct | @effect,@resource-tradeoff\nact select:Strike#1 | high | direct | @effect,@resource-tradeoff\nact select:Bash#0 | low | direct | @effect,@resource-tradeoff');
  assert.ok(!classified.isError, resultText(classified));
  const adopted = await rawGameExec(f.ctx, handle.agent, 'act select Strike+0#1 Strike+0#0');
  assert.ok(!adopted.isError, resultText(adopted));
  assert.equal(adopted.concludesTurn, true);
  assert.equal((await f.calls()).filter(call => call === command).length, 1);
  assert.deepEqual((await f.adoptions())[0].selectedRows.map(row => row.command), ['act select:Strike#1', 'act select:Strike#0']);
  assert.deepEqual((await f.adoptions())[0].unselectedRows.map(row => row.command), ['act select:Bash#0']);
});

test('single target adoption and its alternative bind the native select spelling', async t => {
  const initial = scopedPosition('run_choice', 1);
  initial.actions = ['select:Bash+0#0', 'select:Disarm+0#0'].map(semantic_action => ({ semantic_action,
    candidate: { action: { kind: 'submit_run_selection', selected_card_ordinals: [0] } } }));
  const f = await fixture(t, 'Current rules', false, { actions: { 'act select:Bash#0': scopedPosition('event', 2) } });
  await f.setPosition(initial);
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  assert.ok(!(await rawGameExec(f.ctx, handle.agent, 'classify',
    'act select:Bash#0 | high | direct | @effect,@resource-tradeoff\nact select:Disarm#0 | medium | direct | @effect,@resource-tradeoff')).isError);
  const adopted = await rawGameExec(f.ctx, handle.agent, 'select:Bash+0#0', 'select:Disarm+0#0 | @function');
  assert.ok(!adopted.isError, resultText(adopted));
  assert.equal(adopted.concludesTurn, true);
  const [row] = await f.adoptions();
  assert.equal(row.selectedRow.command, 'act select:Bash#0');
  assert.equal(row.alternativeRow.command, 'act select:Disarm#0');
  assert.equal((await f.calls()).filter(command => command === 'act select:Bash#0').length, 1);
});

test('typed preparation progress follows generated rewards and finishes a single card reward at its bundle', () => {
  const observation = (position, revision) => ({ schema: 'sts-run-play-agent-observation-v3', session_id: 'fixture',
    head_revision: revision, state_revision: revision, position });
  const shopRoot = preparationTaskRoot(observation(scopedPosition('shop', 1), 1));
  assert.equal(preparationTaskProgress(shopRoot, observation(scopedPosition('shop', 1), 1)), 'pending');
  assert.equal(preparationTaskProgress(shopRoot, observation(scopedPosition('run_choice', 2), 2)), 'selection');
  const evidenceSelection = observation(scopedPosition('run_choice', 2), 2);
  evidenceSelection.state_revision = 1;
  assert.equal(preparationTaskProgress(shopRoot, evidenceSelection), 'selection');
  const evidenceFinished = observation(scopedPosition('shop', 3), 3);
  evidenceFinished.state_revision = 1;
  assert.equal(preparationTaskProgress(shopRoot, evidenceFinished), 'complete');
  for (const site of ['reward', 'card_reward']) {
    assert.equal(preparationTaskProgress(shopRoot, observation(scopedPosition(site, 2), 2)), 'complete');
  }
  assert.equal(preparationTaskProgress(shopRoot, observation(scopedPosition('shop', 3), 3)), 'complete');
  const rewardRoot = preparationTaskRoot(observation(scopedPosition('card_reward', 1), 1));
  assert.equal(preparationTaskProgress(rewardRoot, observation(scopedPosition('reward', 2), 2)), 'complete');
  const mapRoot = preparationTaskRoot(observation(scopedPosition('map', 1), 1));
  assert.equal(mapRoot.task.family, 'map');
  assert.equal(preparationTaskProgress(mapRoot, observation(preparationPosition(2, 22, 'event', 2), 2)), 'complete');
  const combat = preparationPosition(2, 22, 'map', 2); combat.observation.domain = 'combat'; combat.observation.run_context = combat.observation.observation.run;
  assert.throws(() => preparationTaskRoot(observation(combat, 2)), /strategic decision/);
});

function checkedCardPosition() {
  const current = preparationPosition(2, 21, 'card_reward');
  const owned = { card: 'DemonForm', upgrades: 0, count: 1, misc_value: 0,
    base_damage_override: null, base_block_override: null, cost_modifier: 0 };
  current.public_snapshot_id = 'checked-card-root';
  const state = current.observation.observation;
  state.cards = [owned]; state.base_energy_per_turn = 3;
  current.deckEvidence = { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: current.public_snapshot_id,
    cards: [{ card: owned, persistent_cost: 3, persistent_damage: 0, persistent_block: 0, mechanics: {
      card: 'DemonForm', upgrades: 0, card_type: 'Power', cost: 3, draw_cards: 0,
      strength_gain: 2, strength_gain_delivery: 'each_player_turn_start_post_draw' } }] };
  current.actions = [
    { semantic_action: 'take:BurningPact#0', candidate: { action: { kind: 'take_card', card: 'BurningPact', upgrades: 0 } } },
    { semantic_action: 'skip-card#0', candidate: { action: { kind: 'skip_card_reward' } } },
  ];
  current.decisionEvidence = { kind: 'card_acquisition_options', candidates: [{ semantic_action: 'take:BurningPact#0',
    cards: [{ current: { card: 'BurningPact', upgrades: 0, card_type: 'Skill', cost: 1, draw_cards: 2,
      exhausts_other_cards: true, exhausts: false } }] }] };
  current.reading = 'Act 2 F21\ntake:BurningPact#0  |  Burning Pact\n  Card Burning Pact | Skill | 1c\n    Printed rule (base/upgraded): Exhaust one other Hand card, then draw 2.\nskip-card#0  |  Skip\n';
  current.preparationOptions = { cardRuleViews: { BurningPact: 'Card Burning Pact | Skill | 1c\nPrinted rule: Exhaust one other Hand card, then draw 2.' } };
  return current;
}

function uncoveredCardEventPosition() {
  const current = checkedCardPosition(), owned = current.observation.observation.cards[0];
  owned.card = 'Strike'; current.deckEvidence.cards[0].persistent_cost = 1;
  current.deckEvidence.cards[0].mechanics = { card: 'Strike', upgrades: 0, card_type: 'Attack', cost: 1, draw_cards: 0 };
  current.observation.observation.context = { kind: 'event', event: 'TheLibrary' };
  current.actions = [{ semantic_action: 'event:TheLibrary:0', candidate: { action: { kind: 'choose_event_option',
    public_semantics: { hp_delta: 0, max_hp_delta: 0, gold_delta_min: 0, gold_delta_max: 0,
      starts_combat: false, random_outcome: false, structured_semantics: { effects: [
        { effect: 'obtain_card', count: 1, pool: { kind: 'specific', card: 'BurningPact' } },
      ] } } } } }];
  current.decisionEvidence.candidates[0].semantic_action = 'event:TheLibrary:0';
  current.actions.push({ semantic_action: 'event:TheLibrary:1', candidate: { action: { kind: 'choose_event_option',
    public_semantics: { hp_delta: 0, max_hp_delta: 0, gold_delta_min: 0, gold_delta_max: 0,
      starts_combat: false, random_outcome: false, structured_semantics: { effects: [
        { effect: 'obtain_card', count: 1, pool: { kind: 'specific', card: 'Sentinel' } },
      ] } } } } });
  current.decisionEvidence.candidates.push({ semantic_action: 'event:TheLibrary:1', cards: [{ current: {
    card: 'Sentinel', upgrades: 0, card_type: 'Skill', cost: 1, block: 5, draw_cards: 0,
  } }] });
  current.reading = current.reading.replace('take:BurningPact#0', 'event:TheLibrary:0');
  return current;
}

test('checked card choice supplies a compact task and executes once without a classification table', async t => {
  const f = await fixture(t, 'Enemy catalogue marker', false, { actions: { 'take:BurningPact#0': scopedPosition('map', 2) } });
  await f.setPosition(checkedCardPosition());
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests.push(JSON.stringify(options.messages));
      assert.equal(requests.length, 1);
      const block = { type: 'tool-call', id: 'checked-choice', name: 'sts',
        arguments: JSON.stringify({ command: 'choose', choice: 'take:BurningPact#0' }) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete the supplied step.' }] }));
  await handle.agent.whenIdle();
  assert.match(requests[0], /Card choice.*reach-owned-functions.*Exhaust one other Hand card/);
  assert.doesNotMatch(requests[0], /Enemy catalogue marker|high\/medium|direct\/sequence|Current classification candidates/);
  assert.ok(!(await f.calls()).some(call => call.startsWith('info rules')));
  assert.equal((await f.calls()).filter(command => command === 'take:BurningPact#0').length, 1);
  const adoptions = await f.adoptions();
  assert.deepEqual(adoptions.map(row => row.phase), ['intent', 'applied']);
  assert.equal(adoptions[0].followsDefault, true);
  assert.equal(adoptions[0].basisKind, 'host-checked-card-choice');
});

test('checked card choice rejects a batch or invented card before execution and recovers its compact reading', async t => {
  const f = await fixture(t, 'Enemy catalogue marker'); await f.setPosition(checkedCardPosition());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  for (const choice of ['take:BurningPact#0, leave', 'take:Sentinel#0']) {
    const result = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice });
    assert.ok(result.isError);
    assert.match(resultText(result), /one current command/);
  }
  const recovered = await rawGameExec(f.ctx, handle.agent, 'read');
  assert.ok(!recovered.isError);
  assert.match(resultText(recovered), /Card choice.*reach-owned-functions/s);
  assert.doesNotMatch(resultText(recovered), /Enemy catalogue marker|Current classification candidates/);
  assert.ok(!(await f.calls()).some(command => command === 'take:Sentinel#0' || command === 'take:BurningPact#0'));
  await assert.rejects(f.adoptions(), /ENOENT/);
});

test('direct preparation supplies its checked draw choice and excludes the other legal choices from adoption', async t => {
  const current = checkedCardPosition();
  for (const [card, cost, draws] of [['BattleTrance', 0, 3], ['Sentinel', 1, 0]]) {
    current.actions.push({ semantic_action: `take:${card}#0`, candidate: { action: { kind: 'take_card', card, upgrades: 0 } } });
    current.decisionEvidence.candidates.push({ semantic_action: `take:${card}#0`, cards: [{ current: {
      card, upgrades: 0, card_type: 'Skill', cost, draw_cards: draws, exhausts: false,
    } }] });
    current.reading += `take:${card}#0  |  ${card}\n  Card ${card} | Skill | ${cost}c\n    Printed rule (base/upgraded): ${card === 'BattleTrance'
      ? 'Draw 3(4) cards. You cannot draw additional cards this turn.' : 'Gain 5(8) Block. If this card is exhausted, gain 2(3) Energy.'}\n`;
  }
  const f = await fixture(t, 'Enemy catalogue marker', false, { actions: { 'take:BattleTrance#0': scopedPosition('map', 2) } });
  await f.setPosition(current);
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests.push(JSON.stringify(options.messages));
      assert.equal(requests.length, 1);
      const block = { type: 'tool-call', id: 'bounded-choice', name: 'sts',
        arguments: JSON.stringify({ command: 'choose', choice: 'take:BattleTrance#0' }) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  const excluded = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: 'take:Sentinel#0' });
  assert.ok(excluded.isError); assert.match(resultText(excluded), /policy comparison option/);
  await assert.rejects(f.adoptions(), /ENOENT/);
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete the supplied step.' }] }));
  await handle.agent.whenIdle();
  assert.match(requests[0], /1 of 4 legal options.*take:BattleTrance#0/s);
  assert.match(requests[0], /host supplied one policy choice/);
  assert.doesNotMatch(requests[0], /Apply the supplied comparison question/);
  assert.doesNotMatch(requests[0], /take:Sentinel#0|skip-card#0|Enemy catalogue marker/);
  assert.equal((await f.calls()).filter(command => command === 'take:BattleTrance#0').length, 1);
});

test('an uncovered ordinary reward supplies its review gap and keeps the game unchanged', async t => {
  const current = checkedCardPosition(), owned = current.observation.observation.cards[0];
  owned.card = 'Strike'; current.deckEvidence.cards[0].persistent_cost = 1;
  current.deckEvidence.cards[0].mechanics = { card: 'Strike', upgrades: 0, card_type: 'Attack', cost: 1, draw_cards: 0 };
  current.observation.observation.relics = [{ relic: 'MummifiedHand' }];
  for (const card of ['Clash', 'Sentinel']) {
    current.actions.push({ semantic_action: `take:${card}#0`, candidate: { action: { kind: 'take_card', card, upgrades: 0 } } });
    current.decisionEvidence.candidates.push({ semantic_action: `take:${card}#0`, cards: [{ current: {
      card, upgrades: 0, card_type: card === 'Clash' ? 'Attack' : 'Skill', cost: 1, draw_cards: 0,
    } }] });
    current.reading += `take:${card}#0  |  ${card}\n  Card ${card} | 1c\n    Printed rule: supplied full ${card} rule.\n`;
  }
  current.reading += 'Relics (1):\n  Mummified Hand\n    Playing a Power changes one Hand card to 0 Energy this turn.\nPotions (1 slots; availability comes from current legal options):\n  P0 empty\n';
  const f = await fixture(t, 'Enemy catalogue marker', false, { actions: { 'skip-card#0': scopedPosition('map', 2) } });
  await f.setPosition(current); const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests.push(JSON.stringify(options.messages)); assert.equal(requests.length, 1);
      const block = { type: 'tool-call', id: 'ordinary-review', name: 'sts_feedback',
        arguments: JSON.stringify({ message: 'Review the uncovered reward drafting conditions.' }) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete the supplied step.' }] }));
  await handle.agent.whenIdle();
  assert.match(requests[0], /retained this reward for policy review/);
  assert.match(requests[0], /take:Clash#0.*conditional-attack-hand/s);
  assert.match(requests[0], /take:Sentinel#0.*uncovered-reward-card/s);
  assert.doesNotMatch(requests[0], /Mummified Hand|Apply the supplied comparison question/);
  assert.doesNotMatch(requests[0], /Enemy catalogue marker|high\/medium|direct\/sequence/);
  assert.equal((await f.calls()).filter(command => command === 'skip-card#0').length, 0);
  await assert.rejects(f.adoptions(), /ENOENT/);
});

test('a card-choice policy gap records feedback and ends the task at its original position', async t => {
  const f = await fixture(t, 'Enemy catalogue marker');
  const current = uncoveredCardEventPosition();
  await f.setPosition(current);
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  const attempt = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: 'event:TheLibrary:0' });
  assert.ok(attempt.isError); assert.match(resultText(attempt), /needs a supplied strategy policy/);
  const feedback = await f.ctx.tools.execute({ name: 'sts_feedback', arguments: { message: 'Missing card-choice policy.' },
    agent: handle.agent, signal: new AbortController().signal, callId: 'card-policy-gap' });
  assert.ok(!feedback.isError); assert.equal(feedback.concludesTurn, true);
  assert.ok(!(await f.calls()).includes('take:BurningPact#0'));
  await assert.rejects(f.adoptions(), /ENOENT/);
});

test('preparation flow adopts one compatible default before model setup and preserves the full option evidence', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { 'take:BurningPact#0': scopedPosition('map', 2) } });
  await f.setPosition(checkedCardPosition());
  execFileSync('git', ['init', '-q'], { cwd: f.repository });
  execFileSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test',
    'commit', '--allow-empty', '-qm', 'fixture'], { cwd: f.repository });
  await writeFile(join(f.repository, 'binary-fixture'), '');
  await writeFile(join(f.repository, 'tools/run_play/client.mjs'), 'export { openWorkbench as openRunPlay } from "./workbench.mjs";');
  const result = await runPreparationFlow({ repository: f.repository, session: f.session,
    binary: join(f.repository, 'binary-fixture'), home: join(f.repository, 'flow'), flowMaxSteps: 1 }, () => {});
  assert.equal(result.outcome, 'step-limit', result.error);
  assert.equal(result.modelTasks, 0);
  assert.equal(result.cardPolicyActions, 1);
  const frame = JSON.parse(await readFile(join(result.output, 'step-000/card-choice.json'), 'utf8'));
  assert.deepEqual(frame.candidates.map(row => row.command), ['take:BurningPact#0', 'skip-card#0']);
  const adoptions = (await readFile(join(result.output, 'step-000/adoptions.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.deepEqual(adoptions.map(row => row.phase), ['intent', 'applied']);
  assert.equal(adoptions[0].owner, 'host-card-policy');
  const journey = JSON.parse((await readFile(join(result.output, 'journey.jsonl'), 'utf8')).trim());
  assert.equal(journey.before.state_revision, 1);
  assert.equal(journey.after.state_revision, 2);
});

test('preparation flow records policy and native fact gaps before model setup or mutation', async t => {
  const f = await fixture(t, 'Current rules');
  execFileSync('git', ['init', '-q'], { cwd: f.repository });
  execFileSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test',
    'commit', '--allow-empty', '-qm', 'fixture'], { cwd: f.repository });
  await writeFile(join(f.repository, 'binary-fixture'), '');
  await writeFile(join(f.repository, 'tools/run_play/client.mjs'), 'export { openWorkbench as openRunPlay } from "./workbench.mjs";');
  for (const kind of ['strategy', 'facts']) {
    const current = kind === 'strategy' ? uncoveredCardEventPosition() : checkedCardPosition();
    if (kind === 'facts') current.decisionEvidence.candidates = [];
    await f.setPosition(current);
    const result = await runPreparationFlow({ repository: f.repository, session: f.session,
      binary: join(f.repository, 'binary-fixture'), home: join(f.repository, 'flow'), flowMaxSteps: 1 }, () => {});
    assert.equal(result.outcome, 'preparation-needs-review', result.error);
    assert.equal(result.preparationGap.kind, kind);
    assert.ok(result.preparationGap.message);
    assert.equal(result.modelTasks, 0);
    assert.equal(result.cardPolicyActions, 0);
    assert.equal(result.steps, 0);
    const after = JSON.parse(await readFile(join(f.repository, 'position.json'), 'utf8'));
    assert.deepEqual(after, current);
    await assert.rejects(readFile(join(result.output, 'step-000/adoptions.jsonl')), /ENOENT/);
  }
});

test('preparation flow adopts the checked Power variant before textual comparison or model setup', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { 'take:DarkEmbrace#1': scopedPosition('map', 2) } });
  execFileSync('git', ['init', '-q'], { cwd: f.repository });
  execFileSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test',
    'commit', '--allow-empty', '-qm', 'fixture'], { cwd: f.repository });
  await writeFile(join(f.repository, 'binary-fixture'), '');
  await writeFile(join(f.repository, 'tools/run_play/client.mjs'), 'export { openWorkbench as openRunPlay } from "./workbench.mjs";');
  const current = checkedCardPosition();
  current.observation.observation.cards[0].card = 'SecondWind';
  current.deckEvidence.cards[0].mechanics = { card: 'SecondWind', upgrades: 0, card_type: 'Skill',
    cost: 1, exhausts: false, exhausts_other_cards: true, draw_cards: 0 };
  current.deckEvidence.cards[0].persistent_cost = 1;
  current.actions = []; current.decisionEvidence.candidates = []; current.reading = '';
  for (const upgrades of [0, 1]) {
    const command = `take:DarkEmbrace#${upgrades}`, cost = upgrades ? 1 : 2;
    current.actions.push({ semantic_action: command, candidate: { action: {
      kind: 'take_card', card: 'DarkEmbrace', upgrades } } });
    current.decisionEvidence.candidates.push({ semantic_action: command, cards: [{ current: {
      card: 'DarkEmbrace', upgrades, card_type: 'Power', cost, draw_cards: 0, exhaust_draw_power: 1,
    } }] });
    current.reading += `${command}  |  Dark Embrace\n  Card Dark Embrace | Power | ${cost}c\n    Printed rule: ` + '完整机制'.repeat(1500) + '\n';
  }
  await f.setPosition(current);
  const result = await runPreparationFlow({ repository: f.repository, session: f.session,
    binary: join(f.repository, 'binary-fixture'), home: join(f.repository, 'flow'), flowMaxSteps: 1 }, () => {});
  assert.equal(result.outcome, 'step-limit', result.error);
  assert.equal(result.modelTasks, 0);
  assert.equal(result.cardPolicyActions, 1);
  const adoptions = (await readFile(join(result.output, 'step-000/adoptions.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.deepEqual(adoptions.map(row => row.phase), ['intent', 'applied']);
  assert.equal(adoptions[0].command, 'take:DarkEmbrace#1');
  assert.equal(adoptions[0].owner, 'host-card-policy');
  const frame = JSON.parse(await readFile(join(result.output, 'step-000/card-choice.json'), 'utf8'));
  assert.equal(frame.candidates.length, 2);
  assert.equal(frame.rules.at(-1).id, 'same-card-variant');
  const calls = (await readFile(join(result.output, 'step-000/card-policy-wire/calls.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.deepEqual(calls, ['observe', 'observe', 'take:DarkEmbrace#1', 'observe', 'close']);
  await assert.rejects(readFile(join(result.output, 'step-000/card-reading-wire/calls.jsonl')), /ENOENT/);
});

test('preparation flow leaves a shop with no purchase or replacement work before model setup', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) } });
  execFileSync('git', ['init', '-q'], { cwd: f.repository });
  execFileSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test',
    'commit', '--allow-empty', '-qm', 'fixture'], { cwd: f.repository });
  await writeFile(join(f.repository, 'binary-fixture'), '');
  await writeFile(join(f.repository, 'tools/run_play/client.mjs'), 'export { openWorkbench as openRunPlay } from "./workbench.mjs";');
  const current = scopedPosition('shop', 1), state = current.observation.observation;
  state.run.gold = 40;
  state.context.inventory = { cards: [], relics: [], potions: [{ slot: 0, potion: 'StrengthPotion', price: 50,
    purchase_status: { status: 'blocked', reason: { kind: 'not_enough_gold', gold: 40, price: 50 } } }],
    purge: { price: 75, purchase_status: { status: 'blocked', reason: { kind: 'not_enough_gold', gold: 40, price: 75 } } } };
  current.actions = [{ semantic_action: 'discard-potion:BlockPotion#0', candidate: { action: { kind: 'discard_run_potion' } } },
    { semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } }];
  await f.setPosition(current);
  const result = await runPreparationFlow({ repository: f.repository, session: f.session,
    binary: join(f.repository, 'binary-fixture'), home: join(f.repository, 'flow'), flowMaxSteps: 1 }, () => {});
  assert.equal(result.outcome, 'step-limit', result.error);
  assert.equal(result.modelTasks, 0);
  assert.equal(result.forcedActions, 1);
  const calls = (await readFile(join(result.output, 'step-000/forced-wire/calls.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.deepEqual(calls, ['observe', 'leave', 'close']);
  const before = JSON.parse(await readFile(join(result.output, 'step-000/before.json'), 'utf8'));
  assert.equal(before.position.actions.length, 2, 'raw legal actions remain in host evidence');
  const after = JSON.parse(await readFile(join(f.repository, 'position.json'), 'utf8'));
  assert.equal(after.observation.observation.context.kind, 'map');
});

test('preparation flow clears a bound Sozu reward without model setup or changing held bottles', async t => {
  const current = scopedPosition('reward', 1), state = current.observation.observation;
  state.run.potion_capacity = 2;
  state.relics = [{ relic: 'Sozu' }];
  state.potions = [{ slot: 0, potion: { potion: 'FearPotion' } }, { slot: 1, potion: { potion: 'BlockPotion' } }];
  const potion = { kind: 'potion', potion: 'EnergyPotion' };
  const cards = { kind: 'card_reward', cards: [{ card: 'ShrugItOff', upgrades: 1 }] };
  state.context = { kind: 'reward', pending_rewards: [potion, cards] };
  current.actions = [
    { semantic_action: 'claim:EnergyPotion#0', candidate: { action: { kind: 'claim_reward', reward_item_index: 0, reward: potion } } },
    { semantic_action: 'open-card#0', candidate: { action: { kind: 'open_card_reward', reward_item_index: 1 } } },
    { semantic_action: 'proceed', candidate: { action: { kind: 'proceed' } } },
  ];
  const after = structuredClone(current); after.revision = 2;
  after.observation.observation.context.pending_rewards = [cards]; after.actions.shift();
  after.actions[0].candidate.action.reward_item_index = 0;
  const f = await fixture(t, 'Current rules', false, { actions: { 'claim:EnergyPotion#0': after } });
  execFileSync('git', ['init', '-q'], { cwd: f.repository });
  execFileSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test',
    'commit', '--allow-empty', '-qm', 'fixture'], { cwd: f.repository });
  await writeFile(join(f.repository, 'binary-fixture'), '');
  await writeFile(join(f.repository, 'tools/run_play/client.mjs'), 'export { openWorkbench as openRunPlay } from "./workbench.mjs";');
  await f.setPosition(current);
  const result = await runPreparationFlow({ repository: f.repository, session: f.session,
    binary: join(f.repository, 'binary-fixture'), home: join(f.repository, 'flow'), flowMaxSteps: 1 }, () => {});
  assert.equal(result.outcome, 'step-limit', result.error);
  assert.equal(result.modelTasks, 0);
  const [journey] = (await readFile(join(result.output, 'journey.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.equal(journey.execution.owner, 'reward-potions-blocked-by-sozu');
  const actual = JSON.parse(await readFile(join(f.repository, 'position.json'), 'utf8'));
  assert.deepEqual(actual.observation.observation.potions, state.potions);
  assert.deepEqual(actual.observation.observation.context.pending_rewards, [cards]);
  assert.equal(actual.actions[0].candidate.action.reward_item_index, 0);
});

test('procedural flow refuses a saved command when the game changed before its workbench opened', async t => {
  const initial = scopedPosition('shop', 1), changed = scopedPosition('shop', 2);
  initial.actions = [{ semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } }];
  changed.actions = structuredClone(initial.actions);
  const f = await fixture(t, 'Current rules', false, { forcedOpeningPosition: changed,
    actions: { leave: scopedPosition('map', 3) } });
  execFileSync('git', ['init', '-q'], { cwd: f.repository });
  execFileSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test',
    'commit', '--allow-empty', '-qm', 'fixture'], { cwd: f.repository });
  await writeFile(join(f.repository, 'binary-fixture'), '');
  await writeFile(join(f.repository, 'tools/run_play/client.mjs'), 'export { openWorkbench as openRunPlay } from "./workbench.mjs";');
  await f.setPosition(initial);
  const result = await runPreparationFlow({ repository: f.repository, session: f.session,
    binary: join(f.repository, 'binary-fixture'), home: join(f.repository, 'flow'), flowMaxSteps: 1 }, () => {});
  assert.equal(result.outcome, 'error');
  assert.match(result.error, /position changed/i);
  assert.equal(result.forcedActions, 0);
  const calls = (await readFile(join(result.output, 'step-000/forced-wire/calls.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.ok(!calls.includes('leave'));
  assert.deepEqual(JSON.parse(await readFile(join(f.repository, 'position.json'), 'utf8')), changed);
});

test('one classification submission adopts its own choice and ends the official loop in one request', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) } });
  await f.setPosition(scopedPosition('shop', 1));
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests.push(JSON.stringify(options.messages));
      assert.equal(requests.length, 1, 'the combined choice ends the turn');
      const block = { type: 'tool-call', id: 'combined-choice', name: 'sts',
        arguments: JSON.stringify({ command: 'classify', basis: fixtureBasis(), choice: 'leave' }) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete one preparation.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests.length, 1);
  assert.match(requests[0], /basis with command classify and the selected complete command as choice/);
  assert.equal((await f.calls()).filter(command => command === 'leave').length, 1);
  const [table] = await f.classifications(), [adoption] = await f.adoptions();
  assert.equal(table.rows.length, 4);
  assert.equal(adoption.selected, 'leave');
  assert.equal(adoption.classificationAt, table.at);
  assert.equal(adoption.selectedRow.category, 'medium');
  assert.equal(adoption.basisKind, 'host-comparison');
  assert.equal((await f.submissions())[0].choice, 'leave');
  assert.ok((await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis(), { choice: 'leave' })).isError);
  assert.equal((await f.calls()).filter(command => command === 'leave').length, 1);
});

test('official preparation assembly supplies one small bottle comparison and binds the actual follow-up', async t => {
  const current = scopedPosition('shop', 1), state = current.observation.observation;
  state.run.gold = 40; state.run.potion_capacity = 2;
  state.potions = [{ slot: 0, potion: { potion: 'SpeedPotion' } }, { slot: 1, potion: { potion: 'FearPotion' } }];
  state.context.inventory = { cards: [], relics: [], potions: [{ slot: 0, potion: 'BlockPotion', price: 40,
    purchase_status: { status: 'blocked', reason: { kind: 'no_empty_potion_slot' } } }], purge: null };
  current.actions = state.potions.map(slot => ({ semantic_action: `discard-potion:${slot.potion.potion}#0`,
    candidate: { action: { kind: 'discard_run_potion', slot: slot.slot, potion: slot.potion.potion } } }));
  current.actions.push({ semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } });
  current.reading = 'Act 2 F21 | HP 65/80 | Gold 40 | Shop\nPotions for sale:\n  Block Potion 40g (slots full) · Gain 12 Block.\n'
    + 'Deck (4):\n  ' + 'old full card discussion '.repeat(300)
    + '\nRelics (1):\n  Sacred Bark\n    Double potion effectiveness.\nPotions (2 slots; availability comes from current legal options):\n'
    + '  P0 Speed Potion\n    Temporary Dexterity.\n  P1 Fear Potion\n    Apply Vulnerable.\n';
  current.preparationOptions = { stockViews: { BlockPotion: 'Block Potion 40g (slots full)\nGain 12 Block.' },
    resources: 'Sacred Bark\nDouble potion effectiveness.\nSpeed Potion\nTemporary Dexterity.\nFear Potion\nApply Vulnerable.' };
  const after = structuredClone(current); after.revision = 2; after.public_snapshot_id = 'after-discard';
  after.observation.observation.potions[0].potion = null;
  after.observation.observation.context.inventory.potions[0].purchase_status = { status: 'purchasable' };
  after.actions = [{ semantic_action: 'buy:BlockPotion#0', candidate: { action: {
    kind: 'buy_potion', potion: 'BlockPotion', shop_slot: 0, price: 40 } } }, { semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } }];
  const f = await fixture(t, 'Global run rules', false, { actions: { 'discard-potion:SpeedPotion#0': after } });
  await f.setPosition(current);
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests.push(JSON.stringify({ messages: options.messages, tools: options.tools }));
      assert.equal(requests.length, 1);
      const block = { type: 'tool-call', id: 'bottle-choice', name: 'sts', arguments: JSON.stringify({ command: 'classify',
        basis: 'discard-potion:SpeedPotion#0 | high\ndiscard-potion:FearPotion#0 | medium\nleave | low', choice: 'discard-potion:SpeedPotion#0' }) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete one preparation.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests.length, 1);
  assert.match(requests[0], /Gain 12 Block[\s\S]*Double potion effectiveness[\s\S]*Temporary Dexterity/);
  assert.doesNotMatch(requests[0], /old full card discussion|Whole-deck|direct\/sequence\/future|Global run rules/);
  assert.ok(Buffer.byteLength(requests[0]) < 4096, `messages plus tools: ${Buffer.byteLength(requests[0])} bytes`);
  assert.equal((await f.calls()).filter(call => call.startsWith('info rules')).length, 0);
  const [classification] = await f.classifications();
  assert.equal(classification.ratingMeaning, 'shop-bottle-tradeoff-v1');
  assert.ok(classification.conditionBindings.every(row => row.id === 'shop-potion-tradeoff-v1'));
  assert.deepEqual(classification.rows.map(row => row.realization), ['sequence', 'sequence', 'direct']);
  const plan = JSON.parse(await readFile(join(f.session, 'dsh-shop-plan.json'), 'utf8'));
  assert.equal(plan.next, 'buy:BlockPotion#0');
  assert.equal(plan.reservedPrice, 40);
  const [budget] = (await readFile(join(f.repository, 'records/input-budgets.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.ok(budget.bytes < budget.target);
});

test('small shop assembly saves the native follow-up and measures supply above the byte target', async t => {
  const current = scopedPosition('shop', 1), state = current.observation.observation;
  current.public_snapshot_id = 'small-shop-root';
  state.run.gold = 150; state.base_energy_per_turn = 3; state.potions = [];
  const owned = { card: { card: 'Inflame', upgrades: 0, count: 1, misc_value: 0,
    cost_modifier: 0, base_damage_override: null, base_block_override: null },
    mechanics: { card: 'Inflame', name: 'Inflame', upgrades: 0, card_type: 'Power', cost: 1,
      draw_cards: 0, exhausts: false, ethereal: false, strength_gain: 2, strength_gain_delivery: 'on_play' },
    persistent_cost: 1, persistent_damage: 0, persistent_block: 0 };
  state.cards = [owned.card];
  current.deckEvidence = { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: current.public_snapshot_id, cards: [owned] };
  const card = price => ({ slot: 0, card: 'ShrugItOff', upgrades: 0, price, purchase_status: { status: 'purchasable' } });
  const membership = { slot: 0, relic: 'MembershipCard', price: 50, purchase_status: { status: 'purchasable' } };
  state.context.inventory = { cards: [card(80)], relics: [membership], potions: [], purge: null };
  const action = item => ({ semantic_action: `buy:${item.card ?? item.relic}#0`, candidate: { domain: 'strategic', action: {
    kind: item.card ? 'buy_card' : 'buy_relic', ...(item.card ? { card: item.card, upgrades: 0 } : { relic: item.relic }),
    shop_slot: 0, price: item.price } } });
  const leave = { semantic_action: 'leave', candidate: { domain: 'strategic', action: { kind: 'leave_shop' } } };
  current.actions = [action(card(80)), action(membership), leave];
  current.reading = 'Act 2 F21 | Gold 150 | Shop\nPurchasable now: Shrug It Off, Membership Card\n'
    + 'buy:ShrugItOff#0  |  buy Shrug It Off 80g\n  Card: Skill | 1c\n  Printed rule: Gain 8 Block, then draw 1 card.\n'
    + 'buy:MembershipCard#0  |  buy Membership Card 50g\n  Relic: Halve shop prices.\nleave  |  leave shop\n'
    + 'Deck (1):\n  Inflame | Power | 1c\n    Gain 2 Strength for the rest of combat.\nRelics (0):\nPotions (0 slots):\n';
  current.preparationOptions = { stockViews: { ShrugItOff: 'Shrug It Off\nGain 8 Block, then draw 1 card.',
    MembershipCard: 'Membership Card\nHalve shop prices.' },
    cardViews: { Inflame: 'Inflame\nGain 2 Strength for the rest of combat.' } };
  const after = (gold, cards, relics, snapshot) => {
    const next = structuredClone(current); next.revision = 2; next.public_snapshot_id = snapshot;
    next.observation.observation.run.gold = gold;
    next.observation.observation.context.inventory = { cards, relics, potions: [], purge: null };
    next.actions = [...cards.map(action), ...relics.map(action), leave];
    return next;
  };
  const afterMembership = after(100, [card(40)], [], 'after-membership');
  const afterShrug = after(70, [], [membership], 'after-shrug');
  const end = after(60, [], [], 'after-pair');
  const previews = { 'buy:ShrugItOff#0': afterShrug, 'buy:MembershipCard#0': afterMembership,
    'buy:ShrugItOff#0 / buy:MembershipCard#0': after(20, [], [], 'reverse-pair'),
    'buy:MembershipCard#0 / buy:ShrugItOff#0': end };
  const f = await fixture(t, 'Global rules', false, { previews, actions: { 'buy:MembershipCard#0': afterMembership } });
  await f.setPosition(current);
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      const index = requests.length;
      requests.push(JSON.stringify({ messages: options.messages, tools: options.tools }));
      const block = { type: 'tool-call', id: 'small-spend', name: 'sts', arguments: JSON.stringify({ command: 'classify',
        basis: index === 1 ? 'buy:ShrugItOff#0 | medium\nbuy:MembershipCard#0 | high'
          : index === 2 ? 'leave | low' : 'buy:ShrugItOff#0 | medium — raw explanation marker\nbuy:MembershipCard#0 | high — raw explanation marker\nleave | low — raw explanation marker', choice: 'buy:MembershipCard#0',
        comparison: 'then buy:ShrugItOff#0' }) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete one preparation.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests.length, 1);
  assert.match(requests[0], /Gain 8 Block[\s\S]*Halve shop prices/);
  assert.doesNotMatch(requests[0], /direct\/sequence\/future|Condition references|Global rules|OTHER_COMMAND/);
  const [classification] = await f.classifications(), [adoption] = await f.adoptions();
  assert.equal(classification.ratingMeaning, 'shop-spending-tradeoff-v1');
  assert.ok(classification.rows.every(row => row.realization === null));
  assert.ok(classification.rows.every(row => !row.reason.includes('raw explanation marker')));
  assert.equal(adoption.basisKind, 'shop-recommendations');
  const plan = JSON.parse(await readFile(join(f.session, 'dsh-shop-plan.json'), 'utf8'));
  assert.equal(plan.next, 'buy:ShrugItOff#0'); assert.equal(plan.reservedPrice, 40);
  assert.equal(plan.goldAfterPair, 60);
  assert.ok(!(await f.calls()).includes('buy:ShrugItOff#0'));

  await handle.dispose();
  current.preparationOptions.stockViews.MembershipCard = 'Membership Card\n' + 'Large native rule '.repeat(400);
  await f.setPosition(current);
  const tooLarge = await f.create({ taskScope: 'preparation' }); t.after(() => tooLarge.dispose());
  tooLarge.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete one preparation.' }] }));
  await tooLarge.agent.whenIdle();
  assert.equal(requests.length, 3);
  assert.ok(requests[1].includes('Large native rule '.repeat(400)));
  assert.match(requests[2], /Recommendation rows \(1 pending of 3\)/);
  assert.match(requests[2], /leave \| <recommendation> · Keep 150 Gold/);
  assert.match(requests[2], /Recorded recommendations:[\s\S]*buy:MembershipCard#0 \| high/);
  assert.doesNotMatch(requests[2], /high \(recorded\)|medium \(recorded\)/,
    'accepted grades appear in the retained rows rather than repeating in budget facts');
  assert.match(requests[2], /submit pending recommendation rows: leave/);
  const budgets = (await readFile(join(f.repository, 'records/input-budgets.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.equal(budgets.at(-1).exceeded, true);
  assert.equal((await f.adoptions()).length, 2);
});

test('a new choice after partial classification requires adoption from the complete table', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const lines = fixtureBasis().split('\n');
  const partial = await rawGameExec(f.ctx, handle.agent, 'classify', lines[0], { choice: 'buy:BottledFlame#0' });
  assert.ok(!partial.isError);
  assert.ok(!(await f.calls()).includes('buy:BottledFlame#0'));
  const final = await rawGameExec(f.ctx, handle.agent, 'classify', lines.slice(1).join('\n'), { choice: 'leave' });
  assert.ok(final.isError, resultText(final));
  assert.match(resultText(final), /Saved choice: buy:BottledFlame#0/);
  assert.ok(!(await f.calls()).includes('leave'));
  const adopted = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: 'leave' });
  assert.ok(!adopted.isError, resultText(adopted));
  assert.deepEqual((await f.adoptions()).map(row => row.selected), ['leave']);
  assert.equal((await f.submissions()).length, 2);
});

test('completing shop rows retains the original choice and explicit comparison', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  const lines = fixtureBasis().split('\n');
  const partial = await rawGameExec(f.ctx, handle.agent, 'classify', lines[0], {
    choice: 'leave', comparison: 'buy:Flex#0 | @resources',
  });
  assert.ok(!partial.isError, resultText(partial));
  const final = await rawGameExec(f.ctx, handle.agent, 'classify', lines.slice(1).join('\n'));
  assert.ok(!final.isError, resultText(final));
  const [adoption] = await f.adoptions();
  assert.equal(adoption.selected, 'leave');
  assert.equal(adoption.basisKind, 'player-comparison');
  assert.equal(adoption.alternativeRow.command, 'buy:Flex#0');
  assert.deepEqual(adoption.comparisonIds, ['@resources']);
  assert.equal(adoption.comparison, 'buy:Flex#0 | @resources');
});

test('native bottle facts above the byte target reach the model intact with a recorded measurement', async t => {
  const f = await fixture(t, 'Global rules', false, { actions: { leave: scopedPosition('map', 2) } });
  const current = scopedPosition('shop', 1), state = current.observation.observation;
  state.run.gold = 40;
  state.potions = [{ slot: 0, potion: { potion: 'SpeedPotion' } }];
  state.context.inventory = { cards: [], relics: [], potions: [{ slot: 0, potion: 'BlockPotion', price: 40,
    purchase_status: { status: 'blocked', reason: { kind: 'no_empty_potion_slot' } } }], purge: null };
  current.actions = [{ semantic_action: 'discard-potion:SpeedPotion#0', candidate: { action: {
    kind: 'discard_run_potion', slot: 0, potion: 'SpeedPotion' } } }, { semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } }];
  current.reading = 'Act 2 F21 | Shop\nPotions for sale:\n  Block Potion 40g · Gain Block.\nRelics (1):\n'
    + '  Native relic\n    ' + 'full effect '.repeat(800) + '\nPotions (1 slot):\n  P0 Speed Potion\n    Full native effect.\n';
  current.preparationOptions = { resources: 'Native relic\n' + 'full effect '.repeat(800) + '\nSpeed Potion\nFull native effect.' };
  await f.setPosition(current);
  let requests = 0;
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests++;
      assert.equal(requests, 1);
      assert.ok(JSON.stringify(options.messages).includes('full effect '.repeat(800)));
      const block = { type: 'tool-call', id: 'keep-bottles', name: 'sts', arguments: JSON.stringify({ command: 'classify',
        basis: 'discard-potion:SpeedPotion#0 | medium\nleave | high', choice: 'leave' }) };
      yield { type: 'block-start', index: 0, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 0, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' }); t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete one preparation.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests, 1);
  const [budget] = (await readFile(join(f.repository, 'records/input-budgets.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.equal(budget.exceeded, true);
  assert.ok(budget.bytes > budget.target);
  assert.equal((await f.adoptions())[0].selected, 'leave');
  assert.ok(!(await f.calls()).includes('discard-potion:SpeedPotion#0'));
  assert.equal(JSON.parse(await readFile(join(f.repository, 'position.json'), 'utf8')).revision, 2);
});

test('combined parent choice opens a fresh target classification and adopts its target once', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: {
    'buy:BottledFlame#0': scopedPosition('run_choice', 2), 'act select:Bash#0': scopedPosition('map', 3),
  } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const parent = await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis(), { choice: 'buy:BottledFlame#0' });
  assert.ok(!parent.isError, resultText(parent));
  const assembly = await f.ctx.systemPrompt.assemble({ agent: handle.agent, signal: new AbortController().signal, scope: scopeOf(f.scope()) });
  assert.match(assembly.sections.find(section => section.name === 'sts:classification').text, /act select:Bash#0/);
  const target = await rawGameExec(f.ctx, handle.agent, 'classify',
    'act select:Bash#0 | medium | direct | @effect,@resource-tradeoff', { choice: 'act select:Bash#0' });
  assert.ok(!target.isError, resultText(target));
  assert.deepEqual((await f.classifications()).map(row => row.stage), ['pending', 'selection']);
  assert.deepEqual((await f.adoptions()).map(row => row.selected), ['buy:BottledFlame#0', 'act select:Bash#0']);
  assert.equal((await f.calls()).filter(command => command === 'act select:Bash#0').length, 1);
});

test('invalid combined choice preserves the table for recovery and rejects batched or reviewed choices', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const invalid = await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis(), { choice: 'leave, go 2,4' });
  assert.ok(invalid.isError);
  assert.ok(!(await f.calls()).includes('leave'));
  const displayLabel = await rawGameExec(f.ctx, handle.agent, 'choose', undefined, { choice: 'buy Flex 25g' });
  assert.ok(displayLabel.isError);
  assert.match(resultText(displayLabel), /command ID from the first column/);
  assert.ok(!(await f.calls()).includes('buy Flex 25g'));
  const valid = await rawGameExec(f.ctx, handle.agent, 'classify', undefined, { choice: 'leave' });
  assert.ok(!valid.isError, resultText(valid));
  assert.equal((await f.classifications()).length, 1);
  assert.equal((await f.calls()).filter(command => command === 'leave').length, 1);
  const review = await fixture(t, 'Current rules');
  await review.setPosition(scopedPosition('shop', 1));
  const reviewer = await review.create({ taskScope: 'preparation' });
  t.after(() => reviewer.dispose());
  const rejected = await rawGameExec(review.ctx, reviewer.agent, 'classify', fixtureBasis().replace(
    'leave | medium | direct | @effect,@resource-tradeoff', 'leave | review | review | @review-access'), { choice: 'leave' });
  assert.ok(rejected.isError);
  assert.match(resultText(rejected), /missing fact/);
  assert.ok(!(await review.calls()).includes('leave'));
});

test('classification refreshes the official comparison context and adoption ends the turn', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) } });
  const current = scopedPosition('shop', 1);
  current.public_snapshot_id = 'structural-shop';
  current.observation.observation.base_energy_per_turn = 3;
  current.observation.observation.cards = current.observation.observation.cards.map(card => ({
    ...card, misc_value: 0, base_damage_override: null, base_block_override: null, cost_modifier: 0 }));
  current.deckEvidence = { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: current.public_snapshot_id,
    cards: current.observation.observation.cards.map(card => ({ card, persistent_cost: card.card === 'Bash' ? 2 : 1,
      persistent_damage: 0, persistent_block: 0, mechanics: { card: card.card, upgrades: card.upgrades,
        card_type: card.card === 'Bash' ? 'Attack' : 'Skill', draw_cards: 0 } })) };
  await f.setPosition(current);
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      const index = requests.length;
      requests.push(JSON.stringify(options.messages));
      assert.ok(index < 2, 'the completed choice must not trigger a report request');
      const text = index === 0 ? 'Discard this classification-only reasoning marker.' : 'Adopt the recorded comparison.';
      yield { type: 'block-start', index: 0, blockType: 'text' };
      yield { type: 'text-delta', index: 0, text };
      yield { type: 'block-end', index: 0, block: { type: 'text', text } };
      const block = { type: 'tool-call', id: `scoped-${index}`, name: 'sts',
        arguments: JSON.stringify(index === 0
          ? { command: 'classify', basis: fixtureBasis() }
          : { command: 'leave' }) };
      yield { type: 'block-start', index: 1, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 1, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 1, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete one preparation.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests.length, 2);
  assert.match(requests[0], /current inventory, legal choices, public routes, exact mechanics and applicable strategy checks/);
  assert.match(requests[0], /Act 2 F21 \| shop/);
  assert.doesNotMatch(requests[0], /Combat investigation|SL return points|Fixed diagnostic cases/);
  assert.doesNotMatch(requests[1], /Discard this classification-only reasoning marker/);
  assert.match(requests[1], /Recorded classifications: benefit \| realization \| basis/);
  assert.match(requests[1], /This table records your classify submission for the current decision/);
  assert.doesNotMatch(requests[1], /Submitted command: classify|Returned execution receipts:\nRecorded classifications:/);
  assert.match(requests[1], /Act 2 F21 \| shop/);
  assert.match(requests[1], /Preparation stage: adoption/);
  for (const request of requests) {
    assert.match(request, /Whole-deck comparison baseline/);
    assert.match(request, /4 cards; 1 Attack, 3 Skill/);
  }
  const recorded = (await f.classifications())[0];
  assert.equal(recorded.deckComparison.size, 4);
  assert.equal(recorded.deckComparison.counts.Attack, 1);
  const adoption = (await f.adoptions())[0];
  assert.equal(adoption.basisKind, 'host-comparison');
  assert.equal(adoption.selected, 'leave');
  assert.ok(adoption.alternativeRow.command);
  assert.deepEqual(adoption.comparisonIds, ['@function', '@access', '@resources']);
  assert.equal(recorded.observation.deck_evidence.public_snapshot_id, 'structural-shop');
  const comparisonGuide = (await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) }))
    .sections.find(s => s.name === 'sts:player-guide').text;
  assert.match(comparisonGuide, /Preparation complete/);
  assert.equal(handle.agent.session.snapshotEvents().filter(e => e.type === 'user/message' && e.surfaceOp?.op === 'replace').length, 1);
  assert.equal(handle.agent.session.snapshotEvents().filter(e => e.type === 'turn/end').at(-1).data.reason.kind, 'completed');
  assert.equal((await f.calls()).filter(x => x === 'leave').length, 1);
  assert.ok(!(await f.calls()).some(x => x === 'go 4,4' || x === 'mark Done'));
  assert.deepEqual(restoredPreparationDecision(handle.agent.session), { command: 'leave',
    basis: (await f.adoptions())[0].renderedBasis });
});

test('official context replacement repairs one row and preserves accepted grades without replaying false prose', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: { leave: scopedPosition('map', 2) } });
  await f.setPosition(scopedPosition('shop', 1));
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      const index = requests.length;
      requests.push(JSON.stringify(options.messages));
      assert.ok(index < 3, 'repair plus comparison should finish this decision');
      const text = index === 0 ? 'Bad unverified reasoning marker.' : 'Use the supplied stage.';
      yield { type: 'block-start', index: 0, blockType: 'text' };
      yield { type: 'text-delta', index: 0, text };
      yield { type: 'block-end', index: 0, block: { type: 'text', text } };
      const args = index === 0 ? { command: 'classify', basis: fixtureBasis()
        .replace('buy:Flex#0 | medium | direct | @effect,@resource-tradeoff', 'buy:Flex#0 | low | direct | No payoff attack is owned yet.') }
        : index === 1 ? { command: 'classify', basis: 'buy:Flex#0 | low | sequence | @owned-support' }
          : { command: 'leave', basis: 'buy:Flex#0 | @resources' };
      const block = { type: 'tool-call', id: `repair-${index}`, name: 'sts', arguments: JSON.stringify(args) };
      yield { type: 'block-start', index: 1, blockType: 'tool-call' };
      yield { type: 'tool-call-delta', index: 1, id: block.id, name: block.name, argumentsDelta: block.arguments };
      yield { type: 'block-end', index: 1, block };
      yield { type: 'finish', reason: { kind: 'tool-calls' } };
    }
  }
  f.ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete one preparation.' }] }));
  await handle.agent.whenIdle();
  assert.equal(requests.length, 3);
  assert.match(requests[1], /1 pending output rows of 4/);
  assert.match(requests[1], /Accepted classification rows.*leave \| medium \| direct/s);
  for (const request of requests.slice(1)) assert.doesNotMatch(request, /No payoff attack|Bad unverified reasoning marker/);
  const submissions = await f.submissions();
  assert.deepEqual(submissions.map(row => row.acceptedCommands.length), [3, 1]);
  assert.match(submissions[0].basis, /No payoff attack/);
  assert.equal(submissions[0].issues.length, 1);
  const [record] = await f.classifications();
  assert.deepEqual(record.rows.find(row => row.command === 'leave'), submissions[0].rows.find(row => row.command === 'leave'));
  assert.equal(record.rows.find(row => row.command === 'buy:Flex#0').category, 'low');
  assert.equal(handle.agent.session.snapshotEvents().filter(e => e.type === 'user/message' && e.surfaceOp?.op === 'replace').length, 2);
  assert.equal((await f.calls()).filter(call => call === 'leave').length, 1);
});

test('partial classification blocks adoption, refreshes only at its source and stops bounded failed repairs', async t => {
  const f = await fixture(t, 'Current rules');
  await f.setPosition(scopedPosition('shop', 1));
  let handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  await rawGameExec(f.ctx, handle.agent, 'classify', 'leave | high | direct | @resource-tradeoff');
  const denied = await rawGameExec(f.ctx, handle.agent, 'leave', 'buy:Flex#0 | @resources');
  assert.ok(denied.isError);
  assert.ok(!(await f.calls()).includes('leave'));
  await f.setPosition(scopedPosition('shop', 2));
  const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  assert.doesNotMatch(assembly.sections.find(s => s.name === 'sts:classification').text, /Accepted classification rows/);
  await handle.dispose();
  handle = await f.create({ taskScope: 'preparation' });
  for (let i = 0; i < 3; i++) {
    const result = await rawGameExec(f.ctx, handle.agent, 'classify', 'buy:Flex#0 | low | direct | invented fact');
    assert.equal(Boolean(result.concludesTurn), i === 2);
    if (i === 2) assert.match(resultText(result), /three submissions without an accepted row/);
  }
  assert.equal((await f.submissions()).length, 4);
  assert.ok(!(await f.calls()).includes('leave'));
});

test('a selected review candidate ends at feedback with its original resources', async t => {
  const f = await fixture(t, 'Current rules');
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const basis = fixtureBasis().replace('medium | direct | @effect,@resource-tradeoff', 'review | direct | @review-access');
  assert.ok(!(await rawGameExec(f.ctx, handle.agent, 'classify', basis)).isError);
  const attempted = await rawGameExec(f.ctx, handle.agent, 'buy:BottledFlame#0', 'leave | @function');
  assert.ok(attempted.isError);
  assert.match(resultText(attempted), /missing fact/);
  const alternative = await rawGameExec(f.ctx, handle.agent, 'leave', 'buy:BottledFlame#0 | @resources');
  assert.ok(alternative.isError);
  const feedback = await f.ctx.tools.execute({ name: 'sts_feedback', arguments: { message: 'Need the triggering condition.' },
    agent: handle.agent, signal: new AbortController().signal, callId: 'review-feedback' });
  assert.equal(feedback.concludesTurn, true);
  assert.match(resultText(feedback), /Preparation paused/);
  assert.ok(!(await f.calls()).some(command => command === 'leave' || command === 'buy:BottledFlame#0'));
});

test('preparation decision-start budget survives purchases, selection and plugin restoration', async t => {
  const initial = scopedPosition('shop', 1);
  initial.observation.observation.run.gold = 130;
  const selected = scopedPosition('run_choice', 2);
  selected.observation.observation.run.gold = 76;
  const complete = scopedPosition('shop', 3);
  complete.observation.observation.run.gold = 76;
  const f = await fixture(t, 'Current rules', false, { actions: {
    'buy:BottledFlame#0': selected, 'act select:Bash#0': complete,
  } });
  await f.setPosition(initial);
  const root = preparationTaskRoot({ schema: 'sts-run-play-agent-observation-v3', session_id: 'fixture',
    head_revision: 1, state_revision: 1, position: initial });
  assert.equal(root.startingGold, 130);
  const assemble = async () => (await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) }))
    .sections.find(x => x.name === 'sts:task-progress').text;
  let handle = await f.create({ taskScope: 'preparation', preparationRoot: root });
  assert.match(await assemble(), /Decision-start budget: Gold 130/);
  const purchase = await gameExec(f.ctx, handle.agent, 'buy:BottledFlame#0');
  assert.match(resultText(purchase), /Decision-start budget: Gold 130/);
  assert.match(resultText(purchase), /Preparation comparison before action \(recorded rows\): buy:BottledFlame#0 \| medium \| direct \| Assess the useful increment/);
  await handle.dispose();
  handle = await f.create({ taskScope: 'preparation', preparationRoot: JSON.parse(JSON.stringify(root)) });
  t.after(() => handle.dispose());
  assert.match(await assemble(), /Decision-start budget: Gold 130/);
  const selection = await gameExec(f.ctx, handle.agent, 'act select:Bash#0');
  assert.match(resultText(selection), /Decision-start budget: Gold 130/);
  assert.match(resultText(selection), /Preparation task complete/);
  assert.match(await assemble(), /Decision-start budget: Gold 130/);
  assert.equal(complete.observation.observation.run.gold, 76);
});

test('preparation adoption binds its comparison and target selection records a separate assessment', async t => {
  const f = await fixture(t, 'Current rules', false, { actions: {
    'buy:BottledFlame#0': scopedPosition('run_choice', 2), 'act select:Bash#0': scopedPosition('shop', 3),
  } });
  await f.setPosition(scopedPosition('shop', 1));
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  assert.ok((await gameExec(f.ctx, handle.agent, 'read', '')).content.length);
  const rejected = await gameExec(f.ctx, handle.agent, 'buy:BottledFlame#0', ' ');
  assert.ok(rejected.isError);
  assert.match(resultText(rejected), /Call classify/);
  assert.ok(!(await f.calls()).includes('buy:BottledFlame#0'));
  await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis());
  const narrative = await rawGameExec(f.ctx, handle.agent, 'buy:BottledFlame#0', 'leave | Invent a different mechanic.');
  assert.ok(narrative.isError);
  assert.match(resultText(narrative), /supplied comparison references/);
  assert.ok(!(await f.calls()).includes('buy:BottledFlame#0'));
  const accepted = await rawGameExec(f.ctx, handle.agent, 'buy:BottledFlame#0', 'leave | @access,@resources');
  assert.match(resultText(accepted), /before action \(recorded rows\):.*Assess the useful increment.*Emphasis: @access,@resources/);
  const selected = await gameExec(f.ctx, handle.agent, 'act select:Bash#0', '');
  assert.ok(!selected.isError);
  assert.match(resultText(selected), /Assess the useful increment.*Emphasis: @access,@resources/);
  assert.match(resultText(selected), /Preparation task complete/);
});

test('preparation records raw grouped assessment and full coverage before executing a member', async t => {
  const initial = preparationPosition(1, 15, 'campfire');
  initial.observation.observation.cards = [{ card: 'Disarm', upgrades: 0, count: 2,
    misc_value: 0, base_damage_override: null, base_block_override: null, cost_modifier: 0 }];
  initial.actions = [0, 1].map(n => ({ semantic_action: `smith:Disarm+0#${n}`,
    candidate: { domain: 'strategic', action: { kind: 'smith', card: 'Disarm', upgrades: 0, card_ordinal: n } } }));
  initial.actions.push({ semantic_action: 'rest', candidate: { domain: 'strategic', action: { kind: 'rest' } } });
  const f = await fixture(t, 'Current rules', false, { actions: { 'smith:Disarm#1': scopedPosition('map', 2) } });
  await f.setPosition(initial);
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const basis = 'smith:Disarm#0 | high | direct | @effect,@resource-tradeoff\nrest | low | direct | @effect,@resource-tradeoff';
  const classified = await rawGameExec(f.ctx, handle.agent, 'classify', basis);
  assert.ok(!classified.isError, resultText(classified));
  assert.ok(!(await f.calls()).includes('smith:Disarm#1'));
  const result = await rawGameExec(f.ctx, handle.agent, 'smith:Disarm#1', 'rest | @protection');
  assert.ok(!result.isError, resultText(result));
  assert.equal(result.concludesTurn, true);
  const [record] = await f.classifications();
  assert.equal(record.schema, 'sts-preparation-classification-v4');
  assert.equal(record.ratingMeaning, 'current-window-recommendation-v1');
  assert.equal(record.expertGuidance.policies[0].id, 'campfire');
  assert.ok(record.expertGuidance.sources.panacea);
  assert.match(record.classificationContext, /Strategy guidance for the current preparation/);
  assert.equal(record.conditionBindings.length, 2);
  assert.ok(record.conditionBindings.every(row => row.id === 'preparation-effect-tradeoff-v1'));
  assert.deepEqual(record.observation.position, initial);
  assert.equal(record.observation.state_revision, 1);
  assert.equal(record.runRules, 'Current rules');
  assert.match(record.classificationContext, /Shared one-copy evaluation: 2 copies/);
  assert.match(record.criteria, /evaluation pool is exactly the current owned inventory plus that row's offered change/);
  assert.match(record.criteria, /Close tradeoffs may share a rating/);
  assert.equal(record.basis, basis);
  assert.equal(record.selected, undefined);
  const [adoption] = await f.adoptions();
  assert.equal(adoption.schema, 'sts-preparation-adoption-v2');
  assert.equal(adoption.selected, 'smith:Disarm#1');
  assert.equal(adoption.classificationAt, record.at);
  assert.deepEqual(adoption.selectedRow, record.rows[1]);
  assert.deepEqual(adoption.alternativeRow, record.rows[2]);
  assert.deepEqual(adoption.comparisonIds, ['@protection']);
  assert.deepEqual(adoption.source, record.source);
  assert.deepEqual(record.groups[0].members, ['smith:Disarm#0', 'smith:Disarm#1']);
  assert.equal(record.rows.length, 3);
  assert.deepEqual(record.rows[1], { ...record.rows[0], command: 'smith:Disarm#1' });
  assert.equal((await f.calls()).filter(x => x === 'smith:Disarm#1').length, 1);
});

test('bound preparation rejects narrative mechanics and records player references separately from supplied facts', async t => {
  const initial = preparationPosition(3, 40, 'shop');
  initial.observation.observation.cards = ['LimitBreak', 'Corruption', 'Exhume'].map(card => ({ card, count: 1, upgrades: 0 }));
  initial.actions = [{ semantic_action: 'buy:Armaments#0',
    candidate: { domain: 'strategic', action: { kind: 'buy_card', card: 'Armaments', upgrades: 0 } } },
  { semantic_action: 'leave', candidate: { domain: 'strategic', action: { kind: 'leave_shop' } } }];
  initial.decisionEvidence = { candidates: [{ semantic_action: 'buy:Armaments#0', cards: [{ current: {
    card: 'Armaments', upgrades: 0, name: 'Armaments', cost: 1, block: 5, combat_upgrade_scope: 'selected_card_in_hand',
  } }] }] };
  const f = await fixture(t, 'Current rules', false, { actions: { 'buy:Armaments#0': scopedPosition('shop', 2) } });
  await f.setPosition(initial);
  const handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const execute = basis => rawGameExec(f.ctx, handle.agent, 'classify', basis);
  const suffix = '\nleave | low | direct | @effect,@resource-tradeoff';
  const rejected = await execute('buy:Armaments#0 | high | sequence | @effect,@resource-tradeoff' + suffix);
  assert.ok(!rejected.isError);
  assert.match(resultText(rejected), /Correction: Use supplied condition references for buy:Armaments#0/);
  assert.ok(!(await f.calls()).includes('buy:Armaments#0'));
  const basis = 'buy:Armaments#0 | high | sequence | @limit-redraw';
  const accepted = await execute(basis);
  assert.ok(!accepted.isError, resultText(accepted));
  assert.ok(!(await f.calls()).includes('buy:Armaments#0'));
  const adopted = await rawGameExec(f.ctx, handle.agent, 'buy:Armaments#0', 'leave | @function,@access');
  assert.ok(!adopted.isError, resultText(adopted));
  const [record] = await f.classifications();
  assert.equal(record.basis, basis);
  assert.equal(record.conditionBindings[0].command, 'buy:Armaments#0');
  assert.deepEqual(record.rows[0].conditionIds, ['@limit-redraw']);
  assert.match(record.rows[0].reason, /Keep Corruption awaiting deployment/);
  assert.match(record.rows[0].reason, /Returning base LimitBreak already supplies another use/);
  assert.equal((await f.calls()).filter(x => x === 'buy:Armaments#0').length, 1);
});

test('classification is read-only, single-call, immutable for its source and transient on remount', async t => {
  const f = await fixture(t, 'Current rules');
  await f.setPosition(scopedPosition('shop', 1));
  let handle = await f.create({ taskScope: 'preparation' });
  const mixed = await rawGameExec(f.ctx, handle.agent, 'classify, leave', fixtureBasis());
  assert.ok(mixed.isError);
  assert.match(resultText(mixed), /basis and choice in one call/);
  const result = await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis());
  assert.ok(!result.isError, resultText(result));
  const assembly = await f.ctx.systemPrompt.assemble({ scope: scopeOf(f.scope()) });
  const guide = assembly.sections.find(s => s.name === 'sts:player-guide').text;
  assert.match(guide, /Classification complete\. Current stage: adoption/);
  assert.doesNotMatch(guide, /Call classify|One line per supplied classification/);
  const repeated = await rawGameExec(f.ctx, handle.agent, 'classify', fixtureBasis('@owned-support'));
  assert.ok(!repeated.isError);
  assert.deepEqual((await f.classifications())[0].rows[0].conditionIds, ['@effect', '@resource-tradeoff']);
  assert.equal((await f.classifications()).length, 1);
  assert.ok(!(await f.calls()).includes('classify'));
  assert.ok(!(await f.calls()).includes('leave'));
  await handle.dispose();
  handle = await f.create({ taskScope: 'preparation' });
  t.after(() => handle.dispose());
  const stale = await rawGameExec(f.ctx, handle.agent, 'leave', 'buy:Flex#0 | Retain Gold.');
  assert.ok(stale.isError);
  assert.match(resultText(stale), /Call classify/);
  assert.ok(!(await f.calls()).includes('leave'));
});


test('repeated command errors end the official preparation task before further guessing', async t => {
  for (const agentLocale of ['en', 'zh-CN']) {
    const f = await fixture(t, 'Current rules');
    await f.setPosition(scopedPosition('shop', 1));
    const handle = await f.create({ taskScope: 'preparation', agentLocale });
    t.after(() => handle.dispose());
    for (let n = 0; n < 3; n++) {
      const result = await rawGameExec(f.ctx, handle.agent, 'invented-command', '');
      assert.equal(Boolean(result.isError), n < 2);
      assert.equal(result.concludesTurn, n === 2 ? true : undefined);
      if (n === 2) {
        assert.match(resultText(result), /^Error: /);
        assert.match(resultText(result), agentLocale === 'en' ? /Preparation stopped after three command errors/ : /连续三次命令错误后准备已停止/);
      }
    }
    assert.ok(!(await f.calls()).includes('invented-command'));
  }
});

test('both languages stop after three classification submissions without an accepted row', async t => {
  for (const agentLocale of ['en', 'zh-CN']) {
    const f = await fixture(t, 'Current rules');
    await f.setPosition(scopedPosition('shop', 1));
    const handle = await f.create({ taskScope: 'preparation', agentLocale });
    t.after(() => handle.dispose());
    for (let n = 0; n < 3; n++) {
      const result = await rawGameExec(f.ctx, handle.agent, 'classify', 'invented | high | direct | @effect');
      assert.equal(Boolean(result.isError), false, resultText(result));
      assert.equal(result.concludesTurn, n === 2 ? true : undefined);
      if (n === 2) assert.match(resultText(result), agentLocale === 'en' ? /three submissions without an accepted row/ : /连续三次提交没有接受任何新行/);
    }
    assert.ok(!(await f.calls()).some(call => /^(buy:|leave$)/.test(typeof call === 'string' ? call : '')));
  }
});

test('scripted configuration errors use the scoped language before opening a game', async t => {
  const f = await fixture(t, 'Current rules');
  await assert.rejects(f.create({ taskScope: 'invented', agentLocale: 'zh-CN' }), /taskScope 必须是 run 或 preparation/);
});
