import test from 'node:test';
import assert from 'node:assert/strict';
import { preparationShopBudget, renderShopBudget, shopContinuation, bindShopPlan, currentShopPlan, shopNeedsPotionSlot,
  shopPotionReplacementScope, shopPotionComparisonReading, bindShopPotionPlan, SHOP_POTION_COMPARISON_GUIDE,
  measureShopPotionInputBudget } from './shop.mjs';
import { shopSpendingScope, shopSpendingReading, measureShopSpendingInputBudget, SHOP_SPENDING_GUIDE } from './shop.mjs';
import { parsePreparationClassification, collectPreparationClassification, preparationCommandBasis, preparationReading,
  preparationProceduralAction, preparationClassificationContext, preparationRecordedTable, preparationStageGuide } from './preparation.mjs';
import { preparationReadingSupply } from './preparation.mjs';
import { preparationFixture } from './preparation-fixture.test.mjs';

const card = (id, price, slot = 0) => ({ command: `buy:${id}#0`, action: { kind: 'buy_card', card: id, upgrades: 0, price, shop_slot: slot } });
const relic = (id, price, slot = 0) => ({ command: `buy:${id}#0`, action: { kind: 'buy_relic', relic: id, price, shop_slot: slot } });
const remove = (id, price) => ({ command: `remove:${id}#0`, action: { kind: 'remove_card', card: id, upgrades: 0, price } });
function reading(gold, items, { revision = 0, relics = [], kind = 'shop', snapshot = `snapshot-${revision}`, blocked = [] } = {}) {
  const stock = list => list.map(item => ({ ...item.action, slot: item.action.shop_slot,
    purchase_status: { status: blocked.includes(item.command) ? 'blocked' : 'purchasable' } }));
  const inventory = { cards: stock(items.filter(i => i.action.kind === 'buy_card')),
    relics: stock(items.filter(i => i.action.kind === 'buy_relic')),
    potions: [], purge: items.find(i => i.action.kind === 'remove_card')
      ? { price: items.find(i => i.action.kind === 'remove_card').action.price, purchase_status: { status: 'purchasable' } } : null };
  return { schema: 'sts-run-play-agent-observation-v3', session_id: 'shop', head_revision: revision, state_revision: revision,
    position: { kind: 'decision', public_snapshot_id: snapshot,
      observation: { domain: 'strategic', observation: { run: { gold, act: 2, floor: 21 },
        cards: [], relics, context: { kind, inventory } } },
      actions: [...items.filter(item => !blocked.includes(item.command)).map(item => ({ semantic_action: item.command, candidate: { action: item.action } })),
        { semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } }] } };
}
function previews(endpoints) {
  const calls = [];
  const preview = async actions => {
    calls.push(actions);
    const result = endpoints[actions.join(' / ')];
    assert.ok(result, `Unexpected native prefix ${actions}`);
    return { preview: true, endpoint: result.position };
  };
  return { calls, preview };
}

test('shop combinations use actual successor prices, shifted slots and preserve legal single actions', async () => {
  const a = card('DarkEmbrace', 80), b = card('ShrugItOff', 60, 1), r = remove('Strike', 75);
  const original = reading(160, [a, b, r]);
  const afterA = reading(80, [card('ShrugItOff', 60), r], { revision: 1 });
  const afterB = reading(100, [a, r], { revision: 1 });
  const afterR = reading(85, [a, b], { revision: 1 });
  const p = previews({ [a.command]: afterA, [b.command]: afterB, [r.command]: afterR,
    [`${a.command} / ${b.command}`]: reading(20, [], { revision: 2 }),
    [`${a.command} / ${r.command}`]: reading(5, [], { revision: 2 }),
    [`${b.command} / ${a.command}`]: reading(20, [], { revision: 2 }),
    [`${b.command} / ${r.command}`]: reading(25, [], { revision: 2 }),
    [`${r.command} / ${a.command}`]: reading(5, [], { revision: 2 }),
    [`${r.command} / ${b.command}`]: reading(25, [], { revision: 2 }) });
  original.shopBudget = await preparationShopBudget(original, p.preview);
  assert.equal(original.position.actions.length, 4);
  assert.equal(p.calls.length, 9);
  const firstCalls = structuredClone(p.calls);
  const localizedBudget = await preparationShopBudget(original, p.preview, undefined, 'zh-CN');
  assert.deepEqual(localizedBudget, original.shopBudget);
  assert.deepEqual(p.calls.slice(firstCalls.length), firstCalls);
  assert.match(renderShopBudget(localizedBudget, undefined, 'zh-CN'), /余额 80.*Strike#0.*余额 5/);
  assert.match(renderShopBudget(original.shopBudget), /DarkEmbrace#0.*balance 80.*Strike#0 \(75 Gold; balance 5/);
  const text = [a.command, b.command, r.command, 'leave'].map(command => `${command} | medium | direct | @effect,@resource-tradeoff`).join('\n');
  const classification = { source: original, rows: parsePreparationClassification(text, original) };
  const chosen = preparationCommandBasis([a.command, []], 'pending', `leave | @resources,@access | then ${r.command}`, original, classification);
  assert.equal(chosen.shopContinuation.goldAfterPair, 5);
  const plan = bindShopPlan(chosen.shopContinuation, afterA);
  assert.equal(currentShopPlan(plan, afterA).next, r.command);
  assert.equal(currentShopPlan(plan, reading(80, [r], { revision: 2 })), undefined);
  assert.equal(bindShopPlan(chosen.shopContinuation, reading(81, [r], { revision: 1 })), undefined);
  assert.throws(() => preparationCommandBasis([a.command, []], 'pending', 'leave | @resources | then buy:Clash#0', original, classification), /supplied follow-up/);
  assert.equal(preparationCommandBasis([a.command, []], 'pending', 'leave | @resources', original, classification).shopContinuation, undefined);
});

test('native discount previews can unlock originally unaffordable visible stock', async () => {
  const membership = relic('MembershipCard', 100), demon = card('DemonForm', 90);
  const original = reading(80, [relic('MembershipCard', 50), demon], { blocked: [demon.command] });
  const after = reading(30, [card('DemonForm', 25)], { revision: 1 });
  const p = previews({ [membership.command]: after,
    [`${membership.command} / ${demon.command}`]: reading(5, [], { revision: 2 }) });
  const budget = await preparationShopBudget(original, p.preview);
  assert.equal(budget.starts.length, 1);
  assert.equal(budget.starts[0].continuations[0].price, 25);
  assert.equal(budget.starts[0].continuations[0].goldAfterPair, 5);
  assert.equal(budget.starts[0].continuations[0].action.price, 25);
});

test('discount continuations keep the supplied removal class and bind fresh typed targets', async () => {
  const membership = relic('MembershipCard', 50), strike = remove('Strike', 75);
  strike.action.card_ordinal = 0;
  const original = reading(80, [membership, strike]);
  const instance = { card: 'Strike', upgrades: 0, count: 2, misc_value: 0,
    base_damage_override: null, base_block_override: null, cost_modifier: 0 };
  original.position.observation.observation.cards = [instance];
  original.shopRemovalPolicy = { source: original, commands: [strike.command] };
  const discounted = { ...remove('Strike', 25), command: 'remove:Strike#1' };
  discounted.action.card_ordinal = 4;
  const hidden = remove('Inflame', 25);
  hidden.action.card_ordinal = 2;
  const after = reading(30, [hidden, discounted], { revision: 1 });
  after.position.observation.observation.cards = [structuredClone(instance)];
  const p = previews({ [membership.command]: after, [strike.command]: reading(5, [], { revision: 1 }),
    [`${membership.command} / ${discounted.command}`]: reading(5, [], { revision: 2 }) });
  original.shopBudget = await preparationShopBudget(original, p.preview);
  const next = original.shopBudget.starts[0].continuations[0];
  assert.deepEqual(next.members.map(row => row.command), [discounted.command]);
  assert.equal(next.action.price, 25);
  assert.equal(next.action.card_ordinal, 4);
  assert.throws(() => shopContinuation(original, membership.command, hidden.command), /supplied follow-up/);
  assert.equal(bindShopPlan(shopContinuation(original, membership.command, discounted.command), after).next, discounted.command);
  assert.ok(!p.calls.some(actions => actions.includes(hidden.command)));
});

test('narrowed removal continuations refresh when public target attributes become ambiguous or change', async () => {
  for (const changed of ['modifier', 'heterogeneous', 'missing', 'stale']) {
    const membership = relic('MembershipCard', 50), strike = remove('Strike', 75);
    const original = reading(80, [membership, strike]);
    const instance = { card: 'Strike', upgrades: 0, count: 2, misc_value: 0,
      base_damage_override: null, base_block_override: null, cost_modifier: 0 };
    original.position.observation.observation.cards = [instance];
    original.shopRemovalPolicy = { source: changed === 'stale' ? { ...original, head_revision: -1 } : original,
      commands: [strike.command] };
    const after = reading(30, [remove('Strike', 25)], { revision: 1 });
    after.position.observation.observation.cards = [structuredClone(instance)];
    if (changed === 'modifier') after.position.observation.observation.cards[0].cost_modifier = -1;
    if (changed === 'heterogeneous') after.position.observation.observation.cards.push({ ...instance, misc_value: 5 });
    if (changed === 'missing') delete after.position.observation.observation.cards[0].cost_modifier;
    const p = previews({ [membership.command]: after, [strike.command]: reading(5, [], { revision: 1 }) });
    const budget = await preparationShopBudget(original, p.preview);
    assert.equal(budget.starts[0].continuations.length, 0, changed);
  }
});

test('native pickup Gold and attached selections determine continuation boundaries', async () => {
  const coin = relic('OldCoin', 100), bottle = relic('BottledFlame', 50);
  const original = reading(100, [coin]);
  const afterCoin = reading(300, [card('DarkEmbrace', 80)], { revision: 1 });
  const p = previews({ [coin.command]: afterCoin });
  const budget = await preparationShopBudget(original, p.preview);
  assert.equal(budget.starts[0].goldAfterFirst, 300);
  assert.equal(budget.starts[0].continuations.length, 0); // New identity was absent from public stock.
  const b = await preparationShopBudget(reading(100, [bottle]), previews({ [bottle.command]:
    reading(50, [], { kind: 'run_choice', revision: 1 }) }).preview);
  assert.match(renderShopBudget(b), /balance 50.*attached run_choice choices/);
});

test('Courier purchases and existing restock stop before exposing generated offers', async () => {
  for (const [first, owned] of [[relic('TheCourier', 100), []], [card('ShrugItOff', 50), [{ relic: 'TheCourier' }]]]) {
    const p = previews({ [first.command]: reading(100, [card('DemonForm', 50)], { revision: 1 }) });
    const budget = await preparationShopBudget(reading(200, [first], { relics: owned }), p.preview);
    assert.equal(budget.starts[0].boundary, 'restock');
    assert.equal(p.calls.length, 1);
    assert.doesNotMatch(renderShopBudget(budget), /DemonForm/);
  }
});

test('duplicate offers survive source-local occurrence changes and stale tables are rejected', async () => {
  const one = card('ShrugItOff', 40), two = { ...card('ShrugItOff', 40, 1), command: 'buy:ShrugItOff#1' };
  const original = reading(80, [one, two]);
  const after = reading(40, [one], { revision: 1 });
  const p = previews({ [one.command]: after, [two.command]: after,
    [`${one.command} / ${one.command}`]: reading(0, [], { revision: 2 }),
    [`${two.command} / ${one.command}`]: reading(0, [], { revision: 2 }) });
  original.shopBudget = await preparationShopBudget(original, p.preview);
  assert.equal(original.shopBudget.starts.length, 2);
  assert.equal(original.shopBudget.starts[0].continuations[0].command, one.command);
  const changed = { ...after, shopBudget: original.shopBudget };
  assert.throws(() => shopContinuation(changed, one.command, one.command), /current budget table/);
});

test('shop discard work follows actual affordable replacement bottles and refreshes after the budget changes', () => {
  const original = reading(40, []), state = original.position.observation.observation;
  state.potions = [{ slot: 0, potion: { potion: 'BlockPotion' } }, { slot: 1, potion: { potion: 'SwiftPotion' } }];
  const stock = { slot: 0, potion: 'StrengthPotion', price: 50,
    purchase_status: { status: 'blocked', reason: { kind: 'not_enough_gold', gold: 40, price: 50 } } };
  state.context.inventory.potions = [stock];
  original.position.actions.unshift(...state.potions.map(slot => ({ semantic_action: `discard-potion:${slot.potion.potion}#0`,
    candidate: { action: { kind: 'discard_run_potion', slot: slot.slot } } })));
  assert.equal(shopNeedsPotionSlot(original), false);
  const supplied = preparationReading(original);
  assert.deepEqual(supplied.position.actions.map(row => row.semantic_action), ['leave']);
  assert.equal(preparationProceduralAction(supplied).row.semantic_action, 'leave');
  assert.equal(original.position.actions.length, 3);
  state.run.gold = 60;
  stock.purchase_status.reason = { kind: 'no_empty_potion_slot' };
  assert.equal(shopNeedsPotionSlot(original), true);
  assert.deepEqual(preparationReading(original).position.actions, original.position.actions);
  assert.equal(preparationProceduralAction(preparationReading(original)), undefined);
  stock.purchase_status.reason = { kind: 'blocked_by_sozu' };
  assert.equal(preparationReading(original).position.actions.length, 1);
  stock.purchase_status.reason = { kind: 'new_purchase_requirement' };
  assert.equal(shopNeedsPotionSlot(original), undefined);
  assert.equal(preparationReading(original).position.actions.length, 3);
  delete state.context.inventory;
  assert.equal(preparationReading(original).position.actions.length, 3);
});

test('irrelevant shop discards can be hidden while a legal potion use still needs a choice', () => {
  const original = reading(0, []);
  original.position.actions.unshift({ semantic_action: 'discard-potion:BloodPotion#0',
    candidate: { action: { kind: 'discard_run_potion' } } }, { semantic_action: 'use-potion:BloodPotion#0',
    candidate: { action: { kind: 'use_run_potion' } } });
  const supplied = preparationReading(original);
  assert.deepEqual(supplied.position.actions.map(row => row.semantic_action), ['use-potion:BloodPotion#0', 'leave']);
  assert.equal(preparationProceduralAction(supplied), undefined);
});

function bottleReading() {
  const current = reading(40, []), state = current.position.observation.observation;
  state.potions = [{ slot: 0, potion: { potion: 'SpeedPotion' } }, { slot: 1, potion: { potion: 'FearPotion' } }];
  state.context.inventory.potions = [{ slot: 0, potion: 'BlockPotion', price: 40,
    purchase_status: { status: 'blocked', reason: { kind: 'no_empty_potion_slot' } } }];
  current.position.actions.unshift(...state.potions.map(slot => ({ semantic_action: `discard-potion:${slot.potion.potion}#0`,
    candidate: { action: { kind: 'discard_run_potion', slot: slot.slot, potion: slot.potion.potion } } })));
  return current;
}

test('bottle comparison languages preserve ratings and replacements while keeping native resources opaque', () => {
  const current = bottleReading(), before = JSON.stringify(current);
  const en = shopPotionReplacementScope(current), zh = shopPotionReplacementScope(current, undefined, 'zh-CN');
  assert.deepEqual(en.candidates.map(row => [row.command, row.reference, row.realization]), zh.candidates.map(row => [row.command, row.reference, row.realization]));
  assert.match(zh.candidates[0].fact, /金币/);
  const basis = 'discard-potion:SpeedPotion#0 | high\ndiscard-potion:FearPotion#0 | low\nleave | medium';
  const rows = ['en', 'zh-CN'].map(locale => parsePreparationClassification(basis, current, undefined, locale));
  assert.deepEqual(rows[0].map(row => [row.command, row.category, row.realization, row.conditionIds]), rows[1].map(row => [row.command, row.category, row.realization, row.conditionIds]));
  assert.match(rows[1][0].reason, /SpeedPotion.*40 金币.*BlockPotion/);
  const native = preparationFixture(current, { resources: 'Native {{opaque}} 资源 🐈' });
  const supplied = shopPotionComparisonReading(native, current, 'zh-CN');
  assert.ok(supplied.includes('Native {{opaque}} 资源 🐈'));
  assert.match(preparationClassificationContext(current, undefined, undefined, 'zh-CN'), /商店药水比较/);
  assert.equal(JSON.stringify(current), before);
});

test('single replacement supplies a bottle tradeoff and binds factual columns independently of model ratings', () => {
  const current = bottleReading(), before = JSON.stringify(current), scope = shopPotionReplacementScope(current);
  assert.equal(scope.candidates.length, 3);
  const context = preparationClassificationContext(current);
  assert.match(context, /Row format: command \| high\/medium\/low\/review/);
  assert.doesNotMatch(context, /Whole-deck|Strength sources|direct\/sequence|Condition references/);
  assert.equal(preparationStageGuide('pending', undefined, { potionReplacement: scope }), SHOP_POTION_COMPARISON_GUIDE);
  const basis = 'discard-potion:SpeedPotion#0 | high\ndiscard-potion:FearPotion#0 | review\nleave | low';
  const rows = parsePreparationClassification(basis, current);
  assert.deepEqual(rows.map(row => row.realization), ['sequence', 'review', 'direct']);
  assert.equal(rows[0].basisKind, 'host-bound-potion-tradeoff');
  assert.deepEqual(rows.map(row => row.conditionIds), [['@replacement'], ['@review-access'], ['@keep-stock']]);
  assert.match(rows[0].reason, /SpeedPotion.*BlockPotion.*40 Gold.*balance 0/);
  const recorded = { source: scope.source, rows };
  assert.match(preparationStageGuide('pending', recorded, { potionReplacement: scope }), /command choose/);
  assert.doesNotMatch(preparationStageGuide('pending', recorded, { potionReplacement: scope }), /direct\/sequence|Rate two/);
  assert.throws(() => parsePreparationClassification('', current), error =>
    /command \| high\/medium\/low\/review/.test(error.message) && !/direct\/sequence/.test(error.message));
  assert.equal(preparationCommandBasis(['discard-potion:SpeedPotion#0', []], 'pending', undefined, current, recorded).command,
    'discard-potion:SpeedPotion#0');
  assert.throws(() => preparationCommandBasis(['discard-potion:FearPotion#0', []], 'pending', undefined, current, recorded), /missing fact/);
  assert.doesNotMatch(preparationRecordedTable(recorded, current), /Whole-deck|Strength sources/);
  const partial = collectPreparationClassification('leave | medium', current);
  assert.equal(partial.pendingCommands.length, 2);
  assert.equal(collectPreparationClassification('discard-potion:SpeedPotion#0 | high\ndiscard-potion:FearPotion#0 | low', current,
    undefined, partial).complete, true);
  assert.throws(() => parsePreparationClassification('leave | high | direct | @effect', current), /command \| high/);
  assert.equal(JSON.stringify(current), before);
  current.position.actions.reverse();
  assert.deepEqual(parsePreparationClassification(basis, current).map(row => [row.command, row.category]).sort(),
    rows.map(row => [row.command, row.category]).sort());
});

test('bottle comparison preserves complete native shelf and resource effects and requires a narrow legal window', () => {
  const current = bottleReading();
  const native = preparationFixture(current, { overview: 'Act 2 F21 | Shop\nBoss: Collector',
    stockViews: { BlockPotion: 'Block Potion 40g (slots full)\nGain 12 Block.' } });
  native.inventory.resources = 'Sacred Bark\nDouble potion effectiveness.\nTemporary Dexterity.\nApply Vulnerable.';
  const compact = shopPotionComparisonReading(native, current);
  assert.match(compact, /Gain 12 Block[\s\S]*Double potion effectiveness[\s\S]*Temporary Dexterity[\s\S]*Apply Vulnerable/);
  assert.doesNotMatch(compact, /Deal 6 damage|Remove card/);
  assert.equal(shopPotionComparisonReading('Incomplete reading', current), undefined);
  const localized = structuredClone(native);
  localized.stock[0].view = '格挡药水\n获得 12 点格挡。';
  assert.match(shopPotionComparisonReading(localized, current), /格挡药水[\s\S]*获得 12 点格挡/);
  localized.stock = [];
  assert.equal(shopPotionComparisonReading(localized, current), undefined);
  for (const change of [
    input => { input.position.actions.push({ semantic_action: 'use-potion:SpeedPotion#0', candidate: { action: { kind: 'use_run_potion' } } }); },
    input => { input.position.observation.observation.context.inventory.potions.push({ slot: 1, potion: 'SwiftPotion', price: 20,
      purchase_status: { status: 'blocked', reason: { kind: 'no_empty_potion_slot' } } }); },
    input => { input.position.observation.observation.potions[0].potion = null; },
    input => { input.position.actions[0].candidate.action.potion = 'UnknownPotion'; },
  ]) { const input = bottleReading(); change(input); assert.equal(shopPotionReplacementScope(input), undefined); }
});

test('replacement intent binds the real successor purchase and expires with source or price changes', () => {
  const original = bottleReading(), scope = shopPotionReplacementScope(original);
  const after = reading(40, [{ command: 'buy:BlockPotion#0', action: { kind: 'buy_potion', potion: 'BlockPotion', shop_slot: 0, price: 40 } }], { revision: 1 });
  const plan = bindShopPotionPlan(scope, 'discard-potion:SpeedPotion#0', after);
  assert.equal(plan.next, 'buy:BlockPotion#0');
  assert.equal(plan.goldAfterPair, 0);
  assert.deepEqual(currentShopPlan(plan, after), plan);
  assert.equal(currentShopPlan(plan, reading(40, [], { revision: 2 })), undefined);
  assert.equal(bindShopPotionPlan(scope, 'leave', after), undefined);
  const other = structuredClone(after); other.session_id = 'other-shop';
  assert.equal(bindShopPotionPlan(scope, 'discard-potion:SpeedPotion#0', other), undefined);
  assert.equal(bindShopPotionPlan(scope, 'discard-potion:SpeedPotion#0', original), undefined);
  after.position.actions[0].candidate.action.price = 41;
  assert.equal(bindShopPotionPlan(scope, 'discard-potion:SpeedPotion#0', after), undefined);
});

test('bottle payload measurement reports target excess and UTF-8 components', () => {
  assert.equal(measureShopPotionInputBudget(['abc', 'def']).bytes, 6);
  const budget = measureShopPotionInputBudget(['x'.repeat(4090), '机制', 'a', '']);
  assert.equal(budget.bytes, 4097);
  assert.equal(budget.exceeded, true);
  assert.deepEqual(budget.componentBytes, [4090, 6, 1, 0]);
  assert.deepEqual(budget.includes, ['reading', 'guide', 'classification', 'progress']);
  assert.equal(measureShopPotionInputBudget(['x'.repeat(4096)]).exceeded, false);
});

async function smallShop() {
  const shrug = card('ShrugItOff', 80), membership = relic('MembershipCard', 50), dark = card('DarkEmbrace', 170, 1);
  const original = reading(165, [shrug, membership, dark], { blocked: [dark.command] });
  const state = original.position.observation.observation;
  state.base_energy_per_turn = 3;
  const owned = (card, count, type, cost, extra = {}) => ({ card: { card, count, upgrades: 0, misc_value: 0,
    cost_modifier: 0, base_damage_override: null, base_block_override: null }, mechanics: { card, name: card,
    upgrades: 0, card_type: type, cost, draw_cards: 0, exhausts: false, ethereal: false, ...extra },
    persistent_cost: cost, persistent_damage: card === 'Strike' ? 6 : 0, persistent_block: 0 });
  const cards = [owned('Strike', 3, 'Attack', 1), owned('Inflame', 1, 'Power', 1,
    { strength_gain: 2, strength_gain_delivery: 'on_play' })];
  state.cards = cards.map(row => row.card);
  original.deck_evidence = { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: original.position.public_snapshot_id, cards };
  const afterMembership = reading(115, [card('ShrugItOff', 40), card('DarkEmbrace', 85, 1)], { revision: 1 });
  const p = previews({ [shrug.command]: reading(85, [membership, dark], { revision: 1, blocked: [dark.command] }),
    [membership.command]: afterMembership,
    [`${shrug.command} / ${membership.command}`]: reading(35, [], { revision: 2 }),
    [`${membership.command} / ${shrug.command}`]: reading(75, [], { revision: 2 }),
    [`${membership.command} / ${dark.command}`]: reading(30, [], { revision: 2 }) });
  original.shopBudget = await preparationShopBudget(original, p.preview);
  return { original, afterMembership };
}

function smallShopNative(current) {
  const native = preparationFixture(current, { overview: 'Act 2 F21 | Shop\nBoss: Collector', cardViews: {
    Strike: 'Strike | Attack | 1c\nDeal 6 damage.',
    Inflame: 'Inflame | Power | 1c\nGain 2 Strength for the rest of combat.',
    Anger: 'Anger | Attack | 0c\nDeal 6 damage, then add a copy to Discard.',
  } });
  const rules = { ShrugItOff: 'Gain 8 Block, then draw 1 card.', DarkEmbrace: 'After deployment, draw 1 for each Exhaust.',
    MembershipCard: 'All shop prices are halved, including removal.',
    Combust: 'Lose 1 HP and deal 5 damage to each enemy at each turn end.',
    PaperFrog: 'Attacks deal more damage against Vulnerable.' };
  const names = { ShrugItOff: 'Shrug It Off', DarkEmbrace: 'Dark Embrace', MembershipCard: 'Membership Card', PaperFrog: 'Paper Frog' };
  for (const row of native.stock) {
    const id = row.action.card ?? row.action.relic ?? row.action.potion;
    row.label = `${names[id] ?? id} ${row.action.price}g${row.command ? '' : ' (insufficient gold)'}`;
    row.view = `${row.label}\n${rules[id] ?? `complete ${id} rule`}`;
  }
  native.inventory.resources = 'Relics (0):\nPotions (2 slots):\nP0 empty\nP1 empty';
  return native;
}

test('localized spending comparison keeps discount continuations and raw rule blocks unchanged', async () => {
  const { original } = await smallShop(), native = smallShopNative(original);
  native.inventory.resources = 'Original {{resources}} 原文 🐈';
  const position = JSON.stringify(original.position), rules = JSON.stringify(native);
  const classifications = [];
  for (const locale of ['en', 'zh-CN']) {
    const scope = shopSpendingScope(original, undefined, locale);
    assert.deepEqual(scope.candidates.map(row => row.command), ['buy:ShrugItOff#0', 'buy:MembershipCard#0', 'leave']);
    const text = shopSpendingReading(native, original, undefined, locale);
    assert.ok(text.includes(native.inventory.resources));
    assert.match(text, locale === 'en' ? /Public stock and legal choices:/ : /公开商品与合法选择：/);
    const rows = parsePreparationClassification('buy:ShrugItOff#0 | medium\nbuy:MembershipCard#0 | high\nleave | low', original, undefined, locale);
    classifications.push(rows.map(row => [row.command, row.category, row.realization, row.conditionIds]));
    const adopted = preparationCommandBasis(['buy:MembershipCard#0', []], 'pending', 'then buy:DarkEmbrace#0', original,
      { source: original, rows }, undefined, locale);
    assert.equal(adopted.shopContinuation.goldAfterPair, 30);
    assert.match(preparationClassificationContext(original, undefined, undefined, locale), locale === 'en' ? /Shop recommendations/ : /商店建议/);
  }
  assert.deepEqual(classifications[0], classifications[1]);
  assert.equal(JSON.stringify(original.position), position);
  assert.equal(JSON.stringify(native), rules);
});

test('small shop advice preserves legal acquisition evidence for owned-copy increments', async () => {
  const combust = card('Combust', 41), original = reading(157, [combust]);
  const effect = { hp_loss_per_application: 1, damage_per_enemy_per_application: 5 };
  const owned = { card: { card: 'Combust', count: 1, upgrades: 0, misc_value: 0, cost_modifier: 0,
    base_damage_override: null, base_block_override: null },
  mechanics: { card: 'Combust', name: 'Combust', upgrades: 0, card_type: 'Power', cost: 1, draw_cards: 0,
    exhausts: false, ethereal: false, end_of_turn_hp_loss_and_damage: effect },
  persistent_cost: 1, persistent_damage: 0, persistent_block: 0 };
  const state = original.position.observation.observation;
  state.base_energy_per_turn = 4; state.cards = [owned.card];
  original.deck_evidence = { schema: 'sts-agent-deck-evidence-v1', public_snapshot_id: original.position.public_snapshot_id, cards: [owned] };
  original.decision_evidence = { kind: 'card_acquisition_options', candidates: [{ semantic_action: combust.command,
    cards: [{ current: owned.mechanics }] }] };
  original.shopBudget = await preparationShopBudget(original, previews({ [combust.command]: reading(116, []) }).preview);
  const rendered = shopSpendingReading(smallShopNative(original), original);
  assert.match(rendered, /Owned Combust copies 1->2.*HP loss 1->2.*damage per enemy 5->10/);
  assert.doesNotMatch(rendered, /public-card:/);
  assert.match(rendered, /buy:Combust#0  \|  Combust 41g/);
});

test('small shopping rates one dimension and binds optional follow-up to native prices', async () => {
  const { original, afterMembership } = await smallShop();
  assert.ok(shopSpendingScope(original));
  const text = 'buy:ShrugItOff#0 | medium\nbuy:MembershipCard#0 | high\nleave | low';
  const rows = parsePreparationClassification(text, original);
  assert.ok(rows.every(row => row.realization === null && row.basisKind === 'host-bound-shop-tradeoff'));
  const classification = { source: original, rows };
  const adopted = preparationCommandBasis(['buy:MembershipCard#0', []], 'pending', 'then buy:DarkEmbrace#0', original, classification);
  assert.equal(adopted.basisKind, 'shop-recommendations');
  assert.equal(adopted.shopContinuation.goldAfterPair, 30);
  assert.equal(bindShopPlan(adopted.shopContinuation, afterMembership).next, 'buy:DarkEmbrace#0');
  assert.throws(() => preparationCommandBasis(['leave', []], 'pending', 'then buy:DarkEmbrace#0', original, classification), /supplied follow-up/);
  assert.throws(() => preparationCommandBasis(['buy:ShrugItOff#0', []], 'pending', 'then buy:DarkEmbrace#0', original, classification), /supplied follow-up/);
  assert.throws(() => parsePreparationClassification(text.replace('high', 'high | direct | @effect'), original), /command \| high/);
  assert.throws(() => preparationCommandBasis(['leave', []], 'pending', undefined, original,
    { source: original, rows: parsePreparationClassification(text.replace('leave | low', 'leave | review'), original) }), /missing fact/);
  original.position.actions.reverse();
  assert.deepEqual(parsePreparationClassification(text, original).map(row => [row.command, row.category]).sort(),
    rows.map(row => [row.command, row.category]).sort());
  assert.equal(shopSpendingScope({ ...original, head_revision: 9 }), undefined);
  const unsupported = structuredClone(original);
  unsupported.position.observation.observation.context.inventory.cards[0].card = 'Armaments';
  assert.equal(shopSpendingScope(unsupported), undefined);
});

test('clear shop recommendations survive appended prose without importing it into bound facts', async () => {
  const { original } = await smallShop();
  const plain = 'buy:ShrugItOff#0 | medium\nbuy:MembershipCard#0 | high\nleave | low';
  const annotated = plain.split('\n').map(line => `${line} — invented explanation marker`).join('\n');
  const rows = parsePreparationClassification(annotated, original);
  assert.deepEqual(rows, parsePreparationClassification(plain, original));
  assert.ok(rows.every(row => !row.reason.includes('invented explanation marker')));
  const classification = { source: original, rows };
  for (const comparison of ['keep the balance', 'then keep the balance']) {
    const decision = preparationCommandBasis(['buy:MembershipCard#0', []], 'pending', comparison, original, classification);
    assert.equal(decision.command, 'buy:MembershipCard#0');
    assert.equal(decision.selectedRow.category, 'high');
    assert.equal(decision.shopContinuation, undefined);
  }
  for (const invalid of [
    plain.replace('high', 'high — explanation | low'),
    plain.replace('high', 'high | direct | @effect'),
    plain.replace('buy:MembershipCard#0', 'buy:Unknown#0'),
    plain.replace('high', 'highest — explanation'),
  ]) assert.throws(() => parsePreparationClassification(invalid, original));
});

test('small shop supply keeps independent blocked stock, enabled support and full native rules', async () => {
  const { original } = await smallShop();
  original.preparation_reading = smallShopNative(original);
  const supply = preparationReadingSupply(original);
  assert.doesNotMatch(supply.candidates['buy:ShrugItOff#0'].view, /Dark Embrace|Exhaust/);
  assert.match(supply.reading, /Dark Embrace 170g[\s\S]*draw 1 for each Exhaust/);
  assert.match(supply.reading, /3x Strike.*1x Inflame[\s\S]*Gain 2 Strength[\s\S]*Gain 8 Block[\s\S]*All shop prices are halved/);
  const context = preparationClassificationContext(original);
  assert.doesNotMatch(context, /Condition references|direct\/sequence|Whole-deck/);
  assert.match(context, /Recommendation rows \(3 pending of 3\)/);
  assert.match(context, /leave \| <recommendation> · Keep 165 Gold and leave/);
  assert.match(supply.reading, /leave  \|  Keep 165 Gold and leave/);
  assert.match(context, /buy:DarkEmbrace#0 \(85, 30\)/);
  const budget = measureShopSpendingInputBudget([supply.reading, SHOP_SPENDING_GUIDE, context, 'Shop task: submit rows.']);
  assert.ok(budget.bytes < 4096);
  const incomplete = smallShopNative(original);
  incomplete.inventory.cards = incomplete.inventory.cards.filter(row => row.card !== 'Inflame');
  assert.throws(() => shopSpendingReading(incomplete, original), error =>
    error.preparationGap.reason === 'shop-spending-native-rules');
  assert.equal(measureShopSpendingInputBudget(['x'.repeat(4097)]).exceeded, true);
  assert.equal(measureShopSpendingInputBudget(['x'.repeat(4096)]).exceeded, false);
});

test('ordinary broader stock keeps one recommendation per spend and summarizes currently blocked unrelated offers', async () => {
  const { original: template } = await smallShop();
  const items = ['Whirlwind', 'Immolate', 'Armaments', 'Warcry', 'Combust', 'SwiftStrike'].map((id, slot) => card(id, 20, slot));
  const blocked = relic('PaperFrog', 900);
  const original = reading(200, [...items, blocked], { blocked: [blocked.command] });
  original.position.observation.observation.base_energy_per_turn = 3;
  original.position.observation.observation.cards = template.position.observation.observation.cards;
  original.deck_evidence = { ...template.deck_evidence, public_snapshot_id: original.position.public_snapshot_id };
  original.shopBudget = await preparationShopBudget(original, async commands => ({ preview: true,
    endpoint: reading(200 - 20 * commands.length, [...items.filter(item => !commands.includes(item.command)), blocked],
      { revision: commands.length, blocked: [blocked.command] }).position }));
  assert.equal(original.shopBudget.starts.length, 6);
  assert.equal(shopSpendingScope(original).candidates.length, 7);
  const rows = items.map(item => `${item.command} | medium`).concat('leave | high').join('\n');
  assert.equal(parsePreparationClassification(rows, original).length, 7);
  original.preparation_reading = smallShopNative(original);
  original.preparation_reading.stock.reverse();
  original.preparation_reading.inventory.cards.reverse();
  const supply = preparationReadingSupply(original);
  for (const row of original.preparation_reading.stock.filter(row => row.command)) {
    assert.ok(supply.reading.includes(row.view.trimStart()), row.action.card);
  }
  assert.match(supply.reading, /Other public stock: Paper Frog 900g \(insufficient gold\)/);
  assert.doesNotMatch(supply.reading, /Attacks deal more damage against Vulnerable/);
  assert.doesNotMatch(preparationClassificationContext(original), /Condition references|direct\/sequence|Whole-deck/);
  const incomplete = smallShopNative(original);
  incomplete.stock.find(row => row.action.card === 'Immolate').card_rules[0].available = false;
  assert.throws(() => shopSpendingReading(incomplete, original),
    error => error.preparationGap.reason === 'shop-spending-native-rules');
  incomplete.stock[0].action.shop_slot = 99;
  assert.throws(() => shopSpendingReading(incomplete, original), /complete native stock/);
  const attached = structuredClone(original); attached.shopBudget.starts[0].boundary = 'run_choice';
  assert.equal(shopSpendingScope(attached), undefined);
});

test('shop attack comparison retains owned payment, hits, targeting and generated-copy rules', async () => {
  const { original } = await smallShop();
  const make = (card, cost, damage, hits, targets, extra = {}) => {
    const row = structuredClone(original.deck_evidence.cards[0]);
    row.card.card = row.mechanics.card = row.mechanics.name = card;
    row.card.count = 1;
    row.persistent_cost = row.mechanics.cost = cost;
    row.persistent_damage = damage;
    row.mechanics.damage = { damage_per_hit: damage, hit_count: hits,
      delivery: { kind: 'attack', amount: { kind: 'base' }, hits: { kind: 'fixed', count: hits }, targets } };
    Object.assign(row.mechanics, extra);
    return row;
  };
  original.deck_evidence.cards = [make('Anger', 0, 6, 1, 'selected_enemy', { copy_to_discard_after_damage: 1 }),
    make('SwordBoomerang', 1, 3, 3, 'random_enemy_per_hit'), original.deck_evidence.cards[1]];
  original.position.observation.observation.cards = original.deck_evidence.cards.map(row => row.card);
  const supplied = shopSpendingReading(smallShopNative(original), original);
  assert.match(supplied, /1x Anger: 0 Energy; 1 hit\(s\); targets selected_enemy; 6 base damage\/hit/);
  assert.match(supplied, /1x SwordBoomerang: 1 Energy; 3 hit\(s\); targets random_enemy_per_hit; 3 base damage\/hit/);
  assert.match(supplied, /Deal 6 damage, then add a copy to Discard/);
});
