import { createPresentation } from '@sts-simulator/presentation';
// Gold, prices, purchase side effects and attached choices come from Rust.
// These private prefixes organize the current publicly offered shopping window.
import { gameContextSource, sameGameContextSource } from './context.mjs';
import { preparationDeckComparison, enduringStrengthGain, attackInventoryText } from './deck.mjs';
import { preparationCardExpertGuidance, renderCandidateExpertGuidance } from './strategy.mjs';

const purchases = new Set(['buy_card', 'buy_relic', 'buy_potion', 'remove_card']);
const stateOf = reading => reading.position?.observation?.domain === 'strategic'
  ? reading.position.observation.observation : undefined;
const identity = action => action.kind === 'buy_card' ? `card:${action.card}:${action.upgrades ?? 0}`
  : action.kind === 'buy_relic' ? `relic:${action.relic}`
    : action.kind === 'buy_potion' ? `potion:${action.potion}` : 'purge';

// A discard participates in shopping when an affordable public bottle needs
// its slot. Rebuild this check after each purchase, including discounts/income.
export function shopNeedsPotionSlot(reading) {
  const state = stateOf(reading), potions = state?.context?.inventory?.potions;
  if (state?.context?.kind !== 'shop' || !Array.isArray(potions)
      || !Number.isSafeInteger(state.run?.gold) || state.run.gold < 0) return;
  const knownBlocks = new Set(['merchant_unavailable', 'not_enough_gold', 'blocked_by_sozu', 'no_empty_potion_slot']);
  if (potions.some(row => !Number.isSafeInteger(row.price) || row.price < 0
      || !['purchasable', 'blocked'].includes(row.purchase_status?.status)
      || (row.purchase_status.status === 'blocked' && !knownBlocks.has(row.purchase_status.reason?.kind)))) return;
  return potions.some(row => row.price <= state.run.gold && row.purchase_status.status === 'blocked'
    && row.purchase_status.reason.kind === 'no_empty_potion_slot');
}

// A single known replacement and no other spending/use choice form a bottle
// comparison. Keep broader shops with the spending owner.
export function shopPotionReplacementScope(reading, normalize = command => command) {
  const state = stateOf(reading), slots = state?.potions, actions = reading.position?.actions;
  if (shopNeedsPotionSlot(reading) !== true || !Array.isArray(slots) || !slots.length
      || slots.some(slot => !Number.isSafeInteger(slot.slot) || !slot.potion?.potion)
      || new Set(slots.map(slot => slot.slot)).size !== slots.length || !Array.isArray(actions)) return;
  const offers = state.context.inventory.potions.filter(row => row.price <= state.run.gold
    && row.purchase_status.status === 'blocked' && row.purchase_status.reason.kind === 'no_empty_potion_slot');
  if (offers.length !== 1 || actions.length !== slots.length + 1) return;
  const leave = actions.filter(row => row.candidate?.action?.kind === 'leave_shop');
  const discards = actions.filter(row => row.candidate?.action?.kind === 'discard_run_potion');
  if (leave.length !== 1 || discards.length !== slots.length
      || new Set(discards.map(row => row.candidate.action.slot)).size !== slots.length
      || discards.some(row => !slots.some(slot => slot.slot === row.candidate.action.slot
        && slot.potion.potion === row.candidate.action.potion))) return;
  const offer = offers[0];
  return { schema: 'sts-shop-potion-comparison-v1', source: gameContextSource(reading), offer,
    location: { act: state.run.act, floor: state.run.floor },
    gold: state.run.gold, goldAfterPurchase: state.run.gold - offer.price,
    candidates: actions.map(row => {
      const action = row.candidate.action, keep = action.kind === 'leave_shop';
      return { command: normalize(row.semantic_action, reading), realization: keep ? 'direct' : 'sequence',
        reference: keep ? '@keep-stock' : '@replacement',
        fact: keep ? `Keep all held bottles and ${state.run.gold} Gold; leave this shop.`
          : `Give up P${action.slot} ${action.potion}, then buy ${offer.potion} for ${offer.price} Gold; purchase balance ${state.run.gold - offer.price}. Discard opens the slot; the purchase is the next preparation.` };
    }) };
}

export function shopInstructions(locale = 'en') {
  const presentation = createPresentation({ locale });
  const text = key => presentation.text(`scripted:shop.${key}`);
  return { potionGuide: text('potionGuide'), spendingGuide: text('spendingGuide'), naming: text('naming'),
    parameters: Object.fromEntries(['command', 'basis', 'choice', 'comparison'].map(key => [key,
      { type: 'string', ...(key === 'command' ? { required: true } : {}), description: text(`parameter${key[0].toUpperCase()}${key.slice(1)}`) }])) };
}
const defaultInstructions = shopInstructions();
export const SHOP_POTION_COMPARISON_GUIDE = defaultInstructions.potionGuide;
export const TARGET_SHOP_POTION_INPUT_BYTES = 4096;

export function measureShopPotionInputBudget(parts) {
  const bytes = parts.reduce((sum, part) => sum + Buffer.byteLength(part), 0);
  return { bytes, target: TARGET_SHOP_POTION_INPUT_BYTES, exceeded: bytes > TARGET_SHOP_POTION_INPUT_BYTES,
    includes: ['reading', 'guide', 'classification', 'progress'], componentBytes: parts.map(part => Buffer.byteLength(part)) };
}

export function renderShopPotionComparison(scope, draft) {
  const accepted = sameGameContextSource(draft?.source, scope.source) ? draft.rows : [];
  return ['Shop bottle comparison: one replacement or keep the current stock.',
    ...(sameGameContextSource(draft?.source, scope.source) && draft.pendingChoice ? [`Saved choice: ${draft.pendingChoice}. Complete the remaining rows through classify.`] : []),
    `Offered replacement: ${scope.offer.potion}, ${scope.offer.price} Gold. Current Gold ${scope.gold}.`,
    'Strategy (maintainer preference): pay for a useful improvement in protection or setup; compare it with the held bottle function and Gold retained.',
    'Row format: command | high/medium/low/review.',
    ...scope.candidates.map(row => {
      const recorded = accepted.find(item => item.command === row.command);
      return `  ${row.command}${recorded ? ` | ${recorded.category} (recorded)` : ''}\n    ${row.fact}`;
    }),
    ...(sameGameContextSource(draft?.source, scope.source) ? (draft.issues ?? []).map(issue => `Correction: ${issue.message}`) : []),
  ].join('\n');
}

export function shopPotionConditionBindings(scope) {
  return scope.candidates.map(row => ({ command: row.command, id: 'shop-potion-tradeoff-v1',
    fixedMechanics: [row.fact], choices: [
      { id: row.reference, text: 'Compare the bottle function gained or kept with the bottle and Gold given up.', realizations: [row.realization] },
      { id: '@review-access', text: 'A concrete bottle-comparison fact needs maintainer review.' },
    ] }));
}

export function bindShopPotionPlan(scope, selected, reading, normalize = command => command) {
  if (!scope?.candidates.some(row => row.command === selected && row.reference === '@replacement')) return;
  const source = gameContextSource(reading), state = stateOf(reading);
  if (source.session_id !== scope.source.session_id || source.head_revision <= scope.source.head_revision
      || state?.run?.act !== scope.location.act || state?.run?.floor !== scope.location.floor) return;
  const row = reading.position?.actions?.find(row => {
    const action = row.candidate?.action;
    return action?.kind === 'buy_potion' && action.potion === scope.offer.potion
      && action.shop_slot === scope.offer.slot && action.price === scope.offer.price;
  });
  if (!row || state?.run?.gold !== scope.gold) return;
  return bindShopPlan({ first: selected, next: normalize(row.semantic_action, reading), nextAction: row.candidate.action,
    expectedSnapshot: reading.position.public_snapshot_id, reservedPrice: scope.offer.price,
    goldAfterFirst: scope.gold, goldAfterPair: scope.goldAfterPurchase }, reading);
}

// The native shelf and held-resource effects supply this small question. The
// permanent inventory stays visible as names/counts; full evidence is host-owned.
export function shopPotionComparisonReading(native, reading) {
  if (!shopPotionReplacementScope(reading)) return;
  const state = stateOf(reading);
  const offers = native?.stock?.filter(row => row.action.kind === 'buy_potion');
  if (!offers || offers.length !== state.context.inventory.potions.length || !native.inventory?.resources) return;
  return [native.overview,
    `Owned permanent cards: ${(state.cards ?? []).map(row => `${row.count}x ${row.card}${row.upgrades > 0 ? '+' : ''}`).join(', ') || 'none'}.`,
    ...(Number.isSafeInteger(state.base_energy_per_turn) ? [`Base Energy: ${state.base_energy_per_turn}/turn.`] : []),
    ...offers.map(row => row.view), native.inventory.resources].filter(Boolean).join('\n');
}

function offer(action, inventory) {
  if (action.kind === 'remove_card') return inventory.purge;
  const list = inventory[{ buy_card: 'cards', buy_relic: 'relics', buy_potion: 'potions' }[action.kind]];
  const field = { buy_card: 'card', buy_relic: 'relic', buy_potion: 'potion' }[action.kind];
  return list?.find(row => row.slot === action.shop_slot && row[field] === action[field]
    && (action.kind !== 'buy_card' || row.upgrades === (action.upgrades ?? 0)));
}

function spendingRows(reading, normalize) {
  const state = stateOf(reading), inventory = state?.context?.inventory;
  if (state?.context?.kind !== 'shop' || !inventory) return [];
  const rows = [], byOffer = new Map();
  for (const row of reading.position.actions ?? []) {
    const action = row.candidate?.action;
    if (!purchases.has(action?.kind)) continue;
    const stock = offer(action, inventory);
    if (!stock || stock.purchase_status?.status !== 'purchasable'
        || !Number.isSafeInteger(stock.price) || stock.price < 0 || stock.price > state.run.gold) {
      throw new Error('Shop budget lacks the matching native purchasable offer.');
    }
    const key = action.kind === 'remove_card' ? 'purge' : `${action.kind}:${action.shop_slot}`;
    const member = { command: normalize(row.semantic_action, reading), semantic: row.semantic_action, action };
    if (byOffer.has(key)) { byOffer.get(key).members.push(member); continue; }
    const item = { ...member, key, identity: identity(action), price: stock.price, members: [member] };
    rows.push(item); byOffer.set(key, item);
  }
  return rows;
}

function previewReading(endpoint, original) {
  if (endpoint?.kind !== 'decision' || endpoint.observation?.domain !== 'strategic') {
    throw new Error('Shop preparation preview lacks a strategic successor.');
  }
  return { ...gameContextSource(original), position: endpoint };
}

// A narrowed removal supply names an equivalent public target class, not a
// command occurrence that survives purchases. Recheck the entire instance
// group in the successor; price and target ordinals belong to that successor.
function suppliedRemovalTargets(original, successor) {
  const policy = original.shopRemovalPolicy;
  if (!policy) return successor;
  const before = stateOf(original), after = stateOf(successor);
  const attributes = ['misc_value', 'base_damage_override', 'base_block_override', 'cost_modifier'];
  const equivalent = action => {
    const old = before.cards?.filter(card => card.card === action.card && card.upgrades === action.upgrades);
    const next = after.cards?.filter(card => card.card === action.card && card.upgrades === action.upgrades);
    return old?.length === 1 && next?.length === 1
      && [old[0], next[0]].every(card => Number.isSafeInteger(card.count) && card.count > 0
        && attributes.every(key => Object.hasOwn(card, key)))
      && attributes.every(key => old[0][key] === next[0][key]);
  };
  const supplied = sameGameContextSource(policy.source, original)
    ? original.position.actions.filter(row => policy.commands.includes(row.semantic_action)
      && row.candidate?.action?.kind === 'remove_card').map(row => row.candidate.action) : [];
  return { ...successor, position: { ...successor.position,
    actions: successor.position.actions.filter(row => row.candidate?.action?.kind !== 'remove_card'
      || (equivalent(row.candidate.action) && supplied.some(action => action.card === row.candidate.action.card
        && action.upgrades === row.candidate.action.upgrades))) } };
}

export async function preparationShopBudget(reading, preview, normalize = command => command) {
  const state = stateOf(reading);
  if (state?.context?.kind !== 'shop' || !state.context.inventory) return;
  if (!Number.isSafeInteger(state.run?.gold) || state.run.gold < 0) throw new Error('Shop budget requires current Gold.');
  const firstRows = spendingRows(reading, normalize), starts = [];
  for (const first of firstRows) {
    const probe = await preview([first.semantic]);
    if (probe.preview !== true) throw new Error('Shop budget requires a non-publishing native preview.');
    const successor = previewReading(probe.endpoint, reading), after = stateOf(successor);
    const row = { ...first, goldAfterFirst: after.run.gold, afterFirstSnapshot: probe.endpoint.public_snapshot_id,
      boundary: after.context.kind, continuations: [] };
    // A replenished shop reveals new random stock only after real adoption.
    const replenishes = state.relics?.some(item => item.relic === 'TheCourier')
      || (first.action.kind === 'buy_relic' && first.action.relic === 'TheCourier');
    if (replenishes) row.boundary = 'restock';
    if (row.boundary === 'shop') {
      const known = new Set(firstRows.filter(item => item.key !== first.key).map(item => item.identity));
      // Include publicly displayed unaffordable stock that a discount/income can unlock.
      const inventory = state.context.inventory;
      for (const item of inventory.cards) if (first.action.kind !== 'buy_card' || item.slot !== first.action.shop_slot)
        known.add(`card:${item.card}:${item.upgrades}`);
      for (const item of inventory.relics) if (first.action.kind !== 'buy_relic' || item.slot !== first.action.shop_slot)
        known.add(`relic:${item.relic}`);
      for (const item of inventory.potions) if (first.action.kind !== 'buy_potion' || item.slot !== first.action.shop_slot)
        known.add(`potion:${item.potion}`);
      if (first.action.kind !== 'remove_card') known.add('purge');
      for (const next of spendingRows(suppliedRemovalTargets(reading, successor), normalize).filter(item => known.has(item.identity))) {
        const pair = await preview([first.semantic, next.semantic]);
        if (pair.preview !== true) throw new Error('Shop combination preview published a state.');
        const end = stateOf(previewReading(pair.endpoint, reading));
        row.continuations.push({ ...next, goldAfterPair: end.run.gold, boundary: end.context.kind });
      }
    }
    starts.push(row);
  }
  return { schema: 'sts-shop-budget-v1', source: gameContextSource(reading), gold: state.run.gold,
    coverage: 'one_or_two_spends_on_public_stock', starts };
}

export function shopContinuation(reading, selected, text) {
  const budget = reading.shopBudget;
  if (!budget || !sameGameContextSource(budget.source, reading)) throw new Error('Shop continuation requires the current budget table.');
  const first = budget.starts.find(row => row.members.some(member => member.command === selected));
  const next = first?.continuations.find(row => row.members.some(member => member.command === text));
  if (!next) throw new Error('Choose a supplied follow-up command from this first purchase\'s budget row.');
  return { expectedSnapshot: first.afterFirstSnapshot, first: selected, next: text,
    nextAction: next.members.find(member => member.command === text).action,
    reservedPrice: next.price, goldAfterFirst: first.goldAfterFirst, goldAfterPair: next.goldAfterPair };
}

export function bindShopPlan(continuation, reading) {
  const state = stateOf(reading);
  if (!continuation || state?.context?.kind !== 'shop'
      || reading.position.public_snapshot_id !== continuation.expectedSnapshot
      || state.run.gold !== continuation.goldAfterFirst
      || !reading.position.actions.some(row => JSON.stringify(row.candidate?.action) === JSON.stringify(continuation.nextAction))) return;
  return { schema: 'sts-shop-plan-v1', source: gameContextSource(reading), ...continuation };
}

export function currentShopPlan(plan, reading) {
  return plan?.schema === 'sts-shop-plan-v1' && sameGameContextSource(plan.source, reading)
    && bindShopPlan(plan, reading) ? plan : undefined;
}

export function renderShopBudget(budget, plan) {
  if (!budget) return '';
  return [`Shop budget: Gold ${budget.gold}. Native private previews of one or two spends on currently public stock.`,
    ...(plan ? [`Planned follow-up from the preceding purchase: ${plan.next}; reserve ${plan.reservedPrice} Gold. Reassess it alongside the current legal alternatives.`] : []),
    ...budget.starts.map(first => `  First ${first.command} (${first.price} Gold): balance ${first.goldAfterFirst}. `
      + (first.boundary !== 'shop' ? `Complete ${first.boundary === 'restock' ? 'the actual purchase and refresh the new stock' : `the attached ${first.boundary} choices`}, then rebuild the budget.`
        : first.continuations.length ? `Then ${first.continuations.map(next => `${next.command} (${next.price} Gold; balance ${next.goldAfterPair}${next.boundary === 'shop' ? '' : `; ${next.boundary}`})`).join('; ')}.`
          : 'Keeping this balance is the available continuation.')),
    'Compare the whole intended spend with retaining Gold. The classification table covers the current single actions. For adoption, an intended follow-up uses: alternative | @resources[, other comparison references] | then <displayed follow-up command>. The host adopts the first action and supplies the intended next spend to its fresh classification.'].join('\n');
}

export const SHOP_SPENDING_GUIDE = defaultInstructions.spendingGuide;
export const SHOP_SPENDING_TOOL_PARAMETERS = defaultInstructions.parameters;
export const TARGET_SHOP_SPENDING_INPUT_BYTES = 4096;
export const SHOP_SPENDING_NAMING_GUIDE = defaultInstructions.naming;

// Ordinary purchases share one spending comparison. Native budget previews
// bind their prices and continuations; a reviewed removal policy owns the target.
// Slot operations, target ambiguity and attached choices keep their own contract.
export function shopSpendingScope(reading, normalize = command => command) {
  const state = stateOf(reading), budget = reading.shopBudget;
  if (state?.context?.kind !== 'shop' || !budget || !sameGameContextSource(budget.source, reading)
      || !reading.deck_evidence || !budget.starts.length
      || budget.starts.some(row => row.boundary !== 'shop')) return;
  const actions = reading.position.actions;
  if (budget.starts.some(row => {
    const stock = offer(row.action, state.context.inventory);
    return !stock || stock.price !== row.price || stock.purchase_status?.status !== 'purchasable';
  })) return;
  if (actions.filter(row => row.candidate?.action?.kind === 'leave_shop').length !== 1
      || actions.some(row => !purchases.has(row.candidate?.action?.kind)
        && row.candidate?.action?.kind !== 'leave_shop')) return;
  const purge = budget.starts.find(row => row.action.kind === 'remove_card');
  if (purge) {
    const targets = new Set(purge.members.map(row => `${row.action.card}:${row.action.upgrades}`));
    const groups = state.cards?.filter(card => card.card === purge.action.card && card.upgrades === purge.action.upgrades);
    if (targets.size !== 1 || groups?.length !== 1
        || (state.relics ?? []).some(row => ['BottledFlame', 'BottledLightning', 'BottledTornado'].includes(row.relic))) return;
  }
  if (purge && !sameGameContextSource(reading.shopRemovalPolicy?.source, reading)) return;
  const profile = preparationDeckComparison(reading, normalize);
  if (!profile) return;
  const supplied = new Set(budget.starts.flatMap(row => row.members.map(member => member.command)));
  if (actions.some(row => purchases.has(row.candidate.action.kind) && !supplied.has(normalize(row.semantic_action, reading)))) return;
  return { schema: 'sts-shop-spending-comparison-v1', source: gameContextSource(reading), profile,
    candidates: actions.map(row => {
      const command = normalize(row.semantic_action, reading), action = row.candidate.action;
      return { command, fact: action.kind === 'leave_shop' ? `Keep ${state.run.gold} Gold and leave this shop.`
        : action.kind === 'remove_card' ? `Remove one owned ${action.card}${action.upgrades > 0 ? '+' : ''}; pay ${action.price} Gold.`
          : `Buy one ${action.card ?? action.relic ?? action.potion}${action.upgrades > 0 ? '+' : ''}; pay ${action.price} Gold.` };
    }) };
}

export function measureShopSpendingInputBudget(parts) {
  const bytes = parts.reduce((sum, part) => sum + Buffer.byteLength(part), 0);
  return { bytes, target: TARGET_SHOP_SPENDING_INPUT_BYTES, exceeded: bytes > TARGET_SHOP_SPENDING_INPUT_BYTES,
    includes: ['reading', 'guide', 'classification', 'progress'], componentBytes: parts.map(part => Buffer.byteLength(part)) };
}

export function shopSpendingConditionBindings(scope) {
  return scope.candidates.map(row => ({ command: row.command, id: 'shop-spending-tradeoff-v1', fixedMechanics: [row.fact],
    choices: [{ id: '@shop-tradeoff', text: 'Recommendation for this spend or keeping Gold using the supplied inventory and budget.' },
      { id: '@review-access', text: 'A concrete shopping fact needs maintainer review.' }] }));
}

export function renderShopSpendingComparison(reading, scope, groups, draft) {
  const accepted = sameGameContextSource(draft?.source, scope.source) ? draft.rows : [];
  const pending = groups.filter(group => !accepted.some(row => row.command === group.command));
  return ['Shop recommendations: current spends and keeping Gold.',
    ...(sameGameContextSource(draft?.source, scope.source) && draft.pendingChoice ? [`Saved choice: ${draft.pendingChoice}. Complete the remaining rows through classify.`] : []),
    `Recommendation rows (${pending.length} pending of ${groups.length}):`,
    ...pending.map(group => `  ${group.command} | <recommendation> · ${scope.candidates.find(row => row.command === group.command).fact}`),
    ...(accepted.length ? ['Recorded recommendations:', ...groups.flatMap(group => {
      const row = accepted.find(row => row.command === group.command);
      return row ? [`  ${group.command} | ${row.category}`] : [];
    })] : []),
    'Strategy (maintainer preference): prefer useful protection, access or enabled engines; compare Gold retained.',
    ...(reading.shopRemovalPolicy ? [
      `Removal strategy: ${reading.shopRemovalPolicy.preference}`,
      ...reading.shopRemovalPolicy.conditions.map(text => `  Checked: ${text}`),
    ] : []),
    'Native budget: command (price, balance), then follow-up command (price, balance). All amounts are Gold.',
    ...(reading.shopPlan ? [`Planned next spend: ${reading.shopPlan.next}; reserve ${reading.shopPlan.reservedPrice} Gold.`] : []),
    ...reading.shopBudget.starts.map(first => {
      return `  ${first.command} (${first.price}, ${first.goldAfterFirst}); `
        + (first.continuations.length ? `then ${first.continuations.map(next => `${next.command} (${next.price}, ${next.goldAfterPair})`).join('; ')}` : 'keep the balance');
    }),
    ...(sameGameContextSource(draft?.source, scope.source) ? (draft.issues ?? []).map(issue => `Correction: ${issue.message}`) : []),
  ].join('\n');
}

// Preserve full printed rules and current instance numbers. Remove repeated
// explanatory scaffolding rather than asking Flash to assemble those facts.
export function shopSpendingReading(native, reading, normalize = command => command) {
  const scope = shopSpendingScope(reading, normalize);
  if (!scope) return;
  const missing = () => { throw Object.assign(new Error('Small shop comparison needs the complete native stock, owned rules and resource effects.'),
    { preparationGap: { kind: 'facts', reason: 'shop-spending-native-rules' } }); };
  if (!native?.inventory?.cards || !native.inventory.resources || !Array.isArray(native.stock)) return missing();
  if (native.inventory.cards.reduce((n, row) => n + row.count, 0) !== scope.profile.size) return missing();
  // Attack costs, hits and targets use the shared typed inventory summary.
  // Keep full rules for support and attacks with additional/conditional effects.
  const related = reading.deck_evidence.cards.filter(row => {
    const m = row.mechanics;
    return m.card_type !== 'Attack' || enduringStrengthGain(m) || m.draw_cards > 0
      || m.exhausts_other_cards || m.exhausts || m.ethereal || m.base_block_delivery === 'once_on_play'
      || m.weak > 0 || m.enemy_strength_down > 0 || m.energy_gain > 0 || m.intangible_gain > 0
      || m.current_hp_gain_on_fatal > 0 || m.status_injections?.length
      || m.vulnerable > 0 || m.copy_to_discard_after_damage > 0 || m.damage_growth_on_play > 0
      || m.has_conditional_playability || m.damage?.delivery?.amount?.kind !== 'base';
  });
  const relatedCards = new Set(related.map(row => `${row.card.card}:${row.card.upgrades}`));
  const relatedRules = native.inventory.cards.filter(row => relatedCards.has(`${row.card}:${row.upgrades}`));
  if (related.some(row => !relatedRules.some(native => native.card === row.card.card && native.upgrades === row.card.upgrades && native.view))) return missing();
  const inventory = stateOf(reading).context.inventory;
  const offered = { buy_card: inventory.cards, buy_relic: inventory.relics, buy_potion: inventory.potions };
  const visible = new Set(reading.shopBudget.starts.flatMap(first =>
    [first.identity, ...first.continuations.map(next => identity(next.action))]));
  const buyCommands = new Set(reading.shopBudget.starts.filter(row => row.action.kind !== 'remove_card').map(row => row.command));
  const counts = { buy_card: 0, buy_relic: 0, buy_potion: 0 }, stock = [], inactive = [];
  const seen = new Set();
  for (const row of native.stock) {
    const action = row.action, kind = action.kind, key = `${kind}:${action.shop_slot}`;
    const item = offered[kind]?.find(item => item.slot === action.shop_slot);
    if (!item || seen.has(key) || item.price !== action.price
        || (kind === 'buy_card' && (item.card !== action.card || item.upgrades !== action.upgrades))
        || (kind === 'buy_relic' && item.relic !== action.relic)
        || (kind === 'buy_potion' && item.potion !== action.potion)) return missing();
    seen.add(key); counts[kind]++;
    const command = row.command && normalize(row.command, reading);
    if (command && !buyCommands.has(command)) return missing();
    if (visible.has(identity(action))) {
      if (kind === 'buy_card' && !row.card_rules?.some(rule => rule.card === action.card && rule.upgrades === action.upgrades && rule.available)) return missing();
      stock.push(command ? `${command}  |  ${row.view.trimStart()}` : row.view);
    }
    else inactive.push(row.label);
  }
  if (Object.entries(counts).some(([kind, count]) => count !== offered[kind].length)) return missing();
  const adviceReading = { ...reading, position: { ...reading.position, actions: inventory.cards.filter(item =>
    visible.has(identity({ kind: 'buy_card', ...item }))).map(item =>
      reading.position.actions.find(row => row.candidate?.action?.kind === 'buy_card'
        && row.candidate.action.shop_slot === item.slot && row.candidate.action.card === item.card
        && row.candidate.action.upgrades === item.upgrades)
      ?? { semantic_action: `public-card:${item.slot}`, candidate: { action: { kind: 'buy_card', card: item.card, upgrades: item.upgrades } } }) } };
  const advice = preparationCardExpertGuidance(adviceReading).map(row =>
    renderCandidateExpertGuidance(row).replace(` (${row.command})`, '')).filter(Boolean);
  return [native.overview,
    `Owned ${scope.profile.size} originals: ${reading.deck_evidence.cards.map(row => `${row.card.count}x ${row.card.card}${row.card.upgrades > 0 ? '+' : ''}`).join(', ')}.`,
    ...reading.deck_evidence.cards.filter(row => row.card.cost_modifier || row.card.base_damage_override !== null
      || row.card.base_block_override !== null || row.card.misc_value)
      .map(row => `Current ${row.card.card}${row.card.upgrades > 0 ? '+' : ''}: ${row.persistent_cost} Energy, ${row.persistent_damage} damage, ${row.persistent_block} Block; misc ${row.card.misc_value}.`),
    attackInventoryText(scope.profile),
    'Related owned rules (before combat modifiers):', ...relatedRules.map(row => row.view),
    'Public stock and legal choices:', ...stock,
    ...scope.candidates.filter(row => row.command === 'leave').map(row => `${row.command}  |  ${row.fact}`),
    ...(inactive.length ? [`Other public stock: ${inactive.join('; ')}.`] : []),
    'Strategy (maintainer preferences):', ...advice,
    native.inventory.resources,
    native.routes,
  ].join('\n');
}
