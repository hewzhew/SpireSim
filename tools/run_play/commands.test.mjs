import test from 'node:test';
import assert from 'node:assert/strict';
import { researchOrCommitOrReview, planId } from './commands/search.mjs';
import { actOrPreview, board } from './commands/combat.mjs';
import { choose } from './commands/choose.mjs';
import { go, info, loot, mapView, cancel } from './commands/travel.mjs';
import { join } from 'node:path';
import { optionSemantics, readingOptionCommands, expandedCombatCommands, distinctCombatCommands, resolveOptionName, summarizeDecisionJson } from './contract.mjs';
import { printReading, previewInventoryChanges } from './render.mjs';

test('cancel uses the bound typed action despite translated and misleading labels', async () => {
  for (const domain of ['strategic', 'combat_atomic']) {
    let picked = false, rendered;
    const successor = { view: '实际后继\n' };
    const reading = { options: [
      { label: 'Cancel', async choose() { assert.fail('display text must not choose a decoy'); } },
      { label: '取消', async choose() { picked = true; return successor; } },
    ], raw: { reading: { options: [
      { candidate: { domain: 'strategic', action: { kind: 'rest' } } },
      { candidate: { domain, action: { kind: 'cancel' } } },
    ] } } };
    await cancel({ game: { async decision(options) { assert.deepEqual(options, { details: ['options'] }); return reading; } },
      printCurrentReading(value) { rendered = value; } });
    assert.ok(picked);
    assert.equal(rendered, successor);
  }
});

function lootFixture(rewards, { full = false, capacity = 1, sozu = false, cardOpen = false } = {}) {
  const pending = rewards.slice(), chosen = [];
  const slots = Array.from({ length: capacity }, (_, slot) => ({ slot,
    potion: full ? { potion: 'FirePotion' } : null }));
  let revision = 0, output = '';
  const reading = () => {
    const snapshot = `loot-${revision}`;
    const actions = cardOpen ? [{ kind: 'skip_card_reward', reward_item_index: 0 }]
      : pending.flatMap((reward, index) => reward.kind === 'potion' && !sozu && slots.every(slot => slot.potion) ? [] : [{
        kind: reward.kind === 'card_reward' ? 'open_card_reward' : 'claim_reward',
        reward_item_index: index, reward,
      }]).concat(cardOpen ? [] : [{ kind: 'proceed', site: 'reward' }]);
    const rawOptions = actions.map((action, index) => ({
      // Deliberately misleading text cannot change typed reward selection.
      label: `gold potion text in row ${index}`, choice: { ordinal_path: [index] },
      candidate: { domain: 'strategic', action },
    }));
    const current = { view: cardOpen ? 'Card reward' : 'Reward', control: 'strategic',
      raw: { reading: { source: { public_snapshot_id: snapshot }, options: rawOptions } },
      doc: { decision: { information: { snapshot_id: snapshot },
        observation: { domain: 'strategic', observation: { relics: sozu ? [{ relic: 'Sozu' }] : [],
          potions: slots.map(slot => ({ ...slot })), context: cardOpen
          ? { kind: 'card_reward', reward_item_index: 0 }
          : { kind: 'reward', ...(pending.length ? { pending_rewards: pending.slice() } : {}) } } } } },
    };
    current.options = rawOptions.map((option, index) => ({ label: option.label, async choose() {
      const action = actions[index];
      chosen.push(action.kind === 'claim_reward' ? action.reward.kind : action.kind);
      if (action.kind === 'claim_reward') {
        pending.splice(action.reward_item_index, 1);
        if (action.reward.kind === 'potion' && !sozu) slots.find(slot => !slot.potion).potion = { potion: action.reward.potion };
      } else if (action.kind === 'open_card_reward') cardOpen = true;
      else assert.fail('loot must never select or skip a card');
      revision++;
      return reading();
    } }));
    return current;
  };
  return { chosen, pending, get output() { return output; }, ctx: {
    game: { async decision() { return reading(); } },
    async decisionDocument(current) { return current.doc; },
    async printCurrentReading(current) { output += current.view; },
    write(text) { output += text; },
  } };
}

test('loot collects typed gold and legal potions past a relic, without masking the remaining choice', async () => {
  const fixture = lootFixture([{ kind: 'relic', relic: 'MummifiedHand' },
    { kind: 'potion', potion: 'SpeedPotion' }, { kind: 'card_reward', cards: [] }, { kind: 'gold', amount: 34 }]);
  await loot(fixture.ctx);
  assert.deepEqual(fixture.chosen, ['gold', 'potion']);
  assert.deepEqual(fixture.pending.map(r => r.kind), ['relic', 'card_reward']);
  assert.equal(fixture.output, 'Reward');
});

test('loot leaves mutually exclusive chest rewards and full-slot potion replacement to the player', async () => {
  for (const rewards of [[{ kind: 'relic', relic: 'MoltenEgg' }, { kind: 'sapphire_key' }, { kind: 'card_reward', cards: [] }],
    [{ kind: 'potion', potion: 'SpeedPotion' }, { kind: 'card_reward', cards: [] }]]) {
    const fixture = lootFixture(rewards, { full: true });
    await loot(fixture.ctx);
    assert.deepEqual(fixture.chosen, []);
    assert.equal(fixture.output, 'Reward');
  }
});

test('loot still opens cards after routine rewards, but never takes or skips them', async () => {
  const fixture = lootFixture([{ kind: 'card_reward', cards: [] }, { kind: 'stolen_gold', amount: 40 }]);
  await loot(fixture.ctx);
  assert.deepEqual(fixture.chosen, ['stolen_gold', 'open_card_reward']);
  await loot(fixture.ctx);
  assert.deepEqual(fixture.chosen, ['stolen_gold', 'open_card_reward']);
  assert.equal(fixture.output, 'Card rewardCard reward');
});

test('loot accepts the real omitted-empty pending_rewards contract without acting', async () => {
  const fixture = lootFixture([]);
  await loot(fixture.ctx);
  assert.deepEqual(fixture.chosen, []);
  assert.equal(fixture.output, 'Reward');
});

test('loot collects gold but preserves competing potions for the last slot', async () => {
  const fixture = lootFixture([{ kind: 'potion', potion: 'SpeedPotion' },
    { kind: 'gold', amount: 34 }, { kind: 'potion', potion: 'PowerPotion' }, { kind: 'card_reward', cards: [] }]);
  await loot(fixture.ctx);
  assert.deepEqual(fixture.chosen, ['gold']);
  assert.deepEqual(fixture.pending.map(reward => reward.kind), ['potion', 'potion', 'card_reward']);
  assert.equal(fixture.output, 'Reward');
});

test('loot collects all potion rewards when they fit, then opens cards', async () => {
  const fixture = lootFixture([{ kind: 'potion', potion: 'SpeedPotion' },
    { kind: 'potion', potion: 'PowerPotion' }, { kind: 'card_reward', cards: [] }], { capacity: 2 });
  await loot(fixture.ctx);
  assert.deepEqual(fixture.chosen, ['potion', 'potion', 'open_card_reward']);
});

test('loot clears Sozu-blocked potion rewards without asking for slot replacement', async () => {
  const fixture = lootFixture([{ kind: 'potion', potion: 'PowerPotion' },
    { kind: 'card_reward', cards: [] }], { full: true, sozu: true });
  await loot(fixture.ctx);
  assert.deepEqual(fixture.chosen, ['potion', 'open_card_reward']);
  assert.doesNotMatch(fixture.output, /Loot paused/);
});

function rewardFixture(withGold) {
  const actions = [
    { semantic: 'use-potion:BloodPotion#0', label: 'use potion[0] Blood Potion', kind: 'use_run_potion' },
    ...(withGold ? [{ semantic: 'claim:Gold#0', label: 'claim reward[0] 27 gold' }] : []),
    { semantic: 'claim:BagOfPreparation#0', label: `claim reward[${withGold ? 1 : 0}] relic Bag of Preparation` },
    { semantic: 'claim:SapphireKey#0', label: `claim reward[${withGold ? 2 : 1}] sapphire key` },
    { semantic: 'proceed', label: 'proceed Reward', kind: 'proceed', site: 'reward' },
  ];
  const options = actions.slice(1).map((item, index) => ({
    label: item.label, choice: { ordinal_path: [index + 1] },
    candidate: { domain: 'strategic', action: { kind: item.kind ?? 'claim_reward', site: item.site } },
  }));
  const snapshot = withGold ? 'before-gold' : 'after-gold';
  const doc = { decision: { information: { snapshot_id: snapshot }, candidates: actions },
    semantic_actions: actions.map(a => a.semantic) };
  const reading = { options, raw: { reading: { source: { public_snapshot_id: snapshot }, options } },
    view: options.map((option, index) => `${index}: ${option.label}`).join('\n') };
  return { reading, doc };
}

test('reward names survive gold removal and cannot fall through to a key or similar selector', () => {
  const old = rewardFixture(true);
  const next = rewardFixture(false);
  assert.equal(resolveOptionName('claim:BagOfPreparation#0', old.reading, old.doc), 1);
  assert.equal(resolveOptionName('claim:BagOfPreparation#0', next.reading, next.doc), 0);
  assert.equal(resolveOptionName('Bag of Preparation', next.reading, next.doc), 0);
  for (const selector of ['claim:Gold#0', 'claim:BagOfPreparation#01', 'claim:BagOfPreparation#9']) {
    assert.throws(() => resolveOptionName(selector, next.reading, next.doc), /no exact current option/);
  }
  assert.throws(() => resolveOptionName('claim:BagOfPreparation#0', old.reading, next.doc), /decision changed/);
});

test('numeric reward choices fail without executing even after rows shifted', async () => {
  for (const withGold of [true, false]) {
    const { reading } = rewardFixture(withGold);
    for (const option of reading.options) option.choose = async () => assert.fail('no reward should be claimed');
    const ctx = { game: { async decision() { return reading; } } };
    for (const tokens of [['1'], ['0', '1']]) {
      await assert.rejects(choose(ctx, tokens[0], tokens.slice(1)), /numeric choices are not accepted/);
    }
  }
});

test('named reward chooses the bound original option after removal of gold', async t => {
  t.mock.method(process.stdout, 'write', () => true);
  const { reading, doc } = rewardFixture(false);
  let picked;
  reading.options.forEach((option, index) => {
    option.choose = async () => { picked = index; return { view: 'Rewards' }; };
  });
  await choose({ game: { async decision() { return reading; } },
    async resolveOptionByName(name, source) { return resolveOptionName(name, source, doc); },
    async printCurrentReading() {},
  }, 'claim:BagOfPreparation#0', []);
  assert.equal(picked, 0);
});

function choiceFixture(control, choices) {
  // The hidden management candidate shifts engine ordinals away from rows.
  const actions = ['discard-potion:BloodPotion#0', ...choices.map(choice => choice.command)];
  const options = choices.map((choice, index) => ({
    label: choice.label, choice: { ordinal_path: [index + 1] },
  }));
  return {
    reading: { control, options,
      raw: { reading: { source: { public_snapshot_id: 'cards' }, options } },
      view: choices.map(choice => `./p ${choice.displayCommand ?? choice.command}  |  Effect: ${choice.label}`).join('\n') },
    doc: { decision: { information: { snapshot_id: 'cards' }, candidates: actions.map(() => ({})) },
      semantic_actions: actions },
  };
}

test('card reward and discovery print commands that choose the bound card, not its row as occurrence', async t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  for (const [control, verb] of [['strategic', 'take'], ['combat', 'choose']]) {
    const { reading, doc } = choiceFixture(control, [
      { label: 'Thunderclap', command: `${verb}:ThunderClap+0#0` },
      { label: 'Armaments', command: `${verb}:Armaments+0#0` },
      { label: 'Skip', command: 'skip' },
    ]);
    output = '';
    printReading(reading);
    const command = /^\.\/p (\S+)  \|  Effect: Armaments$/m.exec(output)?.[1];
    assert.equal(command, `${verb}:Armaments+0#0`);
    assert.equal(output.trim().split('\n').length, 3, 'commands replace rows rather than adding a second listing');
    let picked;
    reading.options.forEach((option, index) => {
      option.choose = async () => { picked = index; return { view: 'Next' }; };
    });
    await choose({ game: { async decision() { return reading; } },
      async resolveOptionByName(name, current) { return resolveOptionName(name, current, doc); },
      async printCurrentReading() {},
    }, command, []);
    assert.equal(picked, 1);
    assert.deepEqual(expandedCombatCommands(reading, doc), [], 'discovery already has its choice rows');
    assert.throws(() => readingOptionCommands(reading, { ...doc,
      decision: { ...doc.decision, information: { snapshot_id: 'other' } },
    }), /decision changed/);
  }
});

test('unmarked card selectors bind base first and sole upgraded fallback exactly', () => {
  const mixed = choiceFixture('strategic', [
    { label: 'Select Strike', command: 'select:Strike+0#0' },
    { label: 'Select Strike+', command: 'select:Strike+#0' },
  ]);
  assert.equal(resolveOptionName('select:Strike#0', mixed.reading, mixed.doc), 0);
  assert.equal(resolveOptionName('select:Strike+#0', mixed.reading, mixed.doc), 1);
  for (const selector of ['select:Strike\uFF0B#0', 'Strike\uFF0B']) {
    assert.throws(() => resolveOptionName(selector, mixed.reading, mixed.doc), /use ASCII '\+' \(U\+002B\)/);
  }
  assert.throws(() => resolveOptionName('select:Strike#1', mixed.reading, mixed.doc), /no exact current option/);

  const uneven = choiceFixture('strategic', [
    { label: 'Select Strike', command: 'select:Strike+0#0' },
    { label: 'Select Strike+ copy 1', command: 'select:Strike+#0' },
    { label: 'Select Strike+ copy 2', command: 'select:Strike+#1' },
  ]);
  assert.throws(() => resolveOptionName('select:Strike#1', uneven.reading, uneven.doc), /no exact current option/);

  const upgraded = choiceFixture('strategic', [
    { label: 'Select Strike+', command: 'select:Strike+#0' },
  ]);
  assert.equal(resolveOptionName('select:Strike#0', upgraded.reading, upgraded.doc), 0);
  const multiple = choiceFixture('strategic', [
    { label: 'Select Searing Blow+', command: 'select:SearingBlow+#0' },
    { label: 'Select Searing Blow+2', command: 'select:SearingBlow+2#1' },
  ]);
  assert.throws(() => resolveOptionName('select:SearingBlow#0', multiple.reading, multiple.doc), /no exact current option/);
});

test('grouped smith rows retain exact command bindings across omitted copies', async t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const { reading, doc } = choiceFixture('strategic', [
    { label: 'rest', command: 'rest' },
    ...Array.from({ length: 7 }, (_, index) => ({
      label: `Upgrade one Strike${index === 2 ? ' [Bottled Flame]' : ''}`, command: `smith:Strike+0#${index}`,
    })),
    { label: 'recall', command: 'recall' },
  ]);
  const expandedView = reading.view;
  // Rust omits duplicate rows; their positions in the bound list stay unchanged.
  reading.view = './p rest  |  Effect: rest\n./p smith:Strike#0  |  Effect: Upgrade one Strike (choose 1 of 6 copies) | base damage/hit 6 -> 9\n'
    + './p smith:Strike#2  |  Effect: Upgrade one Strike [Bottled Flame] | base damage/hit 6 -> 9\n./p recall  |  Effect: recall';
  printReading(reading);
  assert.match(output, /\.\/p rest  \|  Effect: rest/);
  assert.match(output, /\.\/p recall  \|  Effect: recall/);
  assert.match(output, /\.\/p smith:Strike#0  \|  Effect: Upgrade one Strike \(choose 1 of 6 copies\) \| base damage\/hit 6 -> 9/);
  const command = /\.\/p (smith:Strike#2)  \|  Effect: Upgrade one Strike \[Bottled Flame\]/.exec(output)?.[1];
  assert.equal(command, 'smith:Strike#2');
  let picked;
  reading.options.forEach((option, index) => { option.choose = async () => { picked = index; return { view: 'Upgraded' }; }; });
  await choose({ game: { async decision() { return reading; } },
    async resolveOptionByName(name, current) { return resolveOptionName(name, current, doc); },
    async printCurrentReading() {},
  }, command, []);
  assert.equal(picked, 3);
  output = '';
  reading.view = expandedView;
  printReading(reading, { expanded: true });
  assert.equal((output.match(/\.\/p smith:/g) ?? []).length, 7);
  assert.match(output, /\.\/p smith:Strike\+0#6  \|/);
  assert.doesNotMatch(output, /@deck\[/);
  assert.equal(resolveOptionName('smith:Strike+0#6', reading, doc), 7);
});

test('full map includes the boss identity and every connection without unrelated room choices', async () => {
  let output = '';
  let boss = 'TheCollector';
  const ctx = { repository: 'D:/rust/sts_simulator', join, write(text) { output += text; },
    game: {
      async observe() { return { position: { kind: 'decision', observation: { domain: 'strategic', observation: { public_map: {
        current: [-1, -1], boss, nodes: [[1, 0, 'monster', false, [[0, 1], [2, 1]]],
          [0, 1, 'elite', true, []], [2, 1, 'rest', false, []]],
      } } } } }; },
      async decision() { assert.fail('explicit full map must not repeat decision context'); },
    }, async printCurrentReading() { assert.fail('full map must not print unrelated room choices'); },
  };
  await mapView(ctx, ['full']);
  assert.match(output, /M\(1,0\)->\(0,1\)\/\(2,1\)/);
  assert.match(output, /K\(0,1\)/);
  assert.equal((output.match(/Boss: TheCollector/g) ?? []).length, 1);
  assert.equal((output.match(/All next-room choices:/g) ?? []).length, 0);
  assert.doesNotMatch(output, /Connected paths to the first Elite|First landmarks/);
  boss = null;
  output = '';
  await mapView(ctx, ['full']);
  assert.match(output, /^Boss not revealed\n/);
  assert.doesNotMatch(output, /TheCollector/);
});

test('relic, removal, event and route choices print actual executable commands', () => {
  const { reading, doc } = choiceFixture('strategic', [
    { label: 'Go (1,7) Elite', command: 'route:1.7', displayCommand: 'go 1,7' },
    { label: 'take boss relic Astrolabe', command: 'boss-relic:Astrolabe#0' },
    { label: 'remove Strike', command: 'remove:Strike+0#2' },
    { label: 'Obtain a random common relic.', command: 'event:Neow:1' },
    { label: '[open] chest', command: 'open-chest' },
  ]);
  doc.decision.observation = { domain: 'strategic', observation: { public_map: {
    current: [1, 6], nodes: [[1, 7, 'elite', false, []]],
  } } };
  const output = reading.view;
  assert.match(output, /\.\/p go 1,7  \|  Effect: Go \(1,7\) Elite/);
  assert.match(output, /\.\/p boss-relic:Astrolabe#0/);
  assert.match(output, /\.\/p remove:Strike\+0#2/);
  assert.match(output, /\.\/p event:Neow:1  \|  Effect: Obtain a random common relic\./);
  assert.match(output, /\.\/p open-chest  \|  Effect: \[open\] chest/);
  assert.equal(resolveOptionName('event:Neow:1', reading, doc), 3, 'event index is not the displayed row');
  assert.doesNotMatch(output, /\.\/p route:/);
});

test('shop leave keeps a named command when purchases reorder the rows', () => {
  for (const choices of [
    [{ label: 'buy Metallicize 84g', command: 'buy:Metallicize+0#0' }, { label: 'buy Flex 26g', command: 'buy:Flex+0#0' }, { label: 'leave shop', command: 'leave' }],
    [{ label: 'buy Flex 26g', command: 'buy:Flex+0#0' }, { label: 'leave shop', command: 'leave' }],
  ]) {
    const { reading, doc } = choiceFixture('strategic', choices);
    const output = reading.view;
    assert.match(output, /\.\/p leave  \|  Effect: leave shop/);
    assert.equal(resolveOptionName('leave', reading, doc), choices.length - 1);
  }
});

test('expanded combat exposes legal potion occurrences and targets, default stays compact', t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const reading = { control: 'combat', view: 'Energy 0\nPotions: P0 Blood Potion, P1 Attack Potion', options: [],
    raw: { reading: { source: { public_snapshot_id: 'combat' } } } };
  const semantics = ['end', 'use-potion:BloodPotion#0', 'use-potion:AttackPotion#0', 'discard-potion:AttackPotion#0'];
  const doc = { semantic_actions: semantics, decision: { information: { snapshot_id: 'combat' }, candidates: semantics.map(() => ({})) } };
  printReading(reading);
  assert.doesNotMatch(output, /\.\/p act/);
  output = '';
  printReading(reading, { expanded: true, combatCommands: expandedCombatCommands(reading, doc) });
  assert.match(output, /\.\/p act use-potion:AttackPotion#0/);
  assert.doesNotMatch(output, /AttackPotion#1|play:Defend/);
  assert.match(output, /\.\/p act discard-potion:AttackPotion#0/);
  doc.semantic_actions[2] = 'use-potion:FirePotion#0@1';
  assert.ok(expandedCombatCommands(reading, doc).includes('use-potion:FirePotion#0@1'));
  assert.throws(() => expandedCombatCommands(reading, { ...doc, semantic_actions: ['end'] }), /contract drifted/);
  assert.throws(() => expandedCombatCommands(reading, { ...doc,
    decision: { ...doc.decision, information: { snapshot_id: 'stale' } },
  }), /decision changed/);
  delete reading.raw.reading.source; // Actual ordinary-combat reading contract.
  assert.deepEqual(expandedCombatCommands(reading, doc), doc.semantic_actions);
});

function combatFixture(cards, monsters, plays, view) {
  return {
    reading: { control: 'combat', view, raw: { reading: { source: { public_snapshot_id: 'combat' } } } },
    doc: { decision: { information: { snapshot_id: 'combat' },
      observation: { observation: { player: { block: 7 }, cards: { hand: { cards } }, monsters } },
      candidates: plays.map(play => ({ action: { kind: 'play_card', hand_index: play.hand, target_monster_index: play.target ?? null } })),
    }, semantic_actions: plays.map(play => play.command) },
  };
}

test('compact combat exposes all usable bottles and leaves discarding to options', () => {
  const { reading, doc } = combatFixture([], [], [], 'Potions: P0 empty, P1 Power Potion, P2 Fire Potion');
  doc.decision.candidates = [
    { kind: 'use_potion', potion_index: 1 },
    { kind: 'use_potion', potion_index: 2, target_monster_index: 0 },
    { kind: 'discard_potion', potion_index: 1 },
  ].map(action => ({ action }));
  doc.semantic_actions = ['use-potion:PowerPotion#0', 'use-potion:FirePotion#0@0', 'discard-potion:PowerPotion#0'];
  assert.deepEqual(distinctCombatCommands(reading, doc), [
    'use-potion:PowerPotion#0  |  P1',
    'use-potion:FirePotion#0@0  |  P2',
  ]);
  let output = '';
  printReading(reading, { combatCommands: distinctCombatCommands(reading, doc), write: text => { output += text; } });
  assert.match(output, /Potions: P0 empty, P1 Power Potion, P2 Fire Potion/);
  assert.match(output, /act use-potion:PowerPotion#0/);
  assert.doesNotMatch(output, /act discard-potion/);
  assert.ok(expandedCombatCommands(reading, doc).includes('use-potion:PowerPotion#0'));
  assert.ok(expandedCombatCommands(reading, doc).includes('discard-potion:PowerPotion#0'));
  doc.decision.candidates.shift();
  doc.semantic_actions.shift();
  assert.equal(distinctCombatCommands(reading, doc).length, 1, 'show only the remaining targeted use');
  doc.decision.candidates.shift();
  doc.semantic_actions.shift();
  assert.deepEqual(distinctCombatCommands(reading, doc), [], 'ordinary bottle management needs no extra command block');
  reading.options = [{ label: 'pending choice' }];
  assert.deepEqual(distinctCombatCommands(reading, doc), [], 'pending choices use their own legal interface');
});

test('compact combat keeps the legal End Turn option and does not invent it during a choice', () => {
  const { reading, doc } = combatFixture([], [], [], 'Hand: Burn[unplayable]');
  doc.decision.candidates = [{ action: { kind: 'end_turn' } }];
  doc.semantic_actions = ['end'];
  assert.deepEqual(distinctCombatCommands(reading, doc), ['end']);
  let output = '';
  printReading(reading, { combatCommands: distinctCombatCommands(reading, doc), write: text => { output += text; } });
  assert.match(output, /act end/);
  reading.selections = [{}];
  assert.deepEqual(distinctCombatCommands(reading, doc), []);
  reading.selections = [];
  doc.decision.candidates = []; doc.semantic_actions = [];
  assert.deepEqual(distinctCombatCommands(reading, doc), []);
});

test('compact zero-energy hand exposes free attacks and untargeted potions without repeating equivalent copies', () => {
  const card = { card_id: 'BloodForBlood', upgrades: 0, effective_cost: 0, current_damage: 27 };
  const { reading, doc } = combatFixture([card, { ...card }, { ...card, current_damage: 30 }], [], [
    { hand: 0, target: 0, command: 'play:BloodForBlood+0#0@0' },
    { hand: 0, target: 1, command: 'play:BloodForBlood+0#0@1' },
    { hand: 1, target: 0, command: 'play:BloodForBlood+0#1@0' },
    { hand: 1, target: 1, command: 'play:BloodForBlood+0#1@1' },
    { hand: 2, target: 0, command: 'play:BloodForBlood+0#2@0' },
  ]);
  doc.decision.candidates.push({ action: { kind: 'use_potion', potion_index: 0 } }, { action: { kind: 'end_turn' } });
  doc.semantic_actions.push('use-potion:AttackPotion#0', 'end');
  assert.deepEqual(distinctCombatCommands(reading, doc), [
    'play:BloodForBlood#0@0  |  0c', 'play:BloodForBlood#0@1  |  0c',
    'play:BloodForBlood#2@0  |  0c', 'use-potion:AttackPotion#0  |  P0', 'end',
  ]);
});

test('strategic target menus display the same act selection command used by classification', () => {
  const { reading, doc } = choiceFixture('strategic', [
    { label: 'Select Demon Form+', command: 'select:DemonForm+#0' },
    { label: 'Select Disarm', command: 'select:Disarm+0#0' },
  ]);
  doc.decision.observation = { domain: 'strategic', observation: { context: { kind: 'run_choice' } } };
  const commands = readingOptionCommands(reading, doc);
  assert.deepEqual(commands, ['act select:DemonForm+#0', 'act select:Disarm#0']);
  assert.equal(commands[0], 'act select:DemonForm+#0');
  doc.decision.observation.domain = 'combat';
  assert.deepEqual(readingOptionCommands(reading, doc), ['select:DemonForm+#0', 'select:Disarm#0']);
});

test('Snecko dissimilar copies expose their exact commands, equivalent copies stay compact', t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const cards = [
    { card_id: 'Strike', upgrades: 0, effective_cost: 3, current_damage: 6 },
    { card_id: 'Defend', upgrades: 0, effective_cost: 3, current_block: 5 },
    { card_id: 'Metallicize', upgrades: 0, effective_cost: 2 },
    { card_id: 'Defend', upgrades: 0, effective_cost: 1, current_block: 5 },
    { card_id: 'FlameBarrier', upgrades: 1, effective_cost: 3, current_block: 16 },
  ];
  const { reading, doc } = combatFixture(cards, [], [
    { hand: 0, command: 'play:Strike+0#0@0' },
    { hand: 1, command: 'play:Defend+0#0' },
    { hand: 2, command: 'play:Metallicize+0#0' },
    { hand: 3, command: 'play:Defend+0#1' },
    { hand: 4, command: 'play:FlameBarrier+#0' },
  ], 'Hand: Strike[3c 6d], Defend[3c 5b], Metallicize[2c], Defend[1c 5b], Flame Barrier+[3c 16b]');
  const render = () => printReading(reading, { combatCommands: distinctCombatCommands(reading, doc) });
  render();
  assert.match(output, /Defend\[3c 5b\].*Defend\[1c 5b\]/);
  assert.match(output, /\.\/p act play:Defend#0  \|  3c/);
  assert.match(output, /\.\/p act play:Defend#1  \|  1c/);
  assert.equal((output.match(/\.\/p act /g) ?? []).length, 5);
  assert.doesNotMatch(output, /#3/);
  cards[1].effective_cost = 1;
  assert.equal(distinctCombatCommands(reading, doc).length, 4, 'equivalent copies share one executable command');
  cards[1].upgrades = 1;
  assert.equal(distinctCombatCommands(reading, doc).length, 5, 'upgrade differences keep separate selectors');
  doc.decision.candidates.splice(1, 1);
  doc.semantic_actions.splice(1, 1);
  assert.deepEqual(distinctCombatCommands(reading, doc), [
    'play:Strike#0@0  |  3c', 'play:Metallicize#0  |  2c',
    'play:Defend#1  |  1c', 'play:FlameBarrier+#0  |  3c',
  ], 'never advertise an illegal copy');
  doc.decision.information.snapshot_id = 'other';
  assert.throws(() => distinctCombatCommands(reading, doc), /decision changed/);
});

test('board binds damage to active monster slots and candidate target indices', () => {
  const cards = [
    { card_id: 'Cleave', upgrades: 0, effective_cost: 1, current_damage: 12, damage_by_monster_order: [99, 12, 8] },
    { card_id: 'Anger', upgrades: 0, effective_cost: 0, current_damage: 6, damage_by_monster_order: [99, 9, 6] },
  ];
  const monsters = [
    { slot: 0, alive: false, intent: { preview_damage_per_hit: 99 } },
    { slot: 2, alive: true }, { slot: 6, alive: true },
  ];
  const { doc } = combatFixture(cards, monsters, [
    { hand: 0, command: 'play:Cleave+0#0' },
    { hand: 1, target: 1, command: 'play:Anger+0#0@2' },
    { hand: 1, target: 2, command: 'play:Anger+0#0@6' },
  ], 'Hand: Cleave[1c], Anger[0c]');
  assert.deepEqual(summarizeDecisionJson(doc), {
    incoming: 0, currentBlock: 7, handBlock: 0, playableLines: [
      'play:Cleave+0#0 cost=1 dmg=M2:12,M6:8',
      'play:Anger+0#0@2 cost=0 dmg=9',
      'play:Anger+0#0@6 cost=0 dmg=6',
    ],
  });
  cards[0].damage_by_monster_order = [8];
  assert.throws(() => summarizeDecisionJson(doc), /damage contract drifted/);
});

test('rendering preserves Rust hand values and live relic counters without a second data source', t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const reading = { control: 'combat', view: 'Hand: Perfected Strike+[2c 39d], Perfected Strike[2c 28d]\nRelics: Nunchaku [9], Kunai [2]' };
  printReading(reading);
  assert.equal(output, reading.view+'\n');
  output = '';
  printReading(reading);
  assert.equal(output, reading.view+'\n');
});

test('single-card selection keeps its member binding separate from cancel option zero', async t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const members = [{ label: 'Defend' }, { label: 'Anger' }, { label: 'Defend' }];
  let picked;
  const reading = { control: 'combat',
    view: 'Selection | Upgrade | Choose 1\n  ./p act select:Defend+0#0  |  Defend\n  ./p act select:Anger+0#0  |  Anger\n  ./p act select:Defend+0#1  |  Defend',
    selections: [{ min: 1, max: 1, members, async select(chosen) { picked = chosen; return { view: 'Combat' }; } }],
    options: [{ label: 'Cancel', async choose() { assert.fail('selection must not cancel'); } }],
  };
  printReading(reading);
  assert.equal(output, reading.view + '\n');
  await choose({ game: { async decision() { return reading; } }, async printCurrentReading() {} }, '2', []);
  assert.deepEqual(picked, [members[2]]);
  assert.deepEqual(expandedCombatCommands(reading, {}), []);
  output = '';
  reading.view = 'Selection | Upgrade | Choose 1\n  select:Defend+0#1  |  Defend';
  printReading(reading);
  assert.doesNotMatch(output, /\.\/p/);
});

test('combat selection rows use the act executor for exact semantic commands', async () => {
  let output = '';
  const reading = { control: 'combat',
    view: 'Selection | Exhume | Choose 1\n  ./p act select:Defend+0#0  |  Defend\n  ./p act select:ShrugItOff+0#1  |  Shrug It Off\n  ./p act select:Disarm+0#0  |  Disarm',
    selections: [{ min: 1, max: 1, members: [] }],
  };
  printReading(reading, { write: text => { output += text; } });
  assert.match(output, /\.\/p act select:Defend\+0#0  \|  Defend/);
  assert.match(output, /\.\/p act select:ShrugItOff\+0#1  \|  Shrug It Off/);
  assert.match(output, /\.\/p act select:Disarm\+0#0  \|  Disarm/);
  const calls = [];
  await actOrPreview({ game: { async decision() { return reading; }, async act(tokens) {
    calls.push(tokens); return { view: 'Resolved selection' };
  } }, async printCurrentReading() {}, write() {} }, 'act', ['select', 'Disarm']);
  assert.deepEqual(calls, [['select:Disarm']]);
  output = '';
  reading.view = 'Selection | Exhume | Choose 1\n  select:Disarm+0#0  |  Disarm';
  printReading(reading, { write: text => { output += text; } });
  assert.doesNotMatch(output, /\.\/p/, 'a hypothetical preview does not advertise current commands');
  reading.selections[0].min = 2; reading.selections[0].max = 2;
  reading.view = 'Selection | Replace hand cards | Choose 2\nInput: ./p act select:<card tokens joined by commas without spaces>\n  Defend+0#0  |  Defend';
  output = '';
  printReading(reading, { write: text => { output += text; } });
  assert.match(output, /Input: \.\/p act select:<card tokens joined by commas without spaces>/);
  assert.doesNotMatch(output, /\.\/p act select:Defend/, 'one member cannot complete a two-card selection');
});

test('explicit smith upgrade resolves a unique copy and keeps duplicate choices explicit', () => {
  const { reading, doc } = choiceFixture('strategic', [
    { label: 'Smith Rampage', command: 'smith:Rampage+0#0' },
    { label: 'Smith Feed copy 1', command: 'smith:Feed+0#0' },
    { label: 'Smith Feed copy 2', command: 'smith:Feed+0#1' },
  ]);
  assert.equal(resolveOptionName('smith:Rampage+0', reading, doc), 0);
  assert.equal(resolveOptionName('smith:Rampage', reading, doc), 0);
  assert.equal(resolveOptionName('smith:Rampage+0#0', reading, doc), 0);
  assert.equal(resolveOptionName('smith:Rampage#0', reading, doc), 0);
  assert.equal(resolveOptionName('smith:Feed+0#1', reading, doc), 2);
  for (const selector of ['smith:Feed+0', 'smith:Feed', 'smith:Rampage+1', 'smith:Rampage+0#1']) {
    assert.throws(() => resolveOptionName(selector, reading, doc), /no exact current option/);
  }
});

test('route coordinates use bound map facts and room type needs no confirmation', async () => {
  const { reading, doc } = choiceFixture('strategic', [
    { label: 'Go (0,1) Monster · First landmarks: (1,1) Elite', command: 'route:0.1' },
    { label: 'Go (1,1) Monster · deliberately misleading label', command: 'route:1.1' },
  ]);
  doc.decision.observation = { domain: 'strategic', observation: { public_map: {
    current: [0, 0], nodes: [[0, 1, 'monster', false, []], [1, 1, 'elite', false, []]],
  } } };
  const picked = [];
  reading.options.forEach((option, index) => {
    const action = { kind: 'choose_route_node', x: index, y: 1, flight: false };
    option.candidate = { domain: 'strategic', action };
    doc.decision.candidates[index + 1] = option.candidate;
    option.choose = async () => { picked.push(index); return { view: 'Entered' }; };
  });
  const ctx = { game: { async decision() { return reading; } },
    async decisionDocument() { return doc; }, async printCurrentReading() {}, write() {},
    runEngine() { return { status: 0, stdout: '' }; },
  };
  await go(ctx, ['0,1']);
  await go(ctx, ['1,1']);
  await choose(ctx, '0', []);
  await choose(ctx, '1', []);
  assert.deepEqual(picked, [0, 1, 0, 1]);
  for (const parameters of [['1,1', 'elite'], ['elite'], ['1,1!']]) {
    await assert.rejects(go(ctx, parameters), /usage: go \[x,y\]/);
  }
  assert.deepEqual(picked, [0, 1, 0, 1], 'removed confirmation syntax does not act');
});

test('explicit coordinates depart rewards and enter the chosen room in one prepared line', async () => {
  const fixture = elite => {
    const rewards = choiceFixture('strategic', [
      { label: 'open card reward #0', command: 'open-card#0' },
      { label: 'proceed Reward', command: 'proceed' },
    ]);
    rewards.reading.raw.reading.options[0].candidate = { action: { kind: 'open_card_reward' } };
    rewards.reading.raw.reading.options[1].candidate = { action: { kind: 'proceed', site: 'reward' } };
    const map = choiceFixture('strategic', [{ label: 'Go (1,1)', command: 'route:1.1' }]);
    const observation = { domain: 'strategic', observation: { public_map: {
      current: [1, 0], nodes: [[1, 1, elite ? 'elite' : 'monster', false, []]],
    } } };
    rewards.doc.decision.observation = observation;
    map.doc.decision.observation = observation;
    const picked = [];
    rewards.reading.options[0].choose = async () => assert.fail('must not reopen cards');
    rewards.reading.options[1].choose = async () => { picked.push('proceed'); return map.reading; };
    map.reading.options[0].choose = async () => { picked.push('route'); return { view: 'Entered' }; };
    return { picked, ctx: { game: {
      async decision() { return rewards.reading; },
      async act(actions) {
        assert.deepEqual(actions, ['proceed', 'route:1.1']);
        picked.push('proceed', 'route');
        return { view: 'Entered' };
      },
    },
      async decisionDocument(reading) { return reading === rewards.reading ? rewards.doc : map.doc; },
      async printCurrentReading() {}, write() {},
      runEngine() { assert.fail('explicit reward departure needs no diagnostic dump'); },
    } };
  };
  for (const isElite of [false, true]) {
    const current = fixture(isElite);
    await go(current.ctx, ['1,1']);
    assert.deepEqual(current.picked, ['proceed', 'route']);
  }
});

test('shop go prepares departure and route atomically, including elite destinations', async () => {
  const { reading, doc } = choiceFixture('strategic', [
    { label: 'not a label-derived leave', command: 'leave' },
  ]);
  reading.raw.reading.options[0].candidate = { action: { kind: 'leave_shop' } };
  doc.decision.observation = { domain: 'strategic', observation: { public_map: {
    current: [1, 0], nodes: [[1, 0, 'shop', false, [[1, 1]]], [1, 1, 'elite', false, []]],
  } } };
  let departed = false;
  reading.options[0].choose = async () => assert.fail('departure must not be submitted alone');
  const ctx = {
    game: {
      async decision() { return reading; },
      async act(actions) {
        if (actions[1] !== 'route:1.1') throw new Error('destination unavailable; prepared line not applied');
        assert.deepEqual(actions, ['leave', 'route:1.1']);
        departed = true;
        return { view: 'Entered' };
      },
    },
    async decisionDocument() { return doc; }, async printCurrentReading() {},
  };
  await assert.rejects(go(ctx, ['1,0']), /destination unavailable/);
  assert.equal(departed, false);
  await go(ctx, ['1,1']);
  assert.equal(departed, true);
});

test('go takes a sole map exit regardless of room type and pauses at forks', async () => {
  for (const [rooms, elite] of [[1, false], [1, true], [2, false], [0, false]]) {
    const { reading, doc } = choiceFixture('strategic', Array.from({ length: rooms }, (_, x) => ({
      label: 'misleading label with (9,9) and long route commentary', command: `route:${x}.1`,
    })));
    doc.decision.observation = { domain: 'strategic', observation: { public_map: {
      current: [0, 0], nodes: Array.from({ length: rooms }, (_, x) => [x, 1, elite ? 'elite' : 'monster', false, []]),
    } } };
    const picked = [];
    reading.options.forEach((option, index) => {
      option.choose = async () => { picked.push(index); return { view: 'Entered' }; };
    });
    let shown;
    const ctx = { game: { async decision() { return reading; } },
      async decisionDocument() { return doc; }, async printCurrentReading(current) { shown = current; },
      runEngine() { return { status: 0, stdout: '' }; },
    };
    if (rooms > 1) {
      assert.deepEqual(await go(ctx, []), { needsChoice: true });
      assert.equal(shown, reading);
      assert.deepEqual(picked, []);
    } else if (!rooms) {
      await assert.rejects(go(ctx, []), /finish the current room first/);
      assert.deepEqual(picked, []);
    } else {
      await go(ctx, []);
      assert.deepEqual(picked, [0]);
    }
    await assert.rejects(go(ctx, ['9,9']), error => {
      assert.match(error.message, /no route option to \(9,9\)/);
      assert.doesNotMatch(error.message, /misleading label|long route commentary/);
      return true;
    });
  }
});

test('go finishes typed empty rewards before resolving a sole route, without dropping hidden rewards', async () => {
  for (const mode of ['empty', 'explicit', 'fork', 'elite', 'full-potion', 'card', 'missing-context', 'card-selection']) {
    const rewards = choiceFixture('strategic', [
      { label: 'not a departure label', command: mode === 'card-selection' ? 'skip-card#0' : 'proceed' },
      { label: 'proceed Reward', command: 'discard-potion:PowerPotion#0' },
    ]);
    rewards.reading.raw.reading.options[0].candidate = { action: mode === 'card-selection'
      ? { kind: 'skip_card_reward', reward_item_index: 0 } : { kind: 'proceed', site: 'reward' } };
    rewards.reading.raw.reading.options[1].candidate = { action: { kind: 'discard_potion', potion_index: 0 } };
    const rooms = mode === 'fork' ? [0, 1] : [0];
    const map = choiceFixture('strategic', rooms.map(x => ({ label: 'route', command: `route:${x}.1` })));
    const publicMap = { current: [0, 0], nodes: rooms.map(x => [x, 1, mode === 'elite' ? 'elite' : 'monster', false, []]) };
    rewards.doc.decision.observation = { domain: 'strategic', observation: {
      public_map: publicMap,
      ...(mode === 'missing-context' ? {} : { context: mode === 'card-selection' ? { kind: 'card_reward' }
        : { kind: 'reward', ...(mode === 'full-potion' ? { pending_rewards: [{ kind: 'potion', potion: 'BlockPotion' }] }
          : mode === 'card' ? { pending_rewards: [{ kind: 'card_reward', cards: [] }] } : {}) } }),
    } };
    map.doc.decision.observation = { domain: 'strategic', observation: { public_map: publicMap } };
    const picked = [];
    let shown;
    rewards.reading.options[0].choose = async () => { picked.push('proceed'); return map.reading; };
    rewards.reading.options[1].choose = async () => assert.fail('must not discard a potion');
    map.reading.options.forEach(option => { option.choose = async () => { picked.push('route'); return { view: 'Entered' }; }; });
    const ctx = { game: { async decision() { return rewards.reading; } },
      async decisionDocument(reading) { return reading === rewards.reading ? rewards.doc : map.doc; },
      async printCurrentReading(reading) { shown = reading; },
      runEngine() { return { status: 0, stdout: '' }; },
    };
    if (['full-potion', 'card', 'missing-context', 'card-selection'].includes(mode)) {
      await assert.rejects(go(ctx, []), /finish the current room first/);
      assert.deepEqual(picked, []);
      if (mode === 'card-selection') {
        await assert.rejects(go(ctx, ['0,1']), /no route option/);
        assert.deepEqual(picked, [], 'go must not silently skip an open card choice');
      }
    } else if (mode === 'fork') {
      assert.deepEqual(await go(ctx, []), { needsChoice: true });
      assert.deepEqual(picked, ['proceed']);
      assert.equal(shown, map.reading);
    } else {
      await go(ctx, mode === 'explicit' ? ['0,1'] : []);
      assert.deepEqual(picked, ['proceed', 'route']);
    }
  }
});

test('go crosses forced effect-free event screens, but stops at costs, choices and forks', async () => {
  for (const mode of ['free', 'revealed-choice', 'revealed-choice-coordinate', 'cost', 'choice', 'unknown', 'invalid-coordinate']) {
    const map = choiceFixture('strategic', [0, 1].map(x => ({ label: `Go (${x},1)`, command: `route:${x}.1` })));
    map.doc.decision.observation = { domain: 'strategic', observation: { public_map: {
      current: [0, 0],
      nodes: [0, 1].map(x => [x, 1, 'monster', false, []]),
    } } };
    const ending = choiceFixture('strategic', [{ label: 'Leave.', command: 'event:Neow:0' }]);
    const first = choiceFixture('strategic', Array.from({ length: mode === 'choice' ? 2 : 1 }, (_, i) => ({
      label: '[Leave] misleading label', command: `event:Neow:${i}`,
    })));
    const typed = (kind, effects) => ({ domain: 'strategic', action: { kind: 'choose_event_option',
      public_semantics: { starts_combat: false, structured_semantics: { effects, transition: { kind } } },
    } });
    ending.reading.raw.reading.options[0].candidate = typed('complete', []);
    if (mode.startsWith('revealed-choice')) {
      ending.reading.raw.reading.options[0].candidate = typed('advance_screen', [{ kind: 'lose_hp', amount: 6 }]);
    }
    first.reading.raw.reading.options.forEach(option => {
      option.candidate = typed('advance_screen', mode === 'cost' ? [{ kind: 'lose_hp', amount: 6 }] : []);
    });
    if (mode === 'unknown') delete first.reading.raw.reading.options[0].candidate.action.public_semantics.structured_semantics;
    let current = first, chosen = 0, shown;
    first.reading.options.forEach(option => { option.choose = async () => { chosen++; current = ending; return ending.reading; }; });
    ending.reading.options[0].choose = async () => { chosen++; current = map; return map.reading; };
    map.reading.options.forEach(option => { option.choose = async () => assert.fail('must stop at fork'); });
    const ctx = { game: { async decision() { return current.reading; } },
      async decisionDocument() { return current.doc; }, async printCurrentReading(reading) { shown = reading; },
    };
    if (mode === 'free' || mode.startsWith('revealed-choice')) {
      assert.deepEqual(await go(ctx, mode === 'revealed-choice-coordinate' ? ['0,1'] : []), { needsChoice: true });
      assert.equal(chosen, mode === 'free' ? 2 : 1);
      assert.equal(shown, mode === 'free' ? map.reading : ending.reading);
    } else {
      await assert.rejects(go(ctx, mode === 'invalid-coordinate' ? ['bad-coordinate'] : []),
        mode === 'invalid-coordinate' ? /usage: go/ : /finish the current room first/);
      assert.equal(chosen, 0);
    }
  }
});

test('chained ordinary row numbers are rejected before the first mutation', async () => {
  await assert.rejects(choose({ game: { async decision() { return {
    options: [{ label: 'open chest', async choose() { assert.fail('must not open chest'); } }],
  }; } } }, '0', ['1']), /multiple row numbers are only valid within one card selection/);
});

test('info forwards complete queries to the Rust owner and preserves ordinary page info', async t => {
  const calls = [];
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const ctx = { binary: 'run_play', session: 'selected', repository: 'repo',
    runEngine(binary, args) {
      calls.push([binary, args]);
      return { status: 0, stdout: args[0] === 'info' ? 'Power: Spore Cloud\n' : 'header\ncandidates=page mechanics\n', stderr: '' };
    },
  };
  await info(ctx, ['Spore', 'Cloud']);
  await info(ctx);
  assert.deepEqual(calls, [
    ['run_play', ['info', '--session', 'selected', '--locale', 'en', 'Spore', 'Cloud']],
    ['run_play', ['show', '--session', 'selected']],
  ]);
  assert.equal(output, 'Power: Spore Cloud\ncandidates=page mechanics\n');
});

test('event phase advance does not imply that immediate trade effects wait for confirmation', async () => {
  let output = '';
  await info({ binary: 'run_play', session: 'selected', repository: 'repo',
    write: text => { output += text; },
    runEngine() { return { status: 0, stdout: 'candidates=1\nmechanics=[heal:18; obtain-curse:1:Parasite; transition=advance-screen]\n' }; },
  });
  assert.match(output, /heal:18; obtain-curse:1:Parasite/);
  assert.match(output, /Listed effects may apply immediately/);
  assert.match(output, /does not imply a later confirmation or a chance to cancel/);
  assert.doesNotMatch(output, /first screen is not the final decision|cost\/confirm/);
});

test('successful choice shows only its successor view', async t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const after = { view: 'Map\n0: Go (2,4) Monster' };
  const before = { view: 'Rewards\n0: proceed Reward', options: [{
    label: 'proceed Reward', async choose() { return after; },
  }] };
  const ctx = { game: { async decision() { return before; } },
    async printCurrentReading(reading) { process.stdout.write(reading.view + '\n'); },
  };
  await choose(ctx, '0', []);
  assert.equal(output, 'Map\n0: Go (2,4) Monster\n');
});

test('selection records only the applied members in selected order', async t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const members = [{ label: 'Strike' }, { label: 'Defend' }, { label: 'Bash' }];
  const ctx = { game: { async decision() { return {
    view: 'Select cards', selections: [{ min: 1, max: 2, members,
      async select(picked) {
        assert.deepEqual(picked, [members[2], members[0]]);
        return { view: 'Done' };
      },
    }],
  }; } }, recordAction(action) { output += action; }, async printCurrentReading() {} };
  await choose(ctx, '2', ['0']);
  assert.equal(output, 'select Bash, Strike');
});

test('bare reading requests primary choices and leaves expansion to options', async () => {
  const ctx = { game: { async decision(options) { assert.deepEqual(options, { details: ['brief'] }); return { view: 'Now' }; } },
    async printCurrentReading(reading) { assert.equal(reading.view, 'Now'); },
  };
  await choose(ctx, undefined, []);
});

test('review and commit use saved objects and never request research', async () => {
  const calls = [];
  let output = '';
  const candidate = {
    planId: 'c0@d3cba96a1bf6',
    async review(options) { calls.push(['review', options]); return { view: 'saved' }; },
    async commit() { calls.push(['commit']); return { view: 'adopted' }; },
  };
  const report = { candidates: [candidate], effort: 'expanded', raw: { report_id: 'test-report-a' } };
  const id = planId(report, 0);
  const ctx = { write: text => { output += text; }, game: {
    async decision() { return { control: 'combat' }; },
    async plans() { calls.push(['plans']); return report; },
    async research() { assert.fail('inspection must not search'); },
  }, async printCurrentReading() {} };
  await researchOrCommitOrReview(ctx, 'review', [id, '2']);
  await researchOrCommitOrReview(ctx, 'commit', [id, 'expanded']);
  assert.deepEqual(calls, [['plans'], ['review', { turn: 2 }], ['plans'], ['commit']]);
  await assert.rejects(researchOrCommitOrReview(ctx, 'commit', [id, 'standard']), /saved report used expanded/);
  assert.equal(calls.filter(([name]) => name === 'commit').length, 1);
  assert.equal(output, '');
});

test('review preserves source-rendered commands and localized text without decoding traces', async () => {
  let shown;
  const report = { raw: { report_id: 'long-plan' }, candidates: [{
    planId: 'c0@da48e39c9f43',
    async review() { return { view: 'c0@da48e39c9f43 | 回合记录\n查看回合: review c0@da48e39c9f43 4\n继续: review c0@da48e39c9f43 12\n' }; },
  }] };
  const ctx = { game: { async decision() { return { control: 'combat' }; }, async plans() { return report; } },
    async printCurrentReading(reading) { shown = reading.view; } };
  const id = planId(report, 0);
  await researchOrCommitOrReview(ctx, 'review', [id]);
  assert.equal(shown, `${id} | 回合记录\n查看回合: review ${id} 4\n继续: review ${id} 12\n`);
});

test('a replaced report cannot silently rebind the same candidate index', async () => {
  let commits = 0, reviews = 0;
  const oldReport = { candidates: [{ planId: 'c0@3f47cdc4f90a' }] };
  const current = { raw: { report_id: 'report-spending-a-potion' }, effort: 'whole_combat',
    candidates: [{ planId: 'c0@0642f52fde72', async commit() { commits++; return { view: 'Actual win' }; },
      async review() { reviews++; return { view: 'c0 | Current route' }; } }] };
  const ctx = { game: { async decision() { return { control: 'combat' }; }, async plans() { return current; } },
    async printCurrentReading() {} };
  for (const command of ['review', 'commit']) {
    await assert.rejects(researchOrCommitOrReview(ctx, command, [planId(oldReport, 0)]), /replaced search report.*no action was applied/);
    await assert.rejects(researchOrCommitOrReview(ctx, command, ['c0']), /complete plan ID/);
    await assert.rejects(researchOrCommitOrReview(ctx, command, ['0']), /complete plan ID/);
  }
  assert.equal(commits, 0); assert.equal(reviews, 0);
  const id = planId(current, 0);
  await researchOrCommitOrReview(ctx, 'review', [id]);
  await researchOrCommitOrReview(ctx, 'commit', [id]);
  assert.equal(reviews, 1); assert.equal(commits, 1);
});

test('invalid indexes and arguments fail before reading or searching', async () => {
  const ctx = { game: new Proxy({}, { get() { assert.fail('unexpected request'); } }) };
  for (const params of [['0junk'], ['-1'], ['1.5'], ['c01'], ['c0junk'], ['0', '0'], ['0', '2', '3']]) {
    await assert.rejects(researchOrCommitOrReview(ctx, 'review', params));
  }
  await assert.rejects(researchOrCommitOrReview(ctx, 'research', ['typo']));
  await assert.rejects(researchOrCommitOrReview(ctx, 'research', ['expanded', '0']));
  for (const params of [['brief'], ['full', 'extra']]) {
    await assert.rejects(researchOrCommitOrReview(ctx, 'plans', params), /plans \[full\]/);
  }
  for (const params of [
    ['whole_combat', '1000', 'work'], ['whole_combat', '1000', 'nodes', '100'],
    ['expanded', '1000', 'work', '100'], ['whole_combat', '1000', 'work', '0'],
    ['whole_combat', '1000', 'work', '1000001'], ['whole_combat', '1000', 'work', '1.5'],
    ['0'], ['20001'], ['5000', 'work'], ['5000', 'work', '1000001'],
  ]) await assert.rejects(researchOrCommitOrReview(ctx, 'research', params));
  await assert.rejects(researchOrCommitOrReview(ctx, 'research',
    ['whole_combat', '20000', 'work', '1000000', 'potions', '2']),
  /Held potions are already included; research has no potions argument/);
});

test('strategic preview exposes changed permanent inventory without querying or publishing', () => {
  const original = { domain: 'strategic', observation: {
    cards: [{ card: 'Strike', upgrades: 0, count: 5 }, { card: 'Defend', upgrades: 0, count: 4 }],
    relics: [{ relic: 'BurningBlood' }],
  } };
  const endpoint = observation => ({ kind: 'decision', observation });
  assert.equal(previewInventoryChanges(original, endpoint(original)), '');
  assert.equal(previewInventoryChanges(original, endpoint({ domain: 'strategic', observation: {
    cards: [{ card: 'Strike', upgrades: 0, count: 4 }, { card: 'Defend', upgrades: 0, count: 4 },
      { card: 'Havoc', upgrades: 0, count: 1 }],
    relics: [{ relic: 'SacredBark' }],
  } })), 'Preview inventory changes:\n  Deck: -1 Strike, +1 Havoc\n  Relics: -1 BurningBlood, +1 SacredBark\n');
  assert.equal(previewInventoryChanges(original, { kind: 'outcome' }), '');
  assert.equal(previewInventoryChanges(original, endpoint({ domain: 'combat' })), '');
});

test('plans full expands the saved report without starting research', async () => {
  const calls = [];
  let output = '';
  const ctx = { write: text => { output += text; }, game: {
    async decision() { return { control: 'combat' }; },
    async plans(options) { calls.push(options); return { view: options?.diagnostics ? 'Full plans' : 'Plans' }; },
    async research() { assert.fail('inspection must not search'); },
  } };
  await researchOrCommitOrReview(ctx, 'plans', []);
  await researchOrCommitOrReview(ctx, 'plans', ['full']);
  assert.deepEqual(calls, [undefined, { diagnostics: true }]);
  assert.equal(output, 'Plans\nFull plans\n');
});

test('default research runs one explicit budget even on Unknown', async () => {
  const calls = [];
  await researchOrCommitOrReview({ game: {
    async decision() { return { control: 'combat' }; }, async research(options) {
    calls.push(options); return { candidates: [], view: 'Unknown' };
  } } }, 'research', []);
  assert.deepEqual(calls, [{ effort: 'whole_combat', maxSearchMs: 2000 }]);
});

test('strategic multi-selection prints the act command that executes its bound members', async () => {
  let output = '';
  const reading = { control: 'strategic',
    view: 'Selection | Remove from deck | Choose 2\n  Input: ./p act select:<card tokens joined by commas without spaces>\n  Parasite+0#0  |  Parasite\n  Strike+0#0  |  Strike',
    selections: [{ min: 2, max: 2, members: [] }],
  };
  printReading(reading, { write: text => { output += text; } });
  assert.match(output, /  Input: \.\/p act select:<card tokens joined by commas without spaces>/);
  const calls = [];
  await actOrPreview({ game: { async decision() { return reading; }, async act(tokens) {
    calls.push(tokens); return { view: 'Act 2 Map' };
  } }, async printCurrentReading() {}, write() {} }, 'act', ['select', 'Parasite+0#0', 'Strike+0#0']);
  assert.deepEqual(calls, [['select:Parasite+0#0,Strike+0#0']]);
  reading.selections[0].min = 0;
  reading.view += '\n  ./p act select:none  |  choose none';
  output = '';
  printReading(reading, { write: text => { output += text; } });
  assert.match(output, /  \.\/p act select:none  \|  choose none/);
  output = '';
  reading.view = 'Selection | Remove from deck | Choose 0..2\n  Input: select:<card tokens joined by commas without spaces>\n  select:none  |  choose none';
  printReading(reading, { write: text => { output += text; } });
  assert.doesNotMatch(output, /\.\/p/, 'hypothetical selection must not advertise live commands');
});

test('whole combat can explicitly request independent time and work budgets', async () => {
  const calls = [];
  let output = '';
  const ctx = { write: text => { output += text; }, game: {
    async decision() { return { control: 'combat' }; },
    async research(options) { calls.push(options); return { view: 'Work limit: 100000 transitions' }; },
  } };
  await researchOrCommitOrReview(ctx, 'research', ['whole_combat', '15000', 'work', '100000']);
  await researchOrCommitOrReview(ctx, 'research', ['15000', 'work', '100000']);
  await researchOrCommitOrReview(ctx, 'research', ['5000']);
  assert.deepEqual(calls, [
    { effort: 'whole_combat', maxSearchMs: 15000, maxGenerationWork: 100000 },
    { effort: 'whole_combat', maxSearchMs: 15000, maxGenerationWork: 100000 },
    { effort: 'whole_combat', maxSearchMs: 5000 },
  ]);
  assert.match(output, /Work limit: 100000/);
});

test('preview uses no act or undo, even when rendering fails', async () => {
  const calls = [];
  const ctx = { game: {
    async decision() { return { summary: 'Hand: Strike[1c]\n', view: 'Energy 3\nHand: Strike[1c]' }; },
    async preview(line) {
      calls.push(line);
      return { get view() { throw new Error('render failed'); } };
    },
    async act() { assert.fail('preview must never publish'); },
    async close() { assert.fail('preview must keep the connection'); },
  }, runEngine() { assert.fail('preview must never undo'); } };
  await assert.rejects(actOrPreview(ctx, 'preview', ['end']), /render failed/);
  assert.deepEqual(calls, [['end']]);
});

test('engine receives draw-dependent lines and intentional unspent energy', async () => {
  const calls = [];
  const ctx = { game: {
    async decision() { return { control: 'combat', view: 'Energy 3\nHand: Offering[0c]' }; },
    async act(line) { calls.push(line); return { view: 'Rewards', control: 'strategic' }; },
  }, async printCurrentReading() {} };
  await actOrPreview(ctx, 'act', ['play:Offering', 'play:Strike', 'end']);
  await actOrPreview(ctx, 'act', ['end']);
  assert.deepEqual(calls, [['play:Offering', 'play:Strike', 'end'], ['end']]);
});

test('act and preview preserve legality, transport and genuine ambiguity errors without spelling retries', async () => {
  const failures = [
    ['play:Defend', 'exact run-play prepared line could not resolve step 1: semantic action play:Defend has no legal match at Combat (1 candidate(s))'],
    ['play:Defend#0', 'play:Defend#0 selects hand[1] Defend+0 with cost 1, but it has no legal play at energy 0; inspect the current candidates instead of renumbering another copy'],
    ['play:Bash+@1', 'exact run-play probe could not resolve step 1: semantic action play:Bash has no legal match at Combat (9 candidate(s))'],
    ['play:Bash+#0@1', 'play:Bash occurrence #0 is absent from the visible hand; matched 0 hand card(s). Occurrences are relative to each current successor; for a transactional line of publicly equivalent copies, repeat play:Bash+@1 without #<occurrence> so every step resolves against its own successor'],
    ['play:Strike@0', 'play:"Strike" requires a combat decision'],
    ['play:Strike', 'play:Strike is ambiguous at Combat; use one exact visible-hand action: play:Strike+0#0@0 [candidate=1 hand=0 cost=1], play:Strike+#0@0 [candidate=2 hand=1 cost=1]'],
    ['play:Strike', 'Run-play response timed out; outcome uncertain, inspect the saved session before retrying'],
  ];
  for (const command of ['act', 'preview']) {
    for (const [action, message] of failures) {
      const calls = [];
      const failure = new Error(message);
      const ctx = { game: {
        async decision() { return { summary: 'Current combat\n', view: 'Energy 3\nHand: Strike[1c], Strike+[1c]\nM0 Jaw Worm: HP 20/46' }; },
        async act(line) { assert.equal(command, 'act'); calls.push(line); throw failure; },
        async preview(line) { assert.equal(command, 'preview'); calls.push(line); throw failure; },
      } };
      await assert.rejects(actOrPreview(ctx, command, [action]), error => error === failure);
      assert.deepEqual(calls, [[action]], `${command}: ${message}`);
    }
  }
});

test('combat-ended suffix trimming retries only the validated prefix through the same executor', async () => {
  for (const command of ['act', 'preview']) {
    const calls = [];
    const execute = async line => {
      calls.push(line);
      if (line.length > 1) throw new Error('exact run-play prepared line could not resolve step 2: play:"Defend" requires a combat decision');
      return { view: 'Rewards' };
    };
    const ctx = { game: {
      async decision() { return { summary: 'Hand: Strike[1c], Defend[1c]\n', view: 'Hand: Strike[1c], Defend[1c]' }; },
      async act(line) { assert.equal(command, 'act'); return execute(line); },
      async preview(line) { assert.equal(command, 'preview'); return execute(line); },
    }, async printCurrentReading() {} };
    await actOrPreview(ctx, command, ['play:Strike@0', 'play:Defend', 'end']);
    assert.deepEqual(calls, [['play:Strike@0', 'play:Defend', 'end'], ['play:Strike@0']]);
  }
});

test('choice-resolution failures state atomic outcomes without asserting outcomes for transport failures', async () => {
  for (const command of ['act', 'preview']) {
    const error = new Error(`exact run-play ${command === 'act' ? 'prepared line' : 'probe'} could not resolve step 2: a combat choice is pending`);
    const ctx = { write() {}, game: {
      async decision() { return { summary: 'Actual hand\n', view: 'Actual hand' }; },
      async act() { throw error; }, async preview() { throw error; },
    } };
    await assert.rejects(actOrPreview(ctx, command, ['play:Headbutt@0', 'play:Strike@0']), thrown => {
      assert.equal(thrown, error);
      assert.match(thrown.message, command === 'act' ? /this act line was not applied/ : /actual position unchanged/);
      return true;
    });
    const timeout = new Error('Run-play response timed out; outcome uncertain');
    ctx.game.act = async () => { throw timeout; };
    ctx.game.preview = async () => { throw timeout; };
    await assert.rejects(actOrPreview(ctx, command, ['end']), thrown => {
      assert.equal(thrown.message, 'Run-play response timed out; outcome uncertain');
      return true;
    });
  }
});

test('preview separates the source-rendered live summary from hypothetical results in either language', async () => {
  for (const summary of [
    'Act 4 F56 | Combat\nTurn 2 | Energy 4\nHand: Defend[1c]\n',
    '第四幕 F56 | 战斗\n回合 2 | 能量 4\n手牌：防御[1费]\n',
    '战斗来源未知 | HP 35/80\n等待逃跑结算\n',
    'Act 1 F0 | HP 80/80 | Neow\n',
  ]) {
    const before = { summary, view: 'Act 99 | Block 999\nHand: misleading display text' };
    const after = { view: 'Act 4 F56 | Combat\nTurn 3 | Energy 4\nHand: Flex[0c]' };
    let output = '', previews = 0;
    await actOrPreview({ game: {
      async decision() { return before; },
      async preview() { previews++; return after; },
      async act() { assert.fail('preview must stay private'); },
      async observe() { assert.fail('the summary must come from the reading itself'); },
    }, write: text => { output += text; } }, 'preview', ['end']);
    assert.equal(previews, 1);
    assert.ok(output.includes(`Actual position:\n${summary.trimEnd()}\n\n`));
    assert.match(output, /Private result[^]*Turn 3[^]*Hand: Flex/);
    assert.doesNotMatch(output, /misleading|999/);
  }
});

test('a preview without its source summary fails before private execution', async () => {
  await assert.rejects(actOrPreview({ game: {
    async decision() { return { view: 'Hand: not a summary' }; },
    async preview() { assert.fail('missing source data must fail before preview'); },
  } }, 'preview', ['end']), /no public position summary/);
});

test('board uses public block and reuses the same typed document for its reading', async () => {
  const { reading, doc } = combatFixture([], [
    { slot: 0, alive: true, intent: { preview_damage_per_hit: 12 } },
  ], [], '当前格挡 7\nBlock 999 is descriptive text\n');
  let output = '', queries = 0;
  await board({ game: { async decision(options) {
    assert.deepEqual(options, { details: ['brief'] }); return reading;
  } }, async decisionDocument(current) {
    assert.equal(current, reading); queries++; return doc;
  }, async printCurrentReading(current, options) {
    assert.equal(current, reading); assert.equal(options.document, doc);
    output += current.view;
  }, write(text) { output += text; } });
  assert.equal(queries, 1);
  assert.match(output, /incoming next enemy turn: 12 \| current block 7\n/);
  assert.doesNotMatch(output, /current block 999/);
});

test('partial preview labels the private boundary and never offers to adopt the entire request', async () => {
  for (const stop of [
    { kind: 'unresolved', requested_step: 2, unapplied_actions: 1, error: 'absent card' },
    { kind: 'terminal', unapplied_actions: 1 },
    { kind: 'terminal', unapplied_actions: 0 },
  ]) {
    let output = '';
    await actOrPreview({ game: {
      async decision() { return { summary: 'Turn 1 | Energy 3\nHand: Defend[1c]\n', view: 'Turn 1 | Energy 3\nHand: Defend[1c]' }; },
      async preview() { return { view: 'Private result', stop }; },
      async act() { assert.fail('inspection must stay private'); },
    }, write: text => { output += text; } }, 'preview', ['end', 'play:Defend']);
    assert.match(output, /current position unchanged/);
    assert.doesNotMatch(output, /adopt the tested sequence/);
    assert.match(output, stop.kind === 'unresolved' ? /repair the failed action and suffix/ : /Private terminal reached/);
  }
});
