import test from 'node:test';
import assert from 'node:assert/strict';
import { preparationReading } from './preparation.mjs';
import { preparationCardExpertGuidance } from './strategy.mjs';
import { preparationCardChoice, preparationCardChoiceRoute, renderPreparationCardChoice,
  bindPreparationCardChoice, adoptPreparationCardChoice, MAX_CARD_CHOICE_OPTIONS,
  TARGET_CARD_CHOICE_INPUT_BYTES, CARD_CHOICE_AUTOMATIC_GUIDE } from './card-choice.mjs';

function card(id, type, cost, extra = {}) {
  return { card: { card: id, upgrades: 0, count: 1, misc_value: 0, cost_modifier: 0,
    base_damage_override: null, base_block_override: null }, persistent_cost: cost, persistent_damage: 0,
    persistent_block: extra.block ?? 0, mechanics: { card: id, upgrades: 0, card_type: type, cost,
      draw_cards: 0, exhausts: false, ethereal: false, ...extra } };
}

function window(owned, offers) {
  const occurrences = new Map();
  const slots = offers.map(row => {
    const occurrence = occurrences.get(row.card.card) ?? 0;
    occurrences.set(row.card.card, occurrence + 1);
    return { row, command: `take:${row.card.card}#${occurrence}` };
  });
  const actions = slots.map(({ row, command }) => ({ semantic_action: command, candidate: { action: {
    kind: 'take_card', card: row.card.card, upgrades: row.card.upgrades } } }));
  actions.push({ semantic_action: 'skip-card#0', candidate: { action: { kind: 'skip_card_reward' } } });
  return { schema: 'sts-run-play-agent-observation-v3', session_id: 'choice-unit', head_revision: 1, state_revision: 1,
    position: { kind: 'decision', public_snapshot_id: 'choice-root', actions,
      observation: { domain: 'strategic', observation: { context: { kind: 'card_reward' },
        run: { act: 2, floor: 21, current_hp: 50, max_hp: 80 }, base_energy_per_turn: 3,
        cards: owned.map(row => row.card) } } },
    deck_evidence: { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: 'choice-root', cards: owned },
    decision_evidence: { kind: 'card_acquisition_options', candidates: slots.map(({ row, command }) => ({
      semantic_action: command, cards: [{ current: row.mechanics }] })) },
    candidateReadings: Object.fromEntries(slots.map(({ row, command }) => [command,
      `  Card ${row.card.card} | ${row.mechanics.card_type} | ${row.persistent_cost}c\n    Printed rule (base/upgraded): supplied ${row.card.card} rule.\n    At this upgrade, printed lifecycle: repeated explanation.`])) };
}

const growth = () => card('DemonForm', 'Power', 3, { strength_gain: 2, strength_gain_delivery: 'each_player_turn_start_post_draw' });
const pact = () => card('BurningPact', 'Skill', 1, { draw_cards: 2, exhausts_other_cards: true });
const sentinel = () => card('Sentinel', 'Skill', 1, { block: 5, base_block_delivery: 'once_on_play', energy_gain_on_exhaust: 2 });

function freeCardEvent(input) {
  const offers = input.position.actions.filter(row => row.candidate.action.kind === 'take_card');
  input.position.observation.observation.context = { kind: 'event', event: 'TheLibrary' };
  input.position.actions = offers.map((row, index) => {
    const command = `event:TheLibrary:${index}`;
    const evidence = input.decision_evidence.candidates.find(candidate => candidate.semantic_action === row.semantic_action);
    evidence.semantic_action = command;
    input.candidateReadings[command] = input.candidateReadings[row.semantic_action];
    return { semantic_action: command, candidate: { action: { kind: 'choose_event_option',
      public_semantics: { hp_delta: 0, max_hp_delta: 0, gold_delta_min: 0, gold_delta_max: 0,
        starts_combat: false, random_outcome: false, structured_semantics: { effects: [
          { effect: 'obtain_card', count: 1, pool: { kind: 'specific', card: row.candidate.action.card } },
        ] } } } } };
  });
  return input;
}

test('an offered Exhaust route stays outside owned Sentinel support; acquiring it changes that fact', () => {
  const input = window([growth()], [pact(), sentinel()]);
  const before = preparationCardChoice(input);
  assert.deepEqual(before.defaultCommands, ['take:BurningPact#0']);
  assert.match(before.candidates.find(row => row.card === 'Sentinel').facts.join(' '), /On play: 5 Block.*Owned trigger routes: none/);
  const after = preparationCardChoice(window([growth(), pact()], [sentinel(), pact()]));
  assert.match(after.candidates.find(row => row.card === 'Sentinel').facts.join(' '), /Owned trigger routes: 1x BurningPact/);
});

test('candidate order changes presentation but leaves host facts and policy defaults unchanged', () => {
  const original = window([growth()], [pact(), sentinel()]);
  const reversed = structuredClone(original);
  reversed.position.actions.reverse();
  const before = preparationCardChoice(original), after = preparationCardChoice(reversed);
  assert.deepEqual(after.rules, before.rules);
  assert.deepEqual(after.ownedFacts, before.ownedFacts);
  assert.deepEqual(after.candidates.map(row => row.command).sort(), before.candidates.map(row => row.command).sort());
  const text = renderPreparationCardChoice(after);
  assert.match(text, /supplied BurningPact rule/);
  assert.doesNotMatch(text, /supplied Sentinel rule|skip-card#0/);
  assert.doesNotMatch(text, /Owned permanent deck/);
  assert.equal(after.candidates.length, 3, 'the complete legal set remains in the host frame');
  assert.doesNotMatch(text, /repeated explanation|high\/medium|direct\/sequence/);
  assert.doesNotMatch(text, /Default commands:/);
});

test('a paid standalone exhaust is not a route exhausting Sentinel; Corruption deployment is a route', () => {
  const self = card('Apparition', 'Skill', 1, { exhausts: true });
  const before = preparationCardChoice(window([growth(), self], [sentinel(), pact()]));
  assert.match(before.candidates.find(row => row.card === 'Sentinel').facts.join(' '), /Owned trigger routes: none/);
  const corruption = card('Corruption', 'Power', 3);
  const after = preparationCardChoice(window([growth(), corruption], [sentinel(), pact()]));
  assert.match(after.candidates.find(row => row.card === 'Sentinel').facts.join(' '), /1x Corruption after deployment/);
});

test('Status generated in Hand has a checked later draw route, independent of offered generators', () => {
  const powerThrough = card('PowerThrough', 'Skill', 1, { status_injections: [
    { card: 'Wound', count: 2, destination: 'hand', persistence: 'persistent' } ] });
  const fire = card('FireBreathing', 'Power', 1);
  const input = window([growth(), powerThrough], [fire, pact()]);
  const text = preparationCardChoice(input).candidates.find(row => row.card === 'FireBreathing').facts.join(' ');
  assert.match(text, /Hand -> ordinary end-turn discard -> Discard -> reshuffle -> Draw -> draw into Hand/);
  const absent = preparationCardChoice(window([growth()], [powerThrough, fire, pact()]));
  assert.match(absent.candidates.find(row => row.card === 'FireBreathing').facts.join(' '), /Owned generated Status routes: none/);
});

test('new state and unsupported composite spending cannot inherit a free card decision', () => {
  const input = window([growth()], [pact(), sentinel()]);
  const frame = preparationCardChoice(input);
  assert.equal(bindPreparationCardChoice('take:BurningPact#0', frame, input).followsDefault, true);
  assert.throws(() => bindPreparationCardChoice('skip-card#0', frame, input), /policy comparison option/);
  assert.throws(() => bindPreparationCardChoice('take:BurningPact#0, leave', frame, input), /one current command/);
  assert.throws(() => bindPreparationCardChoice('take:BurningPact#0', frame, { ...input, state_revision: 2 }), /source changed/);
  const mixed = structuredClone(input);
  mixed.position.actions.push({ semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } });
  assert.equal(preparationCardChoice(mixed), undefined);
  input.deck_evidence.public_snapshot_id = 'old-root';
  assert.throws(() => preparationCardChoice(input), /bound to the current public inventory/);
});

test('a missing strategy policy pauses adoption instead of assigning a fabricated recommendation', () => {
  const input = freeCardEvent(window([card('Strike', 'Attack', 1)], [sentinel()]));
  const frame = preparationCardChoice(input);
  assert.deepEqual(frame.rules, []);
  assert.match(renderPreparationCardChoice(frame), /needs a supplied strategy policy/);
  assert.throws(() => bindPreparationCardChoice('event:TheLibrary:0', frame, input), /needs a supplied strategy policy/);
});

function attack(id, hits = 1, targets = 'selected_enemy') {
  return card(id, 'Attack', 1, { damage: { delivery: { kind: 'attack', amount: { kind: 'base' },
    hits: { kind: 'fixed', count: hits }, targets } } });
}
const established = () => [growth(), attack('SwordBoomerang', 3, 'random_enemy_per_hit'),
  attack('Anger'), attack('Rampage'), card('PerfectedStrike', 'Attack', 2), card('AscendersBane', 'Curse', -2)];
const disarm = () => card('Disarm', 'Skill', 1, { enemy_strength_down: 2,
  enemy_strength_restoration: 'no_scheduled_restoration', exhausts: true });

function bowl(input, hpGain = 2) {
  input.position.actions.push({ semantic_action: 'singing-bowl#0', candidate: { action: { kind: 'singing_bowl' } } });
  input.decision_evidence.resource_options = [{ semantic_action: 'singing-bowl#0', hp_gain: hpGain,
    max_hp_gain: 2, native_rule: 'Gain 2 Max HP and request 2 HP healing, subject to healing restrictions.' }];
  return input;
}

test('Singing Bowl joins a useful card comparison with native HP deltas and keeps all legal options', () => {
  const input = bowl(window(established(), [disarm(), attack('TwinStrike', 2)]));
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'model');
  assert.deepEqual(frame.route.commands, ['take:Disarm#0', 'singing-bowl#0']);
  assert.deepEqual(frame.defaultCommands, ['take:Disarm#0']);
  assert.equal(frame.candidates.length, 4);
  const rendered = renderPreparationCardChoice(frame);
  assert.match(rendered, /HP 50->52; Max HP 80->82/);
  assert.match(rendered, /Is the supplied card function worth/);
  assert.doesNotMatch(rendered, /take:TwinStrike#0|skip-card#0|supplied TwinStrike rule/);
  assert.equal(bindPreparationCardChoice('singing-bowl#0', frame, input).followsDefault, false);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands.sort(), frame.route.commands.sort());
});

test('covered rewards adopt Bowl instead of ordinary skip even when native healing is blocked', () => {
  const input = bowl(window(established(), [attack('TwinStrike', 2)]), 0);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['singing-bowl#0']);
  assert.match(renderPreparationCardChoice(frame), /HP 50->50; Max HP 80->82/);
  assert.equal(bindPreparationCardChoice('singing-bowl#0', frame, input, 'host-card-policy').followsDefault, true);
  assert.throws(() => bindPreparationCardChoice('singing-bowl#0', frame, { ...input, state_revision: 2 }), /source changed/);
});

test('Bowl facts and strategy gaps remain explicit instead of falling back to the full table', () => {
  const input = bowl(window(established(), [card('Feed', 'Attack', 1)]));
  assert.equal(preparationCardChoice(input).route.kind, 'review');
  for (const invalid of [undefined, [], [{ semantic_action: 'old-bowl#0', hp_gain: 2, max_hp_gain: 2 }]]) {
    input.decision_evidence.resource_options = invalid;
    assert.throws(() => preparationCardChoice(input), /matching native immediate resource outcome/);
  }
});

test('a full potion belt cannot force a supported Bowl reward back into classification', () => {
  const input = bowl(window(established(), [disarm()]));
  input.position.observation.observation.potions = [{ potion: { potion: 'BlockPotion' } }, { potion: { potion: 'PowerPotion' } }];
  input.position.actions.unshift({ semantic_action: 'discard-potion:BlockPotion#0',
    candidate: { action: { kind: 'discard_run_potion' } } });
  const scoped = preparationReading(input);
  const frame = preparationCardChoice(scoped);
  assert.equal(frame.route.kind, 'model');
  assert.deepEqual(frame.route.commands, ['take:Disarm#0', 'singing-bowl#0']);
  assert.equal(frame.candidates.length, 3);
  assert.equal(input.position.actions.length, 4, 'the actual native action inventory stays intact');
});

const headbutt = () => { const row = attack('Headbutt'); row.persistent_damage = row.mechanics.damage.damage_per_hit = 9;
  row.mechanics.tags = []; return row; };
const wildStrike = () => { const row = attack('WildStrike'); row.persistent_damage = row.mechanics.damage.damage_per_hit = 12;
  row.mechanics.tags = ['Strike'];
  row.mechanics.status_injections = [{ card: 'Wound', count: 1, destination: 'draw_pile', persistence: 'persistent' }];
  return row; };
function earlyRetrievalDeck() {
  const strike = attack('Strike'); strike.card.count = 5;
  const defend = card('Defend', 'Skill', 1, { block: 5, base_block_delivery: 'once_on_play' }); defend.card.count = 4;
  const pommel = attack('PommelStrike'); pommel.persistent_damage = pommel.mechanics.damage.damage_per_hit = 9;
  pommel.mechanics.draw_cards = 1;
  const perfected = card('PerfectedStrike', 'Attack', 2, { damage: { damage_per_hit: 6, delivery: {
    kind: 'attack', amount: { kind: 'base_plus_strike_cards', per_card: 2 },
    hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } } }); perfected.persistent_damage = 6;
  const wave = card('IronWave', 'Attack', 1, { block: 5, base_block_delivery: 'once_on_play' });
  return [strike, defend, pommel, perfected, attack('ThunderClap', 1, 'each_enemy'), wave, pact()]
    .map(row => { row.mechanics.tags = ['Strike', 'PommelStrike', 'PerfectedStrike'].includes(row.card.card) ? ['Strike'] : []; return row; });
}
function earlyReward(owned = earlyRetrievalDeck(), offers = [card('Barricade', 'Power', 3), wildStrike(), headbutt()]) {
  const input = window(owned, offers); input.position.observation.observation.run.act = 1; return input;
}

test('early retrieval reuses current strong output and accounts for the competing Wound and retention investments', () => {
  const input = earlyReward();
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['take:Headbutt#0']);
  assert.equal(frame.candidates.length, 4);
  assert.match(frame.rules[0].assessments.find(row => row.card === 'Headbutt').facts.join(' '), /6 base damage per hit \+ 2 x 7 original Strike-tagged copies/);
  assert.match(frame.rules[0].assessments.find(row => row.card === 'WildStrike').facts.join(' '), /summed across original copies: 2.*Wound into Draw/);
  assert.match(renderPreparationCardChoice(frame), /after the useful target enters Discard.*later turn/);
  assert.ok(frame.inputBudget.bytes < TARGET_CARD_CHOICE_INPUT_BYTES);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, frame.route.commands);
});

test('early retrieval conditions generalize to strong fixed output and pause when access or a real discard target is absent', () => {
  const originals = earlyRetrievalDeck();
  const carnage = attack('Carnage'); carnage.persistent_cost = carnage.mechanics.cost = 2;
  carnage.persistent_damage = carnage.mechanics.damage.damage_per_hit = 20;
  assert.deepEqual(preparationCardChoice(earlyReward([...originals.filter(row => row.card.card !== 'PerfectedStrike'), carnage], [headbutt()])).route.commands, ['take:Headbutt#0']);
  for (const rows of [
    originals.filter(row => row.card.card !== 'PerfectedStrike'),
    originals.filter(row => !row.mechanics.draw_cards),
    [...originals, headbutt()],
    originals.map(row => row.card.card === 'PerfectedStrike' ? { ...row, mechanics: { ...row.mechanics, exhausts: true } } : row),
  ]) assert.equal(preparationCardChoice(earlyReward(rows, [headbutt()])).route.kind, 'review');
});

test('supported retention and Status investments retain their own comparison instead of inheriting the basic-foundation default', () => {
  const originals = earlyRetrievalDeck();
  for (const rows of [
    [...originals, card('PowerThrough', 'Skill', 1, { block: 15, base_block_delivery: 'once_on_play' })],
    [...originals, card('Evolve', 'Power', 1)],
    originals.map(row => row.card.card === 'PerfectedStrike' ? { ...row, card: { ...row.card, count: 2 } } : row),
  ]) assert.equal(preparationCardChoice(earlyReward(rows)).route.kind, 'review');
  const upgraded = wildStrike(); upgraded.card.upgrades = upgraded.mechanics.upgrades = 1;
  upgraded.persistent_damage = upgraded.mechanics.damage.damage_per_hit = 17;
  assert.equal(preparationCardChoice(earlyReward(originals, [headbutt(), upgraded])).route.kind, 'review');
  assert.equal(preparationCardChoice(earlyReward(originals, [card('Barricade', 'Power', 3)])).route.kind, 'review',
    'a future investment without a supplied useful competing function stays reviewable');
  const noTags = earlyReward(); delete noTags.deck_evidence.cards[0].mechanics.tags;
  assert.equal(preparationCardChoice(noTags).route.kind, 'review');
});

function upgradePreview(source, { cost = source.persistent_cost, damage = source.persistent_damage,
  block = source.persistent_block, draws } = {}) {
  return { ...source, next_upgrade: { persistent_cost: cost, persistent_damage: damage, persistent_block: block,
    definition: { card: source.card.card, upgrades: source.card.upgrades + 1,
      reviewed_effects: { ...(draws !== undefined ? { unconditional_draw_request: draws } : {}) } } } };
}
function earlyUpgradeDeck() {
  return [...earlyRetrievalDeck(), headbutt()].map(source => {
    if (source.card.card === 'Strike') return upgradePreview(source, { damage: source.persistent_damage + 3 });
    if (source.card.card === 'Defend') return upgradePreview(source, { block: source.persistent_block + 3 });
    if (source.card.card === 'PommelStrike' || source.card.card === 'BurningPact') {
      return upgradePreview(source, { draws: source.mechanics.draw_cards + 1 });
    }
    return { ...source, next_upgrade: null };
  });
}
const wholeHandUpgrade = () => { const row = card('Armaments', 'Skill', 1, { block: 5,
  base_block_delivery: 'once_on_play', combat_upgrade_scope: 'whole_hand', combat_upgrade_persistence: 'combat_only' });
  row.card.upgrades = row.mechanics.upgrades = 1; return row; };
const cleave = () => { const row = attack('Cleave', 1, 'each_enemy'); row.persistent_damage = row.mechanics.damage.damage_per_hit = 8; return row; };
const hemokinesis = () => { const row = attack('Hemokinesis'); row.persistent_damage = row.mechanics.damage.damage_per_hit = 15;
  row.mechanics.self_hp_loss_on_play = { base_hp_loss: 2 }; return row; };

const earlyUppercut = () => { const row = card('Uppercut', 'Attack', 2, { weak: 1, vulnerable: 1,
  play_target: 'one_enemy', damage: { damage_per_hit: 13, delivery: { kind: 'attack',
    amount: { kind: 'base' }, hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } } });
  row.persistent_damage = 13; return row; };
const exhaustBlock = () => card('FeelNoPain', 'Power', 1, { block: 0, exhaust_block_power: 3 });
const earlyFlex = () => card('Flex', 'Skill', 0, { temporary_strength_gain: 4 });
function earlyProtectionDeck() {
  return [...earlyRetrievalDeck(), wholeHandUpgrade(), headbutt(),
    card('AscendersBane', 'Curse', -2, { ethereal: true })].map(source => {
    if (source.card.card !== 'IronWave') return source;
    return { ...source, mechanics: { ...source.mechanics, damage: attack('IronWave').mechanics.damage } };
  });
}

const bodySlam = () => card('BodySlam', 'Attack', 1, { damage: { damage_per_hit: 0,
  delivery: { kind: 'attack', amount: { kind: 'player_block' },
    hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } } });
const combust = () => card('Combust', 'Power', 1, { end_of_turn_hp_loss_and_damage: {
  damage_per_enemy_per_application: 5, hp_loss_per_application: 1 } });
const boomerang = () => { const row = attack('SwordBoomerang', 3, 'random_enemy_per_hit');
  row.persistent_damage = row.mechanics.damage.damage_per_hit = 3; return row; };
function earlyAreaReward(owned = earlyProtectionDeck(), offers = [bodySlam(), combust(), boomerang()]) {
  const tagged = owned.map(source => ({ ...source,
    mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } }));
  const input = earlyReward(tagged, offers);
  input.position.observation.observation.run.current_hp = 59;
  return input;
}

test('a first recurring area function replaces covered random hits and small Block conversion independently of offer order', () => {
  const offers = [bodySlam(), combust(), boomerang()];
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const input = earlyAreaReward(earlyProtectionDeck(), order.map(index => offers[index]));
    const frame = preparationCardChoice(input);
    assert.equal(frame.route.kind, 'automatic', JSON.stringify(frame.route));
    assert.deepEqual(frame.route.commands, ['take:Combust#0']);
    assert.equal(frame.candidates.length, 4);
    assert.equal(bindPreparationCardChoice('take:Combust#0', frame, input, 'host-card-policy').selected.kind, 'gain');
    const reading = renderPreparationCardChoice(frame);
    assert.match(reading, /each actual player turn-end trigger loses 1 HP before requesting 5 non-Attack damage to each enemy/);
    assert.match(reading, /Power stays deployed; the played copy leaves ordinary circulation/);
    assert.match(reading, /current play costs 1 Energy after paying the Block sources/);
    assert.match(reading, /random targets; its hits share one card play/);
    assert.ok(frame.inputBudget.bytes < TARGET_CARD_CHOICE_INPUT_BYTES);
  }
});

test('area and conversion defaults preserve changed HP reserves, usable growth, larger Block and area coverage for review', () => {
  const originals = earlyProtectionDeck();
  for (const support of [
    growth(), card('Flex', 'Skill', 0, { strength_gain: 2, temporary_strength_gain: 2 }),
    card('Rupture', 'Power', 1), card('PowerThrough', 'Skill', 1, { block: 15, base_block_delivery: 'once_on_play' }),
    card('Rage', 'Skill', 0), card('Metallicize', 'Power', 1, { end_of_turn_block_each_turn: { block: 3 } }),
    exhaustBlock(), card('Corruption', 'Power', 3), card('Barricade', 'Power', 3),
    card('SeeingRed', 'Skill', 1, { energy_gain_rule: { delivery: 'on_play', amount: 2 } }),
    cleave(), combust(), card('FireBreathing', 'Power', 1),
  ]) assert.equal(preparationCardChoice(earlyAreaReward([...originals, support])).route.kind, 'review', support.card.card);
  for (const hp of [53, 1]) {
    const input = earlyAreaReward(); input.position.observation.observation.run.current_hp = hp;
    assert.equal(preparationCardChoice(input).route.kind, 'review');
  }
  for (const modify of [
    input => { input.position.observation.observation.relics = [{ relic: 'Vajra' }]; },
    input => { input.position.observation.observation.potions = [{ potion: { potion: 'SteroidPotion' } }]; },
    input => { input.position.observation.observation.relics = [{ relic: 'SneckoEye' }]; },
    input => { delete input.decision_evidence.candidates.find(row => row.cards[0].current.card === 'Combust').cards[0].current.end_of_turn_hp_loss_and_damage; },
    input => { input.position.observation.observation.run.act = 2; },
    input => { delete input.deck_evidence.cards.find(source => source.card.card === 'Armaments').mechanics.tags; },
  ]) { const input = earlyAreaReward(); modify(input); assert.equal(preparationCardChoice(input).route.kind, 'review'); }
  const upgraded = boomerang(); upgraded.mechanics.damage.delivery.hits.count = 4;
  upgraded.card.upgrades = upgraded.mechanics.upgrades = 1;
  assert.equal(preparationCardChoice(earlyAreaReward(originals, [bodySlam(), combust(), upgraded])).route.kind, 'review');
});

const discountedAttack = () => card('BloodForBlood', 'Attack', 4, { cost_change_on_hp_loss: -1,
  damage: { damage_per_hit: 18, delivery: { kind: 'attack', amount: { kind: 'base' },
    hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } } });

const handBurst = () => { const row = card('FiendFire', 'Attack', 2, { exhausts: true, exhausts_other_cards: true,
  damage: { damage_per_hit: 7, delivery: { kind: 'attack', amount: { kind: 'base' },
    hits: { kind: 'per_hand_card_at_resolution_before_exhaust' }, targets: 'selected_enemy' } } });
  row.persistent_damage = 7; return row; };

const ordinaryTwoCost = (id, damage) => { const row = card(id, 'Attack', 2, {
  tags: id === 'PerfectedStrike' ? ['Strike'] : [], damage: { damage_per_hit: damage, hit_count: 1,
    delivery: { kind: 'attack', amount: id === 'PerfectedStrike'
      ? { kind: 'base_plus_strike_cards', per_card: 2 } : { kind: 'base' },
    hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } } });
  row.persistent_damage = damage; return row; };

function laterOutputDeck() {
  return earlyProtectionDeck().map(source => {
    if (source.card.card !== 'PerfectedStrike') return source;
    const upgraded = structuredClone(source);
    upgraded.card.upgrades = upgraded.mechanics.upgrades = 1;
    upgraded.mechanics.damage.delivery.amount.per_card = 3;
    return upgraded;
  });
}

const freeEnergy = () => card('SeeingRed', 'Skill', 0, { exhausts: true, energy_gain_rule: { delivery: 'on_play', amount: 2 } });
const temporaryFlex = () => card('Flex', 'Skill', 0, { temporary_strength_gain: 4, strength_loss_at_end_of_turn: 4 });
const randomFreeAttack = () => card('InfernalBlade', 'Skill', 0, { exhausts: true, is_random_output: true,
  random_attack_to_hand: { count: 1, cost_for_turn: 0 } });
const lastingStrength = () => card('Inflame', 'Power', 1, { strength_gain: 2, strength_gain_delivery: 'on_play' });
const copyingFreeAttack = () => card('Anger', 'Attack', 0, { copy_to_discard_after_damage: 1,
  damage: { damage_per_hit: 6, delivery: { kind: 'attack', amount: { kind: 'base' },
    hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } } });
const hpPayment = () => card('Bloodletting', 'Skill', 0, { energy_gain_rule: { delivery: 'on_play', amount: 3 },
  self_hp_loss_on_play: { base_hp_loss: 3, bypasses_block: true } });

function lastingStrengthReward() {
  const input = window([...laterOutputDeck(), handBurst(), freeEnergy(), pact()].map(source => ({ ...source,
    mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } })),
    [copyingFreeAttack(), hpPayment(), lastingStrength()]);
  input.position.observation.observation.run.current_hp = 20;
  return input;
}

test('first lasting Strength remains a stable default while HP payment and copying attacks retain their own conditions', () => {
  const input = lastingStrengthReward();
  for (const reverse of [false, true]) {
    if (reverse) input.position.actions.reverse();
    const frame = preparationCardChoice(input);
    assert.equal(frame.route.kind, 'automatic');
    assert.deepEqual(frame.route.commands, ['take:Inflame#0']);
    assert.equal(frame.candidates.length, 4);
    const rules = frame.rules.find(rule => rule.id === 'ordinary-reward-defaults');
    assert.equal(rules.assessments.find(row => row.card === 'Anger').disposition, 'preserve');
    assert.equal(rules.assessments.find(row => row.card === 'Bloodletting').disposition, 'preserve');
    assert.match(rules.conditions.join(' '), /3 HP before granting 3 Energy/);
    assert.match(rules.conditions.join(' '), /prior Inflame deployment.*per actually Exhausted Hand card/);
    assert.match(rules.conditions.join(' '), /combat-only copy to Discard/);
  }
});

test('a checked new function takes priority over optional free-Attack synergy while the standalone synergy remains a small comparison', () => {
  const input = lastingStrengthReward();
  input.position.observation.observation.relics = [{ relic: 'Kunai' }];
  input.ownedResourceReading = 'Relics (1):\n  Kunai\n    Every 3 Attack cards played in a single turn, gain 1 Dexterity. Counts cards, not hits.\n';
  for (const reverse of [false, true]) {
    if (reverse) input.position.actions.reverse();
    const frame = preparationCardChoice(input);
    assert.equal(frame.route.kind, 'automatic');
    assert.deepEqual(frame.route.commands, ['take:Inflame#0']);
    assert.deepEqual(frame.defaultCommands, ['take:Inflame#0']);
    assert.equal(frame.candidates.length, 4);
    assert.equal(frame.rules[0].assessments.find(row => row.card === 'Anger').disposition, 'compare');
    assert.equal(bindPreparationCardChoice('take:Inflame#0', frame, input, 'host-card-policy').owner, 'host-card-policy');
    assert.throws(() => bindPreparationCardChoice('take:Anger#0', frame, input), /policy comparison option/);
  }
  const solo = window(input.deck_evidence.cards, [copyingFreeAttack()]);
  solo.position.observation.observation.relics = [{ relic: 'Kunai' }];
  assert.throws(() => renderPreparationCardChoice(preparationCardChoice(solo)), /native rules of the owned supporting relics/);
  solo.ownedResourceReading = input.ownedResourceReading;
  for (const reverse of [false, true]) {
    if (reverse) solo.position.actions.reverse();
    const frame = preparationCardChoice(solo);
    assert.equal(frame.route.kind, 'model');
    assert.deepEqual(frame.route.commands.sort(), ['skip-card#0', 'take:Anger#0']);
    assert.deepEqual(frame.defaultCommands, ['skip-card#0']);
    const text = renderPreparationCardChoice(frame);
    assert.match(text, /Every 3 Attack cards.*Counts cards, not hits/);
    assert.match(text, /one combat-only copy to Discard/);
    assert.match(text, /Permanent deck size:[^\n]*\nMaintainer default: skip-card#0\./);
    assert.equal(text.split('Maintainer default:').length - 1, 1);
    assert.ok(text.indexOf('Maintainer default: skip-card#0') < text.indexOf('Checked owned facts:'));
    assert.doesNotMatch(text, /supplied first useful function/);
    assert.doesNotMatch(text, /take:Bloodletting#0|take:Inflame#0|Choose InfernalBlade/);
    assert.ok(frame.inputBudget.bytes < TARGET_CARD_CHOICE_INPUT_BYTES);
    assert.equal(bindPreparationCardChoice('take:Anger#0', frame, solo).followsDefault, false);
  }
});

test('HP-payment reserve changes and existing Strength keep uncovered increments in review', () => {
  for (const modify of [
    input => { input.position.observation.observation.run.current_hp = 50; },
    input => { input.deck_evidence.cards.push(card('Rupture', 'Power', 1)); input.position.observation.observation.cards = input.deck_evidence.cards.map(row => row.card); },
    input => { input.position.observation.observation.relics = [{ relic: 'TungstenRod' }]; },
    input => { input.deck_evidence.cards = input.deck_evidence.cards.filter(row => row.card.card !== 'SeeingRed'); input.position.observation.observation.cards = input.deck_evidence.cards.map(row => row.card); },
    input => { input.position.observation.observation.relics = [{ relic: 'Vajra' }]; },
    input => { delete input.decision_evidence.candidates[0].cards[0].current.copy_to_discard_after_damage; },
  ]) {
    const input = lastingStrengthReward(); modify(input);
    assert.equal(preparationCardChoice(input).route.kind, 'review');
  }
});

test('first lasting Strength variants use complete native effects and resolve the larger same-cost gain', () => {
  const first = lastingStrength(), upgraded = lastingStrength();
  upgraded.card.upgrades = upgraded.mechanics.upgrades = 1;
  upgraded.mechanics.strength_gain = 3;
  const owned = laterOutputDeck().map(source => ({ ...source, mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } }));
  const input = window(owned, [first, upgraded]);
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:Inflame#1']);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:Inflame#1']);
  upgraded.mechanics.strength_gain_delivery = 'each_player_turn_start_post_draw';
  assert.equal(preparationCardChoice(window(owned, [first, upgraded])).route.kind, 'review');
});
const nonAttackClear = () => { const row = ordinaryTwoCost('SeverSoul', 16);
  row.mechanics.exhausts_other_cards = true; return row; };
const conditionalClash = () => { const row = card('Clash', 'Attack', 0, { damage: {
  damage_per_hit: 14, delivery: { kind: 'attack', amount: { kind: 'base' },
    hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy', play_condition: 'hand_contains_only_attacks' } } });
  row.persistent_damage = 14; return row; };

test('covered late output checks Wound and temporary Strength costs without assuming a future engine', () => {
  const originals = [...laterOutputDeck(), handBurst()].map(source => ({ ...source,
    mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } }));
  const offers = [temporaryFlex(), wildStrike(), randomFreeAttack()];
  for (const order of [[0, 1, 2], [2, 0, 1], [1, 2, 0]]) {
    const input = window(originals, order.map(index => offers[index]));
    const frame = preparationCardChoice(input);
    assert.deepEqual(frame.route.commands, ['skip-card#0'], JSON.stringify(frame.rules));
    const facts = frame.rules[0].conditions.join(' ');
    assert.match(facts, /4 x 7 = 28.*3 x \(7 \+ 4\) = 33.*Increment 5/);
    assert.match(facts, /Strike tag adds 3.*inserted Wound may obstruct a draw/);
    assert.match(facts, /ordinary cost after turn end/);
  }
  const upgradedBurst = handBurst();
  upgradedBurst.mechanics.tags = [];
  upgradedBurst.persistent_damage = upgradedBurst.mechanics.damage.damage_per_hit = 10;
  upgradedBurst.card.upgrades = upgradedBurst.mechanics.upgrades = 1;
  const facts = preparationCardChoice(window([...originals.filter(source => source.card.card !== 'FiendFire'), upgradedBurst], [temporaryFlex()])).rules[0].conditions.join(' ');
  assert.match(facts, /4 x 10 = 40.*3 x \(10 \+ 4\) = 42.*Increment 2/);
  const input = owned => window(owned.map(source => ({ ...source,
    mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } })), [temporaryFlex()]);
  for (const extra of [attack('TwinStrike', 2), card('LimitBreak', 'Skill', 1),
    card('DarkEmbrace', 'Power', 2, { exhaust_draw_power: 1 })]) {
    assert.equal(preparationCardChoice(input([...originals, extra])).route.kind, 'review');
  }
  const artifact = input(originals);
  artifact.position.observation.observation.relics = [{ relic: 'ClockworkSouvenir' }];
  assert.equal(preparationCardChoice(artifact).route.kind, 'review');
  assert.equal(preparationCardChoice(window([...originals, card('Evolve', 'Power', 1)], [wildStrike()])).route.kind, 'review');
});

test('first free random Attack preserves a bounded relic comparison and requires its native rule', () => {
  const originals = [...laterOutputDeck(), handBurst()].map(source => ({ ...source,
    mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } }));
  const offers = [temporaryFlex(), wildStrike(), randomFreeAttack()];
  for (const order of [[0, 1, 2], [2, 1, 0], [1, 0, 2]]) {
    const input = window(originals, order.map(index => offers[index]));
    input.position.observation.observation.relics = [{ relic: 'Kunai' }];
    const frame = preparationCardChoice(input);
    assert.equal(frame.route.kind, 'model', JSON.stringify(frame.rules));
    assert.deepEqual(frame.route.commands, ['take:InfernalBlade#0', 'skip-card#0']);
    assert.deepEqual(frame.defaultCommands, ['skip-card#0']);
    assert.throws(() => renderPreparationCardChoice(frame), /native rules of the owned supporting relics/);
    input.ownedResourceReading = 'Relics (1):\n  Kunai\n    Every time you play 3 Attacks in a single turn, gain 1 Dexterity.\n';
    const reading = renderPreparationCardChoice(preparationCardChoice(input));
    assert.match(reading, /Every time you play 3 Attacks/);
    assert.match(reading, /one actual Attack play.*each hit.*one play/);
    assert.match(reading, /generated card costs 0 this turn.*overflow.*ordinary cost after turn end/);
    assert.doesNotMatch(reading, /take:Flex|take:WildStrike/);
    assert.ok(Buffer.byteLength(reading) < TARGET_CARD_CHOICE_INPUT_BYTES);
  }
  const build = extras => window([...originals, ...extras].map(source => ({ ...source,
    mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } })), [randomFreeAttack()]);
  for (const extras of [[card('FeelNoPain', 'Power', 1, { exhaust_block_power: 3 })],
    [card('Corruption', 'Power', 3)], [card('MasterReality', 'Power', 1)]]) {
    assert.equal(preparationCardChoice(build(extras)).route.kind, 'review');
  }
  const unsupported = randomFreeAttack(); delete unsupported.mechanics.random_attack_to_hand;
  assert.equal(preparationCardChoice(window(originals, [unsupported])).route.kind, 'review');
  const duplicate = build([randomFreeAttack()]);
  duplicate.position.observation.observation.relics = [{ relic: 'Kunai' }];
  assert.deepEqual(preparationCardChoice(duplicate).route.commands, ['skip-card#0']);
});

test('paid BodySlam default is shared by covered-output rewards and shop advice', () => {
  const build = () => {
    const owned = [...laterOutputDeck(), freeEnergy(), card('FlameBarrier', 'Skill', 2,
      { block: 16, base_block_delivery: 'once_on_play' })]
      .map(source => ({ ...source, mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } }));
    const input = window(owned, [bodySlam()]);
    input.position.observation.observation.base_energy_per_turn = 4;
    return input;
  };
  const input = build(), frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['skip-card#0']);
  assert.match(renderPreparationCardChoice(frame), /FlameBarrier \+ BodySlam costs 3 Energy.*16-Block/);
  assert.match(renderPreparationCardChoice(frame), /not a turn-output bound/);
  const shop = build();
  shop.position.observation.observation.context.kind = 'shop';
  shop.position.actions[0].semantic_action = shop.decision_evidence.candidates[0].semantic_action = 'buy:BodySlam#0';
  shop.position.actions[0].candidate.action.kind = 'buy_card';
  shop.position.actions.at(-1).semantic_action = 'leave';
  shop.position.actions.at(-1).candidate.action.kind = 'leave_shop';
  const advice = preparationCardExpertGuidance(shop)[0];
  assert.match(advice.preference, /Preserve existing output access.*shops.*Gold retained/);
  assert.match(advice.change, /FlameBarrier \+ BodySlam costs 3 Energy.*combat upgrade.*shared-Hand/);
  for (const change of [
    value => { const row = value.deck_evidence.cards.at(-1); row.mechanics.block = row.persistent_block = 20; },
    value => { value.deck_evidence.cards.at(-1).persistent_cost = 0; },
    value => { const row = value.decision_evidence.candidates[0].cards[0].current; row.upgrades = 1; row.cost = 0;
      value.position.actions[0].candidate.action.upgrades = 1; },
    value => { const row = card('FeelNoPain', 'Power', 1, { exhaust_block_power: 3, tags: [] });
      value.deck_evidence.cards.push(row); value.position.observation.observation.cards.push(row.card); },
    value => { const row = card('Barricade', 'Power', 3, { tags: [] });
      value.deck_evidence.cards.push(row); value.position.observation.observation.cards.push(row.card); },
    value => { value.position.observation.observation.relics = [{ relic: 'Calipers' }]; },
  ]) { const boundary = build(); change(boundary); assert.equal(preparationCardChoice(boundary).route.kind, 'review'); }
});

test('first free Energy uses owned payment thresholds while whole-Hand Exhaust does not preserve Clash', () => {
  const originals = [...laterOutputDeck(), handBurst(), earlyUppercut()].map(source => {
    const copy = structuredClone(source);
    if (copy.card.card === 'PommelStrike') copy.mechanics.draw_cards = 2;
    if (copy.card.card === 'BurningPact') copy.mechanics.selects_from_hand = true;
    return copy;
  });
  const reward = (owned, offers) => {
    const input = window(owned.map(source => ({ ...source, mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } })), offers);
    input.position.observation.observation.base_energy_per_turn = 4;
    return input;
  };
  const offers = [freeEnergy(), nonAttackClear(), conditionalClash()];
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const frame = preparationCardChoice(reward(originals, order.map(index => offers[index])));
    assert.equal(frame.route.kind, 'automatic');
    assert.deepEqual(frame.route.commands, ['take:SeeingRed#0']);
    assert.equal(frame.candidates.length, 4);
    const reading = renderPreparationCardChoice(frame);
    assert.match(reading, /Base Energy 4->6/);
    assert.match(reading, /Owned payment bundle:.* = 5, crossing the 4-Energy baseline/);
    assert.match(reading, /reach the cards and use their actual effects in order/);
    assert.match(reading, /FiendFire clears the whole remaining Hand, including Clash itself/);
    assert.match(reading, /SeverSoul adds a repeatable 16-base-damage hit and clears only remaining non-Attacks/);
  }
  for (const input of [
    reward(originals.filter(source => source.card.card !== 'PommelStrike'), offers),
    reward(originals.filter(source => source.card.card !== 'FiendFire'), offers),
    reward([...originals, freeEnergy()], offers),
    reward([...originals, exhaustBlock()], offers),
    reward([...originals, nonAttackClear()], [conditionalClash()]),
    reward([...originals, card('Corruption', 'Power', 3)], [conditionalClash()]),
  ]) assert.equal(preparationCardChoice(input).route.kind, 'review');
  const costly = freeEnergy(); costly.persistent_cost = costly.mechanics.cost = 1;
  assert.equal(preparationCardChoice(reward(originals, [costly])).route.kind, 'review');
  const enlargedBaseline = reward(originals, [freeEnergy()]); enlargedBaseline.position.observation.observation.base_energy_per_turn = 6;
  assert.equal(preparationCardChoice(enlargedBaseline).route.kind, 'review', 'the supplied bundle must cross the actual baseline');
  assert.equal(preparationCardChoice(reward(originals, [conditionalClash()])).route.commands[0], 'skip-card#0');
});

test('covered later output can preserve slots without an owned Strength engine and includes real conditional increments', () => {
  const originals = laterOutputDeck();
  const offers = [ordinaryTwoCost('PerfectedStrike', 6), ordinaryTwoCost('SearingBlow', 12), ordinaryTwoCost('HeavyBlade', 14)];
  const reward = (owned, offered) => window(owned.map(source => ({ ...source,
    mechanics: { ...source.mechanics, tags: source.mechanics.tags ?? [] } })), offered);
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const frame = preparationCardChoice(reward(originals, order.map(index => offers[index])));
    assert.equal(frame.route.kind, 'automatic');
    assert.deepEqual(frame.route.commands, ['skip-card#0']);
    assert.equal(frame.candidates.length, 4);
    const reading = renderPreparationCardChoice(frame);
    assert.match(reading, /original pool 7->8.*6 \+ 2 x 8 = 22/);
    assert.match(reading, /PerfectedStrike\+ also gains 3 requested damage/);
    assert.match(reading, /Owned reviewed Strength support: 0/);
    assert.match(reading, /Armaments\+ can improve SearingBlow in the same actual Hand/);
    assert.match(reading, /changes last this combat/);
  }
  for (const input of [
    reward(originals.filter(source => source.card.card !== 'PerfectedStrike'), offers),
    reward([...originals, earlyFlex()], offers),
    reward([...originals, growth()], offers),
    reward([...originals, ordinaryTwoCost('SearingBlow', 12)], offers),
  ]) assert.equal(preparationCardChoice(input).route.kind, 'review');
  const warped = reward(originals, offers); warped.position.observation.observation.relics = [{ relic: 'WarpedTongs' }];
  assert.equal(preparationCardChoice(warped).route.kind, 'review');
  const changed = ordinaryTwoCost('SearingBlow', 16); changed.card.upgrades = changed.mechanics.upgrades = 1;
  assert.equal(preparationCardChoice(reward(originals, [changed])).route.kind, 'review');
  const upgraded = ordinaryTwoCost('PerfectedStrike', 6); upgraded.card.upgrades = upgraded.mechanics.upgrades = 1;
  upgraded.mechanics.damage.delivery.amount.per_card = 3;
  assert.equal(preparationCardChoice(reward(originals, [upgraded])).route.kind, 'review');
  const untagged = ordinaryTwoCost('PerfectedStrike', 6); untagged.mechanics.tags = [];
  assert.equal(preparationCardChoice(reward(originals, [untagged])).route.kind, 'review');
  for (const relic of ['SneckoEye', 'VelvetChoker']) {
    const input = reward(originals, offers); input.position.observation.observation.relics = [{ relic }];
    assert.equal(preparationCardChoice(input).route.kind, 'review');
  }
});

test('early burst, Skill payment and delayed growth compare checked owned functions across offer orders', () => {
  const originals = [...earlyProtectionDeck(), disarm()];
  const offers = [handBurst(), card('Corruption', 'Power', 3), growth()];
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const input = earlyAreaReward(originals, order.map(index => offers[index]));
    input.position.observation.observation.run.pending_act_entry = 2;
    const frame = preparationCardChoice(input);
    assert.equal(frame.route.kind, 'model');
    assert.deepEqual([...frame.route.commands].sort(), ['take:Corruption#0', 'take:DemonForm#0', 'take:FiendFire#0']);
    assert.equal(frame.candidates.length, 4);
    const reading = renderPreparationCardChoice(frame);
    assert.match(reading, /base damage is 7 x k/);
    assert.match(reading, /Use the actual Hand to count targets/);
    assert.match(reading, /Armaments\+ can upgrade FiendFire in the same actual Hand: pay 1 \+ 2 Energy/);
    assert.match(reading, /Exhausting counted cards also lowers later PerfectedStrike/);
    assert.match(reading, /Skills then cost 0 and Exhaust when played/);
    assert.match(reading, /supplies no Strength on its deployment turn/);
    assert.match(reading, /Owned Exhaust-draw Powers: 0; owned Exhaust-block Powers: 0/);
    assert.match(reading, /Owned on-play draw requests:/);
    assert.match(reading, /Comparison question:/);
  }
  for (const input of [
    earlyAreaReward(originals.filter(row => row.card.card !== 'BurningPact'), offers),
    earlyAreaReward([...originals, handBurst()], offers),
    earlyAreaReward([...originals, growth()], offers),
    earlyAreaReward([...originals, card('Corruption', 'Power', 3)], offers),
    earlyAreaReward(originals.filter(row => row.card.card !== 'Defend'), offers),
  ]) assert.equal(preparationCardChoice(input).route.kind, 'review');
  const limited = earlyAreaReward(originals, offers);
  limited.position.observation.observation.relics = [{ relic: 'VelvetChoker' }];
  assert.equal(preparationCardChoice(limited).route.kind, 'review');
  const unpaidProtection = disarm(); unpaidProtection.persistent_cost = unpaidProtection.mechanics.cost = 4;
  assert.equal(preparationCardChoice(earlyAreaReward([...earlyProtectionDeck(), unpaidProtection], offers)).route.kind, 'review');
  const changed = handBurst(); changed.mechanics.damage.delivery.hits.kind = 'fixed';
  changed.mechanics.damage.delivery.hits.count = 3;
  assert.equal(preparationCardChoice(earlyAreaReward(originals, [changed, growth()])).route.kind, 'review');
  const upgraded = handBurst(); upgraded.card.upgrades = upgraded.mechanics.upgrades = 1;
  upgraded.mechanics.damage.damage_per_hit = 10;
  assert.match(renderPreparationCardChoice(preparationCardChoice(earlyAreaReward(originals,
    [upgraded, growth()]))), /base damage is 10 x k/);
});

test('first early persistent reduction competes with the real delayed HP-loss discount using owned timing', () => {
  const offers = [earlyFlex(), disarm(), discountedAttack()];
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const input = earlyAreaReward([...earlyProtectionDeck(), combust()], order.map(index => offers[index]));
    const frame = preparationCardChoice(input);
    assert.deepEqual(frame.route.commands, ['take:Disarm#0']);
    assert.equal(frame.route.kind, 'automatic');
    assert.equal(frame.candidates.length, 4);
    const reading = renderPreparationCardChoice(frame);
    assert.match(reading, /reduce one enemy's Strength by 2 with no scheduled restoration/);
    assert.match(reading, /BloodForBlood starts at 4 Energy against 3 base Energy/);
    assert.match(reading, /Combust must deploy first.*Cost reduction follows actual positive loss/);
    assert.ok(frame.inputBudget.bytes < TARGET_CARD_CHOICE_INPUT_BYTES);
  }
  const originals = [...earlyProtectionDeck(), combust()];
  for (const input of [
    earlyAreaReward(earlyProtectionDeck(), offers),
    earlyAreaReward([...originals, disarm()], offers),
    earlyAreaReward(originals, [discountedAttack(), earlyFlex()]),
  ]) assert.equal(preparationCardChoice(input).route.kind, 'review');
  const upgraded = discountedAttack(); upgraded.mechanics.cost = upgraded.persistent_cost = 3;
  upgraded.card.upgrades = upgraded.mechanics.upgrades = 1;
  upgraded.mechanics.damage.damage_per_hit = 22;
  assert.equal(preparationCardChoice(earlyAreaReward(originals, [disarm(), upgraded])).route.kind, 'review');
});

test('early protection comparison keeps deployed Exhaust fuel and Weak timing separate across offer orders', () => {
  const offers = [earlyFlex(), earlyUppercut(), exhaustBlock()];
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const input = earlyReward(earlyProtectionDeck(), order.map(index => offers[index]));
    const frame = preparationCardChoice(input), reading = renderPreparationCardChoice(frame);
    assert.equal(frame.route.kind, 'model', JSON.stringify(frame.route));
    assert.deepEqual([...frame.route.commands].sort(), ['take:FeelNoPain#0', 'take:Uppercut#0']);
    assert.equal(frame.candidates.length, 4);
    assert.match(reading, /Default to Uppercut for the first recurring Weak/);
    assert.match(reading, /1 Weak turn\(s\).*2 Energy leaves 1 base Energy/);
    assert.match(reading, /Armaments\+ can upgrade Uppercut when both actually share Hand/);
    assert.match(reading, /3 Block per actual card Exhaust/);
    assert.match(reading, /BurningPact costs 1 Energy: select one other Hand card.*Each later use needs redraw, payment and a Hand target/);
    assert.match(reading, /AscendersBane: Ethereal Exhaust if still in Hand at turn end.*separate recovery/);
    assert.doesNotMatch(reading, /supplied Flex rule|take:Flex#0|per round|Corruption/);
    assert.ok(frame.inputBudget.bytes < TARGET_CARD_CHOICE_INPUT_BYTES);
  }
});

test('early protection policies preserve gaps for changed fuel, deployed-copy coverage and temporary Strength payoffs', () => {
  const originals = earlyProtectionDeck();
  const input = owned => earlyReward(owned, [earlyFlex(), earlyUppercut(), exhaustBlock()]);
  for (const rows of [
    originals.filter(row => row.card.card !== 'BurningPact'),
    originals.map(row => row.card.card === 'BurningPact'
      ? { ...row, mechanics: { ...row.mechanics, exhausts: true } } : row),
    [...originals, exhaustBlock()],
    [...originals, card('Intimidate', 'Skill', 0, { weak: 1, exhausts: true })],
    [...originals, attack('TwinStrike', 2)],
    [...originals, card('LimitBreak', 'Skill', 1)],
    [...originals, card('Panacea', 'Skill', 0)],
  ]) assert.equal(preparationCardChoice(input(rows)).route.kind, 'review');
  const missing = exhaustBlock(); delete missing.mechanics.exhaust_block_power;
  assert.equal(preparationCardChoice(earlyReward(originals, [earlyUppercut(), missing])).route.kind, 'review',
    'zero printed Block cannot stand in for a missing triggered-Block fact');
  const artifact = input(originals); artifact.position.observation.observation.relics = [{ relic: 'ClockworkSouvenir' }];
  assert.equal(preparationCardChoice(artifact).route.kind, 'review');
  artifact.position.observation.observation.relics = [];
  artifact.position.observation.observation.potions = [{ potion: { potion: 'AncientPotion' } }];
  assert.equal(preparationCardChoice(artifact).route.kind, 'review');
});

test('native owned upgrade previews reduce early upgrades and first area output to a stable two-function comparison', () => {
  const offers = [hemokinesis(), wholeHandUpgrade(), cleave()];
  const commands = ['take:Armaments#0', 'take:Cleave#0'];
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const frame = preparationCardChoice(earlyReward(earlyUpgradeDeck(), order.map(index => offers[index])));
    assert.equal(frame.route.kind, 'model', JSON.stringify(frame.route));
    assert.deepEqual([...frame.route.commands].sort(), commands);
    assert.equal(frame.candidates.length, 4);
    const reading = renderPreparationCardChoice(frame);
    assert.match(reading, /PommelStrike own upgrade: 1->1 Energy; on-play draw request 1->2/);
    assert.match(reading, /BurningPact own upgrade: 1->1 Energy; on-play draw request 2->3/);
    assert.match(reading, /eligible cards sharing its actual Hand/);
    assert.match(reading, /15-damage single-target request and 2 base HP loss/);
    assert.ok(frame.inputBudget.bytes < TARGET_CARD_CHOICE_INPUT_BYTES);
  }
});

test('first combat-upgrade preference uses native eligible instances and keeps missing or changed support reviewable', () => {
  const originals = earlyUpgradeDeck();
  const changes = [
    originals.map(({ next_upgrade, ...source }) => source),
    originals.map(source => ['PommelStrike', 'BurningPact'].includes(source.card.card) ? { ...source, next_upgrade: null } : source),
    originals.map(source => source.card.card === 'Strike' ? { ...source, card: { ...source.card, count: 1 } } : source),
    originals.map(source => source.next_upgrade ? { ...source, next_upgrade: { ...source.next_upgrade,
      definition: { ...source.next_upgrade.definition, card: 'Defend', upgrades: 3 } } } : source),
    [...originals, wholeHandUpgrade()],
    [...originals, card('Apotheosis', 'Skill', 2)],
  ];
  for (const rows of changes) assert.equal(preparationCardChoice(earlyReward(rows, [wholeHandUpgrade()])).route.kind, 'review');
  for (const relic of ['SneckoEye', 'VelvetChoker', 'WarpedTongs']) {
    const input = earlyReward(originals, [wholeHandUpgrade()]); input.position.observation.observation.relics = [{ relic }];
    assert.equal(preparationCardChoice(input).route.kind, 'review');
  }
  const cheaper = upgradePreview(card('Corruption', 'Power', 3), { cost: 2 });
  const resetCost = upgradePreview(card('Corruption', 'Power', 0), { cost: 2 });
  const basics = originals.filter(source => !['PommelStrike', 'BurningPact'].includes(source.card.card));
  assert.equal(preparationCardChoice(earlyReward([...basics, cheaper], [wholeHandUpgrade()])).route.kind, 'automatic');
  assert.equal(preparationCardChoice(earlyReward([...basics, resetCost], [wholeHandUpgrade()])).route.kind, 'review');
  const mixedDraw = upgradePreview(card('PommelStrike', 'Attack', 0, { draw_cards: 1 }), { cost: 1, draws: 2 });
  assert.equal(preparationCardChoice(earlyReward([...basics, mixedDraw], [wholeHandUpgrade()])).route.kind, 'review',
    'more draw with a larger payment retains its own tradeoff');
});

test('early HP-funded and area attacks retain uncovered upgrades, HP triggers and repeated group coverage', () => {
  const originals = earlyUpgradeDeck();
  for (const source of [card('Rupture', 'Power', 1), attack('BloodForBlood')]) {
    assert.equal(preparationCardChoice(earlyReward([...originals, source], [wholeHandUpgrade(), hemokinesis()])).route.kind, 'review');
  }
  const stronger = hemokinesis(); stronger.card.upgrades = stronger.mechanics.upgrades = 1;
  stronger.persistent_damage = stronger.mechanics.damage.damage_per_hit = 20;
  assert.equal(preparationCardChoice(earlyReward(originals, [wholeHandUpgrade(), stronger])).route.kind, 'review');
  const thinOutput = originals.filter(source => source.card.card !== 'PerfectedStrike');
  assert.equal(preparationCardChoice(earlyReward(thinOutput, [wholeHandUpgrade(), hemokinesis()])).route.kind, 'review');
  assert.equal(preparationCardChoice(earlyReward([...originals, cleave()], [cleave()])).route.kind, 'review');
});

test('late covered attacks use an order-independent host default and retain the entire reward evidence', () => {
  const input = window(established(), [attack('Clash'), attack('TwinStrike', 2), attack('PerfectedStrike')]);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['skip-card#0']);
  assert.equal(frame.candidates.length, 4);
  assert.ok(frame.candidates.filter(row => row.kind === 'gain').every(row => row.nativeReading.includes('Printed rule')));
  assert.equal(bindPreparationCardChoice('skip-card#0', frame, input, 'host-card-policy').followsDefault, true);
  assert.throws(() => bindPreparationCardChoice('take:TwinStrike#0', frame, input), /policy comparison option/);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, frame.route.commands);
});

test('first Disarm takes priority over covered additions; missing, early and already-owned coverage remain reviewable', () => {
  const input = window(established(), [disarm(), attack('Clash'), card('Flex', 'Skill', 0)]);
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:Disarm#0']);
  const repeated = structuredClone(input);
  const current = disarm(); repeated.deck_evidence.cards.push(current); repeated.position.observation.observation.cards.push(current.card);
  assert.equal(preparationCardChoice(repeated).route.kind, 'review');
  const unfamiliar = window(established(), [disarm(), card('Feed', 'Attack', 1)]);
  const gap = preparationCardChoice(unfamiliar);
  assert.equal(gap.route.gap.candidates[0].card, 'Feed');
  assert.equal(gap.candidates.length, 3);
  assert.throws(() => bindPreparationCardChoice('take:Disarm#0', gap, unfamiliar, 'host-card-policy'), /reviewed conditions/);
  input.position.observation.observation.run.act = 1;
  assert.equal(preparationCardChoice(input).route.kind, 'review');
});

test('first persistent reduction takes priority over a third matching protective replacement within its reviewed window', () => {
  const shrug = () => card('ShrugItOff', 'Skill', 1, { block: 11, base_block_delivery: 'once_on_play',
    draw_cards: 1 });
  const copies = shrug(); copies.card.count = 2;
  const originals = [...established(), copies];
  const input = window(originals, [disarm(), shrug(), attack('TwinStrike', 2)]);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['take:Disarm#0']);
  assert.match(frame.rules[0].assessments.find(row => row.card === 'ShrugItOff').preference, /each affected later attack hit/);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, frame.route.commands);
  for (const boundary of [
    window(originals, [shrug()]),
    window([...originals, disarm()], [disarm(), shrug()]),
    window([...established(), card('ShrugItOff', 'Skill', 1, { block: 8, draw_cards: 1 })], [disarm(), shrug()]),
  ]) assert.equal(preparationCardChoice(boundary).route.kind, 'review');
  const costly = disarm(); costly.persistent_cost = costly.mechanics.cost = 4;
  assert.equal(preparationCardChoice(window(originals, [costly, shrug()])).route.kind, 'review');
  const changed = shrug(); changed.mechanics.draw_cards = 2;
  const differentAccess = preparationCardChoice(window(originals, [disarm(), changed]));
  assert.ok(!differentAccess.rules.flatMap(row => row.assessments ?? []).some(row =>
    row.policy === 'first-reduction-over-repeated-replacement'));
});

test('late combined attacks use owned coverage while the first focused Strength burst adds a distinct function', () => {
  const burst = () => { const row = attack('Pummel', 4); row.mechanics.exhausts = true; return row; };
  const wave = () => card('IronWave', 'Attack', 1, { block: 5, base_block_delivery: 'once_on_play' });
  const thunder = () => { const row = attack('ThunderClap', 1, 'each_enemy'); row.mechanics.damage.damage_per_hit = 4; return row; };
  const bash = card('Bash', 'Attack', 2, { vulnerable: 3 });
  const reaper = attack('Reaper', 1, 'each_enemy'); reaper.mechanics.damage.damage_per_hit = 4; reaper.mechanics.exhausts = true;
  const block = card('ShrugItOff', 'Skill', 1, { block: 11, base_block_delivery: 'once_on_play' }); block.card.count = 3;
  const originals = [...established(), bash, reaper, block];
  const input = window(originals, [thunder(), burst(), wave()]);
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:Pummel#0']);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:Pummel#0']);
  const covered = window([...originals, burst()], [thunder(), burst(), wave()]);
  assert.equal(preparationCardChoice(covered).route.kind, 'review', 'another finite burst needs its own increment comparison');
  const missing = [
    window(originals.filter(row => row !== bash), [thunder()]),
    window(originals.filter(row => row !== reaper), [thunder()]),
    window(originals.filter(row => row !== block), [wave()]),
    window([...originals, card('Dropkick', 'Attack', 1)], [thunder()]),
    window([...originals, card('Juggernaut', 'Power', 2)], [wave()]),
    window(originals.filter(row => row.card.card !== 'DemonForm'), [burst()]),
  ];
  for (const boundary of missing) assert.equal(preparationCardChoice(boundary).route.kind, 'review');
  assert.deepEqual(preparationCardChoice(window(originals, [thunder(), wave()])).route.commands, ['skip-card#0']);
});

test('a missing policy retains skip and every offered card, including oversized rewards', () => {
  const input = window([card('Strike', 'Attack', 1)], ['Clash', 'Headbutt', 'Flex', 'Sentinel']
    .map(id => card(id, 'Skill', 1)));
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.gap.reason, 'uncovered-reward-condition');
  assert.equal(frame.candidates.length, 5);
  assert.ok(frame.candidates.some(row => row.kind === 'skip'));
});

test('protective replacement and free Hand planning form a small order-independent setup choice', () => {
  const shrug = () => card('ShrugItOff', 'Skill', 1, { block: 11, base_block_delivery: 'once_on_play', draw_cards: 1 });
  const warcry = () => card('Warcry', 'Skill', 0, { draw_cards: 2, hand_topdeck_after_draw: 1, exhausts: true });
  const input = window([...established(), pact(), shrug()], [shrug(), warcry(), card('PerfectedStrike', 'Attack', 2)]);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'model');
  assert.deepEqual(frame.route.commands, ['take:ShrugItOff#0', 'take:Warcry#0']);
  assert.equal(frame.candidates.length, 4, 'preserved legal set includes the attack and skip');
  const reading = renderPreparationCardChoice(frame);
  assert.match(reading, /On play: 11 Block/);
  assert.match(reading, /place 1 Hand card on top of Draw.*contributes 0 cards to Hand.*awaits a later draw/);
  assert.match(frame.candidates.find(row => row.card === 'Warcry').facts.join(' '), /Warcry.*Exhaust after play/);
  assert.ok(!frame.rules.some(rule => rule.id === 'reach-owned-functions'), 'topdeck planning is separate from net multi-draw');
  for (const command of frame.route.commands) assert.equal(bindPreparationCardChoice(command, frame, input).command, command);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands.slice().sort(), frame.route.commands.slice().sort());

  const boundaries = [
    window([pact(), shrug()], [shrug(), warcry()]),
    window([growth(), shrug()], [warcry()]),
    window([...established(), pact(), warcry()], [warcry()]),
    window([...established(), pact(), shrug(), shrug()], [shrug()]),
    window([...established(), pact()], [card('Warcry', 'Skill', 0, { draw_cards: 1, hand_topdeck_after_draw: 1, exhausts: true })]),
    window([...established(), pact()], [card('ShrugItOff', 'Skill', 1, { block: 8, base_block_delivery: 'once_on_play', draw_cards: 1 })]),
  ];
  for (const boundary of boundaries) assert.equal(preparationCardChoice(boundary).route.kind, 'review');
  for (const relic of ['SneckoEye', 'VelvetChoker']) {
    const restricted = structuredClone(input);
    restricted.position.observation.observation.relics = [{ relic }];
    assert.equal(preparationCardChoice(restricted).route.kind, 'review');
  }
  const early = structuredClone(input); early.position.observation.observation.run.act = 1;
  assert.equal(preparationCardChoice(early).route.kind, 'review');
  const unrelated = window([...established(), pact()], [shrug(), warcry(), card('Feed', 'Attack', 1)]);
  assert.equal(preparationCardChoice(unrelated).route.kind, 'review');
});

test('a draw comparison supplies declared native facts without unrelated inventory and resource rules', () => {
  const input = window([growth(), card('Defend', 'Skill', 1)], [pact(), card('BattleTrance', 'Skill', 0, { draw_cards: 3 })]);
  input.position.observation.observation.relics = [{ relic: 'MummifiedHand' }];
  input.ownedResourceReading = 'Relics (1):\n  Mummified Hand\n    Playing a Power sets one random Hand card to 0 Energy for the turn.\nPotions (1 slots; availability comes from current legal options):\n  P0 empty';
  const frame = preparationCardChoice(input), reading = renderPreparationCardChoice(frame);
  assert.match(reading, /DemonForm: Power, 3 Energy/);
  assert.match(reading, /Mummified Hand.*\n.*Playing a Power sets one random Hand card to 0 Energy/);
  assert.doesNotMatch(reading, /Defend|P0 empty|Relevant handbook|Whole-deck comparison/);
  assert.equal(frame.inputBudget.bytes, Buffer.byteLength(reading) + Buffer.byteLength(CARD_CHOICE_AUTOMATIC_GUIDE));
  assert.ok(frame.inputBudget.bytes < TARGET_CARD_CHOICE_INPUT_BYTES);
});

test('owned Power payment retains the complete native random-reduction rule in a small functional choice', () => {
  const shrug = card('ShrugItOff', 'Skill', 1, { block: 11, base_block_delivery: 'once_on_play', draw_cards: 1 });
  const warcry = card('Warcry', 'Skill', 0, { draw_cards: 2, hand_topdeck_after_draw: 1, exhausts: true });
  const input = window([...established(), pact()], [shrug, warcry]);
  input.position.observation.observation.relics = [{ relic: 'MummifiedHand' }];
  assert.throws(() => renderPreparationCardChoice(preparationCardChoice(input)),
    error => error.preparationGap?.reason === 'missing-payment-relic-rules');
  const rule = '    Whenever you play a Power, one randomly chosen eligible card remaining in your hand costs 0 for this turn. Positive printed/current cost required; X-cost, unplayable and already-free cards are ineligible.';
  input.ownedResourceReading = 'Relics (2):\n  Burning Blood\n    Heal 6 after victory.\n  Mummified Hand\n' + rule + '\nPotions (2):\n  P0 empty';
  const frame = preparationCardChoice(input), reading = renderPreparationCardChoice(frame);
  assert.ok(reading.includes(rule));
  assert.doesNotMatch(reading, /Burning Blood|P0 empty/);
  assert.equal(frame.route.kind, 'model');
  assert.ok(frame.inputBudget.bytes < TARGET_CARD_CHOICE_INPUT_BYTES);
});

test('late third Armaments, first discard retrieval and first targeted protective cleanup use distinct conditions', () => {
  const armaments = () => card('Armaments', 'Skill', 1, { combat_upgrade_scope: 'whole_hand' });
  const headbutt = () => attack('Headbutt');
  const grit = () => card('TrueGrit', 'Skill', 1, { block: 9, base_block_delivery: 'once_on_play',
    exhausts_other_cards: true, selects_from_hand: true });
  const base = () => { const strikes = card('Strike', 'Attack', 1); strikes.card.count = 4;
    return [...established(), pact(), armaments(), armaments(), strikes]; };
  const input = window(base(), [armaments(), headbutt(), grit()]);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'model');
  assert.deepEqual(frame.route.commands, ['take:Headbutt#0', 'take:TrueGrit#0']);
  assert.match(renderPreparationCardChoice(frame), /useful target must be in Discard|repeat-use targets/);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands.slice().sort(), frame.route.commands.slice().sort());
  for (const rows of [base().filter(row => row.card.card !== 'Rampage'), [...base(), headbutt()], [...base(), grit()]]) {
    assert.equal(preparationCardChoice(window(rows, [armaments(), headbutt(), grit()])).route.kind, 'review');
  }
  const random = grit(); random.mechanics.selects_from_hand = false;
  assert.equal(preparationCardChoice(window(base(), [random])).route.kind, 'review');
  assert.equal(preparationCardChoice(window(base().filter(row => row.card.card !== 'Strike'), [grit()])).route.kind, 'review');
  assert.equal(preparationCardChoice(window(base().filter((_row, i) => i !== 7), [armaments()])).route.kind, 'review');
});

test('first free Exhaust recovery prefers the owned protection and healing package with explicit pile conditions', () => {
  const apparition = () => card('Apparition', 'Skill', 1, { exhausts: true, intangible_gain: 1, intangible_damage_cap: 1 });
  const reaper = () => { const row = attack('Reaper', 1, 'each_enemy'); row.mechanics.exhausts = true; return row; };
  const exhume = () => card('Exhume', 'Skill', 0, { exhausts: true, exhaust_pile_to_hand: { count: 1, excluded_card: 'Exhume' } });
  const originals = [growth(), apparition(), apparition(), reaper()];
  const input = window(originals, [exhume(), card('Feed', 'Attack', 1), card('Bludgeon', 'Attack', 3)]);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['take:Exhume#0']);
  assert.match(renderPreparationCardChoice(frame), /Select a target after it enters Exhaust, return it to Hand and pay/);
  assert.equal(frame.candidates.length, 4);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, frame.route.commands);
  for (const boundary of [
    window(originals.slice(1), [exhume()]),
    window([growth(), apparition(), reaper()], [exhume()]),
    window(originals.slice(0, 3), [exhume()]),
    window([...originals, exhume()], [exhume()]),
    window(originals, [card('Exhume', 'Skill', 1, { exhausts: true, exhaust_pile_to_hand: { count: 1, excluded_card: 'Exhume' } })]),
    window(originals, [card('Exhume', 'Skill', 0, { exhausts: true })]),
  ]) assert.equal(preparationCardChoice(boundary).route.kind, 'review');
});

test('first payable recurring multi-turn Weak adds protection to established Strength output', () => {
  const uppercut = () => { const row = attack('Uppercut'); row.persistent_cost = row.mechanics.cost = 2;
    row.mechanics.weak = 2; row.mechanics.vulnerable = 2; return row; };
  const input = window(established(), [card('PerfectedStrike', 'Attack', 2), uppercut(), card('Entrench', 'Skill', 1)]);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['take:Uppercut#0']);
  assert.equal(frame.candidates.length, 4);
  assert.match(renderPreparationCardChoice(frame), /Weak applies after its own hit.*enemy debuff prevention/);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, frame.route.commands);
  for (const boundary of [
    window([...established(), uppercut()], [uppercut()]),
    window(established().filter(row => row.card.card !== 'DemonForm'), [uppercut()]),
    window([growth(), attack('Anger')], [uppercut()]),
  ]) assert.equal(preparationCardChoice(boundary).route.kind, 'review');
  const oneTurn = uppercut(); oneTurn.mechanics.weak = 1;
  assert.equal(preparationCardChoice(window(established(), [oneTurn])).route.kind, 'review');
  const restricted = structuredClone(input); restricted.position.observation.observation.base_energy_per_turn = 2;
  assert.equal(preparationCardChoice(restricted).route.kind, 'review');
  const early = structuredClone(input); early.position.observation.observation.run.act = 1;
  assert.equal(preparationCardChoice(early).route.kind, 'review');
  const snecko = structuredClone(input); snecko.position.observation.observation.relics = [{ relic: 'SneckoEye' }];
  assert.equal(preparationCardChoice(snecko).route.kind, 'review');
});

test('free group Weak complements paid single-target coverage during expensive setup', () => {
  const paidWeak = card('Uppercut', 'Attack', 2, { weak: 3, play_target: 'one_enemy' });
  const freeWeak = card('Intimidate', 'Skill', 0, { weak: 2, play_target: 'all_enemies', exhausts: true });
  const base = [growth(), paidWeak];
  const input = window(base, [sentinel(), freeWeak, card('FireBreathing', 'Power', 1)]);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['take:Intimidate#0']);
  assert.equal(frame.candidates.length, 4);
  assert.match(renderPreparationCardChoice(frame), /complements the owned paid single-target coverage.*Exhaust/);
  assert.match(renderPreparationCardChoice(frame), /Enemy debuff prevention applies to each target/);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, frame.route.commands);
  for (const boundary of [
    window([paidWeak], [freeWeak]),
    window([growth()], [freeWeak]),
    window([...base, freeWeak], [freeWeak]),
    window([growth(), card('Shockwave', 'Skill', 2, { weak: 3, play_target: 'all_enemies', exhausts: true })], [freeWeak]),
    window(base, [card('Intimidate', 'Skill', 0, { weak: 1, play_target: 'all_enemies', exhausts: true })]),
    window(base, [card('Intimidate', 'Skill', 0, { weak: 2, play_target: 'one_enemy', exhausts: true })]),
  ]) assert.equal(preparationCardChoice(boundary).route.kind, 'review');
  for (const relic of ['SneckoEye', 'VelvetChoker']) {
    const boundary = window(base, [freeWeak]);
    boundary.position.observation.observation.relics = [{ relic }];
    assert.equal(preparationCardChoice(boundary).route.kind, 'review');
  }
  const early = window(base, [freeWeak]); early.position.observation.observation.run.act = 1;
  assert.equal(preparationCardChoice(early).route.kind, 'review');
});

test('first HP-funded burst access keeps actual loss and a conservative reserve in its policy', () => {
  const offering = () => card('Offering', 'Skill', 0, { draw_cards: 5, energy_gain: 2,
    energy_gain_rule: { amount: 2, delivery: 'on_play' },
    self_hp_loss_on_play: { base_hp_loss: 6, bypasses_block: true, bypasses_intangible: false }, exhausts: true });
  const input = window([growth(), pact()], [pact(), offering(), card('Havoc', 'Skill', 0)]);
  input.position.observation.observation.run.current_hp = 80;
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'automatic');
  assert.deepEqual(frame.route.commands, ['take:Offering#0']);
  const reading = renderPreparationCardChoice(frame);
  assert.match(reading, /base HP loss 6; gain 2 Energy; request 5 draws; Exhaust/);
  assert.match(reading, /choose when to play it using actual combat HP and modifiers/);
  assert.match(reading, /On play: gain 2 Energy; base HP loss 6 before/);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, frame.route.commands);
  for (const change of [
    row => { row.position.observation.observation.run.current_hp = 59; },
    row => { row.position.observation.observation.run.max_hp = row.position.observation.observation.run.current_hp = 20; },
    row => { row.position.observation.observation.run.act = 1; },
    row => { row.position.observation.observation.relics = [{ relic: 'VelvetChoker' }]; },
    row => { row.position.observation.observation.relics = [{ relic: 'SneckoEye' }]; },
    row => { delete row.decision_evidence.candidates[0].cards[0].current.self_hp_loss_on_play; },
    row => { row.decision_evidence.candidates[0].cards[0].current.energy_gain_rule.delivery = 'on_exhaust'; },
  ]) {
    const boundary = window([growth(), pact()], [offering()]);
    boundary.position.observation.observation.run.current_hp = 80;
    change(boundary);
    assert.equal(preparationCardChoice(boundary).route.kind, 'review');
  }
  const repeated = window([growth(), pact(), offering()], [offering()]);
  repeated.position.observation.observation.run.current_hp = 80;
  assert.equal(preparationCardChoice(repeated).route.kind, 'review');
});

test('planned top-card free play and targeted cleanup preserve the required setup and target lifecycle', () => {
  const havoc = () => card('Havoc', 'Skill', 0, { top_deck_play: {
    exhaust_after_play: true, bypass_fixed_energy_cost: true, x_cost_uses_current_energy: true } });
  const grit = () => card('TrueGrit', 'Skill', 1, { block: 9, base_block_delivery: 'once_on_play',
    exhausts_other_cards: true, selects_from_hand: true });
  const planner = card('Warcry', 'Skill', 0, { draw_cards: 2, hand_topdeck_after_draw: 1, exhausts: true });
  const basics = card('Strike', 'Attack', 1); basics.card.count = 4;
  const originals = [...established(), pact(), planner, basics];
  const input = window(originals, [havoc(), grit(), card('PerfectedStrike', 'Attack', 2)]);
  const frame = preparationCardChoice(input);
  assert.equal(frame.route.kind, 'model');
  assert.deepEqual(frame.route.commands, ['take:Havoc#0', 'take:TrueGrit#0']);
  const reading = renderPreparationCardChoice(frame);
  assert.match(reading, /Played Powers deploy and remain active; other targets.*Exhaust-after-play/);
  assert.match(reading, /X-cost uses current Energy and the target card's own Energy spending rule/);
  assert.match(reading, /Owned Hand-to-top-of-Draw planning: 1x Warcry.*before another draw removes it/);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands.slice().sort(), frame.route.commands.slice().sort());
  for (const rows of [originals.filter(row => row !== planner), [...originals, havoc()], originals.filter(row => row.card.card !== 'DemonForm')]) {
    assert.equal(preparationCardChoice(window(rows, [havoc(), grit()])).route.kind, 'review');
  }
  const expensivePlanner = structuredClone(planner); expensivePlanner.persistent_cost = expensivePlanner.mechanics.cost = 4;
  assert.equal(preparationCardChoice(window(originals.map(row => row === planner ? expensivePlanner : row), [havoc(), grit()])).route.kind, 'review');
  for (const field of ['exhaust_after_play', 'bypass_fixed_energy_cost', 'x_cost_uses_current_energy']) {
    const unreviewed = havoc(); unreviewed.mechanics.top_deck_play[field] = false;
    const boundary = preparationCardChoice(window(originals, [unreviewed]));
    assert.equal(boundary.route.kind, 'review');
    const facts = boundary.candidates.find(row => row.card === 'Havoc').facts.join(' ');
    const supplied = { exhaust_after_play: /Exhaust-after-play/, bypass_fixed_energy_cost: /bypasses fixed Energy cost/,
      x_cost_uses_current_energy: /X-cost uses current Energy/ };
    assert.doesNotMatch(facts, supplied[field]);
  }
});

test('missing printed rules stop the model comparison before candidate names can stand in for effects', () => {
  const input = window([growth()], [pact()]);
  delete input.candidateReadings;
  assert.throws(() => renderPreparationCardChoice(preparationCardChoice(input)),
    error => error.preparationGap?.reason === 'missing-candidate-rules'
      && error.preparationGap.commands[0] === 'take:BurningPact#0');
});

test('all retained Perfected Strike evidence counts native tags rather than names', () => {
  const strike = card('Strike', 'Attack', 1, { tags: ['Strike', 'StarterStrike'] });
  strike.card.count = 4;
  const named = card('TwinStrike', 'Attack', 1, { tags: [] });
  const tagged = card('Bash', 'Attack', 2, { tags: ['Strike'] });
  const perfected = card('PerfectedStrike', 'Attack', 2, { tags: ['Strike'], damage: {
    delivery: { kind: 'attack', amount: { kind: 'base_plus_strike_cards', per_card: 2 } } } });
  const input = window([strike, named, tagged], [perfected, named]);
  const frame = preparationCardChoice(input), reading = frame.candidates.find(row => row.card === 'PerfectedStrike').facts.join(' ');
  assert.match(reading, /Original Strike-tagged pool: 5 copies \(4x Strike, 1x Bash\).*brings it to 6/);
  delete strike.mechanics.tags;
  assert.throws(() => preparationCardChoice(input), error => error.preparationGap?.reason === 'missing-strike-tags');
});

test('early damage, missing Strength, Hand clearing and temporary Strength combinations escape late preserve defaults', () => {
  const input = window(established(), [attack('TwinStrike', 2)]);
  for (const rows of [[], [growth()], established().filter(row => row.card.card !== 'DemonForm')]) {
    const current = window(rows, [attack('TwinStrike', 2)]);
    assert.equal(preparationCardChoice(current).route.kind, 'review');
  }
  input.position.observation.observation.run.act = 1;
  assert.equal(preparationCardChoice(input).route.kind, 'review');
  const clearing = window([...established(), card('SecondWind', 'Skill', 1, { exhausts_other_cards: true })], [attack('Clash')]);
  assert.equal(preparationCardChoice(clearing).route.kind, 'review');
  const amplify = window([...established(), card('LimitBreak', 'Skill', 1)], [card('Flex', 'Skill', 0)]);
  assert.equal(preparationCardChoice(amplify).route.kind, 'review');
  const pellets = window(established(), [card('Flex', 'Skill', 0)]);
  pellets.position.observation.observation.relics = [{ relic: 'OrangePellets' }];
  assert.equal(preparationCardChoice(pellets).route.kind, 'review');
  const offered = window([growth(), attack('Anger'), attack('Rampage')], [attack('TwinStrike', 2), attack('SwordBoomerang', 3)]);
  assert.equal(preparationCardChoice(offered).route.kind, 'review', 'offered attacks supply no owned multi-hit coverage');
});

test('input above the byte target preserves native rules and the supplied comparison route', () => {
  const input = window([growth()], [pact(), card('BattleTrance', 'Skill', 0, { draw_cards: 3 })]);
  input.candidateReadings['take:BattleTrance#0'] += '\nPrinted rule: ' + '机制'.repeat(TARGET_CARD_CHOICE_INPUT_BYTES);
  const frame = preparationCardChoice(input);
  frame.rules = frame.rules.filter(rule => rule.id === 'reach-owned-functions');
  frame.rules[0].question = 'Fixture comparison with two explicitly supplied acceptable choices.';
  assert.equal(preparationCardChoiceRoute(frame).kind, 'model');
  const reading = renderPreparationCardChoice(frame);
  assert.equal(preparationCardChoiceRoute(frame).kind, 'model');
  assert.equal(frame.inputBudget.exceeded, true);
  assert.equal(frame.inputBudget.bytes, frame.inputBudget.componentBytes.reduce((sum, bytes) => sum + bytes, 0));
  assert.ok(reading.includes('机制'.repeat(TARGET_CARD_CHOICE_INPUT_BYTES)));
  assert.ok(frame.candidates[1].nativeReading.endsWith('机制'.repeat(TARGET_CARD_CHOICE_INPUT_BYTES)));
  assert.equal(bindPreparationCardChoice('take:BurningPact#0', frame, input).command, 'take:BurningPact#0');
});

test('the Pact and Breath access pair uses cleanup as a checked benefit and preserves different tradeoffs', () => {
  const originals = () => {
    const strike = card('Strike', 'Attack', 1); strike.card.count = 4;
    return [growth(), strike];
  };
  const breath = draws => card('DeepBreath', 'Skill', 0, { draw_cards: draws });
  const input = window(originals(), [pact(), breath(2)]);
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:BurningPact#0']);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:BurningPact#0']);
  const scarce = window([growth(), card('Strike', 'Attack', 1)], [pact(), breath(2)]);
  assert.equal(preparationCardChoice(scarce).route.gap.reason, 'uncovered-access-comparison');
  const largerDraw = window(originals(), [pact(), breath(3)]);
  assert.equal(preparationCardChoice(largerDraw).route.kind, 'review');
  const tightPayment = window(originals(), [pact(), breath(2)]);
  tightPayment.position.observation.observation.base_energy_per_turn = 2;
  assert.equal(preparationCardChoice(tightPayment).route.kind, 'review');
  const extra = window(originals(), [pact(), breath(2), card('Offering', 'Skill', 0, { draw_cards: 3 })]);
  const uncovered = preparationCardChoice(extra);
  assert.equal(uncovered.route.kind, 'review');
  assert.equal(uncovered.route.gap.candidates.length, 3, 'another access option is retained for an explicit priority decision');
  assert.equal(uncovered.candidates.length, 4);
});

test('compatible defaults resolve automatically, compare small sets, and retain conflicts without truncation', () => {
  const input = window([growth()], [pact(), card('BattleTrance', 'Skill', 0, { draw_cards: 3 }), sentinel()]);
  const frame = preparationCardChoice(input);
  frame.rules = frame.rules.filter(rule => rule.id === 'reach-owned-functions');
  frame.rules[0].question = 'Fixture comparison with two explicitly supplied acceptable choices.';
  frame.route = preparationCardChoiceRoute(frame);
  assert.equal(frame.route.kind, 'model');
  assert.deepEqual(frame.route.commands, ['take:BurningPact#0', 'take:BattleTrance#0']);
  assert.throws(() => bindPreparationCardChoice('take:BurningPact#0', frame, input, 'host-card-policy'), /one compatible/);
  frame.rules.push({ id: 'second-default', commands: ['take:BattleTrance#0'] });
  assert.deepEqual(preparationCardChoiceRoute(frame).commands, ['take:BattleTrance#0']);
  assert.equal(preparationCardChoiceRoute(frame).kind, 'automatic');
  frame.rules.push({ id: 'conflicting-default', commands: ['take:Sentinel#0'] });
  assert.equal(preparationCardChoiceRoute(frame).gap.reason, 'policy-conflict');
  assert.throws(() => bindPreparationCardChoice('take:BurningPact#0', frame, input), error => error.preparationGap?.reason === 'policy-conflict');
  frame.rules = [{ id: 'wide-default', commands: Array.from({ length: MAX_CARD_CHOICE_OPTIONS + 1 }, (_, i) => `option-${i}`) }];
  frame.candidates = frame.rules[0].commands.map(command => ({ command }));
  assert.equal(preparationCardChoiceRoute(frame).gap.reason, 'comparison-too-large');
  assert.equal(frame.candidates.length, MAX_CARD_CHOICE_OPTIONS + 1);
});

test('reviewed access defaults account for Exhaust payoffs and preserve uncovered combinations', () => {
  const trance = () => card('BattleTrance', 'Skill', 0, { draw_cards: 3 });
  const breath = () => card('DeepBreath', 'Skill', 0, { draw_cards: 2 });
  for (const offers of [[pact(), trance()], [breath(), trance()], [pact(), breath(), trance()]]) {
    const input = window([growth()], offers);
    assert.deepEqual(preparationCardChoice(input).defaultCommands, ['take:BattleTrance#0']);
    input.position.actions.reverse();
    assert.deepEqual(preparationCardChoice(input).route.commands, ['take:BattleTrance#0']);
  }
  for (const payoff of [card('DarkEmbrace', 'Power', 2, { exhaust_draw_power: 1 }),
    card('FeelNoPain', 'Power', 1), sentinel()]) {
    const input = window([growth(), payoff], [pact(), trance(), breath()]);
    const frame = preparationCardChoice(input);
    assert.deepEqual(frame.defaultCommands, ['take:BurningPact#0']);
    assert.match(frame.rules.at(-1).conditions.join(' '), new RegExp(payoff.card.card));
  }
  const offeredPayoff = window([growth()], [pact(), trance(), card('FeelNoPain', 'Power', 1)]);
  assert.deepEqual(preparationCardChoice(offeredPayoff).route.commands, ['take:BattleTrance#0']);
  for (const rows of [[growth(), trance()], [growth(), card('Corruption', 'Power', 3)]]) {
    const input = window(rows, [pact(), trance()]);
    const frame = preparationCardChoice(input);
    assert.equal(frame.route.gap.reason, 'uncovered-access-comparison');
    assert.equal(frame.candidates.length, 3);
    assert.throws(() => bindPreparationCardChoice('take:BattleTrance#0', frame, input), /reviewed default/);
  }
  const snecko = window([growth()], [pact(), trance()]);
  snecko.position.observation.observation.relics = [{ relic: 'SneckoEye' }];
  assert.equal(preparationCardChoice(snecko).route.kind, 'review');
  const basic = card('Strike', 'Attack', 1); basic.card.count = 4;
  const alteredCost = window([growth(), basic], [pact(), breath()]);
  alteredCost.position.observation.observation.relics = [{ relic: 'SneckoEye' }];
  assert.equal(preparationCardChoice(alteredCost).route.kind, 'review');
});

test('same-card access variants use native costs and draws while different tradeoffs remain explicit gaps', () => {
  const upgraded = card('BurningPact', 'Skill', 1, { draw_cards: 3, exhausts_other_cards: true });
  upgraded.card.upgrades = 1; upgraded.mechanics.upgrades = 1;
  const input = window([growth()], [pact(), upgraded]);
  const frame = preparationCardChoice(input);
  assert.deepEqual(frame.route.commands, ['take:BurningPact#1']);
  assert.equal(frame.candidates[1].upgrades, 1);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:BurningPact#1']);
  const costly = structuredClone(upgraded); costly.mechanics.cost = 2; costly.persistent_cost = 2;
  const tradeoff = preparationCardChoice(window([growth()], [pact(), costly]));
  assert.equal(tradeoff.route.gap.reason, 'uncovered-card-variant-comparison');
  const payoff = card('DarkEmbrace', 'Power', 2, { exhaust_draw_power: 1 });
  assert.equal(preparationCardChoice(window([growth(), payoff], [pact(), costly])).route.kind, 'review');
  const mixed = window([growth(), payoff], [pact(), upgraded, card('BattleTrance', 'Skill', 0, { draw_cards: 3 })]);
  assert.deepEqual(preparationCardChoice(mixed).route.commands, ['take:BurningPact#1']);
  mixed.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(mixed).route.commands, ['take:BurningPact#1']);
  const mixedTradeoff = window([growth(), payoff], [pact(), costly, card('BattleTrance', 'Skill', 0, { draw_cards: 3 })]);
  assert.equal(preparationCardChoice(mixedTradeoff).route.gap.reason, 'uncovered-card-variant-comparison');
  const changed = structuredClone(upgraded); changed.mechanics.exhausts = true;
  const mixedLifecycle = window([growth(), payoff], [pact(), changed, card('BattleTrance', 'Skill', 0, { draw_cards: 3 })]);
  assert.equal(preparationCardChoice(mixedLifecycle).route.gap.reason, 'uncovered-card-variant-comparison');
});

test('missing native effects and unsupported status routes record distinct gaps', () => {
  const input = window([growth()], [pact()]);
  input.decision_evidence.candidates = [];
  assert.throws(() => preparationCardChoice(input), error => error.preparationGap?.kind === 'facts'
    && error.preparationGap.reason === 'missing-candidate-mechanics');
  const badRoute = card('PowerThrough', 'Skill', 1, { status_injections: [
    { card: 'Wound', count: 2, destination: 'unknown-pile', persistence: 'persistent' } ] });
  assert.throws(() => preparationCardChoice(window([growth(), badRoute], [card('Evolve', 'Power', 1), pact()])),
    error => error.preparationGap?.kind === 'relations');
  delete badRoute.mechanics.status_injections[0].destination;
  assert.throws(() => preparationCardChoice(window([growth(), badRoute], [card('Evolve', 'Power', 1), pact()])),
    error => error.preparationGap?.kind === 'facts');
});

test('comparison input supplies exact owned draw requests while retaining the full inventory only in the host frame', () => {
  const deep = card('DeepBreath', 'Skill', 0, { draw_cards: 2 });
  const frame = preparationCardChoice(window([growth(), deep, card('Defend', 'Skill', 1)], [pact()]));
  const reading = renderPreparationCardChoice(frame);
  assert.match(reading, /DeepBreath: 2 draws, 0 Energy/);
  assert.doesNotMatch(reading, /Defend/);
  assert.match(frame.inventory, /Defend/);
});

test('BattleTrance comparison carries the shared draw-order relation for actual owned draw sources', () => {
  const deep = card('DeepBreath', 'Skill', 0, { draw_cards: 2 });
  const trance = card('BattleTrance', 'Skill', 0, { draw_cards: 3 });
  const frame = preparationCardChoice(window([growth(), deep], [pact(), trance]));
  const reading = renderPreparationCardChoice(frame);
  assert.match(reading, /draws before BattleTrance.*same turn face NoDraw/);
  assert.match(reading, /Owned draw sources affected by this order: 1x DeepBreath \(2 draws, 0 Energy\)/);
  assert.doesNotMatch(reading, /affected by this order:.*BurningPact/);
  assert.match(reading, /Played copy: Skill; ordinary redraw permits later uses/);
  const pactGuidance = frame.candidates.find(row => row.card === 'BurningPact').comparisonGuidance.join(' ');
  assert.match(pactGuidance, /Exhaust moves the selected card out of ordinary future reshuffles/);
  assert.match(pactGuidance, /losing a useful function with removing a weak or obstructive future draw/);
});

test('owned Strength facts supply native card type, condition and lifecycle instead of an engine label', () => {
  const spot = card('SpotWeakness', 'Skill', 1, { strength_gain: 3,
    strength_gain_delivery: 'if_target_intent_base_damage_nonnegative_at_resolution' });
  const frame = preparationCardChoice(window([spot], [pact()]));
  const reading = renderPreparationCardChoice(frame);
  assert.match(reading, /SpotWeakness: Skill, 1 Energy; gain 3 Strength on play when the target has attack intent at resolution; ordinary redraw permits later uses/);
  assert.doesNotMatch(reading, /Owned engines:|if_target_intent_base_damage_nonnegative_at_resolution/);
});

test('automatic adoption uses fresh source and separate intent/applied records, including interrupted execution', async () => {
  const input = window([growth()], [pact(), sentinel()]), frame = preparationCardChoice(input);
  const records = [], commands = [];
  let actual = input;
  const game = { observe: async () => actual, execute: async command => {
    commands.push(command); actual = { ...input, head_revision: 2, state_revision: 2 };
    return { text: 'Completed' };
  } };
  const options = { choice: frame.route.commands[0], owner: 'host-card-policy', record: async row => records.push(row) };
  await adoptPreparationCardChoice(game, frame, input, options);
  assert.deepEqual(commands, ['take:BurningPact#0']);
  assert.deepEqual(records.map(row => row.phase), ['intent', 'applied']);
  assert.equal(records[0].owner, 'host-card-policy');
  await assert.rejects(adoptPreparationCardChoice(game, frame, input, options), /position changed/);
  assert.equal(commands.length, 1);
  assert.equal(records.length, 2);
  actual = input;
  game.execute = async command => { commands.push(command); actual = { ...input, head_revision: 2, state_revision: 2 }; throw new Error('transport lost after save'); };
  records.length = 0;
  await assert.rejects(adoptPreparationCardChoice(game, frame, input, options), error => {
    assert.match(error.message, /transport lost/);
    assert.equal(error.preparationExecution, undefined, 'a saved revision does not acknowledge this request');
    return true;
  });
  assert.deepEqual(records.map(row => row.phase), ['intent']);
  await assert.rejects(adoptPreparationCardChoice(game, frame, input, options), /position changed/);
  assert.equal(commands.length, 2);
});

test('an applied-record failure retains the acknowledged command and observed successor without retrying', async () => {
  const input = window([growth()], [pact(), sentinel()]), frame = preparationCardChoice(input);
  let actual = input, executions = 0;
  const game = { observe: async () => actual, execute: async () => {
    executions++; actual = { ...input, head_revision: 2, state_revision: 2 };
    return { text: 'Completed: take:BurningPact#0' };
  } };
  const options = { choice: frame.route.commands[0], owner: 'host-card-policy', record: async row => {
    if (row.phase === 'applied') throw new Error('injected applied-record failure');
  } };
  await assert.rejects(adoptPreparationCardChoice(game, frame, input, options), error => {
    assert.equal(error.preparationExecution.adopted.command, 'take:BurningPact#0');
    assert.equal(error.preparationExecution.successor.state_revision, 2);
    return true;
  });
  await assert.rejects(adoptPreparationCardChoice(game, frame, input, options), /position changed/);
  assert.equal(executions, 1);
});

test('card adoption rechecks its source after recording intent and never executes a changed root', async () => {
  const input = window([growth()], [pact(), sentinel()]), frame = preparationCardChoice(input);
  let actual = input, executions = 0;
  const game = { observe: async () => actual, execute: async () => { executions++; } };
  const phases = [];
  await assert.rejects(adoptPreparationCardChoice(game, frame, input, {
    choice: frame.route.commands[0], owner: 'host-card-policy', record: async row => {
      phases.push(row.phase); actual = { ...input, head_revision: 2, state_revision: 2 };
    },
  }), /position changed while recording/);
  assert.deepEqual(phases, ['intent']);
  assert.equal(executions, 0);
});

test('native Exhaust-draw and recurring Strength defaults resolve their conditions before the model request', () => {
  const dark = card('DarkEmbrace', 'Power', 2, { exhaust_draw_power: 1 });
  const secondWind = card('SecondWind', 'Skill', 1, { exhausts_other_cards: true });
  assert.deepEqual(preparationCardChoice(window([secondWind], [dark])).defaultCommands, ['take:DarkEmbrace#0']);
  const hits = card('SwordBoomerang', 'Attack', 1, { damage: { delivery: { kind: 'attack', hits: { kind: 'fixed', count: 3 } } } });
  assert.deepEqual(preparationCardChoice(window([hits], [growth()])).defaultCommands, ['take:DemonForm#0']);
});

test('matched Power and Disarm variants resolve in the host with full native effects and stable commands', () => {
  const secondWind = card('SecondWind', 'Skill', 1, { exhausts_other_cards: true });
  const hits = attack('SwordBoomerang', 3);
  const cases = [
    { owned: [secondWind], base: card('DarkEmbrace', 'Power', 2, { exhaust_draw_power: 1 }), cost: 1, field: 'exhaust_draw_power', amount: 1 },
    { owned: [hits], base: growth(), cost: 3, field: 'strength_gain', amount: 3 },
    { owned: established(), base: disarm(), cost: 1, field: 'enemy_strength_down', amount: 3 },
  ];
  for (const { owned, base, cost, field, amount } of cases) {
    const upgraded = structuredClone(base);
    upgraded.card.upgrades = 1; upgraded.mechanics.upgrades = 1;
    upgraded.persistent_cost = cost; upgraded.mechanics.cost = cost; upgraded.mechanics[field] = amount;
    const input = window(owned, [base, upgraded]);
    const expected = `take:${base.card.card}#1`;
    for (const reverse of [false, true]) {
      if (reverse) input.position.actions.reverse();
      const frame = preparationCardChoice(input);
      assert.equal(frame.route.kind, 'automatic');
      assert.deepEqual(frame.route.commands, [expected]);
      assert.deepEqual(frame.defaultCommands, [expected]);
      assert.equal(bindPreparationCardChoice(expected, frame, input, 'host-card-policy').selected.upgrades, 1);
      assert.doesNotMatch(renderPreparationCardChoice(frame), /Which offered|Comparison question:/);
      assert.ok(frame.inputBudget.bytes <= TARGET_CARD_CHOICE_INPUT_BYTES);
      assert.equal(frame.candidates.length, 3, 'full legal variants and skip remain in host evidence');
    }
    const normalize = command => `current ${command}`;
    assert.deepEqual(preparationCardChoice(input, normalize).route.commands, [`current ${expected}`]);
  }
});

test('variant comparisons retain payment, lifecycle, trigger and unreviewed-card gaps before adoption', () => {
  const hits = attack('SwordBoomerang', 3);
  const cheap = growth(); cheap.mechanics.cost = 2; cheap.persistent_cost = 2;
  const higher = growth(); higher.mechanics.strength_gain = 3;
  const tradeoff = preparationCardChoice(window([hits], [cheap, higher]));
  assert.equal(tradeoff.route.gap.reason, 'uncovered-card-variant-comparison');
  assert.equal(tradeoff.route.gap.candidates.length, 2);
  for (const change of [
    row => { row.mechanics.exhausts = true; },
    row => { row.mechanics.ethereal = true; },
    row => { row.mechanics.exhausts_other_cards = false; },
    row => { row.mechanics.play_target = 'one_enemy'; },
    row => { row.mechanics.new_trigger = { timing: 'after_play' }; },
  ]) {
    const first = pact(), second = pact(); second.mechanics.draw_cards = 3;
    change(second);
    const input = window([growth()], [first, second]);
    const frame = preparationCardChoice(input);
    assert.equal(frame.route.gap.reason, 'uncovered-card-variant-comparison');
    assert.throws(() => bindPreparationCardChoice(frame.candidates[1].command, frame, input, 'host-card-policy'), /reviewed payment/);
    assert.equal(frame.candidates.length, 3);
  }
  const altered = window([hits], [growth(), higher]);
  altered.position.observation.observation.relics = [{ relic: 'SneckoEye' }];
  assert.equal(preparationCardChoice(altered).route.kind, 'review');
  const offering = card('Offering', 'Skill', 0, { draw_cards: 3, exhausts: true });
  assert.equal(preparationCardChoice(window([growth()], [offering, offering])).route.kind, 'review');
});

test('equivalent variants remain independent of effect-object key order and use a stable legal command', () => {
  const first = pact(), second = pact();
  second.mechanics = Object.fromEntries(Object.entries(second.mechanics).reverse());
  const input = window([growth()], [second, first]);
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:BurningPact#0']);
  input.position.actions.reverse();
  assert.deepEqual(preparationCardChoice(input).route.commands, ['take:BurningPact#0']);
});
