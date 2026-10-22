import test from 'node:test';
import assert from 'node:assert/strict';
import { preparationReading, preparationReadingSupply, preparationRewardProgress, preparationCandidateGroups, preparationConditionBindings, preparationClassificationContext, preparationProceduralAction,
  preparationCandidates, assertPreparationCommand, parsePreparationClassification, collectPreparationClassification,
  preparationClassificationReceipt, preparationCommandBasis, preparationRecordedTable, preparationStageGuide } from './preparation.mjs';
import { conciseCardCommand, strategicOptionCommand } from '../run_play/contract.mjs';
import { preparationFixture } from './preparation-fixture.test.mjs';

// These tests exercise specialized mechanics; every other candidate gets the shared effect contract.
const specificBindings = (observation, normalize) => preparationConditionBindings(observation, normalize)
  .filter(row => row.id !== 'preparation-effect-tradeoff-v1');

const observation = cards => ({ schema: 'sts-run-play-agent-observation-v3', session_id: 'unit', head_revision: 0, state_revision: 0,
  position: { public_snapshot_id: 'unit-snapshot', observation: { domain: 'strategic', observation: { cards } }, actions: [
    { semantic_action: 'buy:Armaments+0#0', candidate: { action: { kind: 'buy_card', card: 'Armaments' } } },
    { semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } },
  ] } });
const basis = 'buy:Armaments+0#0 | high | sequence | @effect,@resource-tradeoff\nleave | medium | direct | @effect,@resource-tradeoff';
const table = (current, text) => ({ source: current, rows: parsePreparationClassification(text, current, conciseCardCommand) });

test('rendered candidate effects move intact beside their own conditions while inventory and routes retain their place', () => {
  const current = observation([]);
  const effect = '  效果：武装\n  Card Armaments | 1c\n    Gain Block and upgrade a Hand card.';
  current.preparation_reading = preparationFixture(current, { overview: 'Act 2 F21\nOwned: Strike',
    routes: 'Public route\n  Shop before Elite\nMore: 不按标题删除任何规则',
    candidateViews: { 'buy:Armaments+0#0': effect } });
  const supplied = preparationReadingSupply(current, conciseCardCommand);
  assert.equal(supplied.candidates['buy:Armaments#0'].view, effect);
  assert.match(supplied.reading, /Act 2 F21\nOwned: Strike/);
  assert.match(supplied.reading, /Public route\n  Shop before Elite/);
  assert.match(supplied.reading, /More: 不按标题删除/);
  assert.doesNotMatch(supplied.reading, /Card Armaments/);
  current.candidateReadings = supplied.candidates;
  const context = preparationClassificationContext(current, conciseCardCommand);
  assert.match(context, /^Current classification candidates:[^\n]*\nPending commands: buy:Armaments#0; leave\./);
  assert.match(context, /Condition references for buy:Armaments#0[^]*Card Armaments[^]*@effect/);
  assert.match(context, /效果：武装/);
  assert.doesNotMatch(context, /buy:Armaments#0\s+\|\s+Armaments/);
  assert.equal(context.split('Card Armaments').length - 1, 1);
  current.preparation_reading.candidates.push(current.preparation_reading.candidates[0]);
  assert.throws(() => preparationReadingSupply(current, conciseCardCommand), /Duplicate rendered preparation command/);
});

test('a native Boss reward display label stays separate from the exact adoption command', () => {
  const current = observation([]);
  current.position.observation.observation.context = { kind: 'boss_relic' };
  current.position.actions = [{ semantic_action: 'boss-relic:PhilosopherStone#0',
    candidate: { action: { kind: 'take_boss_relic', relic: 'PhilosopherStone' } } }];
  const native = 'boss-relic:PhilosopherStone#0  |  take Philosopher\'s Stone\n  Gain 1 Energy; enemies gain 1 Strength.';
  const effect = "  Effect label: take Philosopher's Stone\n  Gain 1 Energy; enemies gain 1 Strength.";
  current.preparation_reading = preparationFixture(current, { candidateViews: { 'boss-relic:PhilosopherStone#0': effect } });
  const supply = preparationReadingSupply(current);
  current.candidateReadings = supply.candidates;
  const context = preparationClassificationContext(current);
  assert.match(context, /Condition references for boss-relic:PhilosopherStone#0:/);
  assert.match(context, /Effect label: take Philosopher's Stone\n  Gain 1 Energy; enemies gain 1 Strength/);
  assert.doesNotMatch(context, /boss-relic:PhilosopherStone#0\s+\|\s+take/);
  assert.equal(supply.candidates['boss-relic:PhilosopherStone#0'].view, effect, 'native host evidence remains intact');
  assertPreparationCommand(['boss-relic:PhilosopherStone#0', []], 'pending', current);
  assert.throws(() => assertPreparationCommand(['boss-relic:PhilosopherStone#0', ['|', 'take', 'Philosopher\'s', 'Stone']], 'pending', current),
    /Copy the command boss-relic:PhilosopherStone#0 into choice/);
});

test('classification supplies its row format once and keeps candidate facts/references in actual choice order', () => {
  const current = observation([]);
  current.position.actions.reverse();
  current.candidateReadings = { 'buy:Armaments#0': { view: 'Card Armaments | 1c\nPrinted rule: Gain Block and upgrade a Hand card.' } };
  const context = preparationClassificationContext(current, conciseCardCommand);
  assert.equal(context.split('high/medium/low/review').length - 1, 1);
  assert.equal(context.split('direct/sequence/future/review').length - 1, 1);
  assert.equal(context.split('Printed rule: Gain Block and upgrade a Hand card.').length - 1, 1);
  assert.ok(context.indexOf('Condition references for leave:') < context.indexOf('Condition references for buy:Armaments#0:'));
  assert.equal(parsePreparationClassification(basis, current, conciseCardCommand).length, 2);
});

test('source-bound omitted option blocks stay out of decision text while inventory rules and unknown controls remain', () => {
  const current = observation([]);
  current.preparationOmissions = { source: { ...current }, commands: ['remove:Inflame+0#0'] };
  current.preparation_reading = preparationFixture(current, { overview: 'Owned: Inflame\n  Playing Inflame grants 2 Strength.',
    selections: 'unknown  |  Keep this control\n  Its detail',
    candidateViews: { 'buy:Armaments+0#0': 'Printed rule: Gain Block and upgrade a Hand card.' } });
  current.preparation_reading.candidates.push({ command: 'remove:Inflame#0', label: 'Remove Inflame', view: 'Removal changes the deck.', card_rules: [] });
  const supplied = preparationReadingSupply(current, conciseCardCommand);
  assert.match(supplied.reading, /Owned: Inflame\n  Playing Inflame grants 2 Strength/);
  assert.doesNotMatch(supplied.reading, /remove:Inflame|Removal changes/);
  assert.match(supplied.reading, /unknown.*Keep this control\n  Its detail/);
  assert.match(supplied.candidates['buy:Armaments#0'].view, /Printed rule: Gain Block and upgrade a Hand card/);
  current.state_revision = 1;
  assert.match(preparationReadingSupply(current, conciseCardCommand).reading, /remove:Inflame/);
});

test('small choice resource supply retains native owned relic and potion rules as one separate block', () => {
  const resources = 'Relics (1):\n  Mummified Hand\n    Power play changes one Hand card cost.\nPotions (2 slots; availability comes from current legal options):\n  P0 Fear Potion\n    Apply Vulnerable to enemies.\n  P1 empty';
  const current = observation([]);
  current.preparation_reading = preparationFixture(current);
  current.preparation_reading.inventory.resources = resources;
  current.preparation_reading.inventory.view = resources;
  current.preparation_reading.inventory.relics = [{ relic: 'MummifiedHand', view: '木乃伊之手：随机改变一张手牌费用。' }];
  const supplied = preparationReadingSupply(current, conciseCardCommand);
  assert.ok(supplied.reading.includes(resources));
  assert.doesNotMatch(supplied.reading, /Strike|Next section/);
  assert.equal(supplied.relicReadings.MummifiedHand, '木乃伊之手：随机改变一张手牌费用。');
  current.preparation_reading.public_snapshot_id = 'another-position';
  assert.throws(() => preparationReadingSupply(current), /not bound/);
});

test('classification records benefit independently of realization and covers all current commands', () => {
  const current = observation([]);
  const [row] = parsePreparationClassification(basis, current, conciseCardCommand);
  assert.deepEqual(parsePreparationClassification(basis.replace('\n', '\n\n  \n'), current, conciseCardCommand),
    parsePreparationClassification(basis, current, conciseCardCommand));
  assert.equal(row.category, 'high');
  assert.equal(row.realization, 'sequence');
  assert.equal(row.command, 'buy:Armaments#0');
  for (const text of [basis.split('\n')[0], basis + '\nremove:AscendersBane#0 | low | direct | @effect,@resource-tradeoff',
    basis + '\nleave | low | direct | @effect,@resource-tradeoff', basis.replace('high', 'conditional'),
    basis.replace('sequence', '0.9'), basis.replace(' | sequence', '')]) {
    assert.throws(() => parsePreparationClassification(text, current, conciseCardCommand));
  }
});

test('every candidate uses local references and rejects invented facts and borrowed IDs', () => {
  const current = observation([]);
  for (const reason of ['@limit-redraw', '@effect -- deploy this engine', 'No payoff attack is owned yet.']) {
    assert.throws(() => parsePreparationClassification(basis.replace('@effect,@resource-tradeoff', reason),
      current, conciseCardCommand), /supplied condition references/);
  }
  assert.doesNotMatch(preparationStageGuide('pending'), /@limit-redraw|@block-access/);
});

test('classification references stay distinct from adoption comparison references', () => {
  const current = observation([]);
  for (const reason of ['@function', '@protection,@disarm', '@card-reward']) {
    assert.throws(() => parsePreparationClassification(basis.replace('@effect,@resource-tradeoff', reason),
      current, conciseCardCommand), /supplied condition references/);
  }
  assert.equal(parsePreparationClassification(basis, current, conciseCardCommand)[0].realization, 'sequence');
});

test('command-only adoption binds a host comparison while preserving the selected model row', () => {
  const current = observation([]);
  current.position.actions.push({ semantic_action: 'buy:Flex#0', candidate: { action: { kind: 'buy_card', card: 'Flex' } } });
  const classified = table(current, basis + '\nbuy:Flex#0 | low | direct | @effect,@resource-tradeoff');
  const command = ['buy:Flex#0', []];
  const automatic = preparationCommandBasis(command, 'pending', undefined, current, classified, conciseCardCommand);
  assert.equal(automatic.basisKind, 'host-comparison');
  assert.equal(automatic.command, 'buy:Flex#0');
  assert.equal(automatic.selectedRow.category, 'low');
  assert.equal(automatic.alternativeRow.command, 'buy:Armaments#0');
  assert.deepEqual(automatic.comparisonIds, ['@function', '@access', '@resources']);
  assert.deepEqual(classified.rows.map(row => row.category), ['high', 'medium', 'low']);
  const explicit = preparationCommandBasis(command, 'pending', 'leave | @resources', current, classified, conciseCardCommand);
  assert.equal(explicit.basisKind, 'player-comparison');
  assert.equal(explicit.alternativeRow.command, 'leave');
  assert.equal(preparationCommandBasis(command, 'pending', '  ', current, classified, conciseCardCommand).basisKind, 'host-comparison');
  assert.throws(() => preparationCommandBasis(command, 'pending', undefined, { ...current, head_revision: 1 }, classified, conciseCardCommand), /complete current table/);
  assert.throws(() => preparationCommandBasis(command, 'pending', undefined, current, { ...classified, rows: classified.rows.slice(0, 2) }, conciseCardCommand), /complete current table/);
  const review = { ...classified, rows: classified.rows.map(row => row.command === 'buy:Flex#0' ? { ...row, category: 'review' } : row) };
  assert.throws(() => preparationCommandBasis(command, 'pending', undefined, current, review, conciseCardCommand), /missing fact/);
});

test('adoption shares exact basis text while keeping every rating, condition and command association', () => {
  const current = observation([]);
  const classified = table(current, basis);
  classified.rows[0].fixedMechanics.push('One-copy upgrade needs the selected target in Hand.');
  classified.rows[1].fixedMechanics.push('Leaving preserves the current Gold.');
  const before = structuredClone(classified);
  const text = preparationRecordedTable(classified, current, conciseCardCommand);
  assert.match(text, /buy:Armaments#0 \| high \| sequence \| @effect,@resource-tradeoff/);
  assert.match(text, /leave \| medium \| direct \| @effect,@resource-tradeoff/);
  const shared = classified.rows[0].conditions[0].text;
  assert.equal(text.split(shared).length - 1, 1);
  assert.match(text, /Applies to buy:Armaments#0; leave:/);
  assert.match(text, /Applies to buy:Armaments#0:\n    One-copy upgrade/);
  assert.match(text, /Applies to leave:\n    Leaving preserves/);
  for (const row of classified.rows) {
    for (const condition of row.conditions) assert.ok(text.includes(condition.text));
    for (const fact of row.fixedMechanics) assert.ok(text.includes(fact));
  }
  assert.doesNotMatch(text, /Comparison references:|Current access candidates:/);
  assert.deepEqual(classified, before);
  const legacy = { ...classified, rows: classified.rows.map(row => ({ command: row.command,
    category: row.category, realization: row.realization, reason: `Legacy basis for ${row.command}.` })) };
  const restored = preparationRecordedTable(legacy, current, conciseCardCommand);
  for (const row of legacy.rows) assert.equal(restored.split(row.reason).length - 1, 1);
});

test('all legal candidates have one reference contract, including uncovered effect families', () => {
  const current = observation([]);
  current.position.actions.push({ semantic_action: 'event:Example:0', candidate: { action: { kind: 'choose_event_option' } } });
  const bindings = preparationConditionBindings(current, conciseCardCommand);
  assert.equal(bindings.length, preparationCandidateGroups(current, conciseCardCommand).length);
  assert.equal(new Set(bindings.map(row => row.command)).size, bindings.length);
  assert.ok(bindings.every(row => row.choices.some(choice => choice.id === '@review-access')));
  assert.doesNotMatch(preparationClassificationContext(current, conciseCardCommand), /short factual reason/);
});

test('row collection isolates a bad reason, preserves ratings, and supplies only remaining work', () => {
  const current = observation([]);
  const first = collectPreparationClassification(basis.replace('@effect,@resource-tradeoff', 'No payoff attack is owned yet.'), current, conciseCardCommand);
  assert.equal(first.complete, false);
  assert.deepEqual(first.acceptedCommands, ['leave']);
  assert.deepEqual(first.pendingCommands, ['buy:Armaments#0']);
  assert.equal(first.issues.length, 1);
  assert.doesNotMatch(preparationClassificationReceipt(first), /No payoff attack/);
  const supplied = preparationClassificationContext(current, conciseCardCommand, first);
  assert.match(supplied, /1 pending output rows of 2/);
  assert.match(supplied, /Pending commands: buy:Armaments#0\./);
  assert.doesNotMatch(supplied, /Pending commands:[^\n]*leave/);
  assert.match(supplied, /Accepted classification rows.*leave \| medium \| direct/s);
  assert.doesNotMatch(supplied, /Condition references for leave|No payoff attack/);
  const final = collectPreparationClassification('buy:Armaments#0 | high | sequence | @effect,@owned-support', current, conciseCardCommand, first);
  assert.ok(final.complete);
  assert.deepEqual(final.rows.find(row => row.command === 'leave'), first.rows[0]);
  assert.deepEqual(final.acceptedCommands, ['buy:Armaments#0']);
});

test('a conflicting repeat leaves the accepted row unchanged and still accepts a pending row', () => {
  const current = observation([]);
  const first = collectPreparationClassification('leave | low | direct | @resource-tradeoff', current);
  const final = collectPreparationClassification('leave | high | future | @effect\nbuy:Armaments+0#0 | medium | sequence | @owned-support', current, undefined, first);
  assert.ok(final.complete);
  assert.deepEqual(final.rows.find(row => row.command === 'leave'), first.rows[0]);
  assert.match(final.issues[0].message, /Already accepted/);
});

test('duplicate rows in one submission remain unresolved while other valid rows survive', () => {
  const current = observation([]);
  const result = collectPreparationClassification(basis + '\nbuy:Armaments#0 | low | direct | @effect', current, conciseCardCommand);
  assert.deepEqual(result.pendingCommands, ['buy:Armaments#0']);
  assert.deepEqual(result.acceptedCommands, ['leave']);
  assert.equal(result.issues.length, 2);
});

test('partial classification retains a legal full-table choice and comparison across row corrections', () => {
  const current = observation([]);
  const first = collectPreparationClassification('leave | low | direct | @effect', current, conciseCardCommand, undefined,
    { choice: 'buy:Armaments+0#0', comparison: 'leave | @resources' });
  assert.equal(first.pendingChoice, 'buy:Armaments#0', 'the chosen row may still be pending');
  assert.match(preparationClassificationReceipt(first), /Saved choice: buy:Armaments#0/);
  assert.match(preparationClassificationContext(current, conciseCardCommand, first), /Saved choice: buy:Armaments#0/);
  assert.match(preparationStageGuide('pending', undefined, {}, first), /remaining basis rows.*saved choice/s);
  const final = collectPreparationClassification('buy:Armaments#0 | high | sequence | @effect', current, conciseCardCommand, first);
  assert.ok(final.complete);
  assert.equal(final.pendingChoice, 'buy:Armaments#0');
  assert.equal(final.pendingComparison, 'leave | @resources');
  const conflict = collectPreparationClassification('buy:Armaments#0 | high | sequence | @effect', current, conciseCardCommand, first,
    { choice: 'leave' });
  assert.ok(conflict.complete, 'valid correction rows survive a conflicting choice');
  assert.equal(conflict.pendingChoice, 'buy:Armaments#0');
  assert.match(conflict.choiceIssue, /submit command choose/);
  const invalid = collectPreparationClassification('leave | low | direct | @effect', current, conciseCardCommand, undefined,
    { choice: 'remove:AscendersBane#0' });
  assert.equal(invalid.pendingChoice, undefined);
  assert.ok(invalid.choiceIssue);
});

test('each authoritative source component invalidates a draft before further collection', () => {
  const current = observation([]);
  const first = collectPreparationClassification('leave | low | direct | @resource-tradeoff', current, undefined, undefined, { choice: 'leave' });
  for (const patch of [{ schema: 'sts-run-play-agent-observation-v4' }, { session_id: 'other' },
    { head_revision: 1 }, { state_revision: 1 }]) {
    const changed = { ...current, ...patch };
    const next = collectPreparationClassification('buy:Armaments+0#0 | high | direct | @effect', changed, undefined, first);
    assert.deepEqual(next.pendingCommands, ['leave']);
    assert.deepEqual(next.acceptedCommands, ['buy:Armaments+0#0']);
    assert.equal(next.pendingChoice, undefined);
    assert.doesNotMatch(preparationClassificationContext(changed, undefined, first), /Accepted classification rows|Saved choice/);
  }
});

test('row collection rejects illegal commands and foreign references without retaining their prose', () => {
  const current = observation([]);
  const result = collectPreparationClassification('remove:AscendersBane#0 | high | direct | @effect\n'
    + 'buy:Armaments+0#0 | high | direct | @function\n' + 'leave | medium | direct | @effect', current);
  assert.equal(result.issues.length, 2);
  assert.deepEqual(result.pendingCommands, ['buy:Armaments+0#0']);
  assert.deepEqual(result.acceptedCommands, ['leave']);
  assert.throws(() => preparationCommandBasis(['leave', []], 'pending', '', current, undefined), /Call classify/);
});

function fixedAttackUpgrade(card, beforeDamage, beforeHits, afterDamage, afterHits) {
  const current = observation([]), semantic = `smith:${card}+0#0`;
  current.position.observation.observation.context = { kind: 'campfire' };
  current.position.actions = [{ semantic_action: semantic,
    candidate: { domain: 'strategic', action: { kind: 'smith', card, upgrades: 0 } } }];
  const definition = (upgrades, damage, hits) => ({ card, upgrades, cost: 1,
    damage: { damage_per_hit: damage, hit_count: hits, delivery: {
      kind: 'attack', amount: { kind: 'base' }, hits: { kind: 'fixed', count: hits }, targets: 'random_enemy_per_hit' } } });
  current.decision_evidence = { candidates: [{ semantic_actions: [semantic],
    before: definition(0, beforeDamage, beforeHits), after: definition(1, afterDamage, afterHits) }] };
  return current;
}

test('extra-hit upgrades bind the actual hit delta, Strength applications and single-enemy targeting', () => {
  const current = fixedAttackUpgrade('SwordBoomerang', 3, 3, 3, 4);
  const binding = specificBindings(current, conciseCardCommand)[0];
  assert.equal(binding.command, 'smith:SwordBoomerang#0');
  assert.match(binding.fixedMechanics.join('\n'), /base damage per hit 3->3; hits per play 3->4/);
  assert.match(binding.fixedMechanics.join('\n'), /base damage across those hits is 9->12/);
  assert.match(binding.fixedMechanics.join('\n'), /Strength applies to each hit/);
  assert.match(binding.fixedMechanics.join('\n'), /With one enemy, every hit goes to that enemy/);
  assert.ok(binding.choices.some(row => row.id === '@extra-hit'));
  assert.ok(!binding.choices.some(row => row.id === '@base-damage'));
  const localized = preparationConditionBindings(current, conciseCardCommand, 'zh-CN')[0];
  assert.deepEqual(localized.choices.map(row => row.id), binding.choices.map(row => row.id));
  assert.match(localized.fixedMechanics.join('\n'), /每次打出命中数 3->4/);
  assert.match(localized.fixedMechanics.join('\n'), /基础伤害合计 9->12/);
  const rows = parsePreparationClassification('smith:SwordBoomerang#0 | medium | sequence | @extra-hit', current, conciseCardCommand);
  assert.match(rows[0].reason, /hits per play 3->4/);
  assert.throws(() => parsePreparationClassification('smith:SwordBoomerang#0 | low | direct | +1 damage per hit.',
    current, conciseCardCommand), /reference/);
});

test('base-damage upgrades keep the existing hit count separate from an extra Strength application', () => {
  const current = fixedAttackUpgrade('TwinStrike', 5, 2, 7, 2);
  const binding = specificBindings(current, conciseCardCommand)[0];
  assert.match(binding.fixedMechanics.join('\n'), /base damage per hit 5->7; hits per play 2->2/);
  assert.match(binding.fixedMechanics.join('\n'), /base damage across those hits is 10->14/);
  assert.ok(binding.choices.some(row => row.id === '@base-damage'));
  assert.ok(!binding.choices.some(row => row.id === '@extra-hit'));
});

test('an attack upgrade keeps its draw increment in the same comparison as its damage increment', () => {
  const current = fixedAttackUpgrade('PommelStrike', 9, 1, 10, 1);
  const effect = current.decision_evidence.candidates[0];
  for (const reading of [effect.before, effect.after]) reading.damage.delivery.targets = 'selected_enemy';
  effect.before.draw_cards = 1; effect.after.draw_cards = 2;
  const binding = specificBindings(current, conciseCardCommand)[0];
  assert.match(binding.fixedMechanics.join(' '), /base damage per hit 9->10/);
  assert.match(binding.fixedMechanics.join(' '), /draw request 1->2 before NoDraw and Hand capacity/);
  assert.ok(binding.choices.some(row => row.id === '@draw-increment'));
  const rows = parsePreparationClassification('smith:PommelStrike#0 | high | direct | @base-damage', current, conciseCardCommand);
  assert.match(rows[0].reason, /draw request 1->2/, 'a damage reference must retain the accompanying access change');
  delete effect.after.draw_cards;
  assert.doesNotMatch(specificBindings(current, conciseCardCommand)[0].fixedMechanics.join(' '), /draw request/);
});

test('campfire and map comparisons omit inventory disposal while reward potion replacement retains it', () => {
  const original = observation([]);
  original.position.observation.observation.potions = [{ potion: { potion: 'BlockPotion' } }];
  const discard = { semantic_action: 'discard-potion:BlockPotion#0', candidate: { action: { kind: 'discard_run_potion' } } };
  for (const kind of ['campfire', 'map']) {
    original.position.observation.observation.context = { kind };
    const action = { semantic_action: kind === 'campfire' ? 'rest' : 'go 1,2',
      candidate: { action: { kind: kind === 'campfire' ? 'rest' : 'choose_route_node' } } };
    original.position.actions = [discard, action];
    const scoped = preparationReading(original);
    assert.deepEqual(scoped.position.actions, [action]);
    assert.deepEqual(original.position.actions, [discard, action]);
    assert.deepEqual(scoped.preparationOmissions.commands, [discard.semantic_action]);
  }
  original.position.observation.observation.context = { kind: 'reward', pending_rewards: [{ kind: 'potion', potion: 'SwiftPotion' }] };
  original.position.actions = [discard];
  assert.equal(preparationReading(original).position.actions[0], discard);
});

test('attack-upgrade arithmetic stays within fixed base hits and yields to payment-changing upgrades', () => {
  for (const mutate of [
    evidence => { evidence.after.cost = 0; },
    evidence => { evidence.after.damage.delivery.amount.kind = 'player_block'; },
    evidence => { evidence.after.damage.delivery.hits.kind = 'energy_spent'; },
    evidence => { evidence.after.damage.hit_count = 5; },
    evidence => { evidence.after.damage.delivery.targets = 'all_enemies'; },
  ]) {
    const current = fixedAttackUpgrade('SwordBoomerang', 3, 3, 3, 4);
    mutate(current.decision_evidence.candidates[0]);
    assert.ok(!specificBindings(current, conciseCardCommand).some(row => row.id === 'fixed-attack-upgrade-v1'));
  }
});

test('Strike-scaled upgrade binds the whole-pile count and preserves either rating', () => {
  const current = fixedAttackUpgrade('PerfectedStrike', 6, 1, 6, 1);
  const evidence = current.decision_evidence.candidates[0];
  evidence.before.damage.delivery.amount = { kind: 'base_plus_strike_cards', per_card: 2 };
  evidence.after.damage.delivery.amount = { kind: 'base_plus_strike_cards', per_card: 3 };
  const binding = specificBindings(current, conciseCardCommand)[0];
  assert.equal(binding.id, 'strike-scaled-attack-upgrade-v1');
  assert.match(binding.fixedMechanics.join(' '), /6 \+ 2 \* N -> 6 \+ 3 \* N/);
  assert.match(binding.fixedMechanics.join(' '), /Hand, Draw and Discard together, including the played copy exactly once/);
  assert.match(binding.fixedMechanics.join(' '), /entering Exhaust lowers N by one/);
  assert.match(binding.fixedMechanics.join(' '), /increment at the same N: 0 \+ 1 \* N/);
  for (const rating of ['high', 'low']) {
    const [row] = parsePreparationClassification(`smith:PerfectedStrike#0 | ${rating} | direct | @strike-count,@upgrade-opportunity`,
      current, conciseCardCommand);
    assert.equal(row.category, rating);
    assert.match(row.reason, /6 \+ 2 \* N -> 6 \+ 3 \* N/);
  }
  assert.throws(() => parsePreparationClassification('smith:PerfectedStrike#0 | low | direct | needs 3 Strikes drawn with it',
    current, conciseCardCommand), /Use supplied condition references/);
  for (const mutate of [
    row => { row.after.damage.delivery.amount.per_card = undefined; },
    row => { row.after.damage.hit_count = 0; },
    row => { row.after.cost = 0; },
    row => { row.after.card = 'Strike'; },
  ]) {
    const changed = structuredClone(current);
    mutate(changed.decision_evidence.candidates[0]);
    assert.ok(!specificBindings(changed, conciseCardCommand).some(row => row.id === binding.id));
  }
});

test('event combat binds victory rewards and observed HP cost to the current fight option', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  state.context = { kind: 'event', event: 'MaskedBandits' };
  state.run = { current_hp: 26, max_hp: 80 };
  state.public_history = { recent_combat_hp_loss: 54, recent_combat_turns: 11 };
  const pay = { semantic_action: 'event:MaskedBandits:0', candidate: { action: { kind: 'choose_event_option',
    public_semantics: { starts_combat: false, structured_semantics: { transition: { kind: 'advance_screen' } } } } } };
  const fight = { semantic_action: 'event:MaskedBandits:1', candidate: { action: { kind: 'choose_event_option',
    public_semantics: { starts_combat: true, hp_delta: 0, structured_semantics: { transition: { kind: 'start_combat' } } } } } };
  current.position.actions = [pay, fight];
  const [binding] = specificBindings(current);
  assert.equal(binding.command, fight.semantic_action);
  assert.match(binding.fixedMechanics.join(' '), /received after victory.*HP cost is determined by the fight/);
  assert.match(binding.fixedMechanics.join(' '), /26\/80.*net HP loss 54/);
  const basis = `${pay.semantic_action} | medium | direct | @effect,@resource-tradeoff\n`
    + `${fight.semantic_action} | review | review | @review-access`;
  const rows = parsePreparationClassification(basis, current);
  assert.equal(rows[1].bindingId, 'event-combat-risk-v1');
  assert.match(rows[1].reason, /Combat-end healing resolves when combat settles/);
  assert.throws(() => parsePreparationClassification(basis.replace('@review-access', 'BurningBlood heals the incoming damage.'), current), /Use supplied condition references/);
  const assessed = basis.replace('review | review | @review-access', 'high | direct | @combat-risk,@victory-reward');
  const misplaced = assessed.replace('@combat-risk,@victory-reward', 'Win the relic after this fight. | @combat-risk,@victory-reward');
  assert.throws(() => parsePreparationClassification(misplaced, current),
    /Replacement row, preserving your ratings and references: event:MaskedBandits:1 \| high \| direct \| @combat-risk,@victory-reward/);
  assert.equal(parsePreparationClassification(assessed, current)[1].category, 'high');
  assert.throws(() => parsePreparationClassification(misplaced.replace('@victory-reward', '@invented'), current), error => {
    assert.doesNotMatch(error.message, /Replacement row/);
    return true;
  });
  state.run.current_hp = 70;
  delete state.public_history;
  const refreshed = specificBindings(current)[0].fixedMechanics.join(' ');
  assert.match(refreshed, /70\/80/);
  assert.doesNotMatch(refreshed, /net HP loss 54/);
  fight.candidate.action.public_semantics.structured_semantics.transition.kind = 'complete';
  assert.deepEqual(specificBindings(current), []);
});

test('reward navigation completes open/skip/exit and survives reload without consuming the reward', () => {
  const root = observation([]);
  const item = { kind: 'card_reward', cards: [{ card: 'Entrench', upgrades: 0 }] };
  const row = (semantic_action, action) => ({ semantic_action, candidate: { action } });
  root.position.observation.observation.context = { kind: 'reward', pending_rewards: [item] };
  const open = row('open-card#0', { kind: 'open_card_reward', reward_item_index: 0 });
  const exit = row('proceed', { kind: 'proceed' });
  const discard = row('discard-potion:BlockPotion#0', { kind: 'discard_run_potion' });
  root.position.actions = [open, exit, discard];
  assert.equal(preparationProceduralAction(root).row, open);
  const opened = structuredClone(root);
  opened.head_revision = opened.state_revision = 1;
  opened.position.observation.observation.context = { kind: 'card_reward', reward_item_index: 0 };
  let progress = preparationRewardProgress(undefined, root, opened, open.candidate.action);
  const skipped = structuredClone(root);
  skipped.head_revision = skipped.state_revision = 2;
  progress = preparationRewardProgress(progress, opened, skipped, { kind: 'skip_card_reward', reward_item_index: 0 });
  const filtered = preparationReading(skipped, JSON.parse(JSON.stringify(progress)));
  assert.equal(preparationProceduralAction(filtered).row.semantic_action, 'proceed');
  assert.equal(skipped.position.actions.length, 3, 'actual legal navigation remains untouched');
  const externallyMoved = structuredClone(skipped);
  externallyMoved.head_revision++;
  assert.strictEqual(preparationReading(externallyMoved, progress), externallyMoved);
  const exited = structuredClone(skipped);
  exited.head_revision = exited.state_revision = 3;
  exited.position.observation.observation.context = { kind: 'map' };
  assert.equal(preparationRewardProgress(progress, skipped, exited, exit.candidate.action), undefined);
});

test('declined reward indices follow typed gold removal and preserve identical unreviewed rewards', () => {
  const before = observation([]);
  const card = { kind: 'card_reward', cards: [{ card: 'Entrench', upgrades: 0 }] };
  before.position.observation.observation.run = { gold: 20 };
  before.position.observation.observation.context = { kind: 'reward', pending_rewards: [{ kind: 'gold', amount: 5 }, card, card] };
  const after = structuredClone(before);
  after.head_revision = after.state_revision = 1;
  after.position.observation.observation.run.gold = 25;
  after.position.observation.observation.context.pending_rewards.shift();
  after.position.actions = [0, 1].map(i => ({ semantic_action: `open-card#${i}`,
    candidate: { action: { kind: 'open_card_reward', reward_item_index: i } } }));
  const progress = preparationRewardProgress({ source: before, declined: [1] }, before, after,
    { kind: 'claim_reward', reward_item_index: 0 });
  assert.deepEqual(progress.declined, [0]);
  assert.deepEqual(preparationReading(after, progress).position.actions.map(row => row.semantic_action), ['open-card#1']);
  const changed = structuredClone(after);
  changed.head_revision = changed.state_revision = 2;
  changed.position.observation.observation.cards.push({ card: 'Barricade' });
  assert.deepEqual(preparationRewardProgress(progress, after, changed, { kind: 'take_card' }).declined, []);
});

test('adoption compares another legal candidate using a source-bound recorded table', () => {
  const current = observation([]), classified = table(current, basis);
  const command = ['buy:Armaments#0', []];
  const compare = (text, saved = classified) => preparationCommandBasis(command, 'pending', text, current, saved, conciseCardCommand);
  assert.match(compare('leave | @function,@access').renderedBasis, /high \| sequence.*Compared with leave/);
  assert.throws(() => preparationCommandBasis(command, 'pending', 'leave | @access', current, undefined, conciseCardCommand), /Call classify/);
  assert.throws(() => compare('read | @access'), /another current legal/);
  assert.throws(() => compare('buy:Armaments#0 | @access'), /another current legal/);
  assert.throws(() => compare('Just choose it.'), /comparison basis/);
  for (const text of ['leave | Invented mechanics.', 'leave | @unknown', 'leave | @access,@access']) {
    assert.throws(() => compare(text), /supplied comparison references/);
  }
  const bound = compare('leave | @function,@access');
  assert.deepEqual(bound.selectedRow, classified.rows[0]);
  assert.deepEqual(bound.alternativeRow, classified.rows[1]);
  assert.deepEqual(bound.comparisonIds, ['@function', '@access']);
  assert.throws(() => compare('leave | @access', { ...classified, source: { ...current, state_revision: 1 } }), /Call classify/);
  for (const text of [basis.replace('high', 'review'), basis.replace('sequence', 'review')].map(text =>
    text.replace('@effect,@resource-tradeoff', '@review-access'))) {
    assert.throws(() => compare('leave | @access', table(current, text)), /missing fact/);
  }
});

test('a single classified group binds the sole row without an artificial alternative', () => {
  const current = observation([]); current.position.actions = current.position.actions.slice(1);
  const classified = table(current, 'leave | medium | direct | @effect,@resource-tradeoff');
  assert.match(preparationRecordedTable(classified, current), /Sole candidate adoption: command leave/);
  const adoption = preparationCommandBasis(['leave', []], 'pending', undefined, current, classified);
  assert.match(adoption.renderedBasis, /Sole classified candidate/);
  assert.equal(adoption.alternativeRow, null);
  assert.equal(adoption.basisKind, 'sole-candidate');
});

test('strategy checks use current owned upgrades and offered cards', () => {
  const current = observation([{ card: 'DemonForm', count: 1, upgrades: 0 }, { card: 'LimitBreak', count: 1, upgrades: 0 }]);
  const text = preparationClassificationContext(current);
  assert.match(text, /Armaments with LimitBreak/); assert.match(text, /Strength plan/);
  assert.doesNotMatch(text, /Evolve:|Clash:|Corruption with/);
  current.position.observation.observation.cards[1].upgrades = 1;
  assert.doesNotMatch(preparationClassificationContext(current), /Armaments with LimitBreak/);
  current.position.observation.observation.cards.push({ card: 'Corruption', count: 1, upgrades: 0 });
  assert.match(preparationClassificationContext(current), /Corruption with LimitBreak/);
});

test('post-boss classification targets the upcoming Act instead of early Act development', () => {
  const current = observation([]);
  current.position.observation.observation.run = { act: 1, pending_act_entry: 2 };
  const text = preparationClassificationContext(current);
  assert.match(text, /Act-entry preparation: this Act's Boss is completed.*Act 2/);
  assert.doesNotMatch(text, /Act 1 development/);
  current.position.observation.observation.run.pending_act_entry = null;
  assert.match(preparationClassificationContext(current), /Act 1 development/);
});

test('permanent duplication supplies access and repeated-payment comparisons before and during target selection', () => {
  const current = observation([]);
  current.position.actions[0].candidate.action.public_semantics = {
    structured_semantics: { effects: [{ effect: 'duplicate_card', count: 1 }] },
  };
  assert.match(preparationClassificationContext(current), /Permanent duplication:.*Each normal Power play requires/);
  assert.match(preparationStageGuide('selection', undefined, { reason: 'Duplicate' }), /Permanent duplication:/);
  assert.doesNotMatch(preparationStageGuide('selection', undefined, { reason: 'Transform' }), /Permanent duplication:/);
});

function selectionObservation(min = 1, max = min, reason = 'Duplicate') {
  const current = observation([]);
  current.position.observation.observation.context = { kind: 'run_choice', reason };
  const card = (ordinal, card, misc_value = 0) => ({ ordinal, card, upgrades: 0,
    misc_value, base_damage_override: null, base_block_override: null, cost_modifier: 0 });
  current.position.actions = [{ semantic_action: null, candidate: { domain: 'strategic', action: { kind: 'begin_run_card_selection' } },
    run_selection_family: { reason, min_choices: min, max_choices: max,
      selectable_cards: [card(1, 'Strike'), card(3, 'Strike'), card(9, 'DemonForm')],
      card_tokens: ['Strike+0#0', 'Strike+0#1', 'DemonForm+0#0'] } }];
  return current;
}

test('attached targets classify exact Rust members and group only identical public copies', () => {
  const current = selectionObservation();
  assert.equal(preparationProceduralAction(current), undefined, 'one family still contains multiple actual choices');
  assert.deepEqual(preparationCandidates(current, conciseCardCommand), ['act select:Strike#0', 'act select:Strike#1', 'act select:DemonForm#0']);
  assert.deepEqual(preparationCandidateGroups(current, conciseCardCommand)[0].members, ['act select:Strike#0', 'act select:Strike#1']);
  assert.match(preparationClassificationContext(current, conciseCardCommand), /Card selection: Duplicate.*Permanent duplication:/s);
  const text = 'act select:Strike#0 | low | direct | @effect,@resource-tradeoff\nact select:DemonForm#0 | low | direct | @effect,@resource-tradeoff';
  const saved = table(current, text);
  assert.equal(saved.rows.length, 3);
  assert.throws(() => preparationCommandBasis(['act', ['select:DemonForm#0']], 'selection', '', current, undefined, conciseCardCommand), /Call classify/);
  const adopted = preparationCommandBasis(['act', ['select:DemonForm#0']], 'selection', 'act select:Strike#0 | @access', current, saved, conciseCardCommand);
  assert.equal(adopted.command, 'act select:DemonForm#0');
  assert.equal(adopted.selectedRow.category, 'low');
  assertPreparationCommand(['classify', []], 'selection', current);
  current.position.actions[0].run_selection_family.selectable_cards[1].cost_modifier = -1;
  assert.equal(preparationCandidateGroups(current, conciseCardCommand).length, 3);
  delete current.position.actions[0].run_selection_family.card_tokens;
  assert.throws(() => preparationCandidates(current), /exact Rust tokens/);
});

test('multi-card selection rates members linearly and binds every chosen row with count and duplicate checks', () => {
  const current = selectionObservation(2, 2, 'Purge');
  const saved = table(current, 'act select:Strike#0 | high | direct | @effect,@resource-tradeoff\nact select:DemonForm#0 | low | direct | @effect,@resource-tradeoff');
  assert.equal(preparationCandidateGroups(current, conciseCardCommand).length, 2);
  const adopt = (tokens, basis, rows = saved) =>
    preparationCommandBasis(['act', [`select:${tokens}`]], 'selection', basis, current, rows, conciseCardCommand);
  const bound = adopt('Strike#1,Strike#0');
  assert.throws(() => adopt('Strike#1,Strike#0', undefined, { ...saved,
    rows: saved.rows.filter(row => row.command !== 'act select:DemonForm#0') }), /Call classify with the complete current table/);
  assert.equal(bound.command, 'act select:Strike#1,Strike#0');
  assert.deepEqual(bound.selectedRows.map(row => row.command), ['act select:Strike#1', 'act select:Strike#0']);
  assert.match(bound.renderedBasis, /Strike#1.*Strike#0.*complete current classification/);
  assert.deepEqual(bound.unselectedRows.map(row => row.command), ['act select:DemonForm#0']);
  assert.equal(bound.basisKind, 'selection-set');
  const mixed = adopt('Strike#0,DemonForm#0');
  assert.deepEqual(mixed.unselectedRows.map(row => row.command), ['act select:Strike#1']);
  for (const tokens of ['Strike#0', 'Strike#0,Strike#0', 'Strike#0,Strike#1,DemonForm#0']) {
    assert.throws(() => adopt(tokens), /2\.\.2 distinct/);
  }
  assert.throws(() => adopt('Strike#0,AscendersBane#0'), /classified current/);
  const reviewed = { ...saved, rows: saved.rows.map(row => ({ ...row, category: 'review' })) };
  assert.throws(() => adopt('Strike#0,Strike#1', undefined, reviewed), /missing fact/);
  const optional = selectionObservation(0, 2, 'Purge');
  assert.ok(preparationCandidates(optional).includes('act select:none'));
});

test('explicit single-card selection actions use the same classification and Duplicate checks as a family', () => {
  const current = selectionObservation();
  current.position.actions = [{ semantic_action: 'select:DemonForm+0#0', candidate: {
    domain: 'strategic', action: { kind: 'submit_run_selection', selected_card_ordinals: [9] },
  } }];
  assert.deepEqual(preparationCandidates(current, conciseCardCommand), ['act select:DemonForm#0']);
  assert.match(preparationClassificationContext(current, conciseCardCommand), /Permanent duplication:.*Each normal Power play requires/);
  const saved = table(current, 'act select:DemonForm#0 | low | direct | @effect,@resource-tradeoff');
  const adopted = preparationCommandBasis(['act', ['select:DemonForm#0']], 'selection', '', current, saved, conciseCardCommand);
  assert.equal(adopted.command, 'act select:DemonForm#0');
});

test('copy conditions bind actual counts, payment and delayed versus persistent effects', () => {
  const current = selectionObservation(), state = current.position.observation.observation;
  state.base_energy_per_turn = 3;
  const family = current.position.actions[0].run_selection_family;
  const form = family.selectable_cards[2];
  const disarm = { ...form, card: 'Disarm', ordinal: 10 };
  state.cards = [{ ...form, count: 1 }, { ...disarm, count: 2 }];
  const reading = (card, cost, reviewed_effects) => ({ current: { persistent_cost: cost,
    definition: { card: card.card, upgrades: card.upgrades, card_type: card.card === 'DemonForm' ? 'Power' : 'Skill', reviewed_effects } } });
  current.position.actions = [
    { semantic_action: 'select:DemonForm+0#0', candidate: { action: { kind: 'submit_run_selection', selected_card_ordinals: [9] } },
      run_selection_target: { card: form, reading: reading(form, 3, { player_strength_gain: { amount: 2, delivery: 'each_player_turn_start_post_draw' } }) } },
    ...[0, 1].map(n => ({ semantic_action: `select:Disarm+0#${n}`, candidate: { action: { kind: 'submit_run_selection', selected_card_ordinals: [10 + n] } },
      run_selection_target: { card: { ...disarm, ordinal: 10 + n }, reading: reading(disarm, 1, { enemy_strength_reduction: { amount: 2, restoration: 'no_scheduled_restoration' } }) } })),
  ];
  assert.equal(preparationCandidateGroups(current, conciseCardCommand).length, 2);
  const text = preparationClassificationContext(current, conciseCardCommand);
  assert.match(text, /DemonForm: owned copies 1 -> 2/);
  assert.match(text, /Disarm: owned copies 2 -> 3/);
  assert.match(text, /Base Energy 3 leaves 0.*two such copies normally costs 6/);
  assert.match(text, /After n later triggers.*2 \* n Strength/);
  assert.match(text, /Across h hits.*2 \* h.*Exhaust affects its reuse; the applied reduction continues/);
  const classified = parsePreparationClassification('act select:DemonForm#0 | low | sequence | @setup-tradeoff\nact select:Disarm#0 | high | direct | @extra-use', current, conciseCardCommand);
  assert.equal(classified[0].bindingId, 'permanent-copy-increment-v1');
  assert.match(classified[0].reason, /owned copies 1 -> 2.*0 immediate Strength/);
  assert.equal(classified[2].command, 'act select:Disarm#1');
  current.position.actions[0].run_selection_target.card.cost_modifier = -2;
  current.position.actions[0].run_selection_target.reading.current.persistent_cost = 1;
  state.base_energy_per_turn = 4;
  assert.match(preparationClassificationContext(current, conciseCardCommand), /Base Energy 4 leaves 3.*two such copies normally costs 2/);
  state.context.reason = 'Purge';
  assert.equal(specificBindings(current, conciseCardCommand).length, 0);
});

test('empty potion slots remove discard work without removing replacement choices when full', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  const discard = { semantic_action: 'discard-potion:SmokeBomb#0', candidate: { action: { kind: 'discard_run_potion' } } };
  current.position.actions.push(discard);
  state.potions = [{ potion: { potion: 'SmokeBomb' } }, { potion: null }];
  const filtered = preparationReading(current);
  assert.equal(filtered.position.actions.length, 2);
  assert.equal(current.position.actions.length, 3);
  assert.throws(() => parsePreparationClassification(basis + '\ndiscard-potion:SmokeBomb#0 | low | direct | @effect,@resource-tradeoff',
    filtered, conciseCardCommand), /supplied pending commands/);
  state.potions[1].potion = { potion: 'PowerPotion' };
  assert.strictEqual(preparationReading(current), current);
  delete state.potions;
  assert.strictEqual(preparationReading(current), current);
});

test('full potion slots keep card reward options small while replacement stays on the reward page', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  state.context = { kind: 'card_reward' };
  state.potions = [{ potion: { potion: 'SmokeBomb' } }, { potion: { potion: 'PowerPotion' } }];
  const row = (semantic_action, action) => ({ semantic_action, candidate: { action } });
  current.position.actions = [row('discard-potion:SmokeBomb#0', { kind: 'discard_run_potion' }),
    row('take:Disarm#0', { kind: 'take_card' }), row('singing-bowl#0', { kind: 'singing_bowl' }),
    row('skip-card#0', { kind: 'skip_card_reward' })];
  const filtered = preparationReading(current);
  assert.deepEqual(filtered.position.actions.map(row => row.semantic_action), ['take:Disarm#0', 'singing-bowl#0', 'skip-card#0']);
  assert.deepEqual(filtered.preparationOmissions.commands, ['discard-potion:SmokeBomb#0']);
  assert.equal(current.position.actions.length, 4);
  state.context = { kind: 'reward', pending_rewards: [{ kind: 'potion', potion: 'BlockPotion' }] };
  current.position.actions = [current.position.actions[0], row('claim:BlockPotion#0', {
    kind: 'claim_reward', reward: { kind: 'potion', potion: 'BlockPotion' } }), row('proceed', { kind: 'proceed' })];
  assert.strictEqual(preparationReading(current), current);
  assert.equal(preparationProceduralAction(current), undefined);
});

test('preparation filtering preserves combat observations with nullable potion slots', () => {
  const combat = { position: { kind: 'decision', observation: {
    domain: 'combat', observation: { potions: [null, { id: 'LiquidBronze' }, null, null] },
  } } };
  assert.strictEqual(preparationReading(combat), combat);
});

test('Corruption supplies deployment schedules together and Exhume keeps recovery incremental', () => {
  const current = observation([{ card: 'LimitBreak', count: 1, upgrades: 0 }, { card: 'Corruption', count: 1, upgrades: 1 }, { card: 'Exhume', count: 1, upgrades: 1 }]);
  const text = preparationClassificationContext(current);
  assert.doesNotMatch(text, /Armaments with LimitBreak: upgrading LimitBreak before use allows/);
  assert.match(text, /Before Corruption deployment[\s\S]*LimitBreak\+ normally enters Discard[\s\S]*After Corruption deployment, both versions cost 0 and Exhaust/);
  assert.match(text, /Returning the unupgraded LimitBreak already supplies another use/);
  current.position.observation.observation.cards.splice(1, 1);
  assert.match(preparationClassificationContext(current), /Armaments with LimitBreak/);
  assert.doesNotMatch(preparationClassificationContext(current), /After Corruption deployment|With Corruption deployed/);
});

function boundObservation(extraCards = []) {
  const current = observation([{ card: 'LimitBreak', count: 1, upgrades: 0 }, ...extraCards]);
  current.decision_evidence = { candidates: [{ semantic_action: 'buy:Armaments+0#0', cards: [{ current: {
    card: 'Armaments', name: 'Armaments', cost: 1, block: 5, combat_upgrade_scope: 'selected_card_in_hand',
  } }] }] };
  return current;
}

test('bound output retains deployment and base recovery independently of its benefit rating', () => {
  const current = boundObservation([{ card: 'Corruption', count: 1, upgrades: 1 }, { card: 'Exhume', count: 1, upgrades: 1 }]);
  const text = 'buy:Armaments#0 | high | sequence | @limit-redraw,@block-access\nleave | medium | direct | @effect,@resource-tradeoff';
  const [row] = parsePreparationClassification(text, current, conciseCardCommand);
  assert.deepEqual(row.conditionIds, ['@limit-redraw', '@block-access']);
  assert.match(row.reason, /Keep Corruption awaiting deployment/);
  assert.match(row.reason, /After Corruption deployment.*upgrade effect before.*both LimitBreak versions.*Exhaust/);
  assert.match(row.reason, /awaiting deployment throughout the repeated uses/);
  assert.match(row.reason, /Returning base LimitBreak already supplies another use/);
  assert.match(row.reason, /recovery benefit is shared by both versions/);
  current.position.observation.observation.relics = [{ relic: 'StrangeSpoon' }];
  assert.match(parsePreparationClassification(text, current, conciseCardCommand)[0].reason, /50% chance to enter Discard/);
  const plain = preparationClassificationContext(boundObservation(), conciseCardCommand);
  assert.match(plain, /Condition references for buy:Armaments#0/);
  assert.match(plain, /Condition references for buy:Armaments#0:[\s\S]*@limit-redraw:/);
  assert.match(plain, /Condition references for leave:\n  @effect:/);
  assert.doesNotMatch(plain, /Corruption deployment|Exhume play/);
});

test('the active stage supplies its own procedure rather than replaying the initial workflow', () => {
  const first = preparationStageGuide('pending');
  assert.match(first, /Current task: rate the candidates and choose one preparation/);
  assert.match(first, /command classify/);
  assert.doesNotMatch(first, /Submit command as the chosen/);
  const second = preparationStageGuide('pending', { rows: [] });
  assert.match(second, /Classification complete\. Current stage: adoption/);
  assert.match(second, /command choose and choice: the selected row's first column, copied verbatim/);
  assert.match(second, /Compare the strongest recorded recommendations/);
  assert.doesNotMatch(second, /basis: ALTERNATIVE_COMMAND/);
  assert.doesNotMatch(second, /NEXT_PURCHASE_COMMAND/);
  assert.match(preparationStageGuide('pending', { rows: [] }, { kind: 'shop' }),
    /optional comparison: OTHER_COMMAND \| @access,@resources \| then NEXT_PURCHASE_COMMAND/);
  assert.doesNotMatch(second, /multi-target|complete act select:|e\.g\. leave/);
  assert.doesNotMatch(second, /Call classify|One line per supplied classification/);
  const selection = preparationStageGuide('selection');
  assert.match(selection, /Attached target choice.*parent preparation has been adopted/);
  assert.match(selection, /command classify/);
  assert.doesNotMatch(selection, /Submit command as the chosen/);
  const selected = preparationStageGuide('selection', { rows: [] });
  assert.match(selected, /command choose and choice: the complete command built from the current Selection adoption instruction/);
  assert.doesNotMatch(selected, /choice: the selected row's first column/);
});

test('condition rows reject invented mechanics, foreign references and inconsistent review dimensions', () => {
  const current = boundObservation([{ card: 'Corruption', count: 1, upgrades: 1 }]);
  const line = reason => `buy:Armaments#0 | medium | sequence | ${reason}\nleave | high | direct | @effect,@resource-tradeoff`;
  for (const reason of ['Deploy Corruption and repeatedly redraw LimitBreak+.', '@unlimited-exhume',
    '@limit-redraw (after deployment)', '@limit-redraw,@limit-redraw', '@review-access']) {
    assert.throws(() => parsePreparationClassification(line(reason), current, conciseCardCommand));
  }
  const reviewed = line('@review-access').replace('| sequence |', '| review |');
  assert.equal(parsePreparationClassification(reviewed, current, conciseCardCommand)[0].realization, 'review');
});

test('condition binding uses typed effects and applies to additions with an owned base LimitBreak', () => {
  const current = boundObservation(), effect = current.decision_evidence.candidates[0].cards[0].current;
  effect.block = 8; effect.cost = 0; effect.name = 'Armaments+'; effect.combat_upgrade_scope = 'whole_hand';
  assert.match(specificBindings(current)[0].fixedMechanics[0], /Armaments\+.*cost 0 and 8 Block.*current Hand/);
  current.position.observation.observation.cards[0].upgrades = 1;
  assert.deepEqual(specificBindings(current), []);
  current.position.observation.observation.cards[0].upgrades = 0;
  current.position.actions[0].candidate.action.kind = 'remove_card';
  assert.deepEqual(specificBindings(current), []);
  current.position.actions[0].candidate.action.kind = 'buy_card'; delete effect.block;
  assert.deepEqual(specificBindings(current), []);
});

test('temporary Strength comparisons bind native amounts, owned damage modifiers and the same-Hand payoff', () => {
  const current = observation([]);
  current.position.actions = [
    { semantic_action: 'take:Flex+0#0', candidate: { action: { kind: 'take_card', card: 'Flex', upgrades: 0 } } },
    { semantic_action: 'skip-card#0', candidate: { action: { kind: 'skip_card_reward' } } },
  ];
  const state = current.position.observation.observation;
  state.relics = [{ relic: 'Boot' }, { relic: 'MutagenicStrength' }];
  const definition = { card: 'Flex', upgrades: 0, name: 'Flex', cost: 0,
    strength_gain_delivery: 'on_play_with_separate_lose_strength', strength_gain: 2,
    temporary_strength_gain: 2, strength_loss_at_end_of_turn: 2 };
  current.decision_evidence = { candidates: [{ semantic_action: 'take:Flex+0#0', cards: [{ current: definition }] }] };
  const binding = specificBindings(current)[0];
  assert.equal(binding.id, 'temporary-strength-payoff-v1');
  const localized = preparationConditionBindings(current, undefined, 'zh-CN')[0];
  assert.deepEqual(localized.choices.map(row => row.id), binding.choices.map(row => row.id));
  assert.match(localized.fixedMechanics.join(' '), /请求 2 力量.*失去 2 力量.*同一手牌/);
  assert.match(binding.fixedMechanics.join(' '), /2 Strength.*2 Lose Strength.*shared-Hand sequence/);
  assert.ok(binding.choices.some(choice => choice.id === '@damage-threshold'));
  assert.ok(binding.choices.some(choice => choice.id === '@opening-stack'));
  const basis = 'take:Flex+0#0 | low | sequence | @same-hand-payoff,@damage-threshold,@draw-slot-tradeoff\nskip-card#0 | high | direct | @effect,@resource-tradeoff';
  const rows = parsePreparationClassification(basis, current);
  assert.equal(rows[0].category, 'low');
  assert.equal(rows[0].realization, 'sequence');
  assert.match(rows[0].reason, /final damage amounts/);
  assert.equal(rows[1].category, 'high');
  assert.throws(() => parsePreparationClassification(basis.replace('@draw-slot-tradeoff', '@draw-slot-tradeoff: needs the same Hand'), current),
    /Fourth column: comma-separated reference IDs/);
  state.relics = [];
  const without = specificBindings(current)[0];
  assert.ok(without.choices.every(choice => !['@damage-threshold', '@opening-stack'].includes(choice.id)));
  assert.throws(() => parsePreparationClassification(basis, current), /supplied condition references/);
  definition.temporary_strength_gain = 4;
  assert.deepEqual(specificBindings(current), []);
  definition.strength_gain = 4; definition.strength_loss_at_end_of_turn = 4;
  assert.match(specificBindings(current)[0].fixedMechanics[0], /4 Strength.*4 Lose Strength/);
  definition.card = 'Inflame';
  assert.deepEqual(specificBindings(current), []);
});

test('potion-denying Energy relics bind the retained reserve separately from later refill loss', () => {
  const current = observation([]), state = current.position.observation.observation;
  state.context = { kind: 'boss_relic' };
  state.potions = [{ slot: 0, potion: { potion: 'FearPotion' } }, { slot: 1, potion: null }];
  current.position.actions = [{ semantic_action: 'boss-relic:Sozu#0',
    candidate: { action: { kind: 'take_boss_relic', relic: 'Sozu' } } }];
  const definition = { relic: 'Sozu', energy_per_turn_delta: 1,
    potion_acquisition_rule: { prevents_new_potions: true, owned_potions_remain_usable: true } };
  current.decision_evidence = { kind: 'boss_relic_options', candidates: [
    { semantic_action: 'boss-relic:Sozu#0', definition } ] };
  const binding = specificBindings(current)[0];
  assert.match(binding.fixedMechanics.join('\n'), /P0 FearPotion.*remain usable/);
  assert.match(binding.fixedMechanics.join('\n'), /reward, shop and event potion acquisitions are blocked/);
  assert.ok(binding.choices.some(row => row.id === '@future-potion-tradeoff'));
  const row = parsePreparationClassification('boss-relic:Sozu#0 | high | direct | @recurring-energy', current)[0];
  assert.match(row.reason, /lost later refill opportunities/);
  state.potions[0].potion = null;
  assert.match(specificBindings(current)[0].fixedMechanics.join('\n'), /Current carried potions: none/);
  definition.potion_acquisition_rule.prevents_new_potions = false;
  assert.ok(!specificBindings(current).some(row => row.id === 'relic-energy-potion-acquisition-v1'));
});

test('payment conditions bind acquisition and a grouped upgrade crossing base Energy', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  state.base_energy_per_turn = 3;
  const before = { card: 'BloodForBlood', name: 'Blood for Blood', cost: 4, cost_change_on_hp_loss: -1 };
  const after = { ...before, cost: 3 };
  current.position.actions[0] = { semantic_action: 'buy:BloodForBlood+0#0', candidate: { action: { kind: 'buy_card', card: 'BloodForBlood' } } };
  current.decision_evidence = { candidates: [{ semantic_action: 'buy:BloodForBlood+0#0', cards: [{ current: before, upgraded: after }] }] };
  let binding = specificBindings(current, conciseCardCommand)[0];
  assert.match(binding.fixedMechanics.join(' '), /needs 1 additional Energy.*needs 1 such events/);
  const classified = parsePreparationClassification('buy:BloodForBlood#0 | medium | sequence | @hp-loss-payment\nleave | medium | direct | @effect,@resource-tradeoff', current, conciseCardCommand);
  assert.match(classified[0].reason, /before payment.*Fully blocked or prevented damage supplies zero/);
  assert.equal(classified[0].bindingId, 'base-energy-payment-v1');
  current.position.actions[0] = { semantic_action: 'smith:BloodForBlood+0#0', candidate: { action: { kind: 'smith', card: 'BloodForBlood' } } };
  current.decision_evidence.candidates = [{ semantic_actions: ['smith:BloodForBlood+0#0'], before, after }];
  binding = specificBindings(current, conciseCardCommand)[0];
  assert.match(binding.fixedMechanics.join(' '), /crosses the base-Energy payment threshold.*leaves 0 base Energy/);
  assert.match(binding.fixedMechanics.join(' '), /needs 0 such events/);
  assert.ok(binding.choices.some(row => row.id === '@upgrade-payment'));
  state.base_energy_per_turn = 4;
  assert.deepEqual(specificBindings(current, conciseCardCommand), []);
  delete state.base_energy_per_turn;
  assert.deepEqual(specificBindings(current, conciseCardCommand), []);
});

test('generic high-cost facts coexist with Armaments relations and reject mismatched effect sources', () => {
  const current = boundObservation([{ card: 'Corruption', count: 1, upgrades: 1 }]);
  current.position.observation.observation.base_energy_per_turn = 3;
  current.position.actions.push({ semantic_action: 'buy:Bludgeon+0#0', candidate: { action: { kind: 'buy_card', card: 'Bludgeon' } } });
  const row = { semantic_action: 'buy:Bludgeon+0#0', cards: [{ current: { card: 'Bludgeon', name: 'Bludgeon', cost: 3 } }] };
  current.decision_evidence.candidates.push(row);
  row.cards[0].current.cost = 4;
  const bindings = specificBindings(current, conciseCardCommand);
  assert.equal(bindings.length, 2);
  assert.match(preparationClassificationContext(current, conciseCardCommand), /After Corruption deployment.*Armaments/);
  row.cards[0].current.card = 'BloodForBlood';
  assert.equal(specificBindings(current, conciseCardCommand).length, 1);
});

function copies(kind = 'smith') {
  const current = observation([{ card: 'Disarm', upgrades: 0, count: 2, misc_value: 0, base_damage_override: null, base_block_override: null, cost_modifier: 0 }]);
  current.position.actions = [0, 1].map(n => ({ semantic_action: `${kind === 'remove_card' ? 'remove' : kind}:Disarm+0#${n}`,
    candidate: { domain: 'strategic', action: { kind, card: 'Disarm', upgrades: 0, card_ordinal: n + 1, ...(kind === 'remove_card' ? { price: 100 } : {}) } } }));
  current.position.actions.push({ semantic_action: 'rest', candidate: { domain: 'strategic', action: { kind: 'rest' } } });
  return current;
}

test('one public-copy classification covers every command and adoption can select any member', () => {
  const current = copies(), text = 'smith:Disarm#0 | high | direct | @effect,@resource-tradeoff\nrest | low | direct | @effect,@resource-tradeoff';
  const rows = parsePreparationClassification(text, current, conciseCardCommand);
  assert.equal(rows.length, 3); assert.deepEqual(rows[1], { ...rows[0], command: 'smith:Disarm#1' });
  assert.match(preparationClassificationContext(current, conciseCardCommand), /Shared one-copy evaluation: 2 copies/);
  assert.deepEqual(parsePreparationClassification('```text\n' + text + '\n```', current, conciseCardCommand), rows);
  assert.throws(() => parsePreparationClassification(text + '\nsmith:Disarm#1 | low | direct | @effect,@resource-tradeoff', current, conciseCardCommand), /Duplicate classification.*2 matching legal command\(s\) share one output row/);
  const classified = table(current, text);
  assert.match(preparationCommandBasis(['smith:Disarm#1', []], 'pending', 'rest | @protection', current, classified, conciseCardCommand).renderedBasis, /smith:Disarm#1/);
  assert.throws(() => preparationCommandBasis(['smith:Disarm#1', []], 'pending', 'smith:Disarm#0 | @protection', current, classified, conciseCardCommand), /another current legal/);
  assert.equal(preparationCandidateGroups(copies('remove_card')).length, 2);
  assert.deepEqual(preparationCandidateGroups(copies('toke'), conciseCardCommand)[0].members, ['toke:Disarm#0','toke:Disarm#1']);
});

test('different instance groups, prices, upgrades and incomplete public bindings stay separate', () => {
  for (const change of [
    current => current.position.observation.observation.cards.push({ ...current.position.observation.observation.cards[0], cost_modifier: -1, count: 1 }),
    current => { delete current.position.observation.observation.cards[0].base_damage_override; },
    current => { current.position.observation.observation.cards[0].count = 1; },
    current => { current.position.actions[1].candidate.action.price = 125; },
    current => { current.position.actions[1].candidate.action.upgrades = 1; },
    current => { current.position.actions[1].candidate.action.card_ordinal = 1; },
    current => { current.position.observation.observation.relics = [{ relic: 'BottledLightning' }]; },
  ]) {
    const current = copies('remove_card'); change(current);
    assert.equal(preparationCandidateGroups(current).length, 3);
    assert.throws(() => parsePreparationClassification('remove:Disarm+0#0 | low | direct | @effect,@resource-tradeoff\nrest | high | direct | @effect,@resource-tradeoff', current), /remaining candidates/);
  }
});

test('classification accepts supplied multi-token semantic commands', () => {
  const current = observation([]); current.position.actions = [{ semantic_action: 'select:Strike+0#0 Defend+0#0', candidate: { action: {} } }];
  assert.equal(parsePreparationClassification('select:Strike+0#0 Defend+0#0 | high | direct | @effect,@resource-tradeoff', current)[0].category, 'high');
});


test('route classification shares the displayed coordinates with execution', () => {
  const current = observation([]);
  current.position.observation.domain = 'strategic';
  current.position.kind = 'decision';
  current.position.observation.observation.context = { kind: 'map' };
  current.position.observation.observation.public_map = { current: [1,4], nodes: [[1,5,'elite',true,[]],[2,5,'rest',false,[]]] };
  current.position.actions = [{semantic_action:'route:1.5',candidate:{action:{kind:'choose_route_node',x:1,y:5}}},
    {semantic_action:'route:2.5',candidate:{action:{kind:'choose_route_node',x:2,y:5}}}];
  const basis = 'go 1,5 | high | direct | @effect,@resource-tradeoff\ngo 2,5 | high | direct | @effect,@resource-tradeoff';
  const classified = {source:current,rows:parsePreparationClassification(basis,current,strategicOptionCommand)};
  assert.match(preparationClassificationContext(current,strategicOptionCommand), /go 1,5/);
  const adoption = preparationCommandBasis(['go',['1,5']],'pending','go 2,5 | @opportunity',current,classified,strategicOptionCommand);
  assert.equal(adoption.selectedRow.command,'go 1,5');
  assert.equal(adoption.alternativeRow.command,'go 2,5');
  const inconsistent = { ...classified, rows: classified.rows.map(row => row.command === 'go 1,5' ? { ...row, category: 'low' } : row) };
  assert.throws(() => preparationCommandBasis(['go', ['1,5']], 'pending', undefined, current, inconsistent, strategicOptionCommand), /higher recorded recommendations/);
  assert.throws(() => preparationCommandBasis(['go',['1,5','elite']],'pending','go 2,5 | @opportunity',current,classified,strategicOptionCommand), /classified current candidate/);
});

test('route conditions keep a later shop separate from before-first-Elite preparation', () => {
  const current = observation([]);
  current.position.observation.domain = 'strategic';
  const state = current.position.observation.observation;
  state.context = { kind: 'map' };
  state.public_map = { current: [2, 0], nodes: [[1, 1, 'monster', false, []], [2, 1, 'question_mark', false, []]] };
  current.position.kind = 'decision';
  current.position.actions = ['route:1.1', 'route:2.1'].map(semantic_action => ({ semantic_action, candidate: { action: { kind: 'choose_route_node' } } }));
  const count = (min, max = min) => ({ count_per_observed_path: { min, max } });
  current.decision_evidence = { kind: 'route_paths', candidates: current.position.actions.map((row, index) => ({
    semantic_action: row.semantic_action,
    route_semantics: { shops: count(0, index), campfires: count(2, 3), first_elite: {
      observed_paths_with_first_elite: 10, shops_before: count(0), campfires_before: count(0, index),
    } },
    path: { coverage: 'complete_within_horizon', first_elite: {
      min_hallway_fights_before: 1, max_hallway_fights_before: 3,
      min_unknowns_before: 1, max_unknowns_before: 3,
    } },
  })) };
  const context = preparationClassificationContext(current, strategicOptionCommand);
  assert.match(context, /Condition references for go 2,1/);
  assert.match(context, /Before the first Elite.*Shops 0\.\.0; campfires 0\.\.1/);
  assert.match(context, /Whole published horizon: Shops 0\.\.1/);
  const rows = parsePreparationClassification('go 1,1 | medium | direct | @early-development\ngo 2,1 | high | direct | @first-preparation,@later-development', current, strategicOptionCommand);
  assert.equal(rows[1].bindingId, 'route-preparation-window-v1');
  assert.match(rows[1].reason, /shop occurring afterward.*Before the first Elite.*Shops 0\.\.0/);
  assert.throws(() => parsePreparationClassification('go 1,1 | medium | direct | @early-development\ngo 2,1 | high | direct | @effect,@resource-tradeoff', current, strategicOptionCommand), /Use supplied condition references/);
});


test('procedural reward handling reveals cards and retains real replacement and key choices', () => {
  const current=observation([]);current.position.observation.observation.context={kind:'reward'};
  const row=(command,action)=>({semantic_action:command,candidate:{action}});
  const discard=row('discard-potion:DexterityPotion#0',{kind:'discard_run_potion'});
  const proceed=row('proceed',{kind:'proceed'});
  const open=row('open-card#0',{kind:'open_card_reward'});
  const gold=row('claim:Gold#0',{kind:'claim_reward',reward:{kind:'gold',amount:20}});
  current.position.actions=[discard,proceed,open,gold];
  assert.equal(preparationProceduralAction(current).row,gold);
  gold.candidate.action.reward.kind='stolen_gold';
  assert.equal(preparationProceduralAction(current).row,gold);
  current.position.actions=[discard,proceed,open];
  assert.equal(preparationProceduralAction(current).row,open);
  current.position.actions=[discard,proceed];
  current.position.observation.observation.context.pending_rewards=[{kind:'potion',potion:'BlockPotion'}];
  assert.equal(preparationProceduralAction(current),undefined);
  delete current.position.observation.observation.context.pending_rewards;
  assert.equal(preparationProceduralAction(current).row,proceed);
  current.position.actions=[row('claim:Relic#0',{kind:'claim_reward',reward:{kind:'relic'}}),row('claim:SapphireKey#0',{kind:'claim_reward',reward:{kind:'sapphire_key'}}),proceed];
  assert.equal(preparationProceduralAction(current),undefined);
  current.position.observation.observation.context={kind:'event'};
  const exit=row('event:WorldOfGoop:0',{kind:'choose_event_option',public_semantics:{structured_semantics:{effects:[],transition:{kind:'complete'}}}});
  current.position.actions=[discard,exit];
  assert.equal(preparationProceduralAction(current).row,exit);
  exit.candidate.action.public_semantics.structured_semantics.transition.kind='advance_screen';
  assert.equal(preparationProceduralAction(current).row,exit);
  exit.candidate.action.public_semantics.structured_semantics.effects=[{kind:'lose_hp',amount:11}];
  assert.equal(preparationProceduralAction(current),undefined);
});

test('free potion claims fit together and bind the next claim after reward indices shift', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  state.run = { potion_capacity: 3 };
  state.potions = [
    { slot: 0, potion: { potion: 'DexterityPotion' } }, { slot: 1, potion: null }, { slot: 2, potion: null },
  ];
  const rewards = ['BlockPotion', 'SmokeBomb'].map(potion => ({ kind: 'potion', potion }));
  state.context = { kind: 'reward', pending_rewards: [...rewards, { kind: 'relic', relic: 'Boot' }, { kind: 'sapphire_key' }] };
  const claim = (reward, reward_item_index) => ({ semantic_action: `claim:${reward.potion ?? reward.relic ?? 'SapphireKey'}#0`,
    candidate: { action: { kind: 'claim_reward', reward_item_index, reward } } });
  const proceed = { semantic_action: 'proceed', candidate: { action: { kind: 'proceed' } } };
  current.position.actions = [...state.context.pending_rewards.map(claim), proceed];
  const first = preparationProceduralAction(current);
  assert.equal(first.owner, 'reward-potions-with-space');
  assert.equal(first.row, current.position.actions[0]);
  state.potions[1].potion = { potion: 'BlockPotion' };
  state.context.pending_rewards.shift();
  current.position.actions = [...state.context.pending_rewards.map(claim), proceed];
  assert.equal(preparationProceduralAction(current).row.candidate.action.reward_item_index, 0);
  assert.equal(preparationProceduralAction(current).row.candidate.action.reward.potion, 'SmokeBomb');
  state.potions[2].potion = { potion: 'SmokeBomb' };
  state.context.pending_rewards.shift();
  current.position.actions = [...state.context.pending_rewards.map(claim), proceed];
  assert.equal(preparationProceduralAction(current), undefined, 'relic and Sapphire remain a real choice');
});

test('reward inventory is settled before automatic card classification, retaining unwanted reward choices', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  state.run = { potion_capacity: 2 };
  state.potions = [{ slot: 0, potion: { potion: 'SmokeBomb' } }, { slot: 1, potion: null }];
  const card = { kind: 'card_reward', cards: [{ card: 'BodySlam', upgrades: 0 }] };
  state.context = { kind: 'reward', pending_rewards: [card, { kind: 'potion', potion: 'SwiftPotion' }, { kind: 'relic', relic: 'RedMask' }] };
  const actions = () => state.context.pending_rewards.map((reward, reward_item_index) => ({
    semantic_action: reward.kind === 'card_reward' ? `open-card#${reward_item_index}` : `claim:${reward.potion ?? reward.relic}#0`,
    candidate: { action: reward.kind === 'card_reward' ? { kind: 'open_card_reward', reward_item_index }
      : { kind: 'claim_reward', reward_item_index, reward } },
  })).concat({ semantic_action: 'proceed', candidate: { action: { kind: 'proceed' } } });
  current.position.actions = actions();
  assert.equal(preparationProceduralAction(current).row.semantic_action, 'claim:SwiftPotion#0');
  state.potions[1].potion = { potion: 'SwiftPotion' };
  state.context.pending_rewards.splice(1, 1);
  current.position.actions = actions();
  assert.equal(preparationProceduralAction(current), undefined, 'the relic is classified before automatic card revelation');
  assert.deepEqual(preparationCandidates(current), ['open-card#0', 'claim:RedMask#0', 'proceed']);
  const [reveal] = specificBindings(current);
  assert.equal(reveal.id, 'reward-card-reveal-v1');
  const rows = parsePreparationClassification('open-card#0 | medium | direct | @card-choice-access\n'
    + 'claim:RedMask#0 | high | direct | @effect,@resource-tradeoff\nproceed | low | direct | @effect,@resource-tradeoff', current);
  assert.match(rows[0].reason, /take\/skip choices receive their own classification/);
  assert.throws(() => parsePreparationClassification('open-card#0 | review | direct | @card-choice-access\n'
    + 'claim:RedMask#0 | high | direct | @effect,@resource-tradeoff\nproceed | low | direct | @effect,@resource-tradeoff', current), /pair @review-access/);
  assert.match(preparationStageGuide('pending', undefined, state.context), /resolve relic\/key choices first, then open card rewards/);
  state.relics = [{ relic: 'RedMask' }];
  state.context.pending_rewards.splice(1, 1);
  current.position.actions = actions();
  assert.equal(preparationProceduralAction(current).row.semantic_action, 'open-card#0');
  const opened = structuredClone(current);
  opened.head_revision = opened.state_revision = 1;
  opened.position.observation.observation.context = { kind: 'card_reward', reward_item_index: 0 };
  const skipped = structuredClone(current);
  skipped.head_revision = skipped.state_revision = 2;
  const progress = preparationRewardProgress(undefined, opened, skipped, { kind: 'skip_card_reward', reward_item_index: 0 });
  assert.equal(preparationProceduralAction(preparationReading(skipped, progress)).row.semantic_action, 'proceed');
  state.context.pending_rewards.push({ kind: 'potion', potion: 'BlockPotion' });
  current.position.actions = actions();
  assert.equal(preparationProceduralAction(current), undefined, 'a full potion inventory requires a real choice');
  assert.ok(preparationCandidates(current).includes('open-card#0'), 'cards remain available when an unwanted reward stays unclaimed');
});

test('slot scarcity, denied claims and incomplete potion inventories retain classification', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  state.run = { potion_capacity: 2 };
  state.potions = [{ slot: 0, potion: { potion: 'DexterityPotion' } }, { slot: 1, potion: null }];
  const rewards = ['BlockPotion', 'SmokeBomb'].map(potion => ({ kind: 'potion', potion }));
  state.context = { kind: 'reward', pending_rewards: rewards };
  const claim = (reward, reward_item_index) => ({ semantic_action: `claim:${reward.potion}#0`,
    candidate: { action: { kind: 'claim_reward', reward_item_index, reward } } });
  const proceed = { semantic_action: 'proceed', candidate: { action: { kind: 'proceed' } } };
  current.position.actions = [...rewards.map(claim), proceed];
  assert.equal(preparationProceduralAction(current), undefined, 'two bottles compete for one slot');
  state.context.pending_rewards = [rewards[0]];
  current.position.actions = [claim(rewards[0], 0), proceed];
  state.potions[1].potion = { potion: 'StrengthPotion' };
  assert.equal(preparationProceduralAction(current), undefined, 'full inventory needs replacement');
  state.potions[1].potion = null;
  state.relics = [{ relic: 'Sozu' }];
  assert.equal(preparationProceduralAction(current).owner, 'reward-potions-blocked-by-sozu');
  state.relics = [];
  current.position.actions = [{ semantic_action: 'discard-potion:DexterityPotion#0',
    candidate: { action: { kind: 'discard_run_potion' } } }, proceed];
  assert.equal(preparationProceduralAction(current), undefined, 'no actual claim is available');
  current.position.actions = [claim(rewards[0], 1), proceed];
  assert.equal(preparationProceduralAction(current), undefined, 'claim must bind the actual pending item');
  current.position.actions = [claim(rewards[0], 0), proceed];
  delete state.potions[1].potion;
  assert.equal(preparationProceduralAction(current), undefined);
  state.potions[1].potion = null;
  state.run.potion_capacity = 3;
  assert.equal(preparationProceduralAction(current), undefined, 'inventory coverage is incomplete');
});

test('Sozu-blocked reward clearing binds shifted items and keeps inventory and other choices intact', () => {
  const current = observation([]), state = current.position.observation.observation;
  state.relics = [{ relic: 'Sozu' }];
  state.run = { potion_capacity: 2 };
  state.potions = [{ slot: 0, potion: { potion: 'FearPotion' } }, { slot: 1, potion: { potion: 'BlockPotion' } }];
  const held = structuredClone(state.potions);
  const card = { kind: 'card_reward', cards: [{ card: 'ShrugItOff', upgrades: 1 }] };
  state.context = { kind: 'reward', pending_rewards: [card, { kind: 'potion', potion: 'EnergyPotion' }] };
  const open = { semantic_action: 'open-card#0', candidate: { action: { kind: 'open_card_reward', reward_item_index: 0 } } };
  const claim = { semantic_action: 'claim:EnergyPotion#0', candidate: { action: { kind: 'claim_reward', reward_item_index: 1,
    reward: state.context.pending_rewards[1] } } };
  const proceed = { semantic_action: 'proceed', candidate: { action: { kind: 'proceed' } } };
  current.position.actions = [open, claim, proceed];
  assert.equal(preparationProceduralAction(current).row, claim);
  assert.deepEqual(state.potions, held);
  claim.candidate.action.reward_item_index = 0;
  assert.equal(preparationProceduralAction(current), undefined, 'a stale item number retains review');
  claim.candidate.action.reward_item_index = 1;
  state.context.pending_rewards.pop(); current.position.actions = [open, proceed];
  assert.equal(preparationProceduralAction(current).row, open);
  assert.deepEqual(state.potions, held);
});

test('route preparation keeps destinations and moves overlay closing outside classification', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  state.context = { kind: 'map' };
  state.potions = [{ slot: 0, potion: { potion: 'BlockPotion' } }];
  const route = (x, y) => ({ semantic_action: `route:${x}.${y}`,
    candidate: { action: { kind: 'choose_route_node', x, y } } });
  const close = { semantic_action: 'cancel', candidate: { action: { kind: 'cancel', site: 'map' } } };
  current.position.actions = [route(1, 2), route(3, 2), close];
  const supplied = preparationReading(current);
  assert.deepEqual(preparationCandidates(supplied), ['route:1.2', 'route:3.2']);
  assert.equal(preparationProceduralAction(supplied), undefined);
  assert.equal(current.position.actions.length, 3);
  assert.equal(supplied.session_id, current.session_id);
  assert.equal(supplied.head_revision, current.head_revision);
  const rows = parsePreparationClassification('route:1.2 | high | direct | @effect,@resource-tradeoff\n'
    + 'route:3.2 | medium | direct | @effect,@resource-tradeoff', supplied);
  assert.equal(rows.length, 2);
  assert.throws(() => assertPreparationCommand(['cancel', []], 'pending', supplied), /supplied candidate/);
  current.position.actions = [route(1, 2), close];
  assert.equal(preparationProceduralAction(preparationReading(current)).row, current.position.actions[0]);
  current.position.actions = [close];
  assert.deepEqual(preparationReading(current).position.actions, [close]);
  state.context = { kind: 'run_choice' };
  current.position.actions = [route(1, 2), close];
  assert.deepEqual(preparationReading(current).position.actions, current.position.actions);
});

test('boss relic references keep owned on-play triggers independent of recurring Power deployment', () => {
  const card = (card, upgrades = 0, count = 1) => ({ card, upgrades, count, misc_value: 0,
    base_damage_override: null, base_block_override: null, cost_modifier: 0 });
  const current = observation([card('Hemokinesis', 1, 2), card('Brutality'), card('Strike', 0, 5)]);
  const state = current.position.observation.observation;
  state.context = { kind: 'boss_relic' };
  state.run = { gold: 126 };
  current.position.actions = ['RunicCube', 'Ectoplasm'].map(relic => ({ semantic_action: `boss-relic:${relic}#0`,
    candidate: { action: { kind: 'take_boss_relic', relic } } }));
  current.decision_evidence = { kind: 'boss_relic_options', candidates: [
    { semantic_action: 'boss-relic:RunicCube#0', definition: { relic: 'RunicCube',
      card_draw_on_hp_loss: { minimum_hp_lost_per_event: 1, cards_drawn_per_event: 2 } } },
    { semantic_action: 'boss-relic:Ectoplasm#0', definition: { relic: 'Ectoplasm',
      energy_per_turn_delta: 1, gold_gain_rule: 'prevent_gold_gain' } },
  ], owned_hp_loss_sources: [
    { card: structuredClone(state.cards[0]), on_play: { base_hp_loss: 2 }, hp_loss_each_turn_after_deployment: 0 },
    { card: structuredClone(state.cards[1]), hp_loss_each_turn_after_deployment: 1 },
  ] };
  const bindings = specificBindings(current);
  assert.equal(bindings.length, 2);
  assert.match(bindings[0].fixedMechanics.join('\n'), /requests 2 card\(s\)/);
  assert.match(bindings[0].fixedMechanics.join('\n'), /2x Hemokinesis\+.*required card is Hemokinesis itself/);
  const basis = 'boss-relic:RunicCube#0 | high | sequence | @on-play-draw,@draw-tradeoff\n'
    + 'boss-relic:Ectoplasm#0 | medium | direct | @recurring-energy,@retained-gold,@future-gold-tradeoff';
  const rows = parsePreparationClassification(basis, current);
  assert.deepEqual(rows[0].conditionIds, ['@on-play-draw', '@draw-tradeoff']);
  assert.match(rows[1].reason, /Owned Gold 126 remains available/);
  assert.throws(() => parsePreparationClassification(basis.replace('@on-play-draw,@draw-tradeoff', 'Needs Brutality first.'), current), /Use supplied condition references/);
  state.cards = state.cards.filter(row => row.card !== 'Brutality');
  assert.ok(specificBindings(current)[0].choices.some(row => row.id === '@on-play-draw'));
  assert.ok(!specificBindings(current)[0].choices.some(row => row.id === '@recurring-draw'));
  assert.throws(() => parsePreparationClassification(basis.replace('@on-play-draw,@draw-tradeoff', '@recurring-draw'), current), /Use supplied condition references/);
  state.run.gold = 26;
  assert.match(specificBindings(current)[1].fixedMechanics.join('\n'), /Owned Gold 26 remains available/);
  current.decision_evidence.candidates[0].definition.relic = 'TinyHouse';
  assert.equal(specificBindings(current).length, 1, 'mismatched candidate fact stays out');
});

test('Apparition accept and decline retain the whole package while model ratings remain independent', () => {
  const current = observation([]);
  const state = current.position.observation.observation;
  state.context = { kind: 'event', event: 'Ghosts' };
  state.run = { act: 2, current_hp: 48, max_hp: 75 };
  current.position.actions = [
    { semantic_action: 'event:Ghosts:0', candidate: { action: { kind: 'choose_event_option', public_semantics: {
      hp_delta: -11, max_hp_delta: -38, gold_delta_min: 0, gold_delta_max: 0, structured_semantics: { effects: [
        { effect: 'obtain_card', count: 3, pool: { kind: 'specific', card: 'Apparition', upgrades: 0 } },
      ] },
    } } } },
    { semantic_action: 'event:Ghosts:1', candidate: { action: { kind: 'choose_event_option', public_semantics: {
      hp_delta: 0, max_hp_delta: 0, gold_delta_min: 0, gold_delta_max: 0, structured_semantics: { effects: [] },
    } } } },
  ];
  current.decision_evidence = { kind: 'card_acquisition_options', candidates: [{ semantic_action: 'event:Ghosts:0', cards: [{
    current: { card: 'Apparition', upgrades: 0, cost: 1, intangible_gain: 1, intangible_damage_cap: 1,
      intangible_caps_direct_hp_loss_to_one: true, exhausts: true, ethereal: true },
    upgraded: { card: 'Apparition', upgrades: 1, ethereal: false },
  }] }] };
  const text = 'event:Ghosts:0 | low | direct | @hp-cap-tradeoff\nevent:Ghosts:1 | high | direct | @retain-hp-cap';
  const rows = parsePreparationClassification(text, current);
  assert.equal(rows[0].category, 'low', 'the host preserves the actual model rating');
  assert.match(rows[0].reason, /current-HP cost 11.*permanent cap reduction 38/);
  const localized = parsePreparationClassification(text, current, undefined, 'zh-CN');
  assert.deepEqual(localized.map(row => [row.command, row.category, row.realization, row.conditionIds]), rows.map(row => [row.command, row.category, row.realization, row.conditionIds]));
  assert.match(localized[0].reason, /当前生命成本 11.*永久上限减少 38/);
  assert.match(rows[0].reason, /Every new combat.*again/);
  assert.match(rows[1].reason, /preserves current HP 48\/75.*adds 3 permanent Apparition/s);
  assert.match(rows[1].reason, /multi-hit attacks still have separate damage instances/);
  assert.match(rows[1].reason, /upgrade removes Ethereal/);
  state.run.current_hp = 30;
  current.position.actions[0].candidate.action.public_semantics.hp_delta = 0;
  assert.match(specificBindings(current)[0].fixedMechanics.join(' '), /Immediate HP 30->30.*current-HP cost 0/);
  current.decision_evidence.candidates[0].cards[0].current.upgrades = 1;
  assert.equal(specificBindings(current).length, 0, 'an unmatched upgrade package supplies no binding');
});

test('Boss relic references preserve cap and replacement tradeoffs for either selected benefit', () => {
  const current = observation([{ card: 'Strike', upgrades: 1, count: 2 }, { card: 'Defend', upgrades: 0, count: 4 },
    { card: 'PerfectedStrike', upgrades: 0, count: 1 }]);
  current.position.observation.observation.context = { kind: 'boss_relic' };
  current.position.actions = ['VelvetChoker', 'PandorasBox'].map(relic => ({ semantic_action: `boss-relic:${relic}#0`,
    candidate: { action: { kind: 'take_boss_relic', relic } } }));
  current.decision_evidence = { kind: 'boss_relic_options', candidates: [
    { semantic_action: 'boss-relic:VelvetChoker#0', definition: { relic: 'VelvetChoker', energy_per_turn_delta: 1, card_play_limit_per_turn: 6 } },
    { semantic_action: 'boss-relic:PandorasBox#0', definition: { relic: 'PandorasBox', starter_card_replacement: {
      target_cards: ['Strike', 'Defend'], pool_rarities: ['Common', 'Uncommon', 'Rare'], replacements_per_card: 1,
      initial_upgrades: 0, applies_obtain_effects: true,
    } } },
  ] };
  const text = 'boss-relic:VelvetChoker#0 | high | direct | @recurring-energy\nboss-relic:PandorasBox#0 | medium | direct | @replace-starters';
  const rows = parsePreparationClassification(text, current);
  assert.match(rows[0].reason, /at most 6 card plays.*resets each turn/);
  assert.match(rows[1].reason, /replaces 6 owned starter cards.*2x Strike\+, 4x Defend/);
  assert.match(rows[1].reason, /deck keeps its card count/);
  assert.throws(() => parsePreparationClassification(text.replace('@replace-starters', '@shared-play-cap'), current), /Use supplied condition references/);
  current.position.observation.observation.cards = [{ card: 'PerfectedStrike', upgrades: 0, count: 1 }];
  assert.match(specificBindings(current)[1].fixedMechanics.join(' '), /replaces 0 owned starter cards/);
});

test('removing Ethereal preserves a redrawable copy, with hand persistence bound to an owned Pyramid', () => {
  const current = observation([{ card: 'Apparition', upgrades: 0, count: 3 }]);
  const state = current.position.observation.observation;
  state.context = { kind: 'campfire' }; state.relics = [];
  current.position.actions = [{ semantic_action: 'smith:Apparition+0#0',
    candidate: { action: { kind: 'smith', card: 'Apparition', upgrades: 0 } } }];
  current.decision_evidence = { kind: 'campfire_options', candidates: [{ semantic_actions: ['smith:Apparition+0#0'],
    before: { card: 'Apparition', upgrades: 0, cost: 1, intangible_gain: 1, ethereal: true, exhausts: true },
    after: { card: 'Apparition', upgrades: 1, cost: 1, intangible_gain: 1, ethereal: false, exhausts: true },
  }] };
  const rows = parsePreparationClassification('smith:Apparition#0 | high | direct | @preserve-for-redraw', current, conciseCardCommand);
  assert.match(rows[0].reason, /upgraded unplayed copy enters Discard.*subsequent draws or retrieval/);
  assert.match(rows[0].reason, /Exhaust after play true->true/);
  assert.match(rows[0].reason, /Only the selected permanent copy/);
  assert.throws(() => parsePreparationClassification('smith:Apparition#0 | high | direct | @pyramid-hand', current, conciseCardCommand),
    /Use supplied condition references/);
  state.relics.push({ relic: 'RunicPyramid' });
  const localized = parsePreparationClassification('smith:Apparition#0 | high | direct | @pyramid-hand', current, conciseCardCommand, 'zh-CN');
  assert.match(localized[0].reason, /RunicPyramid 跳过普通手牌弃置/);
  assert.match(parsePreparationClassification('smith:Apparition#0 | high | direct | @pyramid-hand', current, conciseCardCommand)[0].reason,
    /Owned RunicPyramid skips ordinary hand discard/);
  current.decision_evidence.candidates[0].after.ethereal = true;
  assert.equal(specificBindings(current, conciseCardCommand).length, 0);
});
