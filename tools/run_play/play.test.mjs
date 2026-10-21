import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { actTrimmingEndedCombat, applyPotionSkin, decisionDocumentFromObservation, expandRepeatSugar, freshRecordDirectory, mapGraphFromObservation, normalizeActionTokens, parseChoiceToken, parseDecisionPotions, renderMapGrid, resolvePotionToken, resolveSession, summarizeDecisionJson } from './play.mjs';

test('CLI help prints usage without opening a game connection', () => {
  const entry = fileURLToPath(new URL('../../scripts/p.mjs', import.meta.url));
  for (const flag of ['help', '--help', '-h']) {
    const result = spawnSync(process.execPath, [entry, flag],
      { encoding: 'utf8', windowsHide: true });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /usage: node scripts\/p\.mjs/);
    assert.match(result.stdout, /research \[ms\] \[work transitions\]/);
    assert.match(result.stdout, /research <effort> \[ms\]/);
    assert.match(result.stdout, /accept \[--continue\]/);
    assert.match(result.stdout, /player inspect \[STEP/);
    assert.equal(result.stderr, '');
  }
});

test('parallel invocations get distinct transcript directories within one clock tick', t => {
  t.mock.method(Date, 'now', () => 1234);
  const paths = Array.from({ length: 32 }, () => freshRecordDirectory('D:/repo', 'same-session'));
  assert.equal(new Set(paths).size, paths.length);
  assert.ok(paths.every(path => path.includes('same-session-1234-')));
});

test('resident observation preserves the typed action and snapshot contract', () => {
  const observation={position:{kind:'decision',public_snapshot_id:'snap-1',
    observation:{observation:{potions:[{potion_id:'SpeedPotion'}]}},
    actions:[{semantic_action:'use-potion:SpeedPotion#0',candidate:{action:{kind:'use_potion',potion_index:0}}}]}};
  const doc=decisionDocumentFromObservation(observation);
  assert.equal(doc.decision.information.snapshot_id,'snap-1');
  assert.equal(doc.semantic_actions[0],'use-potion:SpeedPotion#0');
  assert.deepEqual(doc.decision.candidates[0].action,{kind:'use_potion',potion_index:0});
  assert.equal(parseDecisionPotions(doc).slots[0].stableId,'SpeedPotion');
  assert.throws(() => decisionDocumentFromObservation({position:{kind:'terminal'}}), /no current typed decision/);
});

test('repeat sugar expands play tokens in place and rejects abuse', async () => {
  assert.deepEqual(expandRepeatSugar(['play:Strike+0@1*3', 'end']),
    ['play:Strike+0@1', 'play:Strike+0@1', 'play:Strike+0@1', 'end']);
  assert.deepEqual(expandRepeatSugar(['play:Defend+0', 'end']), ['play:Defend+0', 'end']);
  assert.deepEqual(expandRepeatSugar(['play:Cleave+0*1']), ['play:Cleave+0']);
  for (const bad of [['play:Strike+0*0'], ['play:Strike+0*13'], ['end*3'], ['use-potion:FirePotion#0*2']]) {
    assert.throws(() => expandRepeatSugar(bad), /\*1\.\.\*12|only meaningful on play/);
  }
});
test('space-separated selection avoids shell comma quoting and keeps typed selection', () => {
  assert.deepEqual(normalizeActionTokens(['select', 'Strike+0#0', 'Strike+0#1']),
    ['select:Strike+0#0,Strike+0#1']);
  assert.deepEqual(normalizeActionTokens(['select:Strike+0#0,Strike+0#1']),
    ['select:Strike+0#0,Strike+0#1']);
  for (const invalid of [
    ['select'],
    ['select', 'Strike+0#0', 'end'],
    ['select', 'Strike+0#0,Strike+0#1'],
  ]) {
    assert.throws(() => normalizeActionTokens(invalid), /sole action/);
  }
});
test('displayed go coordinates become the same typed route action inside a line', () => {
  assert.deepEqual(normalizeActionTokens(['go', '1,0', 'go', '2,1']), ['route:1.0', 'route:2.1']);
  assert.deepEqual(normalizeActionTokens(['play:Strike#0', 'go', '3,4']), ['play:Strike#0', 'route:3.4']);
  for (const tokens of [['go'], ['play:Strike#0', 'go'], ['go', '(1,0)'], ['go', '1.0']])
    assert.throws(() => normalizeActionTokens(tokens), /explicit x,y.*No action taken/);
});
test('short session names resolve uniquely or fail with candidates', async t => {
  const tempRoot = await realpath(tmpdir());
  const repo = await mkdtemp(join(tempRoot, 'play-sess-'));
  t.after(async () => {
    const target = await realpath(repo);
    assert.ok(target.startsWith(tempRoot + sep) && basename(target).startsWith('play-sess-'));
    await rm(target, { recursive: true });
  });
  const sessions = join(repo, '.oracle-lab', 'run-play');
  await mkdir(join(sessions, 'a20h-workbench-1'), { recursive: true });
  await mkdir(join(sessions, 'a20h-manual-2'), { recursive: true });
  await writeFile(join(repo, 'direct'), 'x');

  assert.equal(resolveSession('work', repo), join(sessions, 'a20h-workbench-1'));
  assert.equal(resolveSession('MANUAL', repo), join(sessions, 'a20h-manual-2'));
  assert.equal(resolveSession(join('relative', 'path'), repo), join(repo, 'relative', 'path'));
  assert.throws(() => resolveSession('a20h', repo), /ambiguous session "a20h": a20h-manual-2, a20h-workbench-1/);
  assert.throws(() => resolveSession('missing', repo), /no session under \.oracle-lab\/run-play matches "missing"/);
});


test('combat-end trimming drops trailing plays and end, retries the prefix once', async () => {
  const attempts = [];
  const accepted = [];
  const game = {
    async act(tokens) {
      attempts.push([...tokens]);
      if (tokens.length > 1) {
        throw new Error(`exact run-play prepared line could not resolve step 2: semantic action play:"Defend" requires a combat decision`);
      }
      return { view: 'Rewards\n' };
    },
  };
  const reading = await actTrimmingEndedCombat(game, ['play:Strike@0', 'play:Defend', 'end'],
    { onAccepted: line => accepted.push([...line]) });
  assert.equal(reading.view, 'Rewards\n');
  assert.deepEqual(attempts, [['play:Strike@0', 'play:Defend', 'end'], ['play:Strike@0']]);
  assert.deepEqual(accepted, [['play:Strike@0']]);
  // Failures that are not combat endings stay errors.
  const strict = { act: async tokens => { throw new Error('step 1: play:Bash has no legal match at Combat'); } };
  await assert.rejects(actTrimmingEndedCombat(strict, ['play:Bash@0', 'end']), /no legal match/);
});

test('choice tokens use plain row indices', () => {
  assert.deepEqual(parseChoiceToken('3'), { index: 3 });
  assert.deepEqual(parseChoiceToken('12'), { index: 12 });
  for (const bad of ['e', 'e12', 'x3', '3e', '-1']) {
    assert.throws(() => parseChoiceToken(bad), /invalid choice token/);
  }
});

test('typed public map renders from a strategic decision away from the map screen', () => {
  const entries = [
    [1,5,'elite',false,[[1,6]]], [5,5,'elite',false,[[5,6]]], [6,5,'rest',false,[[5,6]]],
    [1,6,'rest',false,[[1,7]]], [5,6,'monster',false,[[4,7],[5,7],[6,7]]],
    [1,7,'elite',false,[[2,8]]], [5,7,'rest',false,[[4,8]]], [4,7,'elite',true,[[3,8]]], [6,7,'question_mark',false,[[5,8]]],
    [2,8,'treasure',false,[[3,9]]], [3,8,'treasure',false,[[4,9]]], [4,8,'treasure',false,[[4,9]]], [5,8,'treasure',false,[[4,9]]],
    [3,9,'elite',false,[[3,10]]], [4,9,'monster',false,[[3,10],[5,10]]],
    [0,14,'rest',false,[[0,15]]], [1,14,'rest',false,[[0,15]]], [0,15,'boss',false,[]],
  ];
  const observation = {position: {kind: 'decision', site: 'campfire', observation: {
    domain: 'strategic', observation: {public_map: {current: [5,6], boss: 'Hexaghost', nodes: entries}},
  }}};
  const { nodes, current } = mapGraphFromObservation(observation);
  assert.equal(nodes.size, 18);
  assert.equal(nodes.get('1,5').type, 'Elite');
  assert.equal(nodes.get('4,7').type, 'Elite+key');
  const grid = renderMapGrid(nodes, current);
  assert.match(grid, /R\(1,6\)->\(1,7\)/);
  assert.match(grid, /\*M\(5,6\)->\(4,7\)\/\(5,7\)\/\(6,7\)/);
  assert.match(grid, /\?\(6,7\)->\(5,8\)/);
  // Flipped rendering puts small y first so forward travel reads downward,
  // and structurally fixed rows carry their tags.
  const flipped = renderMapGrid(nodes, current, { flipped: true });
  const y5Index = flipped.indexOf('y05');
  const y9Index = flipped.indexOf('y09');
  assert.ok(y5Index < y9Index, 'y05 should precede y09 when flipped');
  assert.match(flipped, /treasure row/);
  // Boss-approach rest band tags when every cell on the row below the boss rests.
  assert.match(flipped, /pre-boss rest row/);
});

const POTION_DOC = {
  decision: {
    candidates: [
      { action: { kind: 'end_turn' } },
      { action: { kind: 'discard_potion', potion_index: 0 } },
      { action: { kind: 'use_potion', potion_index: 0, target_monster_index: null } },
      { action: { kind: 'use_potion', potion_index: 1, target_monster_index: null } },
    ],
    observation: { observation: { potions: [{ potion_id: 'SwiftPotion' }, { potion_id: 'EssenceOfSteel' }] } },
  },
  semantic_actions: [
    'end',
    'discard-potion:SwiftPotion#0',
    'use-potion:SwiftPotion#0',
    'use-potion:EssenceOfSteel#0',
  ],
};

test('potion skin resolves names through exact, de-spaced and unique-substring ladder', () => {
  assert.equal(resolvePotionToken('play:Defend', POTION_DOC), null);
  assert.deepEqual(resolvePotionToken('potion:EssenceOfSteel', POTION_DOC),
    { action: 'use-potion:EssenceOfSteel#0', name: 'EssenceOfSteel' });
  assert.deepEqual(resolvePotionToken('potion:Essence', POTION_DOC),
    { action: 'use-potion:EssenceOfSteel#0', name: 'EssenceOfSteel' });
  assert.throws(() => resolvePotionToken('potion:Swift!', POTION_DOC), /no potion matching/);
  // Duplicated identical potions are not ambiguous (same effect, first slot).
  const dupes = JSON.parse(JSON.stringify(POTION_DOC));
  dupes.decision.observation.observation.potions[1] = { potion_id: 'SwiftPotion' };
  dupes.decision.candidates[3] = { action: { kind: 'use_potion', potion_index: 1, target_monster_index: null } };
  dupes.semantic_actions[3] = 'use-potion:SwiftPotion#1';
  assert.deepEqual(resolvePotionToken('potion:Swift', dupes),
    { action: 'use-potion:SwiftPotion#0', name: 'SwiftPotion' });
  assert.throws(() => resolvePotionToken('potion:t', POTION_DOC), /matches .* — spell more of the name/);
  assert.throws(() => resolvePotionToken('potion:Fairy', POTION_DOC), /no potion matching "Fairy" \(have: SwiftPotion, EssenceOfSteel\)/);
  assert.throws(() => resolvePotionToken('potion:Swift', { decision: { candidates: [], observation: { observation: { potions: [null, null] } } }, semantic_actions: [] }), /no potion to use/);
  // A listed potion without a use form (not usable / needs a target)
  // is refused with a pointer to board instead of a guessed form.
  assert.throws(() => resolvePotionToken('potion:Essence', {
    decision: { candidates: [], observation: { observation: { potions: [{ potion_id: 'EssenceOfSteel' }] } } },
    semantic_actions: [],
  }), /EssenceOfSteel has no use-potion candidate/);
});

test('potion spelling leaves hand limits and preceding plays to the engine', () => {
  assert.deepEqual(applyPotionSkin(['potion:Swift'], POTION_DOC), ['use-potion:SwiftPotion#0']);
  assert.deepEqual(applyPotionSkin(['potion:Essence', 'play:Defend', 'end'], POTION_DOC),
    ['use-potion:EssenceOfSteel#0', 'play:Defend', 'end']);
  assert.deepEqual(applyPotionSkin(['play:Defend', 'potion:Swift', 'end'], POTION_DOC),
    ['play:Defend', 'use-potion:SwiftPotion#0', 'end']);
});

test('real decision-json fixture summarizes and canaries on drift', async () => {
  const doc = JSON.parse(await readFile(new URL('./fixtures/decision-json-jawworm-t1.json', import.meta.url), 'utf8'));
  // Ground truth from the live lab session this was captured from: Jaw
  // Worm single 11 attack, hand of three Strikes and two Defends.
  const summary = summarizeDecisionJson(doc);
  assert.equal(summary.incoming, 11);
  assert.equal(summary.handBlock, 10);
  assert.equal(summary.playableLines.length, 6);
  assert.match(summary.playableLines[0], /^end$/);
  assert.match(summary.playableLines[1], /^play:Strike\+0#0@0 cost=1 dmg=6$/);
  assert.match(summary.playableLines[3], /^play:Defend\+0#0 cost=1 block=5$/);
  // Multi-hit intents count hits from the structured intent.
  const multiHit = JSON.parse(JSON.stringify(doc));
  const monster = multiHit.decision.observation.observation.monsters[0];
  monster.intent.intent = { AttackDebuff: { damage: 3, hits: 6 } };
  monster.intent.preview_damage_per_hit = 6;
  assert.equal(summarizeDecisionJson(multiHit).incoming, 36);
  // Non-attacking intents contribute nothing.
  const calm = JSON.parse(JSON.stringify(doc));
  calm.decision.observation.observation.monsters[0].intent = { evidence: 'visible_exact', hidden_reason: null, intent: { Defend: { gain: 12 } }, preview_damage_per_hit: null };
  assert.equal(summarizeDecisionJson(calm).incoming, 0);
  // Drift must be loud, never silent emptiness: a missing observation or a
  // broken candidate/semantic parallelism is a contract break.
  assert.throws(() => summarizeDecisionJson({ decision: {} }), /missing its combat observation/);
  const skewed = JSON.parse(JSON.stringify(doc));
  skewed.semantic_actions.pop();
  assert.throws(() => summarizeDecisionJson(skewed), /contract drifted: 6 candidates but 5 semantic actions/);
  // Potions: empty slots are null; duplicates map forms per occurrence.
  const { slots, useForms } = parseDecisionPotions(doc);
  assert.deepEqual(slots, []);
  assert.equal(useForms.size, 0);
});
