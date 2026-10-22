import test from 'node:test';
import assert from 'node:assert/strict';
import { preparationDeckComparison, renderDeckComparison, preparationDeckConditionBindings, preparationCardAcquisition, renderStatusRoutes } from './deck.mjs';
import { preparationConditionBindings, parsePreparationClassification } from './preparation.mjs';
import { preparationExpertGuidance, preparationCardPolicies } from './strategy.mjs';

function owned(card, count, type, cost, extra = {}, upgrades = 0) {
  return { card: { card, count, upgrades, misc_value: 0, cost_modifier: 0,
    base_damage_override: null, base_block_override: null },
  mechanics: { card, upgrades, card_type: type, cost, draw_cards: 0,
    draw_cards_each_turn: 0, exhausts: false, ethereal: false, ...extra },
  persistent_cost: cost, persistent_damage: 0, persistent_block: 0 };
}
function root(rows, offers = []) {
  const position = { public_snapshot_id: 'root', observation: { domain: 'strategic', observation: {
    cards: rows.map(row => row.card), base_energy_per_turn: 3 } }, actions: [
    ...offers.map(row => ({ semantic_action: `take:${row.card.card}#0`, candidate: { action: {
      kind: 'take_card', card: row.card.card, upgrades: row.card.upgrades } } })),
    { semantic_action: 'skip-card#0', candidate: { action: { kind: 'skip_card_reward' } } },
  ] };
  return { schema: 'sts-run-play-agent-observation-v3', session_id: 'deck-unit', head_revision: 0, state_revision: 0,
    position, deck_evidence: { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: 'root', cards: rows },
    decision_evidence: { kind: 'card_acquisition_options', candidates: offers.map(row => ({
      semantic_action: `take:${row.card.card}#0`, cards: [{ current: row.mechanics }] })) } };
}

test('deck comparison languages preserve access probabilities and self-Exhaust realization requirements', () => {
  const sentinel = owned('Sentinel', 1, 'Skill', 1, { base_block_delivery: 'once_on_play', block: 5, energy_gain_on_exhaust: 2 });
  for (const sources of [[], [owned('TrueGrit', 2, 'Skill', 1, { exhausts_other_cards: true, selects_from_hand: true })]]) {
    const observation = root(sources, [sentinel]);
    const before = JSON.stringify(observation);
    const profiles = ['en', 'zh-CN'].map(locale => preparationDeckComparison(observation, undefined, locale));
    assert.deepEqual(profiles[0].candidates.map(row => row.access?.map(access => access.windows)), profiles[1].candidates.map(row => row.access?.map(access => access.windows)));
    assert.equal(profiles[0].otherCardExhaustCopies, sources.length ? 2 : 0);
    assert.equal(profiles[0].otherCardExhaustCopies, profiles[1].otherCardExhaustCopies);
    const bindings = ['en', 'zh-CN'].map(locale => preparationDeckConditionBindings(observation, undefined, locale));
    assert.deepEqual(bindings[0].map(row => [row.command, row.id, row.choices.map(choice => [choice.id, choice.realizations])]),
      bindings[1].map(row => [row.command, row.id, row.choices.map(choice => [choice.id, choice.realizations])]));
    const route = bindings[1].find(row => row.command === 'take:Sentinel#0').choices.find(row => row.id === '@self-exhaust-energy');
    assert.deepEqual(route.realizations, sources.length ? ['sequence'] : ['future']);
    assert.match(renderDeckComparison(profiles[1], { locale: 'zh-CN' }), /完整卡组比较基线/);
    assert.equal(JSON.stringify(observation), before);
  }
});

test('first Exhaust-draw policy uses typed copy counts independently of the display of an empty route', () => {
  const offer = owned('DarkEmbrace', 1, 'Power', 2, { exhaust_draw_power: 1 });
  const observation = root([owned('TrueGrit', 1, 'Skill', 1, { exhausts_other_cards: true })], [offer]);
  Object.assign(observation.position.observation.observation, { run: { act: 2 }, context: { kind: 'card_reward' }, relics: [] });
  const profile = preparationDeckComparison(observation, undefined, 'zh-CN');
  const policies = preparationCardPolicies(observation, profile);
  const selected = policies.find(row => row.id === 'first-exhaust-draw');
  assert.ok(selected);
  profile.reusableOtherCardExhaust = 'none';
  assert.deepEqual(preparationCardPolicies(observation, profile).find(row => row.id === selected.id).commands, selected.commands);
  profile.reusableOtherCardExhaustCopies = 0;
  profile.reusableOtherCardExhaust = '显示文字不参与策略判断';
  assert.equal(preparationCardPolicies(observation, profile).some(row => row.id === selected.id), false);
});

test('recurring area damage carries the native loss and damage pair rather than an immediate attack total', () => {
  const effect = { damage_per_enemy_per_application: 7, hp_loss_per_application: 1 };
  const power = owned('Combust', 1, 'Power', 1, { end_of_turn_hp_loss_and_damage: effect }, 1);
  const profile = preparationDeckComparison(root([], [power]));
  const candidate = profile.candidates.find(row => row.card === 'Combust');
  assert.deepEqual(candidate.endOfTurnHpLossAndDamage, effect);
  assert.equal(candidate.attackDelivery, undefined);
  assert.equal(candidate.baseDamagePerHit, undefined);
  assert.equal(candidate.onPlayDraw, undefined);
});

function eventOffers(original) {
  const current = structuredClone(original);
  current.position.observation.observation.context = { kind: 'event', event: 'TheLibrary', screen: 1 };
  current.position.observation.observation.run = { act: 2, floor: 20 };
  current.position.actions = current.position.actions.filter(row => row.candidate.action.kind === 'take_card');
  current.position.actions.forEach((row, index) => {
    const oldCommand = row.semantic_action, card = row.candidate.action.card;
    row.semantic_action = `event:TheLibrary:${index}`;
    row.candidate.action = { kind: 'choose_event_option', event: 'TheLibrary', screen: 1, option_index: index,
      public_semantics: { structured_semantics: { effects: [{ effect: 'obtain_card', count: 1,
        pool: { kind: 'specific', card, upgrades: 0 } }] } } };
    current.decision_evidence.candidates.find(candidate => candidate.semantic_action === oldCommand).semantic_action = row.semantic_action;
  });
  return current;
}

test('displayed grouped targets share one comparison while the host retains every instance and owned count', () => {
  const strike = owned('Strike', 2, 'Attack', 1), current = root([strike]);
  current.position.actions = [0, 1].map(i => ({ semantic_action: `remove:Strike#${i}`, candidate: {
    action: { kind: 'remove_card', card: 'Strike', upgrades: 0, card_ordinal: i, price: 75 } } }));
  const profile = preparationDeckComparison(current);
  const displayed = renderDeckComparison(profile, { candidateCommands: ['remove:Strike#0'] });
  assert.match(displayed, /2 Attack/);
  assert.match(displayed, /remove:Strike#0: deck 2->1/);
  assert.doesNotMatch(displayed, /remove:Strike#1:/);
  assert.equal(profile.candidates.length, 2);
  assert.match(renderDeckComparison(profile), /remove:Strike#1:/);
});

test('Status routes distinguish Hand insertion, subsequent draws, persistent cycles and Ethereal cleanup', () => {
  const cases = [
    ['PowerThrough', 'Wound', 'hand', 'persistent'],
    ['WildStrike', 'Wound', 'draw_pile', 'persistent'],
    ['Immolate', 'Burn', 'discard_pile', 'persistent'],
    ['RecklessCharge', 'Dazed', 'draw_pile', 'ethereal'],
    ['Turbo', 'Void', 'discard_pile', 'ethereal'],
  ];
  for (const [card, status, destination, persistence] of cases) {
    const current = root([owned(card, 1, 'Skill', 1, { status_injections: [{ card: status, count: 1, destination, persistence }] })]);
    const profile = preparationDeckComparison(current);
    const text = renderStatusRoutes(profile.statusGeneration);
    assert.match(text, /Draw -> draw into Hand.*Draw-triggered effects apply at draw into Hand/);
    if (destination === 'hand') assert.match(text, /create in Hand -> ordinary end-turn discard -> Discard -> reshuffle/);
    if (destination === 'discard_pile') assert.match(text, /create in Discard -> reshuffle/);
    if (persistence === 'persistent') assert.match(text, /ordinary end-turn discard permits later reshuffles and draws/);
    else assert.match(text, /Ethereal exhausts it if it remains in Hand at turn end.*Discarding it earlier permits a later reshuffle/);
    assert.match(text, /Exhausting the Status cuts this ordinary redraw route; Retain or a skipped hand discard keeps it in Hand/);
    assert.match(renderDeckComparison(profile), new RegExp(status));
  }
  assert.throws(() => renderStatusRoutes([{ card: 'Unknown', injections: [{ card: 'Wound', count: 1, destination: 'unknown', persistence: 'persistent' }] }]), /typed destination/);
});

test('Status payoff binds only owned source routes and qualifying original types across reward, shop and event offers', () => {
  const powerThrough = owned('PowerThrough', 1, 'Skill', 1, { status_injections: [{ card: 'Wound', count: 2, destination: 'hand', persistence: 'persistent' }] });
  const curse = owned('AscendersBane', 1, 'Curse', -2, { ethereal: true });
  const wildStrike = owned('WildStrike', 1, 'Attack', 1, { status_injections: [{ card: 'Wound', count: 1, destination: 'draw_pile', persistence: 'persistent' }] });
  for (const card of ['Evolve', 'FireBreathing']) {
    const original = root([powerThrough, curse], [owned(card, 1, 'Power', 1, { name: card }), wildStrike]);
    const shop = structuredClone(original);
    shop.position.actions[0].candidate.action.kind = 'buy_card';
    shop.position.actions[0].semantic_action = `buy:${card}#0`;
    shop.decision_evidence.candidates[0].semantic_action = `buy:${card}#0`;
    for (const current of [original, shop, eventOffers(original)]) {
      const binding = preparationConditionBindings(current).find(row => row.id === 'status-draw-route-v1');
      const text = binding.fixedMechanics.join('\n');
      assert.match(text, /1x PowerThrough: 2 Wound; create in Hand -> ordinary end-turn discard -> Discard -> reshuffle -> Draw -> draw into Hand/);
      assert.doesNotMatch(text, /WildStrike/);
      assert.equal(text.includes('AscendersBane'), card === 'FireBreathing');
      assert.match(text, /generated Hand card occupies a slot immediately; its later ordinary discard, reshuffle and draw can supply the trigger/);
    }
  }
});

test('temporary Strength binds the actual attack suite rather than sibling acquisition candidates', () => {
  const extra = { name: 'Flex', strength_gain: 2, temporary_strength_gain: 2, strength_loss_at_end_of_turn: 2,
    strength_gain_delivery: 'on_play_with_separate_lose_strength' };
  const current = root([owned('Strike', 4, 'Attack', 1), owned('SwordBoomerang', 1, 'Attack', 1)],
    [owned('Flex', 1, 'Skill', 0, extra), owned('TwinStrike', 1, 'Attack', 1), owned('HeavyBlade', 1, 'Attack', 2)]);
  const binding = preparationConditionBindings(current).find(row => row.id === 'temporary-strength-payoff-v1');
  assert.match(binding.fixedMechanics.join('\n'), /Owned Attack-type inventory \(5 copies\).*4x Strike.*1x SwordBoomerang/);
  assert.doesNotMatch(binding.fixedMechanics.join('\n'), /TwinStrike|HeavyBlade/);
  current.position.observation.observation.cards.push(current.decision_evidence.candidates[1].cards[0].current);
  assert.throws(() => preparationConditionBindings(current), /current public inventory/);
});

test('Sentinel separates direct protective play from Energy requiring exhaustion of Sentinel itself', () => {
  const sentinel = owned('Sentinel', 1, 'Skill', 1, { block: 8, base_block_delivery: 'once_on_play', energy_gain_on_exhaust: 3 }, 1);
  const current = root([owned('Defend', 4, 'Skill', 1)], [sentinel]);
  const binding = preparationDeckConditionBindings(current)[0];
  assert.match(binding.fixedMechanics.join('\n'), /First usable benefit:.*8 Block directly.*3 Energy.*this card itself enters Exhaust/);
  const row = 'take:Sentinel#0 | low | direct | @protective-draw\nskip-card#0 | high | direct | @keep-vs-1';
  assert.equal(parsePreparationClassification(row, current)[0].realization, 'direct');
  assert.throws(() => parsePreparationClassification(row.replace('low | direct', 'low | future'), current), /@protective-draw uses direct/);
  assert.equal(parsePreparationClassification(row.replace('low | direct | @protective-draw', 'low | future | @self-exhaust-energy'), current)[0].realization, 'future');
  const pact = owned('BurningPact', 1, 'Skill', 1, { exhausts_other_cards: true });
  const supported = root([pact], [sentinel]);
  const sequence = row.replace('low | direct | @protective-draw', 'medium | sequence | @self-exhaust-energy');
  assert.equal(parsePreparationClassification(sequence, supported)[0].realization, 'sequence');
  const deployed = root([owned('Corruption', 1, 'Power', 3)], [sentinel]);
  assert.equal(parsePreparationClassification(sequence, deployed)[0].realization, 'sequence');
  assert.match(preparationDeckConditionBindings(deployed)[0].fixedMechanics.join(' '), /Corruption.*then played Skills Exhaust/);
  assert.throws(() => parsePreparationClassification(row.replace('@protective-draw', '@protective-draw,@self-exhaust-energy'), current), /@self-exhaust-energy uses future/);
});

test('Strike-scaled additions share the reviewed count rule across rewards, shops and known events', () => {
  const offered = owned('PerfectedStrike', 1, 'Attack', 2, { damage: {
    damage_per_hit: 6, hit_count: 1, delivery: { kind: 'attack', amount: { kind: 'base_plus_strike_cards', per_card: 2 },
      hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' },
  } });
  const reward = root([owned('Strike', 3, 'Attack', 1)], [offered]);
  const shop = structuredClone(reward);
  shop.position.actions[0].candidate.action.kind = 'buy_card';
  shop.position.actions[0].semantic_action = 'buy:PerfectedStrike#0';
  shop.decision_evidence.candidates[0].semantic_action = 'buy:PerfectedStrike#0';
  for (const current of [reward, shop, eventOffers(reward)]) {
    const profile = preparationDeckComparison(current);
    const binding = preparationDeckConditionBindings(current)[0];
    assert.match(binding.fixedMechanics.join(' '), /6 \+ 2 \* N/);
    assert.match(binding.fixedMechanics.join(' '), /Hand, Draw and Discard together/);
    assert.match(binding.fixedMechanics.join(' '), /including the played copy exactly once/);
    assert.match(renderDeckComparison(profile), /6 \+ 2 \* N/);
    assert.equal(profile.candidates[0].baseDamagePerHit, undefined, 'conditional damage retains its amount rule');
  }
  offered.mechanics.damage.delivery.amount.per_card = undefined;
  const partial = preparationDeckComparison(root([owned('Strike', 3, 'Attack', 1)], [offered]));
  assert.equal(partial.candidates[0].strikeScaling, undefined, 'a missing multiplier stays unexpanded');
});

test('known event card offers share inventory increments and expert advice while preserving event commands', () => {
  const barrier = owned('Barricade', 1, 'Power', 3);
  const pact = owned('BurningPact', 1, 'Skill', 1, { draw_cards: 3, exhausts_other_cards: true }, 1);
  const slam = owned('BodySlam', 1, 'Attack', 1, { damage: { delivery: { kind: 'attack',
    amount: { kind: 'player_block' }, hits: { kind: 'fixed', count: 1 }, targets: 'one_enemy' } } });
  const original = root([owned('Strike', 5, 'Attack', 1), barrier], [pact, slam]);
  const current = eventOffers(original), before = structuredClone(current);
  const profile = preparationDeckComparison(current);
  assert.equal(profile.candidates.length, 2);
  assert.deepEqual(profile.candidates.map(row => [row.command, row.card, row.sizeBefore, row.sizeAfter]), [
    ['event:TheLibrary:0', 'BurningPact', 6, 7], ['event:TheLibrary:1', 'BodySlam', 6, 7] ]);
  assert.equal(profile.candidates[0].upgrades, 1, 'the native acquisition definition owns the obtain-time upgrade');
  assert.equal(profile.candidates[0].onPlayDraw.requested, 3);
  const guidance = preparationExpertGuidance(current);
  assert.deepEqual(guidance.candidates.map(row => [row.command, row.card]), [
    ['event:TheLibrary:0', 'BurningPact'], ['event:TheLibrary:1', 'BodySlam'] ]);
  assert.ok(guidance.candidates[1].priorityIds.includes('develop-owned-block-reserve'));
  assert.deepEqual(preparationConditionBindings(current).map(row => row.command), ['event:TheLibrary:0', 'event:TheLibrary:1']);
  const access = preparationConditionBindings(current)[0];
  assert.equal(access.id, 'on-play-access-v1');
  assert.match(access.fixedMechanics.join(' '), /BurningPact\+:.*costs 1 Energy.*draw request 3.*selection-dependent/);
  assert.match(access.fixedMechanics.join(' '), /ordinary redraw permits later uses/);
  assert.deepEqual(access.choices.map(choice => choice.id), ['@independent-access', '@selection-turnover', '@access-cost', '@review-access']);
  assert.deepEqual(current, before);
});

test('event acquisition binding requires one known card grant matching the typed mechanics', () => {
  const current = eventOffers(root([], [owned('Armaments', 1, 'Skill', 1)]));
  const row = current.position.actions[0];
  assert.equal(preparationCardAcquisition(current, row).card, 'Armaments');
  for (const mutate of [
    value => { value.position.actions[0].candidate.action.public_semantics.structured_semantics.effects[0].count = 2; },
    value => { value.position.actions[0].candidate.action.public_semantics.structured_semantics.effects[0].pool.kind = 'random'; },
    value => { value.position.actions[0].candidate.action.public_semantics.structured_semantics.effects.push({ effect: 'lose_hp' }); },
    value => { value.decision_evidence.candidates[0].cards[0].current.card = 'LimitBreak'; },
    value => { value.decision_evidence.candidates[0].cards.push(value.decision_evidence.candidates[0].cards[0]); },
    value => { value.decision_evidence.kind = 'event_options'; },
  ]) {
    const changed = structuredClone(current);
    mutate(changed);
    assert.equal(preparationCardAcquisition(changed, changed.position.actions[0]), undefined);
    assert.equal(preparationDeckComparison(changed).candidates.length, 0);
  }
});

test('random-per-hit attacks carry exact hits and single-enemy targeting through card and event offers', () => {
  const sword = owned('SwordBoomerang', 1, 'Attack', 1, { damage: { damage_per_hit: 3, delivery: {
    kind: 'attack', amount: { kind: 'base' }, hits: { kind: 'fixed', count: 3 }, targets: 'random_enemy_per_hit' } } });
  const original = root([owned('Strike', 5, 'Attack', 1)], [sword]);
  for (const current of [original, eventOffers(original)]) {
    const binding = preparationDeckConditionBindings(current)[0];
    assert.match(binding.fixedMechanics.join('\n'), /Base damage per hit 3; hits per play 3/);
    assert.match(binding.fixedMechanics.join('\n'), /Strength applies on each actual hit/);
    assert.match(binding.fixedMechanics.join('\n'), /With one enemy, every hit goes to that enemy/);
  }
});

test('single-card events retain the specialized upgrade, temporary Strength and payment conditions', () => {
  const armaments = owned('Armaments', 1, 'Skill', 1, { name: 'Armaments+', block: 5,
    combat_upgrade_scope: 'whole_hand' }, 1);
  const flex = owned('Flex', 1, 'Skill', 0, { name: 'Flex+', strength_gain: 4,
    temporary_strength_gain: 4, strength_loss_at_end_of_turn: 4,
    strength_gain_delivery: 'on_play_with_separate_lose_strength' }, 1);
  const demon = owned('DemonForm', 1, 'Power', 3);
  const current = eventOffers(root([owned('LimitBreak', 1, 'Skill', 1)], [armaments, flex, demon]));
  current.position.observation.observation.base_energy_per_turn = 2;
  const bindings = preparationConditionBindings(current);
  assert.equal(new Set(bindings.map(row => row.command)).size, bindings.length);
  assert.equal(bindings.find(row => row.command === 'event:TheLibrary:0').id, 'armaments-limitbreak-v1');
  assert.equal(bindings.find(row => row.command === 'event:TheLibrary:1').id, 'temporary-strength-payoff-v1');
  assert.equal(bindings.find(row => row.command === 'event:TheLibrary:2').id, 'base-energy-payment-v1');
});

test('enduring Strength offers bind current attack inventory and each source timing across acquisition pages', () => {
  const source = (card, type, delivery) => owned(card, 1, type, 1, { strength_gain: 4,
    temporary_strength_gain: 0, strength_gain_delivery: delivery }, 1);
  const spot = source('SpotWeakness', 'Skill', 'if_target_intent_base_damage_nonnegative_at_resolution');
  const sword = owned('SwordBoomerang', 1, 'Attack', 1, { damage: { delivery: {
    kind: 'attack', hits: { kind: 'fixed', count: 3 }, targets: 'random_enemy_per_hit' } } });
  const reaper = owned('Reaper', 1, 'Attack', 2, { exhausts: true, damage: { delivery: {
    kind: 'attack', hits: { kind: 'fixed', count: 1 }, targets: 'each_enemy' } } });
  const initial = root([spot, sword, reaper], [spot]);
  const shop = structuredClone(initial);
  shop.position.actions[0].semantic_action = 'buy:SpotWeakness#0';
  shop.position.actions[0].candidate.action.kind = 'buy_card';
  shop.decision_evidence.candidates[0].semantic_action = 'buy:SpotWeakness#0';
  for (const current of [initial, shop, eventOffers(initial)]) {
    const binding = preparationConditionBindings(current).find(row => row.id === 'enduring-strength-increment-v1');
    assert.ok(binding);
    const facts = binding.fixedMechanics.join(' ');
    assert.match(facts, /request 4 Strength on play when the target has attack intent at resolution/);
    assert.match(facts, /Owned enduring Strength sources: 1x SpotWeakness\+/);
    assert.match(facts, /SpotWeakness across owned upgrade states: copies 1->2/);
    assert.match(facts, /Owned Attack-type inventory \(2 copies\).*SwordBoomerang: 1 Energy; 3 hit\(s\); targets random_enemy_per_hit/);
    assert.match(facts, /Reaper: 2 Energy; 1 hit\(s\); targets each_enemy; Exhaust after play/);
    for (const rating of ['high', 'low']) {
      const table = preparationConditionBindings(current).map(row => `${row.command} | ${rating} | sequence | ${row.choices[0].id}`).join('\n');
      const classified = parsePreparationClassification(table, current);
      assert.equal(classified[0].category, rating);
      assert.match(classified[0].reason, /SwordBoomerang.*Reaper/);
      assert.doesNotMatch(classified[0].reason, /no payoff attack/);
    }
  }
  for (const [card, type, delivery, timing] of [
    ['Inflame', 'Power', 'on_play', /request 4 Strength on play/],
    ['DemonForm', 'Power', 'each_player_turn_start_post_draw', /turn start after draw, following deployment/],
  ]) {
    const current = root([sword], [source(card, type, delivery)]);
    assert.match(preparationConditionBindings(current)[0].fixedMechanics.join(' '), timing);
  }
});

test('Block-based output separates affordable same-turn use from owned cross-turn supports', () => {
  const slam = owned('BodySlam', 1, 'Attack', 1, { damage: { delivery: { kind: 'attack',
    amount: { kind: 'player_block' }, hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } } });
  const block = owned('PowerThrough', 1, 'Skill', 1, { base_block_delivery: 'once_on_play', block: 15 });
  block.persistent_block = 19;
  const conditional = owned('Rage', 1, 'Skill', 0, { block: 5 });
  conditional.persistent_block = 5;
  const initial = root([block, conditional], [slam]);
  const shop = structuredClone(initial);
  shop.position.actions[0].candidate.action.kind = 'buy_card';
  shop.position.actions[0].semantic_action = 'buy:BodySlam#0';
  shop.decision_evidence.candidates[0].semantic_action = 'buy:BodySlam#0';
  for (const current of [initial, shop, eventOffers(initial)]) {
    const binding = preparationConditionBindings(current).find(row => row.id === 'current-block-output-v1');
    assert.ok(binding);
    const facts = binding.fixedMechanics.join(' ');
    assert.match(facts, /Current Block at attack resolution supplies base damage/);
    assert.match(facts, /PowerThrough -> BodySlam; 2 of 3 base Energy; Block request 19 before modifiers/);
    assert.doesNotMatch(facts, /Rage: 0 Energy, 5 Block/);
    assert.match(facts, /Owned retention: none/);
    assert.ok(!binding.choices.some(row => row.id === '@retained-output'));
    const table = preparationConditionBindings(current).map(row =>
      `${row.command} | high | sequence | ${row.choices[0].id}`).join('\n');
    assert.equal(parsePreparationClassification(table, current)[0].category, 'high');
  }
  const retention = owned('Barricade', 1, 'Power', 3);
  retention.persistent_cost = 2;
  const multiplier = owned('Entrench', 1, 'Skill', 1, {}, 1);
  const cap = owned('Apparition', 3, 'Skill', 1, { intangible_gain: 1, intangible_damage_cap: 1, exhausts: true }, 1);
  const supported = root([block, retention, multiplier, cap], [slam]);
  const binding = preparationConditionBindings(supported)[0];
  const facts = binding.fixedMechanics.join(' ');
  assert.match(facts, /Barricade: 2 Energy to deploy/);
  assert.match(facts, /Entrench\+: 1 Energy; ordinary redraw/);
  assert.match(facts, /3x Apparition\+: 1 Energy, request 1 Intangible with damage cap 1; Exhaust/);
  assert.match(facts, /Block remaining after the intervening enemy actions/);
  assert.ok(binding.choices.some(row => row.id === '@retained-output'));
  const costly = structuredClone(supported);
  costly.position.observation.observation.base_energy_per_turn = 1;
  assert.ok(!preparationDeckConditionBindings(costly)[0].fixedMechanics.some(text => text.startsWith('Same-Hand payment:')));
  const unknown = structuredClone(initial);
  unknown.decision_evidence.candidates[0].cards[0].current.damage.delivery.amount.kind = 'base';
  assert.ok(!preparationDeckConditionBindings(unknown).some(row => row.id === 'current-block-output-v1'));
});

test('multi-draw coverage counts copies and turnover without treating self-replacement or deployed draw as independent expansion', () => {
  const rows = [owned('Strike', 20, 'Attack', 1), owned('DeepBreath', 1, 'Skill', 0, { draw_cards: 2 }, 1),
    owned('ShrugItOff', 1, 'Skill', 1, { draw_cards: 1 }),
    owned('Warcry', 1, 'Skill', 0, { draw_cards: 2, hand_topdeck_after_draw: 1 }, 1),
    owned('DarkEmbrace', 1, 'Power', 2, { exhaust_draw_power: 2 })];
  const pact = owned('BurningPact', 1, 'Skill', 1, { draw_cards: 3, exhausts_other_cards: true }, 1);
  const current = root(rows, [pact]);
  const profile = preparationDeckComparison(current);
  assert.equal(profile.onPlayDrawCoverage.copies, 1);
  assert.equal(profile.onPlayDrawCoverage.owned, '1x DeepBreath+');
  assert.ok(Math.abs(profile.onPlayDrawCoverage.windows[0].probability - 5 / 24) < 1e-12);
  assert.match(renderDeckComparison(profile), /multi-draw inventory: 1 of 24 original cards/);
  assert.match(preparationDeckConditionBindings(current)[0].fixedMechanics.join(' '), /multi-draw cards.*copies 1->2/);
  rows[1].card.count = 4;
  const moreDraw = preparationDeckComparison(root(rows, [pact]));
  assert.equal(moreDraw.onPlayDrawCoverage.copies, 4);
  assert.ok(moreDraw.onPlayDrawCoverage.windows[0].probability > profile.onPlayDrawCoverage.windows[0].probability);
  const reusable = preparationDeckComparison(root([pact]));
  assert.equal(reusable.onPlayDrawCoverage.copies, 1);
  assert.equal(reusable.onPlayDraw[0].handChange, null);
  const empty = preparationDeckComparison(root([]));
  assert.equal(empty.onPlayDrawCoverage.copies, 0);
  assert.deepEqual(empty.onPlayDrawCoverage.windows, []);
});

test('Strength support stays observable with zero attacks, specialized attack rules and unknown source timing', () => {
  const source = owned('Inflame', 1, 'Power', 1, { strength_gain: 2, temporary_strength_gain: 0, strength_gain_delivery: 'on_play' });
  const empty = root([], [source]);
  assert.match(preparationConditionBindings(empty)[0].fixedMechanics.join(' '), /Attack-type inventory \(0 copies\): none/);
  const heavy = owned('HeavyBlade', 1, 'Attack', 2, { damage: { delivery: {
    kind: 'attack', hits: { kind: 'fixed', count: 1 }, targets: 'selected_enemy' } } });
  const current = root([heavy], [source]);
  const facts = preparationConditionBindings(current)[0].fixedMechanics.join(' ');
  assert.match(facts, /HeavyBlade: 2 Energy; 1 hit\(s\)/);
  assert.match(facts, /Strength factors.*full card rule/);
  assert.doesNotMatch(facts, /adds 2 damage|Strength multiplier 1/);
  for (const patch of [{ strength_gain_delivery: undefined }, { temporary_strength_gain: 2 }, { strength_gain: 0 }]) {
    const changed = structuredClone(current);
    Object.assign(changed.decision_evidence.candidates[0].cards[0].current, patch);
    assert.ok(!preparationConditionBindings(changed).some(row => row.id === 'enduring-strength-increment-v1'));
  }
});

test('whole-deck counts retain original attacks and refresh after an actual inventory change', () => {
  const cards = [owned('Strike', 5, 'Attack', 1), owned('Headbutt', 1, 'Attack', 1), owned('Defend', 4, 'Skill', 1)];
  const headbutt = owned('Headbutt', 1, 'Attack', 1);
  const current = root(cards, [headbutt]), saved = structuredClone(current);
  const profile = preparationDeckComparison(current);
  assert.equal(profile.counts.Attack, 6);
  assert.deepEqual(profile.attackCosts, { 1: 6 });
  assert.deepEqual(profile.candidates.map(row => [row.sizeAfter, row.typeAfter]), [[11, 7], [10, undefined]]);
  assert.match(renderDeckComparison(profile), /original copies retained/);
  assert.deepEqual(current, saved);
  current.position.observation.observation.cards[1].count++;
  assert.equal(preparationDeckComparison(current).size, 11);
  assert.equal(preparationDeckComparison(current).counts.Attack, 7);
  const early = preparationDeckComparison(root([owned('Strike', 5, 'Attack', 1)], [headbutt]));
  assert.equal(early.counts.Attack, 5);
  assert.equal(early.candidates[0].typeAfter, 6);
  assert.equal(early.candidates[0].command, profile.candidates[0].command);
});

test('removal exposes access to retained functions and the function forgone at the actual preparation site', () => {
  const strike = owned('Strike', 3, 'Attack', 1);
  strike.persistent_damage = 6;
  const demon = owned('DemonForm', 1, 'Power', 3);
  const shrug = owned('ShrugItOff', 1, 'Skill', 1, { draw_cards: 1, base_block_delivery: 'once_on_play' });
  shrug.persistent_block = 8;
  const limit = owned('LimitBreak', 1, 'Skill', 1, { exhausts: true });
  const curse = owned('Injury', 1, 'Curse', -2);
  const current = root([strike, demon, shrug, limit, curse]);
  current.position.observation.observation.context = { kind: 'shop' };
  current.position.actions = [strike, demon, shrug, limit, curse].map(row => ({
    semantic_action: `remove:${row.card.card}#0`, candidate: { action: {
      kind: 'remove_card', card: row.card.card, upgrades: 0 } },
  }));
  const saved = structuredClone(current);
  const profile = preparationDeckComparison(current), removed = profile.candidates[0];
  assert.equal(removed.sizeBefore, 7);
  assert.equal(removed.sizeAfter, 6);
  assert.equal(removed.damage, 6);
  assert.deepEqual(removed.access.map(row => [row.copiesBefore, row.copiesAfter]), [[1, 1], [1, 1], [1, 1]]);
  assert.ok(Math.abs(removed.access[0].windows[0].before - 5 / 7) < 1e-12);
  assert.ok(Math.abs(removed.access[0].windows[0].after - 5 / 6) < 1e-12);
  assert.deepEqual(profile.candidates[1].access.map(row => row.kind), ['retained_draw', 'retained_protection']);
  assert.deepEqual(profile.candidates[2].access.map(row => row.kind), ['retained_setup']);
  assert.match(renderDeckComparison(profile), /uniform sampling without replacement/);
  const bindings = preparationDeckConditionBindings(current);
  assert.match(bindings[1].fixedMechanics.join(' '), /leaves ordinary card circulation after deployment/);
  assert.match(bindings[3].fixedMechanics.join(' '), /Exhausts after play/);
  assert.match(bindings[4].fixedMechanics.join(' '), /unplayable/);
  assert.doesNotMatch(bindings[4].fixedMechanics.join(' '), /costs -2 Energy/);
  assert.match(bindings[0].choices.find(row => row.id === '@gold-opportunity').text, /through intervening fights/);
  const table = profile.candidates.map(row => `${row.command} | medium | direct | @retained-access,@foregone-function`).join('\n');
  assert.equal(parsePreparationClassification(table, current).length, 5);
  assert.throws(() => parsePreparationClassification(table.replace('@retained-access,@foregone-function', 'Adds no immediate Block'), current), /supplied condition references/);
  assert.deepEqual(current, saved);
  current.position.observation.observation.context = { kind: 'campfire' };
  assert.ok(preparationDeckConditionBindings(current)[0].choices.some(row => row.id === '@preparation-opportunity'));
  assert.ok(!preparationDeckConditionBindings(current)[0].choices.some(row => row.id === '@gold-opportunity'));
});

test('draw-one, draw-two and topdeck costs have different immediate Hand changes', () => {
  const pommel = owned('PommelStrike', 1, 'Attack', 1, { draw_cards: 1 });
  const pommelPlus = owned('PommelStrike', 1, 'Attack', 1, { draw_cards: 2 }, 1);
  const warcry = owned('Warcry', 1, 'Skill', 0, { draw_cards: 2, hand_topdeck_after_draw: 1, exhausts: true }, 1);
  const profile = preparationDeckComparison(root([pommel, pommelPlus, warcry], [pommel]));
  assert.deepEqual(profile.onPlayDraw.map(row => row.handChange), [0, 1, 0]);
  assert.equal(profile.candidates[0].onPlayDraw.handChange, 0);
  assert.match(renderDeckComparison(profile), /PommelStrike\+: cost 1, draw request 2, Hand contribution \+1/);
  assert.match(renderDeckComparison(profile), /Warcry\+: cost 0, draw request 2, then topdeck 1, Hand contribution \+0/);
  const pact = owned('BurningPact', 1, 'Skill', 1, { draw_cards: 2, exhausts_other_cards: true, selects_from_hand: true });
  const pactProfile = preparationDeckComparison(root([pact]));
  assert.equal(pactProfile.onPlayDraw[0].handChange, null);
  assert.match(renderDeckComparison(pactProfile), /additional Hand removal\/selection uses the supplied rule/);
});

test('deployed draw and Exhaust triggers are separated from immediate draw and recovery', () => {
  const dark = owned('DarkEmbrace', 1, 'Power', 2, { exhaust_draw_power: 1 });
  const brutality = owned('Brutality', 1, 'Power', 0, { draw_cards_each_turn: 1 });
  const fuel = [owned('Pummel', 2, 'Attack', 1, { exhausts: true }),
    owned('TrueGrit', 1, 'Skill', 1, { exhausts_other_cards: true }),
    owned('AscendersBane', 1, 'Curse', -2, { ethereal: true }),
    owned('PowerThrough', 1, 'Skill', 1, { status_injections: [{ card: 'Wound', count: 2, destination: 'hand' }] }),
    owned('Headbutt', 1, 'Attack', 1)];
  const rich = preparationDeckComparison(root([...fuel, brutality], [dark]));
  assert.deepEqual(rich.onPlayDraw, []);
  assert.equal(rich.laterTurnDraw[0].requested, 1);
  assert.equal(rich.candidates[0].drawPerExhaustAfterDeployment, 1);
  assert.match(rich.playedExhaust, /2x Pummel/);
  assert.match(rich.ethereal, /AscendersBane/);
  assert.match(rich.otherCardExhaust, /TrueGrit/);
  assert.equal(rich.statusGeneration[0].card, '1x PowerThrough');
  assert.match(rich.reusableOtherCardExhaust, /TrueGrit/);
  const sparse = preparationDeckComparison(root([owned('Strike', 5, 'Attack', 1)], [dark]));
  assert.equal(sparse.playedExhaust, 'none');
  assert.equal(sparse.otherCardExhaust, 'none');
});

test('an additional reduction preserves its numeric application and owned stacking context', () => {
  const disarm = owned('Disarm', 1, 'Skill', 1, { enemy_strength_down: 2,
    enemy_strength_restoration: 'no_scheduled_restoration', play_target: 'one_enemy', exhausts: true });
  const bloodletting = owned('Bloodletting', 1, 'Skill', 0, { energy_gain_rule: { delivery: 'on_play', amount: 2 },
    self_hp_loss_on_play: { base_hp_loss: 3 } });
  const profile = preparationDeckComparison(root([disarm, bloodletting], [disarm]));
  assert.equal(profile.candidates[0].enemyStrengthReduction.amount, 2);
  assert.equal(profile.candidates[0].enemyStrengthReduction.owned[0].amount, 2);
  assert.equal(profile.onPlayEnergy[0].hpLoss, 3);
  assert.match(renderDeckComparison(profile), /successive applications on the same enemy add their reductions/);
  assert.match(renderDeckComparison(profile), /request 3 HP loss, gain 2 Energy/);
  assert.match(renderDeckComparison(profile), /Exhaust governs card reuse; the applied reduction remains/);
});

test('another protective draw increases access despite a stronger single owned card', () => {
  const power = owned('PowerThrough', 1, 'Skill', 1, { base_block_delivery: 'once_on_play', block: 20 }, 1);
  power.persistent_block = 20;
  const barrier = owned('FlameBarrier', 1, 'Skill', 2, { base_block_delivery: 'once_on_play', block: 12 });
  const original = [power, owned('Strike', 25, 'Attack', 1)];
  const sparse = preparationDeckComparison(root(original, [barrier]));
  const access = sparse.candidates[0].access[0];
  assert.equal(access.kind, 'fixed_block');
  assert.equal(access.copiesBefore, 1);
  assert.equal(access.copiesAfter, 2);
  assert.equal(access.owned, '1x PowerThrough+');
  const five = access.windows.find(row => row.draws === 5);
  assert.ok(Math.abs(five.before - 5 / 26) < 1e-12);
  assert.ok(Math.abs(five.after - (1 - 22 * 21 / (27 * 26))) < 1e-12);
  assert.match(renderDeckComparison(sparse), /sample 5: 19\.2%->34\.2%/);
  power.card.count = 8;
  original[1].card.count = 18;
  const abundant = preparationDeckComparison(root(original, [barrier])).candidates[0].access[0];
  assert.ok(abundant.windows[0].after - abundant.windows[0].before < five.after - five.before);
  assert.ok(abundant.windows[0].before > five.before);
});

test('access reference honors persistent Block overrides and excludes conditional or restored effects', () => {
  const power = owned('PowerThrough', 1, 'Skill', 1, { base_block_delivery: 'once_on_play', block: 20 }, 1);
  power.card.base_block_override = 7; power.persistent_block = 7;
  const rage = owned('Rage', 3, 'Skill', 0, { block: 30 }); rage.persistent_block = 30;
  const barrier = owned('FlameBarrier', 1, 'Skill', 2, { base_block_delivery: 'once_on_play', block: 12 });
  const disarm = owned('Disarm', 1, 'Skill', 1, { enemy_strength_down: 2,
    enemy_strength_restoration: 'no_scheduled_restoration', play_target: 'one_enemy', exhausts: true });
  const restored = owned('DarkShackles', 1, 'Skill', 0, { enemy_strength_down: 9,
    enemy_strength_restoration: 'at_enemy_turn_end', play_target: 'one_enemy', exhausts: true });
  const unknown = owned('UnknownReduction', 1, 'Skill', 0, { enemy_strength_down: 99 });
  const profile = preparationDeckComparison(root([power, rage, disarm, restored, unknown], [barrier, disarm]));
  assert.equal(profile.candidates[0].access[0].copiesBefore, 0);
  assert.equal(profile.candidates[1].access[0].copiesBefore, 1);
  assert.equal(profile.persistentReduction.length, 1);
  assert.equal(profile.persistentReduction[0].card, '1x Disarm');
  assert.equal(profile.candidates[1].access.length, 1);
});

test('inventory sampling matches enumeration and uses a fixed sample size across deck additions', () => {
  const disarm = owned('Disarm', 2, 'Skill', 1, { enemy_strength_down: 2,
    enemy_strength_restoration: 'no_scheduled_restoration', play_target: 'one_enemy', exhausts: true });
  const profile = preparationDeckComparison(root([disarm, owned('Strike', 5, 'Attack', 1)], [disarm]));
  const windows = profile.candidates[0].access[0].windows;
  const enumerated = (size, copies, draws) => {
    let total = 0, hit = 0;
    function visit(from, remaining, contains) {
      if (remaining === 0) { total++; if (contains) hit++; return; }
      for (let i = from; i <= size - remaining; i++) visit(i + 1, remaining - 1, contains || i < copies);
    }
    visit(0, draws, false); return hit / total;
  };
  for (const window of windows) {
    assert.ok(Math.abs(window.before - enumerated(7, 2, window.draws)) < 1e-12);
    assert.ok(Math.abs(window.after - enumerated(8, 3, window.draws)) < 1e-12);
  }
  assert.deepEqual(windows.map(row => row.draws), [5, 7]);
  const single = preparationDeckComparison(root([owned('Pummel', 1, 'Attack', 1)], [owned('Pummel', 1, 'Attack', 1)]));
  assert.deepEqual(single.candidates[0].access[0].windows, [{ draws: 1, before: 1, after: 1 }]);
});

test('protection classifications retain quantity and lifecycle facts for either model rating', () => {
  const disarm = owned('Disarm', 2, 'Skill', 1, { enemy_strength_down: 2,
    enemy_strength_restoration: 'no_scheduled_restoration', play_target: 'one_enemy', exhausts: true });
  const barrier = owned('FlameBarrier', 1, 'Skill', 2, { base_block_delivery: 'once_on_play', block: 12 });
  const current = root([disarm, owned('Strike', 24, 'Attack', 1)], [disarm, barrier]);
  const bindings = preparationConditionBindings(current);
  assert.equal(bindings.length, 3);
  assert.match(bindings[0].fixedMechanics.join(' '), /requested total changes 4->6/);
  const text = 'take:Disarm#0 | low | direct | @coverage-cost\n'
    + 'take:FlameBarrier#0 | medium | direct | @protective-draw\n'
    + 'skip-card#0 | high | direct | @keep-vs-1';
  const [reduction, block] = parsePreparationClassification(text, current);
  assert.equal(reduction.category, 'low');
  assert.equal(reduction.realization, 'direct');
  assert.deepEqual(reduction.conditionIds, ['@coverage-cost']);
  assert.match(reduction.reason, /requested total changes 4->6/);
  assert.match(reduction.reason, /Exhaust controls its reuse; the applied reduction remains/);
  assert.match(block.reason, /On play request 12 Block/);
  assert.throws(() => parsePreparationClassification(text.replace('@coverage-cost', 'Exhaust ends its reduction.'), current),
    /Use supplied condition references/);
  const armaments = owned('Armaments', 1, 'Skill', 1, { base_block_delivery: 'once_on_play', block: 5,
    combat_upgrade_scope: 'selected_card_in_hand' });
  const priority = preparationConditionBindings(root([owned('LimitBreak', 1, 'Skill', 1)], [armaments]));
  assert.equal(priority.length, 2);
  assert.equal(priority[0].id, 'armaments-limitbreak-v1');
});

test('attack increments bind whole inventory, copy access and deployed payoff separately', () => {
  const pummel = owned('Pummel', 1, 'Attack', 1, { exhausts: true });
  const dark = owned('DarkEmbrace', 1, 'Power', 1, { exhaust_draw_power: 1 }, 1);
  const current = root([owned('Strike', 5, 'Attack', 1), pummel, dark,
    owned('PowerThrough', 1, 'Skill', 1)], [pummel]);
  const text = 'take:Pummel#0 | medium | sequence | @extra-output,@added-function\n'
    + 'skip-card#0 | high | direct | @keep-vs-1';
  const [attack, skip] = parsePreparationClassification(text, current);
  assert.equal(attack.bindingId, 'attack-whole-deck-increment-v1');
  assert.equal(attack.category, 'medium');
  assert.equal(attack.realization, 'sequence');
  assert.match(attack.reason, /Attack copies 6->7/);
  assert.match(attack.reason, /copies 1->2/);
  assert.match(attack.reason, /Attack-type cards.*copies 6->7/);
  assert.match(attack.reason, /DarkEmbrace\+: pay 1 Energy to deploy, request 1\/event/);
  assert.match(attack.reason, /further uses follow owned recovery/);
  assert.equal(skip.conditions[0].alternativeCommand, 'take:Pummel#0');
  current.deck_evidence.cards.pop();
  current.position.observation.observation.cards.pop();
  current.deck_evidence.cards = current.deck_evidence.cards.filter(row => row.card.card !== 'DarkEmbrace');
  current.position.observation.observation.cards = current.position.observation.observation.cards.filter(row => row.card !== 'DarkEmbrace');
  assert.match(preparationConditionBindings(current)[0].fixedMechanics.join(' '), /Exhaust event after deployment: none/);
  pummel.mechanics.cost = -1;
  assert.match(preparationConditionBindings(current)[0].fixedMechanics.join(' '), /X-cost play uses available Energy/);
});

test('skip comparisons identify actual rivals and retain the model rating and emphasis', () => {
  const cards = [owned('Strike', 5, 'Attack', 1)];
  const current = root(cards, [owned('BattleTrance', 1, 'Skill', 0, { draw_cards: 3 }),
    owned('Headbutt', 1, 'Attack', 1)]);
  const binding = preparationConditionBindings(current).find(row => row.command === 'skip-card#0');
  assert.equal(binding.id, 'reward-preservation-comparison-v1');
  assert.deepEqual(binding.choices.filter(row => row.emphasis === 'foregone_gain').map(row => row.alternativeCommand),
    ['take:BattleTrance#0', 'take:Headbutt#0']);
  const basis = 'take:BattleTrance#0 | high | direct | @independent-access\n'
    + 'take:Headbutt#0 | medium | direct | @output-access\n'
    + 'skip-card#0 | low | direct | @gain-vs-1';
  const rows = parsePreparationClassification(basis, current);
  assert.equal(rows[2].category, 'low');
  assert.equal(rows[2].conditions[0].alternativeCommand, 'take:BattleTrance#0');
  assert.equal(rows[2].conditions[0].emphasis, 'foregone_gain');
  assert.throws(() => parsePreparationClassification(basis.replace('@gain-vs-1', '@gain-vs-3'), current),
    /Use supplied condition references/);
  assert.throws(() => parsePreparationClassification(basis.replace('@gain-vs-1', 'Preserve access.'), current),
    /Use supplied condition references/);
  assert.ok(preparationConditionBindings(root(cards)).every(row => row.id === 'preparation-effect-tradeoff-v1'));
  current.position.actions = current.position.actions.filter(row => row.candidate.action.card !== 'BattleTrance');
  const after = preparationConditionBindings(current).find(row => row.command === 'skip-card#0');
  assert.deepEqual(after.choices.filter(row => row.alternativeCommand).map(row => row.alternativeCommand),
    ['take:Headbutt#0', 'take:Headbutt#0']);
});

test('BattleTrance access preserves turn-start timing and the actual same-turn draw costs', () => {
  const brutality = owned('Brutality', 1, 'Power', 0, { draw_cards_each_turn: 1 });
  const trance = owned('BattleTrance', 1, 'Skill', 0, { draw_cards: 3 });
  const sparse = root([owned('Strike', 5, 'Attack', 1), brutality], [trance]);
  const basis = 'take:BattleTrance#0 | high | direct | @independent-access\n'
    + 'skip-card#0 | low | direct | @gain-vs-1';
  const [row] = parsePreparationClassification(basis, sparse);
  assert.equal(row.bindingId, 'draw-order-access-v1');
  assert.match(row.reason, /Brutality turn-start draw resolve before.*BattleTrance/);
  assert.match(row.reason, /NoDraw is removed at that turn's end/);
  assert.match(row.reason, /drawing contributes 2 to Hand/);
  assert.match(row.reason, /Owned on-play draw: none/);
  const covered = root([brutality, owned('ShrugItOff', 2, 'Skill', 1, { draw_cards: 1 }),
    owned('DarkEmbrace', 1, 'Power', 1, { exhaust_draw_power: 1 }, 1)], [trance]);
  const [ordered] = parsePreparationClassification(basis.replace('high | direct | @independent-access',
    'medium | sequence | @same-turn-order'), covered);
  assert.equal(ordered.category, 'medium');
  assert.equal(ordered.realization, 'sequence');
  assert.match(ordered.reason, /ShrugItOff.*request 1/);
  assert.match(ordered.reason, /DarkEmbrace\+.*request 1\/event/);
  trance.mechanics.draw_cards = 4; trance.mechanics.upgrades = 1; trance.card.upgrades = 1;
  const upgraded = root([brutality], [trance]);
  assert.match(preparationConditionBindings(upgraded)[0].fixedMechanics.join(' '), /BattleTrance\+.*requests 4.*contributes 3/);
});

test('attack additions preserve more specific protection and Energy comparisons', () => {
  const cards = [owned('Strike', 5, 'Attack', 1)];
  const iron = owned('IronWave', 1, 'Attack', 1, { base_block_delivery: 'once_on_play', block: 5 });
  const bludgeon = owned('Bludgeon', 1, 'Attack', 4);
  const current = root(cards, [iron, bludgeon]);
  const bindings = preparationConditionBindings(current);
  assert.equal(bindings.find(row => row.command === 'take:IronWave#0').id, 'protection-quantity-access-v1');
  assert.equal(bindings.find(row => row.command === 'take:Bludgeon#0').id, 'base-energy-payment-v1');
});

test('engine priority follows reusable owned fuel, independent draw and the actual upcoming Act', () => {
  const fuel = owned('TrueGrit', 1, 'Skill', 1, { exhausts_other_cards: true });
  const pommel = owned('PommelStrike', 1, 'Attack', 1, { draw_cards: 1 });
  const dark = owned('DarkEmbrace', 1, 'Power', 2, { exhaust_draw_power: 1 });
  const current = root([fuel, pommel], [dark]);
  current.position.observation.observation.run = { act: 1, pending_act_entry: 2 };
  assert.equal(preparationDeckComparison(current).candidates[0].preference.id, 'owned-reusable-exhaust-access');
  delete current.position.observation.observation.run.pending_act_entry;
  assert.equal(preparationDeckComparison(current).candidates[0].preference, undefined);
  current.position.observation.observation.run.act = 2;
  pommel.card.upgrades = 1; pommel.mechanics.upgrades = 1; pommel.mechanics.draw_cards = 2;
  assert.match(preparationDeckComparison(current).candidates[0].preference.text, /owned 1 on-play multi-draw copies/);
  pommel.mechanics.draw_cards = 1;
  fuel.mechanics.exhausts = true;
  assert.equal(preparationDeckComparison(current).candidates[0].preference, undefined);
  fuel.mechanics.exhausts = false;
  dark.mechanics.cost = 4;
  assert.equal(preparationDeckComparison(current).candidates[0].preference, undefined);
});

test('instance overrides supply payment and fixed Block while absent deliveries remain uncounted', () => {
  const corruption = owned('Corruption', 1, 'Power', 3);
  corruption.card.cost_modifier = -3;
  corruption.persistent_cost = 0;
  const defend = owned('Defend', 1, 'Skill', 1, { base_block_delivery: 'once_on_play', block: 5 });
  defend.card.base_block_override = 17;
  defend.persistent_block = 17;
  const rage = owned('Rage', 1, 'Skill', 0, { block: 3 });
  rage.persistent_block = 3;
  const profile = preparationDeckComparison(root([corruption, defend, rage]));
  assert.deepEqual(profile.fixedPlayedBlock, [{ card: '1x Defend', cost: 1, block: 17 }]);
  assert.equal(profile.counts.Power, 1);
});

test('stale sources, changed instance attributes and incomplete groups stop structural claims', () => {
  const current = root([owned('Strike', 5, 'Attack', 1)]);
  current.deck_evidence = structuredClone(current.deck_evidence);
  current.position.public_snapshot_id = 'next';
  assert.throws(() => preparationDeckComparison(current), /current public inventory/);
  current.position.public_snapshot_id = 'root';
  current.position.observation.observation.cards[0].cost_modifier = -1;
  assert.throws(() => preparationDeckComparison(current), /owned card attributes/);
  current.position.observation.observation.cards[0].cost_modifier = 0;
  current.deck_evidence.cards.push(structuredClone(current.deck_evidence.cards[0]));
  assert.throws(() => preparationDeckComparison(current), /current public inventory/);
});

test('structural advice keeps every legal candidate and supplies no model rating', () => {
  const current = root([owned('Strike', 5, 'Attack', 1)], [owned('UnknownFutureCard', 1, 'Attack', 1)]);
  const before = structuredClone(current.position.actions);
  const profile = preparationDeckComparison(current, command => `normalized ${command}`);
  assert.equal(profile.candidates[0].command, 'normalized take:UnknownFutureCard#0');
  assert.ok(profile.candidates.every(row => !Object.hasOwn(row, 'category')));
  assert.deepEqual(current.position.actions, before);
  delete current.deck_evidence;
  assert.equal(preparationDeckComparison(current), undefined);
});
