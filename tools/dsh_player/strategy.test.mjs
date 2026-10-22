import test from 'node:test';
import assert from 'node:assert/strict';
import { access } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { preparationExpertGuidance, preparationCardExpertGuidance, preparationCardFamilyGuidance, renderExpertGuidance, renderCandidateExpertGuidance, STRATEGY_SOURCES, preparationShopRemovalPolicy } from './strategy.mjs';
import catalogue from './strategy-roles.json' with { type: 'json' };
import { conciseCardCommand } from '../run_play/contract.mjs';
import { preparationClassificationContext, preparationRecordedTable, parsePreparationClassification, preparationCommandBasis, preparationReading } from './preparation.mjs';

test('candidate guidance hides only its command label and keeps the supplied preference opaque', () => {
  const candidate = { card: 'Strike', command: 'take:Strike#0', preference: 'Keep (take:Strike#0) {{raw}} in this recorded explanation.', change: '原始 🐈' };
  assert.equal(renderCandidateExpertGuidance(candidate, { includeCommand: false }), 'Offered Strike: Keep (take:Strike#0) {{raw}} in this recorded explanation. Adjust using current conditions: 原始 🐈');
  assert.match(renderCandidateExpertGuidance(candidate), /^Offered Strike \(take:Strike#0\):/);
});

const root = () => ({ schema: 'sts-run-play-agent-observation-v3', session_id: 'strategy-unit', head_revision: 0, state_revision: 0,
  position: { observation: { domain: 'strategic', observation: {
  context: { kind: 'card_reward' }, cards: [
    { card: 'Strike', count: 5, upgrades: 0 }, { card: 'Bash', count: 1, upgrades: 0 },
    { card: 'SwordBoomerang', count: 1, upgrades: 0 },
  ], relics: [{ relic: 'ToyOrnithopter' }], potions: [null, null],
} }, actions: ['Bloodletting', 'PerfectedStrike'].map(card => ({ semantic_action: `take:${card}#0`,
  candidate: { action: { kind: 'take_card', card } } })) } });

test('static strategy languages preserve card identities families source roles and site policy identities', () => {
  const cards = 'SwordBoomerang Combust Headbutt PerfectedStrike SeverSoul Bloodletting Flex Clash DemonForm LimitBreak DarkEmbrace Corruption Sentinel BattleTrance Metallicize Clothesline DualWield Evolve Armaments TrueGrit FeelNoPain Barricade Entrench BodySlam'.split(' ');
  const observation = root();
  observation.position.actions = [...cards, 'Reaper'].map(card => ({ semantic_action: `take:${card}#0`,
    candidate: { action: { kind: 'take_card', card } } }));
  const original = structuredClone(observation);
  const english = preparationCardExpertGuidance(observation), chinese = preparationCardExpertGuidance(observation, undefined, 'zh-CN');
  const identities = rows => rows.map(({ preference, change, ...row }) => row);
  assert.deepEqual(identities(chinese), identities(english));
  for (const card of cards) {
    const before = english.find(row => row.card === card), after = chinese.find(row => row.card === card);
    assert.ok(before.preference && before.change, card);
    assert.notEqual(before.preference, after.preference, card);
    assert.notEqual(before.change, after.change, card);
  }
  const enFamilies = preparationCardFamilyGuidance(english), zhFamilies = preparationCardFamilyGuidance(chinese, 'zh-CN');
  assert.equal(enFamilies.length, 10);
  assert.deepEqual(zhFamilies.map(row => row.id), enFamilies.map(row => row.id));
  assert.ok(zhFamilies.every((row, index) => row.text !== enFamilies[index].text));
  assert.deepEqual(preparationCardExpertGuidance(observation), english, 'another language cannot change the default advice');
  assert.deepEqual(observation, original);
  for (const site of ['card_reward', 'shop', 'campfire', 'map', 'event', 'boss_relic', 'treasure', 'reward', 'run_transition']) {
    const current = root(); current.position.actions = [];
    current.position.observation.observation.context = { kind: site };
    const en = preparationExpertGuidance(current), zh = preparationExpertGuidance(current, undefined, 'zh-CN');
    assert.deepEqual(identities(zh.policies), identities(en.policies));
    assert.notEqual(zh.policies[0].preference, en.policies[0].preference);
    assert.notEqual(zh.policies[0].change, en.policies[0].change);
    assert.deepEqual(zh.sources, en.sources);
  }
});

test('the handbook supplies only current candidates and refreshes owned support after an inventory change', () => {
  const observation = root();
  const before = preparationExpertGuidance(observation);
  assert.deepEqual(before.candidates.map(row => row.command), ['take:Bloodletting#0', 'take:PerfectedStrike#0']);
  const text = renderExpertGuidance(before);
  assert.match(text, /Starter attacks: 5x Strike, 1x Bash/);
  assert.match(text, /Strength-source cards in this handbook: none/);
  assert.match(text, /Current occupied potion slots: 0/);
  observation.position.observation.observation.cards.push({ card: 'Inflame', count: 1, upgrades: 1 });
  observation.position.observation.observation.potions[0] = { potion: 'StrengthPotion' };
  observation.position.actions = [{ semantic_action: 'take:Headbutt#0', candidate: { action: { kind: 'take_card', card: 'Headbutt' } } }];
  const after = preparationExpertGuidance(observation);
  assert.deepEqual(after.candidates.map(row => row.card), ['Headbutt']);
  assert.match(renderExpertGuidance(after), /Strength-source cards in this handbook: 1x Inflame\+/);
  assert.match(renderExpertGuidance(after), /Current occupied potion slots: 1/);
  assert.equal(before.candidates[0].card, 'Bloodletting');
  observation.position.observation.observation.run = { gold: 388 };
  observation.position.observation.observation.relics.push({ relic: 'Ectoplasm' });
  assert.match(renderExpertGuidance(preparationExpertGuidance(observation)), /retained current Gold 388.*Future Gold rewards supply 0/);
  assert.match(renderExpertGuidance(preparationExpertGuidance(observation)), /Ordinary and elite victories still offer card choices and permitted potion rewards/);
});

test('site defaults cover current strategic decisions while unrelated card advice stays out', () => {
  for (const site of ['shop', 'campfire', 'map', 'event', 'boss_relic', 'treasure', 'reward', 'run_transition']) {
    const observation = root();
    observation.position.observation.observation.context = { kind: site };
    observation.position.actions = [{ semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } }];
    const guidance = preparationExpertGuidance(observation);
    assert.equal(guidance.policies.length, 1, site);
    assert.deepEqual(guidance.candidates, [], site);
  }
  const observation = root();
  observation.position.observation.observation.context = { kind: 'run_choice', reason: 'Duplicate' };
  observation.position.actions = [];
  assert.equal(preparationExpertGuidance(observation).policies[0].id, 'copy');
});

test('shop price guidance binds the offered price and its purchase trigger at an unaffordable shop', () => {
  const observation = root(), state = observation.position.observation.observation;
  state.run = { gold: 38 };
  state.context = { kind: 'shop', inventory: { purge: { price: 100, purchase_status: { status: 'insufficient_gold' } } } };
  observation.position.actions = [{ semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } }];
  const text = renderExpertGuidance(preparationExpertGuidance(observation));
  assert.match(text, /Current offered removal price: 100 Gold.*successful shop-removal purchase.*entering another shop keeps that base price/);
  state.context = { kind: 'campfire' };
  assert.doesNotMatch(renderExpertGuidance(preparationExpertGuidance(observation)), /Current offered removal price/);
});

test('source references resolve to tracked notes and guidance preserves actual legal commands', async () => {
  for (const source of Object.values(STRATEGY_SOURCES)) {
    await access(fileURLToPath(new URL(`../../${source.reference}`, import.meta.url)));
  }
  const observation = root();
  observation.position.actions.push({ semantic_action: 'take:UnknownFutureCard#2', candidate: { action: { kind: 'take_card', card: 'UnknownFutureCard' } } });
  const before = structuredClone(observation);
  const guidance = preparationExpertGuidance(observation, command => `normalized ${command}`);
  assert.deepEqual(guidance.candidates.map(row => row.command), ['normalized take:Bloodletting#0', 'normalized take:PerfectedStrike#0']);
  assert.deepEqual(observation, before);
  assert.equal(guidance.policies[0].id, 'draft');
});

test('Apparition event preference binds the actual package and HP tradeoff', () => {
  const observation = planRoot([fact('Disarm', 'Skill', 1, { enemy_strength_down: 2 })], 'event', []);
  const state = observation.position.observation.observation;
  state.context = { kind: 'event', event: 'Ghosts' };
  state.run = { act: 2, current_hp: 48, max_hp: 75 };
  const accept = { semantic_action: 'event:Ghosts:0', candidate: { action: { kind: 'choose_event_option',
    public_semantics: { hp_delta: -11, max_hp_delta: -38, structured_semantics: { effects: [
      { effect: 'lose_max_hp', amount: 38 },
      { effect: 'obtain_card', count: 3, pool: { kind: 'specific', card: 'Apparition', upgrades: 0 } },
    ] } } } } };
  observation.position.actions = [accept, { semantic_action: 'event:Ghosts:1', candidate: { action: { kind: 'choose_event_option' } } }];
  observation.decision_evidence = { kind: 'card_acquisition_options', candidates: [{ semantic_action: accept.semantic_action, cards: [{
    current: { card: 'Apparition', upgrades: 0, cost: 1, intangible_gain: 1, intangible_damage_cap: 1, exhausts: true, ethereal: true },
  }] }] };
  const before = structuredClone(observation);
  const guidance = preparationExpertGuidance(observation);
  const preference = guidance.policies.find(row => row.id === 'apparition-protection-window');
  assert.deepEqual(preference.sources, [], 'the maintenance preference is separate from quoted expert sources');
  assert.match(renderExpertGuidance(guidance), /obtain 3 Apparition; HP 48->37; Max HP 75->37/);
  assert.match(preference.preference, /favor accepting the first Intangible package during Act 2/);
  assert.match(preference.preference, /Ordinary Block, Weak and Strength reduction are complementary/);
  assert.match(preference.change, /actual HP-payment demands.*card-play restriction/);
  assert.deepEqual(observation, before);
  const owned = fact('Apparition', 'Skill', 1, { intangible_gain: 1 }, 3);
  state.cards.push(owned.card); observation.deck_evidence.cards.push(owned);
  assert.doesNotMatch(preparationExpertGuidance(observation).policies.find(row => row.id === preference.id).preference,
    /favor accepting the first/);
  state.cards.pop(); observation.deck_evidence.cards.pop();
  accept.candidate.action.public_semantics.structured_semantics.effects[1].pool.card = 'Bite';
  assert.ok(preparationExpertGuidance(observation).policies.every(row => row.id !== 'apparition-protection-window'));
});

test('Boss comparison attaches permanent play limits and starter replacement to the current inventory in both stages', () => {
  const starter = fact('Strike', 'Attack', 1, {}, 3); starter.card.upgrades = 1; starter.mechanics.upgrades = 1;
  const observation = planRoot([starter, fact('Defend', 'Skill', 1, {}, 4),
    fact('PerfectedStrike', 'Attack', 2), fact('BurningPact', 'Skill', 1, { draw_cards: 2 }),
    fact('Blind', 'Skill', 0)], 'boss_relic', []);
  observation.position.actions = ['VelvetChoker', 'PandorasBox'].map(relic => ({ semantic_action: `boss-relic:${relic}#0`,
    candidate: { action: { kind: 'take_boss_relic', relic } } }));
  observation.decision_evidence = { kind: 'boss_relic_options', candidates: [
    { semantic_action: 'boss-relic:VelvetChoker#0', definition: { relic: 'VelvetChoker', energy_per_turn_delta: 1, card_play_limit_per_turn: 6 } },
    { semantic_action: 'boss-relic:PandorasBox#0', definition: { relic: 'PandorasBox', starter_card_replacement: {
      target_cards: ['Strike', 'Defend'], replacements_per_card: 1, initial_upgrades: 0,
      pool_rarities: ['Common', 'Uncommon', 'Rare'], applies_obtain_effects: true,
    } } },
  ] };
  const saved = structuredClone(observation);
  const guidance = preparationExpertGuidance(observation);
  assert.ok(ids(guidance).includes('card-play-cap-window'));
  assertConstructionLanguageContract(observation);
  const replacement = guidance.construction.priorities.find(row => row.id === 'starter-replacement-window');
  assert.match(replacement.ownedFacts.join(' '), /replace 7 of 10 cards: 3x Strike\+, 4x Defend/);
  assert.doesNotMatch(replacement.ownedFacts.join(' '), /PerfectedStrike/);
  const text = preparationClassificationContext(observation);
  assert.match(text, /Draw Skills, zero-cost cards, setup Powers.*share that play allowance/);
  assert.match(text, /Common\/Uncommon\/Rare/);
  const rows = parsePreparationClassification('boss-relic:VelvetChoker#0 | medium | direct | @shared-play-cap\n'
    + 'boss-relic:PandorasBox#0 | high | direct | @replace-starters', observation);
  assert.match(preparationRecordedTable({ rows }, observation),
    /Give high priority to replacing several weak starter draws/);
  assert.deepEqual(observation, saved);
  observation.decision_evidence.candidates[1].definition.relic = 'CallingBell';
  assert.ok(!ids(preparationExpertGuidance(observation)).includes('starter-replacement-window'));
  observation.position.actions = [];
  assert.ok(!ids(preparationExpertGuidance(observation)).includes('card-play-cap-window'));
});

test('the complete Ironclad catalogue supplies shared-role advice including replay growth and HP resources', () => {
  const observation = root();
  observation.position.actions = Object.keys(catalogue.cards).map(card => ({ semantic_action: `take:${card}#0`,
    candidate: { action: { kind: 'take_card', card } } }));
  const guidance = preparationExpertGuidance(observation);
  assert.equal(guidance.candidates.length, 72);
  assert.equal(new Set(guidance.candidates.map(row => row.card)).size, 72);
  assert.ok(guidance.candidates.every(row => row.sourceRole && row.familyIds.length > 0));
  const rampage = guidance.candidates.find(row => row.card === 'Rampage');
  assert.ok(rampage.familyIds.includes('scaling'));
  const reaper = guidance.candidates.find(row => row.card === 'Reaper');
  assert.ok(reaper.familyIds.includes('hp-resource'));
  assert.ok(guidance.families.find(row => row.id === 'scaling').text.includes('Replay growth'));
  assert.ok(guidance.families.find(row => row.id === 'hp-resource').text.includes('post-victory heal'));
  assert.deepEqual(reaper.familyIds, ['hp-resource']);
  const anger = guidance.candidates.find(row => row.card === 'Anger');
  assert.deepEqual(anger.familyIds, ['transition', 'strength', 'payment']);
  assert.deepEqual(guidance.candidates.find(row => row.card === 'TrueGrit').familyIds, ['protection', 'exhaust']);
  assert.deepEqual(guidance.candidates.find(row => row.card === 'Armaments').familyIds, ['protection']);
});

const fact = (card, card_type, cost, mechanics = {}, count = 1) => ({
  card: { card, count, upgrades: 0, misc_value: 0, cost_modifier: 0, base_damage_override: null, base_block_override: null },
  mechanics: { card, upgrades: 0, card_type, cost, draw_cards: 0, exhausts: false, ethereal: false, ...mechanics },
  persistent_cost: cost, persistent_damage: 0, persistent_block: 0,
});
function planRoot(rows, site = 'card_reward', offered = ['BattleTrance', 'Headbutt']) {
  const observation = root();
  const state = observation.position.observation.observation;
  observation.position.public_snapshot_id = 'current';
  state.run = { act: 2, floor: 21, current_hp: 55, max_hp: 80, gold: 130 };
  state.base_energy_per_turn = 3;
  state.context = { kind: site };
  state.cards = rows.map(row => row.card);
  observation.deck_evidence = { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: 'current', cards: rows };
  observation.position.actions = offered.map(card => ({ semantic_action: `${site === 'shop' ? 'buy' : 'take'}:${card}#0`,
    candidate: { action: { kind: site === 'shop' ? 'buy_card' : 'take_card', card, upgrades: 0 } } }));
  return observation;
}
const ids = guidance => guidance.construction.priorities.map(row => row.id);

function assertConstructionLanguageContract(observation, normalize) {
  const saved = structuredClone(observation);
  const english = preparationExpertGuidance(observation, normalize).construction;
  const chinese = preparationExpertGuidance(observation, normalize, 'zh-CN').construction;
  const decisions = construction => ({ ...construction,
    priorities: construction.priorities.map(({ ownedFacts, preference, ...row }) => row) });
  assert.deepEqual(decisions(chinese), decisions(english));
  for (const [index, priority] of english.priorities.entries()) {
    assert.equal(chinese.priorities[index].ownedFacts.length, priority.ownedFacts.length, priority.id);
    assert.notEqual(chinese.priorities[index].preference, priority.preference, priority.id);
    assert.ok(!chinese.priorities[index].preference.includes('{{'), priority.id);
  }
  assert.deepEqual(preparationExpertGuidance(observation, normalize).construction, english);
  assert.deepEqual(observation, saved);
}

test('development windows use the same current Act and priority identities in both languages', () => {
  for (const act of [1, 2, 3, 4]) {
    const observation = planRoot([fact('Strike', 'Attack', 1)], 'card_reward', []);
    observation.position.observation.observation.run.act = act;
    assertConstructionLanguageContract(observation);
    assert.deepEqual(ids(preparationExpertGuidance(observation)), ['development-window']);
  }
});

test('established Strength shopping supplies a basic removal target and preserves uncovered inventory conditions', () => {
  const build = () => {
    const strike = fact('Strike', 'Attack', 1, {}, 3); strike.persistent_damage = 6;
    const current = planRoot([strike,
      fact('Inflame', 'Power', 1, { strength_gain: 2, strength_gain_delivery: 'on_play' }),
      fact('Anger', 'Attack', 0), fact('TwinStrike', 'Attack', 1), fact('Carnage', 'Attack', 2)], 'shop', []);
    const state = current.position.observation.observation;
    state.context.inventory = { purge: { price: 75, purchase_status: { status: 'purchasable' } } };
    let ordinal = 0;
    for (const row of current.deck_evidence.cards) for (let i = 0; i < row.card.count; i++) {
      current.position.actions.push({ semantic_action: `remove:${row.card.card}#${i}`, candidate: { domain: 'strategic',
        action: { kind: 'remove_card', card: row.card.card, upgrades: row.card.upgrades, card_ordinal: ordinal++, price: 75 } } });
    }
    current.position.actions.push({ semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } });
    return current;
  };
  const input = build(), policy = preparationShopRemovalPolicy(input);
  assert.deepEqual(policy.commands, ['remove:Strike#0', 'remove:Strike#1', 'remove:Strike#2']);
  assert.match(policy.conditions.join(' '), /Inflame.*transition attacks: 3/);
  assert.match(policy.preference, /offered purchases and keeping Gold/);
  input.position.actions.reverse();
  assert.deepEqual(preparationShopRemovalPolicy(input).commands.sort(), [...policy.commands].sort());
  for (const change of [
    current => { current.position.observation.observation.run.act = 1; },
    current => { current.position.observation.observation.run.gold = 74; },
    current => { current.deck_evidence.cards[1].mechanics.strength_gain = 0; },
    current => { current.deck_evidence.cards[4].persistent_cost = 4; },
    current => { current.deck_evidence.cards[0].persistent_damage = 9; },
    current => { current.position.observation.observation.cards[0].cost_modifier = -1; },
    current => { current.position.observation.observation.relics = [{ relic: 'StrikeDummy' }]; },
    current => { current.position.observation.observation.relics = [{ relic: 'BottledFlame' }]; },
    current => { current.position.observation.observation.context.kind = 'campfire'; },
  ]) {
    const current = build(); change(current);
    assert.equal(preparationShopRemovalPolicy(current), undefined);
  }
  for (const [id, type] of [['Normality', 'Curse'], ['PerfectedStrike', 'Attack']]) {
    const current = build(), row = fact(id, type, 1);
    current.deck_evidence.cards.push(row); current.position.observation.observation.cards.push(row.card);
    current.position.actions.push({ semantic_action: `remove:${id}#0`, candidate: { action: {
      kind: 'remove_card', card: id, upgrades: 0, price: 75 } } });
    assert.equal(preparationShopRemovalPolicy(current), undefined);
  }
  const ambiguous = build(), modified = fact('Strike', 'Attack', 0);
  modified.card.cost_modifier = -1; modified.persistent_damage = 6;
  ambiguous.deck_evidence.cards.push(modified); ambiguous.position.observation.observation.cards.push(modified.card);
  assert.equal(preparationShopRemovalPolicy(ambiguous), undefined);
  const unremovable = build(), bane = fact('AscendersBane', 'Curse', -2);
  unremovable.deck_evidence.cards.push(bane); unremovable.position.observation.observation.cards.push(bane.card);
  assert.deepEqual(preparationShopRemovalPolicy(unremovable).commands, policy.commands);
});

test('established Strength removal handles upgraded basics and retains the native Strike-count tradeoff', () => {
  const build = () => {
    const strike = fact('Strike', 'Attack', 1, { tags: ['Strike'] }, 4);
    strike.card.upgrades = strike.mechanics.upgrades = 1; strike.persistent_damage = 9;
    const perfected = fact('PerfectedStrike', 'Attack', 2, { tags: ['Strike'] });
    const current = planRoot([strike, perfected,
      fact('SpotWeakness', 'Skill', 1, { tags: [], strength_gain: 4,
        strength_gain_delivery: 'if_target_intent_base_damage_nonnegative_at_resolution' }),
      ...['SwordBoomerang', 'Pummel', 'Uppercut'].map(id => fact(id, 'Attack', 1, { tags: [] }))], 'shop', []);
    current.position.observation.observation.context.inventory = { purge: { price: 100, purchase_status: { status: 'purchasable' } } };
    current.position.actions = Array.from({ length: 4 }, (_, i) => ({ semantic_action: `remove:Strike+#${i}`,
      candidate: { action: { kind: 'remove_card', card: 'Strike', upgrades: 1, price: 100 } } }));
    return current;
  };
  const current = build(), policy = preparationShopRemovalPolicy(current);
  assert.equal(policy.commands.length, 4);
  assert.match(policy.conditions.join(' '), /Strike\+: 4 copies.*9 base damage.*5->4.*PerfectedStrike.*Other payable transition attacks: 3/);
  current.position.actions.reverse();
  assert.deepEqual(preparationShopRemovalPolicy(current).commands.sort(), policy.commands.sort());
  for (const change of [
    value => { delete value.deck_evidence.cards[1].mechanics.tags; },
    value => { value.deck_evidence.cards.at(-1).persistent_cost = 4; },
    value => { value.deck_evidence.cards[0].persistent_damage = 12; },
    value => { value.position.observation.observation.relics = [{ relic: 'StrikeDummy' }]; },
  ]) { const boundary = build(); change(boundary); assert.equal(preparationShopRemovalPolicy(boundary), undefined); }
  const ordinary = fact('Strike', 'Attack', 1, { tags: ['Strike'] }); ordinary.persistent_damage = 6;
  current.deck_evidence.cards.push(ordinary); current.position.observation.observation.cards.push(ordinary.card);
  current.position.actions.push({ semantic_action: 'remove:Strike#0', candidate: {
    action: { kind: 'remove_card', card: 'Strike', upgrades: 0, price: 100 } } });
  assert.deepEqual(preparationShopRemovalPolicy(current).commands, ['remove:Strike#0']);
});

test('covered output supplies a removal class without Strength and keeps the Strike-tag cost visible', () => {
  const build = () => {
    const strike = fact('Strike', 'Attack', 1, { tags: ['Strike'] }, 5); strike.persistent_damage = 6;
    const damage = { delivery: { kind: 'attack', amount: { kind: 'base_plus_strike_cards', per_card: 3 },
      hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } };
    const perfected = fact('PerfectedStrike', 'Attack', 2, { tags: ['Strike'], damage }); perfected.persistent_damage = 6;
    const current = planRoot([strike, perfected,
      ...['IronWave', 'Headbutt', 'Uppercut'].map(id => fact(id, 'Attack', 1, { tags: [] }))], 'shop', ['Combust']);
    const state = current.position.observation.observation;
    state.potions = [{ slot: 0, potion: null }, { slot: 1, potion: null }];
    state.context.inventory = { purge: { price: 75, purchase_status: { status: 'purchasable' } } };
    for (const row of current.deck_evidence.cards) current.position.actions.push({ semantic_action: `remove:${row.card.card}#0`,
      candidate: { action: { kind: 'remove_card', card: row.card.card, upgrades: 0, price: 75 } } });
    current.position.actions.push({ semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } });
    return current;
  };
  const current = build(), policy = preparationShopRemovalPolicy(current);
  assert.equal(policy.id, 'covered-output-basic-removal');
  assert.deepEqual(policy.commands, ['remove:Strike#0']);
  assert.match(policy.conditions.join(' '), /enduring Strength: none.*6 base damage per hit \+ 3 x 6.*6->5/);
  assert.match(policy.preference, /offered purchases and keeping Gold/);
  assert.deepEqual(preparationReading(current).position.actions.map(row => row.semantic_action), ['buy:Combust#0', 'remove:Strike#0', 'leave']);
  current.position.actions.reverse();
  assert.deepEqual(preparationShopRemovalPolicy(current).commands, policy.commands);
  for (const change of [
    row => { row.deck_evidence.cards[1].mechanics.damage.delivery.amount.per_card = 1; },
    row => { delete row.deck_evidence.cards[1].mechanics.tags; },
    row => { row.deck_evidence.cards.at(-1).persistent_cost = 4; },
    row => { row.position.observation.observation.relics = [{ relic: 'SneckoEye' }]; },
    row => { row.position.observation.observation.relics = [{ relic: 'VelvetChoker' }]; },
    row => { row.position.observation.observation.relics = [{ relic: 'StrikeDummy' }]; },
  ]) { const boundary = build(); change(boundary); assert.equal(preparationShopRemovalPolicy(boundary), undefined); }
});

test('repeated Combust advice binds the actual owned and offered turn-end requests', () => {
  const effect = (hp, damage) => ({ hp_loss_per_application: hp, damage_per_enemy_per_application: damage });
  const owned = fact('Combust', 'Power', 1, { end_of_turn_hp_loss_and_damage: effect(1, 7) }, 2);
  const current = planRoot([owned], 'shop', ['Combust']);
  current.decision_evidence = { kind: 'card_acquisition_options', candidates: [{ semantic_action: 'buy:Combust#0',
    cards: [{ current: { card: 'Combust', upgrades: 0, card_type: 'Power', cost: 1,
      end_of_turn_hp_loss_and_damage: effect(1, 5) } }] }] };
  let advice = preparationExpertGuidance(current).candidates[0];
  assert.match(advice.preference, /protection, access or recovery.*shopping Gold/);
  assert.match(advice.change, /copies 2->3.*HP loss 2->3.*damage per enemy 14->19.*actual access/);
  current.decision_evidence.candidates[0].cards[0].current.end_of_turn_hp_loss_and_damage = effect(1, 7);
  assert.match(preparationExpertGuidance(current).candidates[0].change, /damage per enemy 14->21/);
  current.deck_evidence.public_snapshot_id = 'stale';
  assert.match(preparationCardExpertGuidance(current)[0].preference, /first affordable/);
  current.deck_evidence.cards = []; current.position.observation.observation.cards = [];
  assert.match(preparationCardExpertGuidance(current)[0].preference, /first affordable/);
});

test('independent access remains a comparison as existing draw quantity changes', () => {
  const growth = fact('SpotWeakness', 'Skill', 1, { strength_gain: 4,
    strength_gain_delivery: 'if_target_intent_base_damage_nonnegative_at_resolution' });
  const shrug = fact('ShrugItOff', 'Skill', 1, { draw_cards: 1 });
  const build = (owned, act = 2, draw = 3, cost = 0) => {
    const current = planRoot(owned, 'card_reward', ['BattleTrance', 'BodySlam']);
    current.position.observation.observation.run.act = act;
    current.decision_evidence = { kind: 'card_acquisition_options', candidates: [
      { semantic_action: 'take:BattleTrance#0', cards: [{ current: { card: 'BattleTrance', upgrades: 0,
        card_type: 'Skill', cost, draw_cards: draw } }] },
      { semantic_action: 'take:BodySlam#0', cards: [{ current: { card: 'BodySlam', upgrades: 0,
        card_type: 'Attack', cost: 1 } }] },
    ] };
    return current;
  };
  const current = build([growth, shrug]);
  Object.assign(current, { schema: 'sts-run-play-agent-observation-v3', session_id: 'access-baseline', head_revision: 0, state_revision: 0 });
  const priority = preparationExpertGuidance(current).construction.priorities.find(row => row.id === 'independent-access-window');
  assert.ok(priority);
  assert.match(priority.ownedFacts.join(' '), /SpotWeakness/);
  assert.match(priority.ownedFacts.join(' '), /ShrugItOff: 1 Energy, draw 1, Hand contribution 0/);
  assert.match(priority.ownedFacts.join(' '), /take:BattleTrance#0: 0 Energy, on-play draw 3, Hand contribution 2/);
  assert.match(priority.preference, /Favor a first workable independent draw/);
  assert.deepEqual(priority.comparisonBaseline, { commands: ['take:BattleTrance#0'], comparisonIds: ['@access'] });
  current.position.actions.push({ semantic_action: 'take:PowerThrough#0',
    candidate: { action: { kind: 'take_card', card: 'PowerThrough', upgrades: 0 } } });
  const classification = { source: current, rows: current.position.actions.map(row => ({ command: row.semantic_action,
    category: row.semantic_action.includes('BattleTrance') ? 'medium' : 'high', realization: 'direct', reason: 'original model rating' })) };
  assert.match(preparationClassificationContext(current), /take:BattleTrance#0: 0 Energy, on-play draw 3/);
  assert.match(preparationRecordedTable(classification, current), /take:BattleTrance#0: 0 Energy, on-play draw 3/);
  assert.throws(() => preparationCommandBasis(['take:BodySlam#0', []], 'pending',
    'take:PowerThrough#0 | @function,@protection', current, classification), /Compare this choice with a current independent-access-window candidate/);
  assert.throws(() => preparationCommandBasis(['take:BodySlam#0', []], 'pending',
    'take:BattleTrance#0 | @function', current, classification), /Include @access/);
  const conversion = preparationCommandBasis(['take:BodySlam#0', []], 'pending',
    'take:BattleTrance#0 | @access,@protection', current, classification);
  assert.equal(conversion.selectedRow.category, 'high', 'the comparison requirement preserves model grades and permits another function');
  assert.equal(conversion.alternativeRow.category, 'medium');
  assert.equal(conversion.priorityComparisons[0].id, priority.id);
  const automatic = preparationCommandBasis(['take:BodySlam#0', []], 'pending', undefined, current, classification);
  assert.equal(automatic.command, 'take:BodySlam#0');
  assert.equal(automatic.alternativeRow.command, 'take:BattleTrance#0');
  assert.equal(automatic.basisKind, 'host-comparison');
  assert.ok(automatic.comparisonIds.includes('@access'));
  assert.ok(preparationCommandBasis(['take:BattleTrance#0', []], 'pending',
    'take:BodySlam#0 | @access,@function', current, classification));
  current.position.actions[0].semantic_action = 'take:BattleTrance+0#0';
  current.decision_evidence.candidates[0].semantic_action = 'take:BattleTrance+0#0';
  assert.deepEqual(preparationExpertGuidance(current, conciseCardCommand).construction.priorities
    .find(row => row.id === priority.id).comparisonBaseline.commands, ['take:BattleTrance#0']);
  assert.match(preparationClassificationContext(current, conciseCardCommand), /take:BattleTrance#0: 0 Energy, on-play draw 3/);
  const existingDraw = fact('PommelStrike', 'Attack', 1, { draw_cards: 2 });
  current.position.observation.observation.cards.push(existingDraw.card);
  current.deck_evidence.cards.push(existingDraw);
  assert.throws(() => preparationCommandBasis(['take:BodySlam#0', []], 'pending',
    'take:PowerThrough#0 | @function,@protection', current, classification, conciseCardCommand),
  /Compare this choice with a current independent-access-window candidate/);
  const withDraw = preparationExpertGuidance(current).construction.priorities.find(row => row.id === priority.id);
  assert.match(withDraw.preference, /additional access using the owned draw quantities, copy frequency, lifecycle and payment/);
  assert.doesNotMatch(withDraw.preference, /Favor a first/);
  assert.match(withDraw.ownedFacts.join(' '), /1 of 3 original cards/);
  assert.ok(preparationCommandBasis(['take:BodySlam#0', []], 'pending',
    'take:BattleTrance#0 | @access,@function', current, classification, conciseCardCommand),
  'the model can still choose conversion while comparing its actual access alternative');
  const has = current => ids(preparationExpertGuidance(current)).includes(priority.id);
  assert.ok(has(build([fact('Barricade', 'Power', 3), shrug])));
  assert.ok(!has(build([growth, shrug], 1)));
  assert.ok(!has(build([growth, shrug], 2, 1)));
  assert.ok(!has(build([growth, shrug], 2, 3, 4)));
  assert.ok(!has(build([shrug])));
  assert.ok(has(build([growth, fact('BurningPact', 'Skill', 1, { draw_cards: 2, exhausts_other_cards: true })])));
  const plentiful = build([growth, fact('PommelStrike', 'Attack', 1, { draw_cards: 2 }, 4)]);
  assert.match(preparationExpertGuidance(plentiful).construction.priorities.find(row => row.id === priority.id).ownedFacts.join(' '),
    /4 of 5 original cards/);
  assert.ok(has(build([growth, shrug, fact('DarkEmbrace', 'Power', 2, { exhaust_draw_power: 1 })])),
    'deployed-trigger draw keeps its distinct setup sequence');
});

test('Boss Energy guidance binds an actual payment threshold, retained Gold and owned cleanup', () => {
  const observation = planRoot([fact('Uppercut', 'Attack', 2, {}, 2), fact('Immolate', 'Attack', 2)], 'boss_relic', []);
  const state = observation.position.observation.observation;
  state.run.gold = 388;
  state.relics = [{ relic: 'PeacePipe' }];
  observation.position.actions = [{ semantic_action: 'boss-relic:Ectoplasm#0',
    candidate: { action: { kind: 'take_boss_relic', relic: 'Ectoplasm' } } }];
  observation.decision_evidence = { candidates: [{ semantic_action: 'boss-relic:Ectoplasm#0',
    definition: { relic: 'Ectoplasm', energy_per_turn_delta: 1, gold_gain_rule: 'prevent_gold_gain' } }] };
  const priority = preparationExpertGuidance(observation).construction.priorities.find(row => row.id === 'recurring-payment-window');
  assertConstructionLanguageContract(observation);
  assert.ok(priority);
  assert.match(priority.ownedFacts.join('\n'), /base Energy 3->4.*Uppercut \(2\) with Uppercut \(2\)/);
  assert.match(priority.ownedFacts.join('\n'), /Current Gold 388.*PeacePipe/);
  assert.match(priority.preference, /one-time HP, Gold.*lost future income/);
  assert.match(preparationRecordedTable({ rows: [{ command: 'boss-relic:Ectoplasm#0',
    category: 'high', realization: 'direct', reason: 'Funds the owned paid plays.' }] }, observation), /base Energy 3->4/);
  state.base_energy_per_turn = 4;
  assert.ok(!ids(preparationExpertGuidance(observation)).includes('recurring-payment-window'));
  state.base_energy_per_turn = 3;
  observation.position.actions = [];
  assert.ok(!ids(preparationExpertGuidance(observation)).includes('recurring-payment-window'));
});

test('owned growth and Exhaust access organize both shop and draft decisions before candidate advice', () => {
  const cards = [fact('DemonForm', 'Power', 3, { strength_gain: 2, strength_gain_delivery: 'each_player_turn_start_post_draw' }),
    fact('DarkEmbrace', 'Power', 2, { exhaust_draw_power: 1 }), fact('TrueGrit', 'Skill', 1, { exhausts_other_cards: true })];
  for (const site of ['shop', 'card_reward', 'map', 'campfire']) {
    const observation = planRoot(cards, site, site === 'map' || site === 'campfire' ? [] : undefined);
    const guidance = preparationExpertGuidance(observation);
    assertConstructionLanguageContract(observation);
    assert.ok(ids(guidance).includes('deliver-owned-growth'), site);
    assert.ok(ids(guidance).includes('start-owned-exhaust-access'), site);
    const text = renderExpertGuidance(guidance);
    assert.ok(text.indexOf('Current facts:') < text.indexOf('Owned functions and their development conditions:'));
    assert.match(text, /Owned basis: 1x DemonForm/);
    if (guidance.candidates.length) assert.ok(guidance.candidates[0].priorityIds.includes('deliver-owned-growth'));
  }
});

test('owned conditional and immediate enduring Strength organize development with their actual timing', () => {
  for (const [card, delivery] of [['SpotWeakness', 'if_target_intent_base_damage_nonnegative_at_resolution'],
    ['Inflame', 'on_play']]) {
    const source = fact(card, card === 'Inflame' ? 'Power' : 'Skill', 1,
      { strength_gain: 3, strength_gain_delivery: delivery });
    const current = planRoot([source, fact('Reaper', 'Attack', 2)], 'card_reward', ['Rampage', 'BattleTrance']);
    const guidance = preparationExpertGuidance(current);
    const priority = guidance.construction.priorities.find(row => row.id === 'deliver-owned-growth');
    assertConstructionLanguageContract(current);
    assert.ok(priority);
    assert.match(priority.ownedFacts.join('\n'), new RegExp(`${card}: 1 Energy; Strength request 3 at ${delivery}`));
    assert.match(preparationClassificationContext(current), /another source's earlier access and additional useful gain/);
    assert.ok(guidance.candidates.find(row => row.card === 'BattleTrance').priorityIds.includes('deliver-owned-growth'));
    current.deck_evidence.cards[0].mechanics.temporary_strength_gain = 3;
    assert.ok(!ids(preparationExpertGuidance(current)).includes('deliver-owned-growth'));
  }
});

test('first offered recurring Strength connects owned multi-hit output to payable setup protection', () => {
  const converters = [fact('Pummel', 'Attack', 1, { damage: { delivery: { kind: 'attack', hits: { kind: 'fixed', count: 5 } } } }),
    fact('Whirlwind', 'Attack', -1, { damage: { delivery: { kind: 'attack', hits: { kind: 'per_effective_x' } } } })];
  const protection = [fact('Disarm', 'Skill', 1, { enemy_strength_down: 2 }, 2),
    fact('Blind', 'Skill', 0, { weak: 2 }), fact('FlameBarrier', 'Skill', 2, { block: 12 })];
  const observation = planRoot([...converters, ...protection], 'card_reward', ['DemonForm']);
  const state = observation.position.observation.observation;
  state.base_energy_per_turn = 4;
  observation.position.actions[0].candidate.action.upgrades = 0;
  observation.decision_evidence = { candidates: [{ semantic_action: 'take:DemonForm#0', cards: [{ current: {
    card: 'DemonForm', upgrades: 0, cost: 3, strength_gain: 2, strength_gain_delivery: 'each_player_turn_start_post_draw',
  } }] }] };
  const guidance = preparationExpertGuidance(observation);
  const priority = guidance.construction.priorities.find(row => row.id === 'first-strength-growth');
  assertConstructionLanguageContract(observation);
  assert.match(priority.ownedFacts.join('\n'), /deploy for 3 of 4 Energy, leaving 1/);
  assert.match(priority.ownedFacts.join('\n'), /1x Pummel, 1x Whirlwind/);
  assert.match(priority.ownedFacts.join('\n'), /protection.*2x Disarm, 1x Blind/);
  assert.doesNotMatch(priority.ownedFacts.join('\n'), /FlameBarrier/);
  assert.ok(guidance.candidates[0].priorityIds.includes('first-strength-growth'));
  assert.ok(!ids(guidance).includes('deliver-owned-growth'), 'the offered source remains an acquisition');
  state.cards.push(fact('Inflame', 'Power', 1, { strength_gain: 2 }).card);
  observation.deck_evidence.cards.push(fact('Inflame', 'Power', 1, { strength_gain: 2 }));
  assert.ok(!ids(preparationExpertGuidance(observation)).includes('first-strength-growth'));
  state.cards.pop(); observation.deck_evidence.cards.pop();
  observation.decision_evidence.candidates[0].cards[0].current.cost = 5;
  assert.ok(!ids(preparationExpertGuidance(observation)).includes('first-strength-growth'));
});

test('owned payment needs access and its triggers; new draw updates that priority from current inventory', () => {
  const cards = [fact('Corruption', 'Power', 3), fact('Sentinel', 'Skill', 1, { energy_gain_on_exhaust: 2 }, 3),
    fact('ShrugItOff', 'Skill', 1, { draw_cards: 1 })];
  const observation = planRoot(cards), saved = structuredClone(observation);
  const before = preparationExpertGuidance(observation);
  assertConstructionLanguageContract(observation);
  assert.ok(ids(before).includes('access-for-owned-payment'));
  assert.ok(ids(before).includes('skill-lifecycle'));
  assert.match(renderExpertGuidance(before), /3x Sentinel/);
  assert.match(renderExpertGuidance(before), /source card itself is exhausted/);
  assert.deepEqual(observation, saved);
  const drawn = fact('BattleTrance', 'Skill', 0, { draw_cards: 3 });
  observation.position.observation.observation.cards.push(drawn.card);
  observation.deck_evidence.cards.push(drawn);
  const after = preparationExpertGuidance(observation).construction.priorities.find(row => row.id === 'access-for-owned-payment');
  assertConstructionLanguageContract(observation);
  assert.ok(after);
  assert.match(after.preference, /additional access using the owned draw quantities/);
  assert.match(after.ownedFacts.join(' '), /1 of 6 original cards/);
  assert.ok(ids(before).includes('access-for-owned-payment'));
});

test('a first Strength amplifier supplies its owned source, conversion and one-use payoff to both stages', () => {
  const observation = planRoot([
    fact('DemonForm', 'Power', 3, { strength_gain: 2, strength_gain_delivery: 'each_player_turn_start_post_draw' }),
    fact('Pummel', 'Attack', 1, { exhausts: true, damage: { delivery: { kind: 'attack', hits: { kind: 'fixed', count: 5 } } } }),
    fact('BattleTrance', 'Skill', 0, { draw_cards: 3 }),
  ], 'card_reward', ['LimitBreak']);
  observation.decision_evidence = { candidates: [{ semantic_action: 'take:LimitBreak#0', cards: [{ current: {
    card: 'LimitBreak', upgrades: 0, cost: 1, exhausts: true,
  } }] }] };
  const saved = structuredClone(observation);
  const guidance = preparationExpertGuidance(observation);
  const priority = guidance.construction.priorities.find(row => row.id === 'first-strength-amplifier');
  assertConstructionLanguageContract(observation);
  assert.match(priority.ownedFacts.join('\n'), /DemonForm.*Strength request 2 at each_player_turn_start_post_draw/);
  assert.match(priority.ownedFacts.join('\n'), /Pummel.*5 Strength applications per play; Exhaust after play/);
  assert.match(priority.ownedFacts.join('\n'), /Owned on-play access: 1x BattleTrance/);
  assert.match(priority.preference, /S times H.*including after the amplifier Exhausts/);
  assert.ok(guidance.candidates[0].priorityIds.includes('first-strength-amplifier'));
  assert.match(preparationClassificationContext(observation, command => command), /first offered Strength amplifier/);
  assert.match(preparationRecordedTable({ rows: [{ command: 'take:LimitBreak#0',
    category: 'high', realization: 'sequence', reason: 'Amplify the owned source before later attacks.' }] }, observation),
  /first offered Strength amplifier/);
  assert.deepEqual(observation, saved);
});

test('first-amplifier preference requires an existing reviewed enduring source and a payable legal addition', () => {
  const source = fact('Inflame', 'Power', 1, { strength_gain: 2, strength_gain_delivery: 'on_play' });
  const converter = fact('TwinStrike', 'Attack', 1, { damage: { delivery: { kind: 'attack', hits: { kind: 'fixed', count: 2 } } } });
  const build = rows => {
    const observation = planRoot(rows, 'shop', ['LimitBreak']);
    observation.decision_evidence = { candidates: [{ semantic_action: 'buy:LimitBreak#0', cards: [{ current: {
      card: 'LimitBreak', upgrades: 0, cost: 1, exhausts: true,
    } }] }] };
    return observation;
  };
  const hasPriority = observation => ids(preparationExpertGuidance(observation)).includes('first-strength-amplifier');
  assert.ok(hasPriority(build([source, converter])));
  assert.ok(!hasPriority(build([converter])));
  assert.ok(!hasPriority(build([fact('Flex', 'Skill', 0, { strength_gain: 2, temporary_strength_gain: 2,
    strength_gain_delivery: 'on_play_with_separate_lose_strength' }), converter])));
  assert.ok(!hasPriority(build([fact('Inflame', 'Power', 1, { strength_gain: 2 }), converter])));
  assert.ok(!hasPriority(build([source, converter, fact('LimitBreak', 'Skill', 1, { exhausts: true })])));
  const overBudget = build([source, converter]);
  overBudget.decision_evidence.candidates[0].cards[0].current.cost = 4;
  assert.ok(!hasPriority(overBudget));
  const wrongSource = build([source, converter]);
  wrongSource.position.actions = [];
  assert.ok(!hasPriority(wrongSource));
});

test('offered engines stay outside the owned plan and facts keep their source binding', () => {
  const observation = planRoot([fact('Strike', 'Attack', 1)], 'card_reward', ['DemonForm', 'Corruption', 'DarkEmbrace']);
  const guidance = preparationExpertGuidance(observation);
  assert.deepEqual(ids(guidance), ['development-window']);
  assert.equal(guidance.construction.publicSnapshotId, 'current');
  observation.deck_evidence.public_snapshot_id = 'old';
  assert.throws(() => preparationExpertGuidance(observation), /current public inventory/);
  delete observation.deck_evidence;
  assert.equal(preparationExpertGuidance(observation).construction, undefined);
});

test('the upcoming Act and current Status burden reach classification and refreshed comparison', () => {
  const observation = planRoot([fact('PowerThrough', 'Skill', 1,
    { status_injections: [{ card: 'Wound', count: 2, destination: 'hand', persistence: 'persistent' }] })], 'card_reward', ['Evolve']);
  observation.position.observation.observation.run = { act: 1, pending_act_entry: 2 };
  const guidance = preparationExpertGuidance(observation);
  assertConstructionLanguageContract(observation);
  assert.equal(guidance.construction.preparationAct, 2);
  assert.ok(ids(guidance).includes('generated-status-support'));
  assert.match(renderExpertGuidance(guidance), /2 Wound into hand/);
  assert.match(preparationClassificationContext(observation), /Pair the owned Status-generating plays/);
  assert.match(preparationRecordedTable({ rows: [{ command: 'take:Evolve#0', category: 'medium', realization: 'sequence', reason: 'owned Status source' }] }, observation), /Pair the owned Status-generating plays/);
});

test('recent HP cost favors reachable first-Elite preparation even with independent draw', () => {
  const observation = planRoot([fact('Bloodletting', 'Skill', 0,
    { energy_gain_rule: { delivery: 'on_play', amount: 2 } })], 'map', []);
  const state = observation.position.observation.observation;
  state.public_history = { recent_combat_hp_loss: 21 };
  observation.position.actions = ['route:3.2', 'route:5.2'].map(semantic_action => ({ semantic_action }));
  observation.decision_evidence = { kind: 'route_paths', candidates: [false, true].map((campfire, index) => ({
    semantic_action: observation.position.actions[index].semantic_action,
    route_semantics: { first_elite: { observed_paths_with_first_elite: 1,
      shops_before: { count_per_observed_path: { min: 0, max: 0 } },
      campfires_before: { count_per_observed_path: { min: 0, max: campfire ? 1 : 0 } } },
    shops: { count_per_observed_path: { min: 0, max: 1 } },
    campfires: { count_per_observed_path: { min: 1, max: 2 } } },
    path: { coverage: 'complete_within_horizon', first_elite: { paths_with_first_elite: 1,
      can_bail_to_rest_before: campfire, can_bail_to_shop_before: false,
      min_hallway_fights_before: 0, max_hallway_fights_before: 2, min_unknowns_before: 1, max_unknowns_before: 3 } },
  })) };
  const normalize = command => command.replace('route:', 'go ').replace('.', ',');
  const guidance = preparationExpertGuidance(observation, normalize);
  const priority = guidance.construction.priorities.find(row => row.id === 'preserve-first-elite-preparation');
  assertConstructionLanguageContract(observation, normalize);
  assert.ok(priority);
  assert.ok(priority.ownedFacts.some(text => text.includes('go 5,2')));
  assert.ok(!priority.ownedFacts.some(text => text.includes('go 3,2')));
  assert.match(preparationClassificationContext(observation, normalize), /Last adopted combat net HP loss 21/);
  assert.match(preparationRecordedTable({ rows: ['go 3,2', 'go 5,2'].map(command => ({ command,
    category: 'medium', realization: 'direct', reason: 'compare connected preparation' })) },
  observation, normalize), /preparation window, default to retaining useful recovery/);
  state.run.current_hp = state.run.max_hp;
  assert.ok(!ids(preparationExpertGuidance(observation, normalize)).includes('preserve-first-elite-preparation'));
  state.run.current_hp = 55;
  state.cards.push(fact('BattleTrance', 'Skill', 0).card);
  observation.deck_evidence.cards.push(fact('BattleTrance', 'Skill', 0, { draw_cards: 3 }));
  const withDraw = preparationExpertGuidance(observation, normalize).construction.priorities
    .find(row => row.id === 'preserve-first-elite-preparation');
  assert.ok(withDraw);
  assert.match(withDraw.ownedFacts.join('\n'), /1x BattleTrance/);
  state.public_history.recent_combat_hp_loss = 0;
  assert.ok(!ids(preparationExpertGuidance(observation, normalize)).includes('preserve-first-elite-preparation'));
});

test('owned Block retention binds actual supply, multiplication and conversion in both stages', () => {
  const block = fact('PowerThrough', 'Skill', 1, { base_block_delivery: 'once_on_play', block: 20 });
  block.persistent_block = 23;
  const entrench = fact('Entrench', 'Skill', 1);
  entrench.card.upgrades = entrench.mechanics.upgrades = 1;
  const rows = [fact('Barricade', 'Power', 2), block, entrench,
    fact('BodySlam', 'Attack', 0, { damage: { delivery: { kind: 'attack', amount: { kind: 'player_block' } } } }),
    fact('PommelStrike', 'Attack', 1, { draw_cards: 2 })];
  for (const site of ['card_reward', 'shop', 'campfire']) {
    const observation = planRoot(rows, site, ['Entrench', 'BodySlam']);
    const guidance = preparationExpertGuidance(observation);
    const priority = guidance.construction.priorities.find(row => row.id === 'develop-owned-block-reserve');
    assertConstructionLanguageContract(observation);
    assert.ok(priority);
    assert.match(priority.ownedFacts.join('\n'), /Barricade: deploy for 2 Energy/);
    assert.match(priority.ownedFacts.join('\n'), /PowerThrough \(1 Energy, 23 Block\)/);
    assert.match(priority.ownedFacts.join('\n'), /Entrench\+: 1 Energy.*ordinary redraw/);
    assert.match(priority.ownedFacts.join('\n'), /BodySlam: 0 Energy; current Block/);
    assert.match(preparationClassificationContext(observation), /Cross-turn retention first requires deployment/);
    assert.match(preparationRecordedTable({ rows: observation.position.actions.map(row => ({ command: row.semantic_action,
      category: 'high', realization: 'sequence', reason: 'use owned reserve' })) }, observation),
    /Cross-turn retention first requires deployment/);
    assert.ok(guidance.candidates.every(row => row.priorityIds.includes('develop-owned-block-reserve')));
  }
  const offeredOnly = planRoot([block], 'card_reward', ['Barricade', 'Entrench', 'BodySlam']);
  assert.ok(!ids(preparationExpertGuidance(offeredOnly)).includes('develop-owned-block-reserve'));
  assert.match(renderExpertGuidance(preparationExpertGuidance(offeredOnly)), /generator and Entrench in the same Hand and pay for both/);
  const retainedOnly = planRoot([fact('Barricade', 'Power', 3)], 'card_reward', ['Entrench']);
  assert.match(renderExpertGuidance(preparationExpertGuidance(retainedOnly)), /none listed; other protection/);
});

test('strategy matching stays auditable while candidate advice belongs to its local command', () => {
  const current = planRoot([fact('Barricade', 'Power', 3), fact('Entrench', 'Skill', 1),
    fact('DeepBreath', 'Skill', 0, { draw_cards: 2 })], 'card_reward', ['BodySlam', 'Bloodletting']);
  const guidance = preparationExpertGuidance(current);
  const body = guidance.candidates.find(row => row.card === 'BodySlam');
  assert.ok(body.priorityIds.includes('develop-owned-block-reserve'));
  assert.ok(body.familyIds.includes('scaling'));
  assert.ok(guidance.sources.cx.reference);
  const supplied = preparationClassificationContext(current);
  for (const priority of guidance.construction.priorities) assert.ok(!supplied.includes(priority.id));
  assert.doesNotMatch(supplied, /Development priorities:|Strategy topics:|Current access candidates:/);
  assert.match(supplied, /Owned functions and their development conditions:/);
  assert.match(supplied, /Cross-turn retention first requires deployment, then Block remaining after enemy actions/);
  assert.match(supplied, /another multiplication also requires its redraw or recovery/);
  const start = supplied.indexOf('Condition references for take:BodySlam#0:');
  const end = supplied.indexOf('Condition references for take:Bloodletting#0:');
  const bodySection = supplied.slice(start, end);
  assert.match(bodySection, /Offered BodySlam \(take:BodySlam#0\):/);
  assert.doesNotMatch(bodySection, /Offered Bloodletting/);
  assert.equal(supplied.split('Offered BodySlam (').length - 1, 1);
  assert.doesNotMatch(renderExpertGuidance(guidance, { includeCandidates: false }), /Offered BodySlam/);
});
