import test from 'node:test';
import assert from 'node:assert/strict';
import { Context } from '@deepseek-ai/cordis';
import LlmRuntime, { LlmAdapter, LlmError, createUserMessage } from '@deepseek-ai/dsh-llm';
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session';
import ProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import AgentRegistry from '@deepseek-ai/dsh-agent';
import AgentLoop from '@deepseek-ai/dsh-agent-loop';
import * as LlmRetry from '@deepseek-ai/dsh-llm-retry';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import ToolRuntime, { defineTool } from '@deepseek-ai/dsh-tools';
import * as SessionPersistence from '@deepseek-ai/dsh-session-persistence-jsonl';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import GameContextCompaction, { requestDecisionReset } from './context.mjs';
import { mkdtemp, writeFile, readFile, readdir, realpath, rm, rmdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { runPlayer, runPreparationFlow, continuationTask, watchPlayerStopRequest, preparationCombatCandidate, preparationNextCombat, preparationCombatReview, preparationCombatEscape } from './launch.mjs';

const languageRepository = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const languageBinary = join(languageRepository, 'target/dev-opt', process.platform === 'win32' ? 'run_play.exe' : 'run_play');

test('scripted dry setup supplies file tools preserves locale bindings and leaves exact state unchanged',
  { skip: !existsSync(languageBinary), timeout: 20000 }, async t => {
    const base = await realpath(tmpdir()), directory = await mkdtemp(join(base, 'sts-scripted-language-'));
    const session = join(directory, basename(directory));
    const recordPrefix = basename(session) + '-dsh-';
    t.after(async () => {
      const records = await realpath(join(languageRepository, '.oracle-lab/run-play-records'));
      for (const entry of await readdir(records, { withFileTypes: true })) {
        if (!entry.isDirectory() || !entry.name.startsWith(recordPrefix)) continue;
        const target = await realpath(join(records, entry.name));
        assert.equal(dirname(target), records);
        assert.ok(basename(target).startsWith(recordPrefix));
        await rm(target, { recursive: true, force: true });
      }
      const target = await realpath(directory);
      assert.equal(dirname(target), base);
      assert.ok(basename(target).startsWith('sts-scripted-language-'));
      await rm(target, { recursive: true, force: true });
    });
    const run = promisify(execFile);
    await run(languageBinary, ['new', '--session', session, '--seed', '202610230733', '--ascension', '20', '--goal', 'heart-victory'],
      { cwd: languageRepository, windowsHide: true });
    const fingerprint = async () => JSON.parse((await run(languageBinary, ['state-fingerprint', '--session', session],
      { cwd: languageRepository, windowsHide: true })).stdout);
    const before = await fingerprint();
    for (const locale of ['en', 'zh-CN']) for (const agentLocale of ['en', 'zh-CN']) {
      const home = join(directory, locale + '-' + agentLocale), output = [];
      const result = await runPlayer({ repository: languageRepository, session, binary: languageBinary, home, locale, agentLocale, dry: true }, text => output.push(text));
      assert.equal(result.outcome, 'dry-setup-passed', JSON.stringify(result));
      assert.equal(result.modelUsed, false);
      assert.equal(result.steps, 0);
      const manifest = JSON.parse(await readFile(join(result.output, 'manifest.json'), 'utf8'));
      assert.equal(manifest.agentLocale, agentLocale);
      assert.match(manifest.task, agentLocale === 'en' ? /^Continue the bound run/ : /^从提供的实际游戏读数继续已绑定对局/);
      for (const name of ['sts', 'sts_feedback', 'read', 'glob', 'grep', 'todo_write']) assert.ok(manifest.tools.includes(name), name);
      assert.match(output.join('\n'), agentLocale === 'en' ? /Neow/ : /涅奥/);
      assert.match(output[0], locale === 'en' ? /^Player / : /^玩家 /);
      assert.match(output.at(-1), locale === 'en' ? /^Result: dry-setup-passed/ : /^结果：dry-setup-passed/);
      const bindingPath = join(home, 'players', result.id + '.json'), binding = await readFile(bindingPath);
      await assert.rejects(runPlayer({ repository: languageRepository, session, binary: languageBinary, home, resume: result.id,
        locale, agentLocale: agentLocale === 'en' ? 'zh-CN' : 'en', dry: true }, () => {}),
      locale === 'en' ? /resume agent language differs/ : /接续语言与已保存玩家不同/);
      const resumedOutput = [];
      const resumed = await runPlayer({ repository: languageRepository, session, binary: languageBinary, home, resume: result.id,
        locale: locale === 'en' ? 'zh-CN' : 'en', agentLocale, dry: true }, text => resumedOutput.push(text));
      assert.equal(resumed.outcome, 'dry-setup-passed');
      assert.match(resumedOutput[0], locale === 'en' ? /^玩家 / : /^Player /);
      const resumedManifest = JSON.parse(await readFile(join(resumed.output, 'manifest.json'), 'utf8'));
      assert.equal(resumedManifest.agentLocale, agentLocale);
      assert.equal(resumedManifest.task, manifest.task);
      const attachment = await readFile(join(resumed.output, 'attachment-observation-wire', 'exchange-000.jsonl'), 'utf8');
      assert.equal(attachment.includes('涅奥'), agentLocale === 'zh-CN', 'the auxiliary native observation uses the saved player language');
      assert.deepEqual(await readFile(bindingPath), binding);
      assert.deepEqual(await fingerprint(), before);
    }
  });

test('launcher help and validation use operator language independently from agent language', async () => {
  const run = promisify(execFile), entry = fileURLToPath(new URL('./launch.mjs', import.meta.url));
  for (const locale of ['en', 'zh-CN']) {
    const { stdout } = await run(process.execPath, [entry, '--locale', locale, '--agent-locale', locale === 'en' ? 'zh-CN' : 'en', '--help'],
      { windowsHide: true });
    assert.match(stdout, /\[--locale en\|zh-CN\] \[--agent-locale en\|zh-CN\]/);
    assert.match(stdout, locale === 'en' ? /--locale selects operator messages/ : /--locale 选择操作提示语言/);
    await assert.rejects(run(process.execPath, [entry, '--locale', locale], { windowsHide: true }),
      error => (locale === 'en' ? /--repository and --session are required/ : /需要 --repository 和 --session/).test(error.stderr));
  }
  await assert.rejects(run(process.execPath, [entry, '--locale', 'invalid', '--help'], { windowsHide: true }),
    error => /--locale must be en or zh-CN/.test(error.stderr));
  await assert.rejects(runPlayer({ locale: 'zh-CN', agentLocale: 'en', model: 'deepseek-v4-pro' }), /此玩家不使用 deepseek-v4-pro/);
  await assert.rejects(runPreparationFlow({ locale: 'zh-CN', agentLocale: 'en', dry: true }), /从实际游戏开始/);
  assert.throws(() => preparationCombatReview({}, {}, 'zh-CN'), /实际起始生命/);
});

test('Chinese continuation translates only the host explanation and preserves the operator task', () => {
  const task = '用户任务 🐱 {{literal}}\nact play:Strike@0';
  for (const outcome of ['interrupted', 'time-limit']) {
    const text = continuationTask(task, outcome, 'zh-CN');
    assert.match(text, /宿主/);
    assert.match(text, /最新游戏读数/);
    assert.ok(text.endsWith('\n\n' + task));
  }
  assert.equal(continuationTask(task, 'completed', 'zh-CN'), task);
});

test('low-HP escape checks the legal potion and full private settlement before adoption', async () => {
  const before = { position: { observation: { domain: 'combat', observation: {
    player: { hp: 26, max_hp: 80, gold: 205 }, potions: [{ potion_id: 'SmokeBomb' }],
  } }, actions: [{ semantic_action: 'use-potion:SmokeBomb#0', candidate: { action: { kind: 'use_potion' } },
    combat_effect: { effect: { potion: { potion_id: 'SmokeBomb' } } } }] } };
  const start = { endpoint: { observation: { domain: 'combat', observation: { turn: { player_escaping: true } } },
    actions: [{ semantic_action: 'end', candidate: { action: { kind: 'end_turn' } } }] } };
  const finish = { endpoint: { kind: 'decision', observation: { domain: 'strategic', observation: {
    run: { current_hp: 37, gold: 205 },
  } } }, view: 'Actual escape settlement' };
  const calls = [];
  const game = { preview: async line => { calls.push(line); return line.length === 1 ? start : finish; } };
  const escape = await preparationCombatEscape(game, before);
  assert.deepEqual(calls, [['use-potion:SmokeBomb#0'], ['use-potion:SmokeBomb#0', 'end']]);
  assert.deepEqual(escape.actionLine, calls[1]);
  assert.equal(escape.searchResult, 'unknown');
  assert.equal(escape.settledHp, 37);
  calls.length = 0;
  before.position.observation.observation.player.hp = 40;
  assert.equal(await preparationCombatEscape(game, before), undefined);
  assert.deepEqual(calls, []);
  before.position.observation.observation.player.hp = 26;
  before.position.actions = [];
  assert.equal(await preparationCombatEscape(game, before), undefined, 'inventory alone grants no legal escape, including Boss fights');
  before.position.actions = [{ semantic_action: 'use-potion:SmokeBomb#0', candidate: { action: { kind: 'use_potion' } },
    combat_effect: { effect: { potion: { potion_id: 'SmokeBomb' } } } }];
  finish.endpoint.observation.observation.run.current_hp = 0;
  assert.equal(await preparationCombatEscape(game, before), undefined, 'unsafe settlement cannot be adopted');
  finish.endpoint.observation.observation.run.current_hp = 37;
  finish.endpoint.observation.observation.run.gold = 0;
  assert.equal(await preparationCombatEscape(game, before), undefined, 'the exit must preserve current Gold');
  finish.endpoint.observation.observation.run.gold = 205;
  start.endpoint.observation.observation.turn.player_escaping = false;
  assert.equal(await preparationCombatEscape(game, before), undefined);
});

test('material combat HP costs are reviewed before adopting a replayed win', () => {
  const observation = (hp, max_hp = 80) => ({ position: { observation: { domain: 'combat', observation: { player: { hp, max_hp } } } } });
  const candidate = final_hp => ({ ref: 'c0', meetsRequestedSatisfaction: true, outcome: { final_hp } });
  const review = preparationCombatReview(candidate(26), observation(80));
  assert.deepEqual(review, { reasons: ['loss-at-least-one-third-max-hp', 'remaining-hp-at-most-one-third-max-hp'],
    startingHp: 80, maxHp: 80, settledHp: 26, netHpLoss: 54, smallHpCostAllowance: 8, candidate: 'c0', adopted: false });
  assert.equal(preparationCombatReview(candidate(60), observation(80)), undefined);
  assert.deepEqual(preparationCombatReview(candidate(20), observation(30)).reasons,
    ['remaining-hp-at-most-one-third-max-hp']);
  assert.deepEqual(preparationCombatReview(candidate(40), observation(60, 60)).reasons,
    ['loss-at-least-one-third-max-hp']);
  assert.equal(preparationCombatReview(candidate(26), observation(26)), undefined, 'preserved low HP is not a new combat cost');
  assert.equal(preparationCombatReview(candidate(30), observation(26)), undefined);
  const heart = observation(80);
  heart.position.observation.run_context = { run_goal: 'heart_victory', encounter_id: 'TheHeart' };
  assert.equal(preparationCombatReview(candidate(1), heart), undefined, 'a verified goal victory can finish with any surviving HP');
  heart.position.observation.run_context.encounter_id = 'Collector';
  assert.ok(preparationCombatReview(candidate(1), heart), 'earlier Boss victories still leave a subsequent route');
  assert.throws(() => preparationCombatReview(candidate(26), { position: {} }), /actual starting HP/);
});

test('a costly survival fallback stops for review while HP remains above the low-HP threshold', () => {
  const root = { position: { observation: { domain: 'combat', observation: {
    player: { hp: 72, max_hp: 80 },
  } } } };
  const candidates = [[53, 2], [46, 1], [44, 1], [39, 0]].map(([final_hp, potions], index) => ({
    ref: `c${index}`, meetsRequestedSatisfaction: false,
    outcome: { final_hp, total_potions_consumed: potions },
  }));
  const selected = preparationCombatCandidate(candidates, root);
  assert.strictEqual(selected, candidates[0], 'the observed survival line remains available for investigation');
  const review = preparationCombatReview(selected, root);
  assert.deepEqual(review.reasons, ['unqualified-hp-cost-exceeds-small-allowance']);
  assert.equal(review.netHpLoss, 19);
  assert.equal(review.smallHpCostAllowance, 8);
  assert.equal(review.adopted, false);
  assert.equal(selected.outcome.total_potions_consumed, 2);
  root.position.observation.observation.player.hp = 53;
  const next = { ref: 'c0', meetsRequestedSatisfaction: false, outcome: { final_hp: 42 } };
  assert.equal(preparationCombatReview(next, root).netHpLoss, 11,
    'a second ordinary win outside the existing allowance also requires review');
});

test('combat review shares the selection allowance and preserves requested-quality and goal contracts', () => {
  const root = { position: { observation: { domain: 'combat', observation: { player: { hp: 80, max_hp: 80 } } } } };
  const candidate = (hp, quality = false) => ({ ref: 'c0', meetsRequestedSatisfaction: quality,
    outcome: { final_hp: hp } });
  assert.equal(preparationCombatReview(candidate(72), root), undefined);
  assert.equal(preparationCombatReview(candidate(71), root).smallHpCostAllowance, 8);
  assert.equal(preparationCombatReview(candidate(60, true), root), undefined);
  assert.ok(preparationCombatReview(candidate(53, true), root), 'the existing large-loss review still applies');
  root.position.observation.run_context = { run_goal: 'heart_victory', encounter_id: 'TheHeart' };
  assert.equal(preparationCombatReview(candidate(1), root), undefined);
});

test('combat adoption conserves potions within requested quality and keeps persistent rewards', () => {
  const line = (ref, hp, potions, quality = true) => ({ ref, meetsRequestedSatisfaction: quality,
    outcome: { final_hp: hp, final_gold: 20, total_potions_consumed: potions,
      consumed_entry_potion_slots: potions ? [0] : [], pending_rewards: [],
      persistent: { max_hp: 75, recoverable_gold_delta: 0, unrecovered_stolen_gold: 0,
        ritual_dagger_value: 0, genetic_algorithm_value: 0, external_burden_count: 0 },
      final_relics: [], meta_changes: [], generated_potions: [] } });
  const spend = line('c0', 58, 1), preserve = line('c1', 51, 0);
  assert.strictEqual(preparationCombatCandidate([spend, preserve]), preserve);
  preserve.meetsRequestedSatisfaction = false;
  assert.strictEqual(preparationCombatCandidate([spend, preserve]), spend);
  spend.meetsRequestedSatisfaction = false;
  assert.strictEqual(preparationCombatCandidate([spend, preserve]), spend, 'below target retains the leading survival line');
  spend.meetsRequestedSatisfaction = preserve.meetsRequestedSatisfaction = true;
  spend.outcome.persistent.max_hp++;
  assert.strictEqual(preparationCombatCandidate([spend, preserve]), spend);
  spend.outcome.persistent.max_hp--;
  spend.outcome.pending_rewards.push({ kind: 'gold', amount: 10 });
  assert.strictEqual(preparationCombatCandidate([spend, preserve]), spend);
  spend.outcome.pending_rewards = [{ kind: 'relic', relic_id: 'BlackStar' }];
  assert.strictEqual(preparationCombatCandidate([spend, preserve]), spend);
  assert.equal(preparationCombatCandidate([]), undefined);

  const root = { position: { observation: { domain: 'combat',
    observation: { player: { hp: 58, max_hp: 75 } } } } };
  const potionWin = line('c0', 65, 1), reservedWin = line('c1', 57, 0, false);
  assert.strictEqual(preparationCombatCandidate([potionWin, reservedWin], root), reservedWin,
    'a one-HP net cost preserves the Power Potion despite missing the zero-loss search target');
  reservedWin.outcome.final_hp = 50;
  assert.strictEqual(preparationCombatCandidate([potionWin, reservedWin], root), potionWin,
    'a larger cost keeps the leading quality win');
  reservedWin.outcome.final_hp = 57;
  potionWin.outcome.pending_rewards = [{ kind: 'gold', amount: 10 }];
  assert.strictEqual(preparationCombatCandidate([potionWin, reservedWin], root), potionWin,
    'the small-HP preference also retains the leading permanent resources');
  potionWin.outcome.pending_rewards = [];
  root.position.observation.observation.player.hp = 26;
  reservedWin.outcome.final_hp = 25;
  assert.strictEqual(preparationCombatCandidate([potionWin, reservedWin], root), potionWin,
    'a small new loss does not spend the remaining one-third HP reserve');
  root.position.observation.observation.player.hp = 58;
  reservedWin.outcome.final_hp = 57;
  root.position.observation.run_context = { run_goal: 'heart_victory', encounter_id: 'TheHeart' };
  assert.strictEqual(preparationCombatCandidate([potionWin, reservedWin], root), potionWin,
    'Heart victory directly adopts the leading verified win');
});

test('potion conservation accepts greater cyclic relic progress and keeps other relic state exact', () => {
  const line = (ref, hp, potions, counter) => ({ ref, meetsRequestedSatisfaction: true,
    outcome: { final_hp: hp, final_gold: 13, total_potions_consumed: potions,
      consumed_entry_potion_slots: potions === 2 ? [0, 1] : [1],
      pending_rewards: [{ kind: 'potion', potion_id: 'SpeedPotion' }],
      persistent: { max_hp: 75, recoverable_gold_delta: 0, unrecovered_stolen_gold: 0,
        ritual_dagger_value: 0, genetic_algorithm_value: 0, external_burden_count: 0 },
      final_relics: [{ id: 'PenNib', counter }, { id: 'LizardTail' }],
      meta_changes: [], generated_potions: [] } });
  const spend = line('c0', 74, 2, 2), reserve = line('c1', 71, 1, 9);
  const root = { position: { observation: { observation: { player: { hp: 69, max_hp: 75 } } } } };
  assert.strictEqual(preparationCombatCandidate([spend, reserve], root), reserve,
    'a ready Pen Nib and retained Smoke Bomb survive the three-HP preference');
  spend.outcome.final_relics[0].counter = 9;
  reserve.outcome.final_relics[0].counter = 2;
  assert.strictEqual(preparationCombatCandidate([spend, reserve], root), spend,
    'losing ready cyclic progress still requires a different resource tradeoff');
  spend.outcome.final_relics[0].counter = 2;
  reserve.outcome.final_relics[0].counter = 9;
  reserve.outcome.final_relics[1].used_up = true;
  assert.strictEqual(preparationCombatCandidate([spend, reserve], root), spend,
    'consuming a one-use relic is preserved as a material resource change');
  delete reserve.outcome.final_relics[1].used_up;
  for (const counter of [-1, 10, 1.5]) {
    reserve.outcome.final_relics[0].counter = counter;
    assert.strictEqual(preparationCombatCandidate([spend, reserve], root), spend,
      'invalid or unknown cyclic state stays outside the preference');
  }
  reserve.outcome.final_relics[0] = { id: 'PenNib', counter: 9, amount: 1 };
  assert.strictEqual(preparationCombatCandidate([spend, reserve], root), spend,
    'auxiliary state stays exact');
  for (const [id, trigger] of [['Sundial', 3], ['HappyFlower', 3], ['InkBottle', 10], ['Nunchaku', 10]]) {
    spend.outcome.final_relics[0] = { id };
    reserve.outcome.final_relics[0] = { id, counter: trigger - 1 };
    assert.strictEqual(preparationCombatCandidate([spend, reserve], root), reserve);
  }
  spend.outcome.final_relics[0] = { id: 'UnknownRelic', counter: 2 };
  reserve.outcome.final_relics[0] = { id: 'UnknownRelic', counter: 9 };
  assert.strictEqual(preparationCombatCandidate([spend, reserve], root), spend,
    'an unclassified counter keeps the exact-state contract');
});

test('Shield and Spear settlement can prefer Heart attack coverage with the same resources', () => {
  const root = { position: { observation: { domain: 'combat',
    observation: { player: { hp: 70, max_hp: 75 } },
    run_context: { status: 'available', act: 4, encounter_id: 'ShieldAndSpear', run_goal: 'heart_victory' } } } };
  const line = (ref, counter, nextIncenseTurn) => ({ ref, nextIncenseTurn, meetsRequestedSatisfaction: true,
    outcome: { final_hp: 75, final_gold: 30, total_potions_consumed: 1,
      consumed_entry_potion_slots: [0], pending_rewards: [],
      persistent: { max_hp: 75, recoverable_gold_delta: 0, unrecovered_stolen_gold: 0,
        ritual_dagger_value: 40, genetic_algorithm_value: 0, external_burden_count: 0 },
      final_relics: [{ id: 'IncenseBurner', counter }, { id: 'PenNib', counter: 9 }],
      meta_changes: [], generated_potions: [] } });
  const late = line('c0', 0, 6), second = line('c1', 4, 2), third = line('c2', 3, 3), first = line('c3', 5, 1);
  assert.deepEqual(preparationNextCombat(second, root), { encounter: 'TheHeart', incenseTriggerTurn: 2,
    earlyAttackCoverage: true,
    basis: 'Heart opens with Debilitate, then attacks on turns 2 and 3; attack order is unrevealed.' });
  const { basis, ...chinesePreparation } = preparationNextCombat(second, root, 'zh-CN');
  assert.match(basis, /攻击顺序尚未揭晓/);
  const { basis: englishBasis, ...englishPreparation } = preparationNextCombat(second, root);
  assert.deepEqual(chinesePreparation, englishPreparation);
  assert.strictEqual(preparationCombatCandidate([late, second], root), second);
  assert.strictEqual(preparationCombatCandidate([late, third], root), third);
  assert.strictEqual(preparationCombatCandidate([late, first], root), late, 'the greatest counter is not the next attack window');
  assert.strictEqual(preparationCombatCandidate([second, third], root), second);
  assert.strictEqual(preparationCombatCandidate([third, second], root), third, 'the attack pair has no fixed preference between turns 2 and 3');

  for (const change of [
    row => row.outcome.final_hp--,
    row => row.outcome.total_potions_consumed++,
    row => row.outcome.consumed_entry_potion_slots = [1],
    row => row.outcome.final_gold--,
    row => row.outcome.persistent.ritual_dagger_value--,
    row => row.outcome.final_relics[0].used_up = true,
    row => row.outcome.final_relics[0].amount = 1,
    row => row.outcome.final_relics[1].counter--,
    row => row.outcome.generated_potions.push({ slot: 0, potion_id: 'FirePotion' }),
    row => row.outcome.pending_rewards.push({ kind: 'potion', potion_id: 'FirePotion' }),
  ]) {
    const changed = structuredClone(second);
    change(changed);
    assert.strictEqual(preparationCombatCandidate([late, changed], root), late, 'timing does not spend a different resource');
  }
  for (const context of [undefined, { status: 'unavailable' },
    { ...root.position.observation.run_context, act: 3 },
    { ...root.position.observation.run_context, encounter_id: 'Collector' },
    { ...root.position.observation.run_context, run_goal: 'act3_victory' }]) {
    const otherRoot = structuredClone(root);
    otherRoot.position.observation.run_context = context;
    assert.equal(preparationNextCombat(second, otherRoot), undefined);
    assert.strictEqual(preparationCombatCandidate([late, second], otherRoot), late);
  }
  for (const turn of [undefined, 0, 7, 2.5]) {
    const unknown = { ...second, nextIncenseTurn: turn };
    assert.equal(preparationNextCombat(unknown, root), undefined);
    assert.strictEqual(preparationCombatCandidate([late, unknown], root), late);
  }
  for (const counter of [-1, 6, 1.5]) {
    const unknown = structuredClone(second);
    unknown.outcome.final_relics[0].counter = counter;
    assert.equal(preparationNextCombat(unknown, root), undefined);
    assert.strictEqual(preparationCombatCandidate([late, unknown], root), late);
  }
});

test('resuming after a host stop explains the canceled request before the game task', () => {
  const task = 'Continue the bound run.';
  assert.match(continuationTask(task, 'interrupted'), /^The host paused.*documented tool budgets\.\n\nContinue the bound run\.$/s);
  assert.match(continuationTask(task, 'time-limit'), /configured wall-time limit/);
  for (const outcome of ['interrupted', 'time-limit']) {
    assert.match(continuationTask(task, outcome), /host supplies a fresh source-checked game reading/);
    assert.doesNotMatch(continuationTask(task, outcome), /Start with sts read/);
  }
  for (const outcome of [undefined, 'completed', 'error', 'arbitrary text']) {
    assert.equal(continuationTask(task, outcome), task);
  }
});

async function fixture(t, failureCode) {
  const ctx = new Context();
  t.after(() => ctx.fiber.dispose());
  for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime]) await ctx.plugin(plugin);
  await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
  await ctx.plugin(AgentLoop, { agents: [], maxParallelToolCalls: 1 });
  await ctx.plugin(LlmRetry);
  let toolExecutions = 0, requests = 0;
  const retryInputs = [];
  ctx.tools.register(defineTool({ name: 'fixture_play', description: 'Adopt one game action', parameters: {},
    output: { schema: { type: 'string' }, render: (_args, value) => [{ type: 'text', text: value }] },
    execute() { toolExecutions++; return 'Game action completed once.'; },
  }));
  class Adapter extends LlmAdapter {
    providerRetryPolicy() {
      return { mode: 'normal', maxRetries: 2, retryableCodes: ['TRANSPORT'],
        initialDelayMs: 1, maxDelayMs: 1, jitterRatio: 0 };
    }
    async *stream(options) {
      requests++;
      if (requests === 1) {
        const block = { type: 'tool-call', id: 'fixture-action', name: 'fixture_play', arguments: '{}' };
        yield { type: 'block-start', index: 0, blockType: 'tool-call' };
        yield { type: 'tool-call-delta', index: 0, id: block.id, name: block.name, argumentsDelta: block.arguments };
        yield { type: 'block-end', index: 0, block };
        yield { type: 'finish', reason: { kind: 'tool-calls' } };
        return;
      }
      retryInputs.push(JSON.stringify(options.messages));
      if (requests === 2) {
        yield { type: 'block-start', index: 0, blockType: 'text' };
        yield { type: 'text-delta', index: 0, text: 'Uncommitted failed stream.' };
        throw new LlmError('Injected model-request failure', failureCode);
      }
      yield { type: 'block-start', index: 0, blockType: 'text' };
      yield { type: 'text-delta', index: 0, text: 'Completed.' };
      yield { type: 'block-end', index: 0, block: { type: 'text', text: 'Completed.' } };
      yield { type: 'finish', reason: { kind: 'stop' } };
    }
  }
  ctx.llm.registerAdapter(['fixture'], new Adapter());
  const handle = await ctx.agents.create({ sessionId: SessionId(`retry-fixture-${failureCode}`),
    meta: {}, agentOptions: { provider: 'fixture', model: 'fixture' } });
  t.after(() => handle.dispose());
  handle.agent.followup(createUserMessage({ content: [{ type: 'text', text: 'Play and finish.' }], source: { kind: 'user' } }));
  await handle.agent.whenIdle();
  return { events: handle.agent.session.snapshotEvents(), visible: JSON.stringify(handle.agent.session.deriveMessages()),
    requests, toolExecutions, retryInputs };
}

test('official failed-step retry preserves a completed game action and excludes failed stream text', async t => {
  const result = await fixture(t, 'TRANSPORT');
  assert.equal(result.toolExecutions, 1);
  assert.equal(result.requests, 3);
  assert.equal(result.events.filter(e => e.type === 'tool/call').length, 1);
  assert.equal(result.events.filter(e => e.type === 'tool/result').length, 1);
  assert.equal(result.events.filter(e => e.type === 'llm/retry').length, 1);
  assert.equal(result.events.at(-1).data.reason.kind, 'completed');
  assert.deepEqual(result.retryInputs[0], result.retryInputs[1]);
  assert.match(result.visible, /Game action completed once/);
  assert.doesNotMatch(result.visible, /Uncommitted failed stream|Injected model-request failure/);
});

test('a permanent model-request error ends the turn without retrying game actions', async t => {
  const result = await fixture(t, 'AUTH');
  assert.equal(result.toolExecutions, 1);
  assert.equal(result.requests, 2);
  assert.equal(result.events.filter(e => e.type === 'llm/retry').length, 0);
  assert.equal(result.events.at(-1).data.reason.error.code, 'AUTH');
});

test('host stop request delivers cancellation once and disposal removes the watcher', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'sts-player-stop-'));
  const path = join(directory, 'stop-request');
  t.after(async () => { await rm(path, { force: true }); await rmdir(directory); });
  let requests = 0;
  const close = watchPlayerStopRequest(path, () => { requests++; }, 5);
  t.after(close);
  await writeFile(path, '');
  await new Promise(resolve => setTimeout(resolve, 25));
  assert.equal(requests, 1);
  const disposed = watchPlayerStopRequest(path, () => { requests++; }, 5);
  disposed();
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(requests, 1);
});

test('a first factual checkpoint follows the protected system head and reloads through official JSONL persistence', async t => {
  const root = await mkdtemp(join(tmpdir(), 'sts-player-persistence-'));
  const contexts = [];
  t.after(async () => {
    for (const ctx of contexts) await ctx.fiber.dispose();
    assert.equal(dirname(resolve(root)), resolve(tmpdir()));
    assert.ok(root.split(/[\\/]/).at(-1).startsWith('sts-player-persistence-'));
    await rm(root, { recursive: true, force: true });
  });
  const makeContext = async () => {
    const ctx = new Context();
    contexts.push(ctx);
    for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime, TokenMeter]) await ctx.plugin(plugin);
    await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
    ctx.systemPrompt.section({ name: 'fixture', order: 300, text: 'Report the actual preparation.' });
    await ctx.plugin(SessionPersistence, { root, compression: 'none' });
    await ctx.plugin(GameContextCompaction, { auto: false });
    await ctx.plugin(AgentLoop, { agents: [], maxParallelToolCalls: 1 });
    return ctx;
  };
  const ctx = await makeContext();
  const requests = [];
  class Adapter extends LlmAdapter {
    async *stream(options) {
      requests.push(JSON.stringify(options.messages));
      yield { type: 'block-start', index: 0, blockType: 'text' };
      yield { type: 'text-delta', index: 0, text: 'Reported actual shop.' };
      yield { type: 'block-end', index: 0, block: { type: 'text', text: 'Reported actual shop.' } };
      yield { type: 'finish', reason: { kind: 'stop' } };
    }
  }
  ctx.llm.registerAdapter(['fixture'], new Adapter());
  const id = SessionId('first-factual-checkpoint');
  const handle = await ctx.agents.create({ sessionId: id, meta: {},
    agentOptions: { provider: 'fixture', model: 'fixture' },
    setup: async scoped => {
      scoped.on('system-prompt/assemble', (_assembly, context, next) => {
        if (context.agent.session.surface.nodes.length === 0) {
          requestDecisionReset(context.agent, 'Act 2 F21 | Shop\nGold 130; actual purchasable stock supplied.');
        }
        return next();
      });
    } });
  handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Complete one preparation.' }] }));
  await handle.agent.whenIdle();
  assert.equal(handle.agent.session.eventAt(handle.agent.session.surface.nodes[0]).type, 'system/message');
  assert.match(requests[0], /actual purchasable stock supplied/);
  assert.match(requests[0], /Complete one preparation/);
  const events = handle.agent.session.snapshotEvents();
  await ctx.sessions.flush(handle.agent.session);
  await handle.dispose();
  await ctx.fiber.dispose();
  const reloaded = await makeContext();
  const resumed = await reloaded.agents.resume({ resumeSessionId: id,
    agentOptions: { provider: 'fixture', model: 'fixture' } });
  const restored = resumed.agent.session.snapshotEvents();
  assert.deepEqual(restored.slice(0, events.length), events);
  assert.equal(restored.at(-1).type, 'session/end-seed');
  assert.match(JSON.stringify(resumed.agent.session.deriveMessages()), /actual purchasable stock supplied/);
  await resumed.dispose();
});
