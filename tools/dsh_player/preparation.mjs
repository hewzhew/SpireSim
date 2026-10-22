import { createPresentation } from '@sts-simulator/presentation';
import { gameContextSource, sameGameContextSource, gameDecisionTask } from './context.mjs';
import { preparationExpertGuidance, renderExpertGuidance, renderCandidateExpertGuidance, apparitionPreparationOffers, preparationBossRelicOffers, preparationShopRemovalPolicy } from './strategy.mjs';
import { preparationDeckComparison, preparationDeckConditionBindings, preparationCardAcquisition, strikeScaledAttack, renderDeckComparison, attackInventoryText, renderStatusRoutes } from './deck.mjs';
import { shopInstructions, renderShopBudget, shopContinuation, shopNeedsPotionSlot, shopPotionReplacementScope,
  renderShopPotionComparison, shopPotionComparisonReading, shopPotionConditionBindings,
  shopSpendingScope, shopSpendingReading, shopSpendingConditionBindings, renderShopSpendingComparison } from './shop.mjs';

const families = new Set(['shop', 'campfire', 'event', 'rewards', 'treasure', 'boss_relic', 'run_choice', 'map', 'run_transition']);
const queries = new Set(['read', 'options']);

export function preparationInstructions(locale = 'en') {
  const presentation = createPresentation({ locale });
  const text = (key, params) => presentation.text(`scripted:preparation.${key}`, params);
  const parameters = Object.fromEntries(['command', 'basis', 'choice'].map(key => [key,
    { type: 'string', ...(key === 'command' ? { required: true } : {}), description: text(`parameter${key[0].toUpperCase()}${key.slice(1)}`) }]));
  return {
    text, comparisons: ['function', 'access', 'protection', 'resources', 'opportunity'].map(key => ({ id: `@${key}`, text: text(`comparison${key[0].toUpperCase()}${key.slice(1)}`) })),
    comparisonBasis: text('comparisonBasis'), procedure: text('procedure'), basis: text('basis'), description: text('description'), parameters,
    shopParameters: { ...parameters, comparison: { type: 'string', description: text('parameterComparison') } },
  };
}
const defaultInstructions = preparationInstructions();
export const PREPARATION_COMPARISONS = defaultInstructions.comparisons;
export const PREPARATION_COMPARISON_BASIS = defaultInstructions.comparisonBasis;
export const PREPARATION_PROCEDURE = defaultInstructions.procedure;
export const PREPARATION_BASIS = defaultInstructions.basis;
export const PREPARATION_TOOL_DESCRIPTION = defaultInstructions.description;
export const PREPARATION_TOOL_PARAMETERS = defaultInstructions.parameters;
export const PREPARATION_SHOP_TOOL_PARAMETERS = defaultInstructions.shopParameters;

// The native frame binds rule blocks to actions and entity IDs. Display text is
// opaque; it never identifies a command, card, relic or section boundary.
export function preparationReadingSupply(observation, normalize = command => command) {
  const native = observation.preparation_reading;
  if (native?.schema !== 'sts-preparation-reading-v1'
      || !native.public_snapshot_id || native.public_snapshot_id !== observation.position?.public_snapshot_id) {
    throw Object.assign(new Error('Preparation reading is not bound to the current public snapshot.'),
      { preparationGap: { kind: 'facts', reason: 'preparation-reading-source' } });
  }
  const commands = new Set(preparationCandidates(observation, normalize));
  const omissions = sameGameContextSource(observation.preparationOmissions?.source, observation)
    ? new Set(observation.preparationOmissions.commands.map(command => normalize(command, observation))) : new Set();
  const candidates = {}, reading = [];
  reading.push(native.overview, native.selections);
  for (const block of native.candidates) {
    const command = normalize(block.command, observation);
    if (omissions.has(command)) continue;
    if (commands.has(command)) {
      if (Object.hasOwn(candidates, command)) throw new Error(`Duplicate rendered preparation command: ${command}`);
      candidates[command] = block;
    } else reading.push(`${command}  |  ${block.label}\n${block.view}`);
  }
  reading.push(...native.stock.filter(row => !row.command).map(row => row.view), native.inventory.view, native.routes);
  return { reading: shopPotionComparisonReading(native, observation) ?? shopSpendingReading(native, observation, normalize)
    ?? reading.filter(Boolean).join('\n'),
    relicReadings: Object.fromEntries(native.inventory.relics.map(row => [row.relic, row.view])), candidates };
}

export function preparationStageGuide(progress, classification, context, draft, locale = 'en') {
  const { text, procedure, basis } = preparationInstructions(locale);
  const shop = shopInstructions(locale);
  if (progress !== 'complete' && !classification && draft?.pendingChoice) return text('pendingChoice', { choice: draft.pendingChoice });
  if (progress === 'pending' && context?.potionReplacement) return classification ? text('potionAdoption') : shop.potionGuide;
  if (progress === 'pending' && context?.shopSpending) return classification ? text('shopAdoption') : shop.spendingGuide;
  const current = progress === 'complete' ? text('complete')
    : (progress === 'selection' ? text('selection') : '')
      + (progress === 'selection' && context?.reason === 'Duplicate' ? text('duplication') + '\n' : '')
      + (classification ? text('adoption', { choiceInstruction: text(progress === 'selection' ? 'selectionChoice' : 'rowChoice') })
        : text('classification', { procedure, basis, route: context?.kind === 'map' ? text('route') : '', pending: draft?.rows.length ? text('pending') : '' }));
  return text('stage', { current, rewardOrder: context?.kind === 'reward' ? text('rewardOrder') : '', shopPlan: context?.kind === 'shop' ? text('shopPlan') : '' });
}

// Rust supplies exact tokens and the ordered domain. Rate targets individually;
// a multi-card choice is assembled at adoption without expanding combinations.
function preparationSelectionFamily(observation) {
  const rows = (observation.position?.actions ?? []).filter(row => row.run_selection_family);
  if (!rows.length) return;
  if (rows.length !== 1) throw new Error('Preparation requires one current card-selection family.');
  const family = rows[0].run_selection_family;
  if (!Array.isArray(family.card_tokens) || family.card_tokens.length !== family.selectable_cards?.length
      || new Set(family.card_tokens).size !== family.card_tokens.length
      || family.card_tokens.some(token => typeof token !== 'string' || !token || /[\s,]/.test(token))
      || !Number.isSafeInteger(family.min_choices) || !Number.isSafeInteger(family.max_choices)
      || family.min_choices < 0 || family.min_choices > family.max_choices) {
    throw new Error('Preparation card selection lacks its exact Rust tokens or choice range.');
  }
  return family;
}

function preparationSelectionTargets(observation, normalize = command => command) {
  const family = preparationSelectionFamily(observation);
  if (family) return family.selectable_cards.map((card, index) => ({ card,
    command: normalize(`act select:${family.card_tokens[index]}`, observation),
    reading: family.card_readings?.[index] }));
  return (observation.position?.actions ?? []).flatMap(row => row.run_selection_target
    ? [{ ...row.run_selection_target, command: normalize(`act ${row.semantic_action}`, observation) }] : []);
}

export function preparationCandidates(observation, normalize = command => command) {
  const actions = observation.position?.actions;
  if (!Array.isArray(actions) || !actions.length) throw new Error('Preparation classification requires current legal candidates.');
  return actions.flatMap(action => {
    if (action.run_selection_family) {
      const family = preparationSelectionFamily(observation);
      return [...family.card_tokens.map(token => normalize(`act select:${token}`, observation)),
        ...(family.min_choices === 0 ? ['act select:none'] : [])];
    }
    const semantic = action.candidate?.action?.kind === 'submit_run_selection'
      ? `act ${action.semantic_action}` : action.semantic_action;
    const command = normalize(semantic, observation);
    if (typeof command !== 'string' || !command.trim()) throw new Error('Preparation candidate lacks a semantic command.');
    return command;
  });
}

// Procedural steps reveal choices or collect unconditional rewards. Keep
// competing potion claims, relic/key alternatives and actual event costs visible
// to the classifier. Inventory discard is independent of an empty event exit.
export function preparationProceduralAction(observation) {
  const actions = observation.position?.actions ?? [];
  if (actions.some(row => row.run_selection_family)) return;
  const state = observation.position?.observation?.observation;
  const context = state?.context;
  if (context?.kind === 'reward') {
    const row = actions.find(row => row.candidate?.action?.kind === 'claim_reward'
      && ['gold', 'stolen_gold'].includes(row.candidate.action.reward?.kind));
    if (row) return { row, owner: 'reward-reveal-and-gold' };
    const slots = state.potions;
    const capacity = state.run?.potion_capacity;
    const potionItems = Array.isArray(context.pending_rewards)
      ? context.pending_rewards.flatMap((reward, index) => reward.kind === 'potion' ? [{ reward, index }] : []) : [];
    const claims = potionItems.map(({ reward, index }) => actions.find(row => {
      const action = row.candidate?.action;
      return action?.kind === 'claim_reward' && action.reward_item_index === index
        && action.reward?.kind === 'potion' && action.reward.potion === reward.potion;
    }));
    // The native reward contract clears a Sozu-blocked potion without changing
    // held inventory. Bind the actual item and current claim before clearing.
    if (claims.length && claims.every(Boolean) && (state.relics ?? []).some(row => row.relic === 'Sozu')) {
      return { row: claims[0], owner: 'reward-potions-blocked-by-sozu' };
    }
    // A complete current inventory and the legal claims establish that every
    // offered bottle fits. Re-read after each claim to bind shifted indices.
    if (Array.isArray(slots) && Number.isSafeInteger(capacity) && capacity > 0 && slots.length === capacity
        && slots.every((slot, index) => slot?.slot === index && Object.hasOwn(slot, 'potion'))
        && !(state.relics ?? []).some(row => row.relic === 'Sozu')
        && potionItems.length > 0 && potionItems.length <= slots.filter(slot => slot.potion === null).length) {
      if (claims.every(Boolean)) return { row: claims[0], owner: 'reward-potions-with-space' };
    }
    // Finish inventory claims before automatic card revelation so their real
    // successor supplies the acquisition baseline. Competing claims remain a
    // player choice; the card command stays legal alongside unwanted rewards.
    if (!actions.some(row => row.candidate?.action?.kind === 'claim_reward')) {
      const open = actions.find(row => row.candidate?.action?.kind === 'open_card_reward');
      if (open) return { row: open, owner: 'reward-reveal-and-gold' };
    }
  }
  if (actions.length === 1) return { row: actions[0], owner: 'forced-single-candidate' };
  const primary = actions.filter(row => row.candidate?.action?.kind !== 'discard_run_potion');
  if (primary.length !== 1) return;
  const action = primary[0].candidate?.action;
  const effects = action?.public_semantics?.structured_semantics;
  // PlannerDecisionContext::Reward omits pending_rewards only when empty.
  if (context?.kind === 'reward' && !(context.pending_rewards ?? []).length && action?.kind === 'proceed') {
    return { row: primary[0], owner: 'empty-reward-exit' };
  }
  if (context?.kind === 'reward' && action?.kind === 'proceed'
      && (context.pending_rewards ?? []).every((_, index) => observation.declinedRewardItems?.includes(index))) {
    return { row: primary[0], owner: 'declined-reward-exit' };
  }
  if ((context?.kind === 'map' && action?.kind === 'choose_route_node')
      || (context?.kind === 'event' && action?.kind === 'choose_event_option'
        && ['complete', 'advance_screen'].includes(effects?.transition?.kind) && effects.effects?.length === 0)) {
    return { row: primary[0], owner: 'sole-route-or-empty-exit' };
  }
}

// Skip closes a card page without consuming its reward. This bounded cursor
// belongs to reward navigation, and is valid only at its exact adopted source.
export function preparationReading(observation, progress) {
  if (observation.position?.observation?.domain !== 'strategic') return observation;
  const state = observation.position.observation.observation;
  const hasEmptySlot = state?.potions?.some(slot => slot.potion === null);
  // These pages offer no potion delivery. Keep slot replacement in the shop
  // or surrounding reward; Rust retains the complete legal action inventory.
  const hideDiscard = ['card_reward', 'campfire', 'map'].includes(state?.context?.kind)
    || hasEmptySlot || shopNeedsPotionSlot(observation) === false;
  const shopRemovalPolicy = preparationShopRemovalPolicy(observation);
  // Closing the route overlay preserves the same preparation. Supply the
  // actual destinations to classification and leave UI navigation in Rust.
  const hasRoutes = state?.context?.kind === 'map'
    && observation.position.actions.some(row => row.candidate?.action?.kind === 'choose_route_node');
  const declined = progress && sameGameContextSource(progress.source, observation) ? new Set(progress.declined) : undefined;
  if (!hideDiscard && !declined && !hasRoutes && !shopRemovalPolicy) {
    return observation.shopRemovalPolicy ? { ...observation, shopRemovalPolicy: undefined } : observation;
  }
  const actions = observation.position.actions.filter(row => (!hideDiscard || row.candidate?.action?.kind !== 'discard_run_potion')
    && (!shopRemovalPolicy || row.candidate?.action?.kind !== 'remove_card'
      || shopRemovalPolicy.commands.includes(row.semantic_action))
    && !(hasRoutes && row.candidate?.action?.kind === 'cancel' && row.candidate.action.site === 'map')
    && (row.candidate?.action?.kind !== 'open_card_reward'
      || !declined?.has(row.candidate.action.reward_item_index)));
  const previousOmissions = sameGameContextSource(observation.preparationOmissions?.source, observation)
    ? observation.preparationOmissions.commands : [];
  const omittedCommands = [...new Set([...previousOmissions,
    ...observation.position.actions.filter(row => !actions.includes(row)).map(row => row.semantic_action)])];
  return { ...observation, ...(declined ? { declinedRewardItems: [...declined] } : {}),
    preparationOmissions: { source: gameContextSource(observation), commands: omittedCommands },
    ...(shopRemovalPolicy ? { shopRemovalPolicy: { ...shopRemovalPolicy, source: gameContextSource(observation) } }
      : observation.shopRemovalPolicy ? { shopRemovalPolicy: undefined } : {}), position: { ...observation.position, actions } };
}

export function preparationRewardProgress(previous, before, after, action) {
  const oldState = before.position?.observation?.observation;
  const state = after.position?.observation?.observation;
  if (!['reward', 'card_reward'].includes(state?.context?.kind)) return;
  let declined = sameGameContextSource(previous?.source, before) ? [...previous.declined] : [];
  const inventory = value => JSON.stringify({ run: value?.run, cards: value?.cards,
    relics: value?.relics, potions: value?.potions });
  // A new inventory can change the acquisition decision. Gold alone does not
  // change a free card reward's payoff, and unconditional gold is claimed first.
  const withoutGold = value => ({ ...value, run: { ...value?.run, gold: undefined } });
  if (inventory(withoutGold(oldState)) !== inventory(withoutGold(state))) declined = [];
  const index = action?.reward_item_index;
  if (action?.kind === 'skip_card_reward' && Number.isSafeInteger(index)) {
    declined.push(index);
  } else if (Number.isSafeInteger(index) && ['claim_reward', 'take_card', 'singing_bowl'].includes(action?.kind)) {
    // Bind the index shift to the actual removal, including identical rewards.
    const oldItems = oldState?.context?.pending_rewards;
    const items = state.context.pending_rewards ?? [];
    if (oldState?.context?.kind === 'reward' && Array.isArray(oldItems)
        && JSON.stringify(oldItems.filter((_, i) => i !== index)) === JSON.stringify(items)) {
      declined = declined.filter(i => i !== index).map(i => i > index ? i - 1 : i);
    } else declined = [];
  }
  if (state.context.kind === 'reward') {
    const items = state.context.pending_rewards ?? [];
    declined = declined.filter(i => items[i]?.kind === 'card_reward');
  }
  return { source: gameContextSource(after), declined: [...new Set(declined)] };
}

// The strategic observation groups copies by all public instance modifiers.
// Actions expose only the card/upgrades and ordinal, so multiple public groups
// of that name/upgrade stay separate until the target can be bound explicitly.
export function preparationCandidateGroups(observation, normalize = command => command) {
  const commands = preparationCandidates(observation, normalize);
  if (new Set(commands).size !== commands.length) throw new Error('Preparation candidates have duplicate commands.');
  const state = observation.position.observation.observation;
  // Bottles bind a particular lineage through relic state. The grouped public
  // cards omit that identity; preserve individual choices while it is present.
  if ((state.relics ?? []).some(row => ['BottledFlame', 'BottledLightning', 'BottledTornado'].includes(row.relic))) {
    return commands.map(command => ({ command, members: [command] }));
  }
  const cards = state.cards ?? [];
  const attributes = ['misc_value', 'base_damage_override', 'base_block_override', 'cost_modifier'];
  const targets = preparationSelectionTargets(observation, normalize);
  if (targets.length) {
    const buckets = new Map();
    targets.forEach(({ card, command }) => {
      if (attributes.some(key => !Object.hasOwn(card, key))) return;
      const key = JSON.stringify([card.card, card.upgrades, ...attributes.map(key => card[key])]);
      const members = buckets.get(key) ?? [];
      members.push(command);
      buckets.set(key, members);
    });
    const membership = new Map([...buckets.values()].flatMap(members => members.map(command => [command, members])));
    return commands.flatMap(command => {
      const members = membership.get(command) ?? [command];
      return command === members[0] ? [{ command, members }] : [];
    });
  }
  const buckets = new Map();
  observation.position.actions.forEach((row, index) => {
    const action = row.candidate?.action;
    if (row.candidate?.domain !== 'strategic' || !['smith', 'remove_card', 'toke'].includes(action?.kind)
        || !Number.isSafeInteger(action.card_ordinal)) return;
    const owned = cards.filter(card => card.card === action.card && card.upgrades === action.upgrades);
    if (owned.length !== 1 || !Number.isSafeInteger(owned[0].count) || owned[0].count < 2
        || attributes.some(key => !Object.hasOwn(owned[0], key))) return;
    const key = JSON.stringify(Object.entries(action).filter(([key]) => key !== 'card_ordinal')
      .sort(([a], [b]) => a.localeCompare(b)));
    const bucket = buckets.get(key) ?? { count: owned[0].count, indices: [], ordinals: new Set() };
    bucket.indices.push(index);
    bucket.ordinals.add(action.card_ordinal);
    buckets.set(key, bucket);
  });
  const membership = new Map();
  for (const bucket of buckets.values()) {
    if (bucket.indices.length < 2 || bucket.indices.length > bucket.count
        || bucket.ordinals.size !== bucket.indices.length) continue;
    for (const index of bucket.indices) membership.set(index, bucket.indices);
  }
  const groups = [];
  commands.forEach((command, index) => {
    const indices = membership.get(index) ?? [index];
    if (index === indices[0]) groups.push({ command, members: indices.map(i => commands[i]) });
  });
  return groups;
}

// Bind maintainer-written relationships to typed, currently offered actions.
// A reference selects an emphasis, not a claim that its access conditions hold.
// Fixed mechanics accompany every selection, including a low rating.
export function preparationConditionBindings(observation, normalize = command => command, locale = 'en') {
  const potionScope = shopPotionReplacementScope(observation, normalize);
  if (potionScope) return shopPotionConditionBindings(potionScope);
  const spendingScope = shopSpendingScope(observation, normalize);
  if (spendingScope) return shopSpendingConditionBindings(spendingScope);
  const specific = [...preparationEnergyBindings(observation, normalize), ...armamentsConditionBindings(observation, normalize),
    ...etherealUpgradeConditionBindings(observation, normalize),
    ...attackUpgradeConditionBindings(observation, normalize),
    ...duplicateConditionBindings(observation, normalize), ...routeConditionBindings(observation, normalize),
    ...bossRelicConditionBindings(observation, normalize), ...eventCombatConditionBindings(observation, normalize),
    ...apparitionConditionBindings(observation, normalize),
    ...rewardRevealConditionBindings(observation, normalize),
    ...statusDrawConditionBindings(observation, normalize),
    ...temporaryStrengthConditionBindings(observation, normalize)];
  const bindings = [...specific, ...preparationDeckConditionBindings(observation, normalize, locale)
    .filter(row => !specific.some(binding => binding.command === row.command))];
  const profile = preparationDeckComparison(observation, normalize, locale);
  return [...bindings, ...preparationCandidateGroups(observation, normalize)
    .filter(group => !bindings.some(binding => binding.command === group.command))
    .map(group => ({ command: group.command, id: 'preparation-effect-tradeoff-v1',
      fixedMechanics: [],
      choices: [
        { id: '@effect', text: 'Assess the useful increment of this supplied effect compared with the current inventory or preserving the resource.' },
        { id: '@owned-support', text: 'Assess realization through specific currently owned resources, their actual access, triggers and deployment order.' },
        { id: '@resource-tradeoff', text: 'Weigh this effect\'s draw, payment, HP, Gold or preparation opportunity against the strongest feasible alternative and keeping the resource.' },
        { id: '@review-access', text: 'A specific missing effect, inventory or access fact needs maintainer review.' },
      ] }))].map(binding => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === binding.command);
    const acquisition = preparationCardAcquisition(observation, row);
    if (!acquisition || !profile) return binding;
    const state = observation.position.observation.observation;
    const copies = state.cards.filter(card => card.card === acquisition.card).reduce((sum, card) => sum + card.count, 0);
    const injections = acquisition.mechanics?.status_injections ?? [];
    return { ...binding, fixedMechanics: [
      `Evaluation pool: the ${profile.size} owned originals plus one ${acquisition.card}${acquisition.upgrades > 0 ? '+' : ''}. Owned ${acquisition.card} copies before this choice: ${copies}; after: ${copies + 1}.`,
      ...(injections.length ? [
        `This addition's generated Status routes: ${renderStatusRoutes([{ card: acquisition.card, injections }])}`,
        `Owned Status-draw payoff Powers: ${profile.statusPayoffs.map(source => `${source.card}: ${source.cost} Energy to deploy`).join('; ') || 'none'}. Apply those owned effects when their qualifying Status draw occurs after deployment.`,
      ] : []), ...binding.fixedMechanics,
    ] };
  });
}

function attackUpgradeConditionBindings(observation, normalize) {
  return preparationCandidateGroups(observation, normalize).flatMap(group => {
    const action = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command)?.candidate?.action;
    if (action?.kind !== 'smith') return [];
    const evidence = observation.decision_evidence?.candidates?.find(row =>
      [row.semantic_action, ...(row.semantic_actions ?? [])].filter(Boolean).some(command => normalize(command, observation) === group.command));
    const before = evidence?.before, after = evidence?.after;
    if (before?.card !== action.card || before.upgrades !== action.upgrades
        || after?.card !== action.card || after.upgrades !== before.upgrades + 1
        || !Number.isSafeInteger(before.cost) || before.cost < 0 || before.cost !== after.cost) return [];
    const attacks = [before, after].map(card => card.damage);
    const [oldScaling, newScaling] = attacks.map(strikeScaledAttack);
    if (oldScaling && newScaling && attacks[0].delivery.targets === attacks[1].delivery.targets
        && (oldScaling.perCard !== newScaling.perCard || oldScaling.base !== newScaling.base)) {
      return [{ command: group.command, id: 'strike-scaled-attack-upgrade-v1', fixedMechanics: [
        `Upgrade one ${action.card} copy: cost ${before.cost}->${after.cost}; damage request before Strength and other combat modifiers changes ${oldScaling.base} + ${oldScaling.perCard} * N -> ${newScaling.base} + ${newScaling.perCard} * N.`,
        oldScaling.countRule,
        `Upgrade increment at the same N: ${newScaling.base - oldScaling.base} + ${newScaling.perCard - oldScaling.perCard} * N. Reach the upgraded copy and pay for it; compare this one-copy improvement with recovery and other upgrades.`,
      ], choices: [
        { id: '@strike-count', text: 'Assess the per-counted-copy damage increase using the supplied whole-pile count rule and the owned removal or Exhaust routes that change it.' },
        { id: '@upgrade-opportunity', text: 'Compare this incremental attack improvement with the strongest other upgrade, recovery or key action using the campfire.' },
        { id: '@review-access', text: 'A specific count, access or payment fact is needed to rate this upgrade; record it through sts_feedback.' },
      ] }];
    }
    if (!attacks.every(damage => damage?.delivery?.kind === 'attack' && damage.delivery.amount?.kind === 'base'
        && damage.delivery.hits?.kind === 'fixed' && Number.isSafeInteger(damage.delivery.hits.count)
        && damage.delivery.hits.count > 0 && damage.hit_count === damage.delivery.hits.count
        && Number.isSafeInteger(damage.damage_per_hit) && damage.damage_per_hit >= 0)
        || attacks[0].delivery.targets !== attacks[1].delivery.targets) return [];
    const [oldDamage, newDamage] = attacks, extraHits = newDamage.hit_count - oldDamage.hit_count;
    const perHitChanges = newDamage.damage_per_hit !== oldDamage.damage_per_hit;
    const drawChanges = Number.isSafeInteger(before.draw_cards) && before.draw_cards >= 0
      && Number.isSafeInteger(after.draw_cards) && after.draw_cards >= 0
      && before.draw_cards !== after.draw_cards;
    if (!extraHits && !perHitChanges && !drawChanges) return [];
    return [{ command: group.command, id: 'fixed-attack-upgrade-v1', fixedMechanics: [
      `Upgrade one ${action.card} copy: cost ${before.cost}->${after.cost}; base damage per hit ${oldDamage.damage_per_hit}->${newDamage.damage_per_hit}; hits per play ${oldDamage.hit_count}->${newDamage.hit_count}.`,
      `Before combat modifiers, base damage across those hits is ${oldDamage.damage_per_hit * oldDamage.hit_count}->${newDamage.damage_per_hit * newDamage.hit_count}. Resolved player Strength applies to each hit; changing the hit count changes the number of Strength applications.`,
      ...(drawChanges ? [`On successful play, unconditional draw request ${before.draw_cards}->${after.draw_cards} before NoDraw and Hand capacity. This changes access as well as the printed attack output, at the same ${after.cost} Energy payment.`] : []),
      `Targeting: ${newDamage.delivery.targets}.`
        + (newDamage.delivery.targets === 'random_enemy_per_hit'
          ? ' Each hit chooses among the current enemies. With one enemy, every hit goes to that enemy.' : ''),
      'The full supplied effects and lifecycle govern each play. Reach the upgraded copy and pay for it; Strength-dependent output also uses the owned source at its stated timing. Compare this one-copy improvement with other uses of the campfire.',
    ], choices: [
      ...(extraHits > 0 ? [{ id: '@extra-hit', text: `Assess ${extraHits} additional hit(s) per play, each receiving resolved Strength and the actual attack modifiers. Compare owned Strength access and payment with the other upgrades.` }] : []),
      ...(perHitChanges ? [{ id: '@base-damage', text: 'Assess the supplied base-damage change across the actual hit count, together with the other effects of this upgrade.' }] : []),
      ...(drawChanges ? [{ id: '@draw-increment', text: 'Compare the extra draws reaching useful owned functions at this unchanged payment with the other campfire improvements.' }] : []),
      { id: '@upgrade-opportunity', text: 'Compare this incremental attack improvement with the strongest other upgrade, recovery or key action using the campfire.' },
      { id: '@review-access', text: 'A specific source, access or payment fact is needed to rate this upgrade; record it through sts_feedback.' },
    ] }];
  });
}

function etherealUpgradeConditionBindings(observation, normalize) {
  const state = observation.position.observation.observation;
  return preparationCandidateGroups(observation, normalize).flatMap(group => {
    const action = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command)?.candidate?.action;
    if (action?.kind !== 'smith') return [];
    const evidence = observation.decision_evidence?.candidates?.find(row => (row.semantic_actions ?? [row.semantic_action])
      .some(command => command && normalize(command, observation) === group.command));
    const before = evidence?.before, after = evidence?.after;
    if (before?.card !== action.card || before.upgrades !== action.upgrades
        || after?.card !== action.card || after.upgrades !== before.upgrades + 1
        || before.ethereal !== true || after.ethereal !== false) return [];
    const pyramid = (state.relics ?? []).some(row => row.relic === 'RunicPyramid');
    return [{ command: group.command, id: 'ethereal-upgrade-window-v1', fixedMechanics: [
      `Upgrade one ${action.card} copy: printed Ethereal true->false. An unplayed base copy normally Exhausts at turn end; the upgraded copy uses the ordinary end-of-turn discard rule.`,
      pyramid ? 'Owned RunicPyramid skips ordinary hand discard, so the upgraded unplayed copy can stay in Hand under that relic rule. Other discard effects and Hand capacity still apply.'
        : 'With ordinary hand discard, the upgraded unplayed copy enters Discard. Using it on a later turn requires reaching it through subsequent draws or retrieval; hand persistence uses an actual retention source.',
      `Printed play cost ${before.cost}->${after.cost}; printed Exhaust after play ${before.exhausts}->${after.exhausts}. Combat powers, relics and instance modifiers apply their own destination rules.`,
      ...(Number.isSafeInteger(before.intangible_gain) && Number.isSafeInteger(after.intangible_gain)
        && after.intangible_gain > 0 ? [`Printed Intangible ${before.intangible_gain}->${after.intangible_gain}; the upgrade changes the unplayed copy's availability. Playing it still grants that supplied protected duration.`] : []),
      'Only the selected permanent copy is upgraded. Compare this timing flexibility with the other upgrades, removal, recovery or key action using the same campfire.',
    ], choices: [
      { id: '@preserve-for-redraw', text: 'Assess preserving an unplayed copy in the combat card cycle and reaching it again in time for a useful play.' },
      ...(pyramid ? [{ id: '@pyramid-hand', text: 'Assess delayed use in Hand under the already-owned Pyramid discard rule, with its Hand capacity and payment.' }] : []),
      { id: '@upgrade-opportunity', text: 'Compare this one-copy timing improvement with the strongest other preparation using this campfire action.' },
      { id: '@review-access', text: 'A specific lifecycle or timing fact is missing; record it through sts_feedback.' },
    ] }];
  });
}

function apparitionConditionBindings(observation, normalize) {
  const state = observation.position.observation.observation;
  const offers = apparitionPreparationOffers(observation, normalize);
  return offers.flatMap(offer => {
    const card = offer.current;
    const fixedMechanics = [
      `${offer.command} adds ${offer.count} permanent Apparition copies. Immediate HP ${offer.hpBefore}->${offer.hpAfter} (this event's current-HP cost ${offer.hpBefore - offer.hpAfter}); Max HP ${offer.maxHpBefore}->${offer.maxHpAfter} (permanent cap reduction ${offer.maxHpBefore - offer.maxHpAfter}). Current HP loss and Max HP reduction are separate quantities.`,
      `Each copy costs ${card.cost} Energy and grants ${card.intangible_gain} Intangible. While active, each incoming damage instance is capped at ${card.intangible_damage_cap} before Block; multi-hit attacks still have separate damage instances.`
        + (card.intangible_caps_direct_hp_loss_to_one ? ' Direct HP-loss requests are also capped at 1.' : ''),
      `Each played copy ${card.exhausts ? 'Exhausts for this combat' : 'follows its supplied played-card destination'}. Every new combat uses the permanent copies again.`
        + (card.ethereal ? ' An unplayed Ethereal copy Exhausts at turn end.' : '')
        + (offer.upgraded?.card === card.card && offer.upgraded.upgrades === card.upgrades + 1
          && card.ethereal && offer.upgraded.ethereal === false ? ' Its supplied upgrade removes Ethereal, allowing later redraw after ordinary discard.' : ''),
      'Several plays can stack protected duration. Useful coverage needs reaching, paying for and using the copies on appropriate turns. Block requests a finite amount; Weak and enemy Strength reduction reduce attacks according to their own hit rules. Intangible supplies the per-instance cap on its active turns, including while deploying the other functions.',
      `Combat and potion healing still use their own triggers and printed amounts, capped at the new ${offer.maxHpAfter} Max HP. Later event/combat HP payments use the smaller reserve.`,
    ];
    const decline = observation.position.actions.filter(row => {
      const action = row.candidate?.action, semantics = action?.public_semantics;
      return action?.kind === 'choose_event_option' && semantics?.hp_delta === 0 && semantics.max_hp_delta === 0
        && semantics.gold_delta_min === 0 && semantics.gold_delta_max === 0
        && semantics.structured_semantics?.effects?.length === 0 && !semantics.starts_combat;
    });
    return [{ command: offer.command, id: 'apparition-package-v1', fixedMechanics, choices: [
      { id: '@protected-turns', text: 'Compare the new low-cost protected turns across remaining combats and the time they provide to reach and deploy useful functions.' },
      { id: '@hp-cap-tradeoff', text: 'Weigh the one-time current HP change and permanent smaller reserve against repeated protected turns, using owned recovery and actual HP-payment demands.' },
      { id: '@access-timing', text: 'Weigh the added draw slots, payment, Ethereal timing and reachable upgrades together with using the protected turns.' },
      { id: '@review-access', text: 'A specific package, HP or timing fact is missing; record it through sts_feedback.' },
    ] }, ...decline.map(row => ({ command: normalize(row.semantic_action, observation), id: 'apparition-decline-v1',
      fixedMechanics: [`This option preserves current HP ${state.run.current_hp}/${state.run.max_hp} and the current deck. Its comparison is the offered permanent protection package.`, ...fixedMechanics], choices: [
        { id: '@retain-hp-cap', text: 'Compare retaining the current HP reserve and existing draw slots with the package\'s actual HP change and recurring protection.' },
        { id: '@forgone-protection', text: 'Leaving gives up the supplied additional protected turns across later combats. Compare their function with existing protection under the matched maintenance preference.' },
        { id: '@review-access', text: 'A specific HP or existing-coverage fact is missing; record it through sts_feedback.' },
      ] }))];
  });
}

function rewardRevealConditionBindings(observation, normalize) {
  if (observation.position.observation.observation.context?.kind !== 'reward') return [];
  return observation.position.actions.flatMap(row => row.candidate?.action?.kind === 'open_card_reward'
    ? [{ command: normalize(row.semantic_action, observation), id: 'reward-card-reveal-v1', fixedMechanics: [
      'This command opens the pending card reward while preserving current HP, Gold and owned cards. The revealed take/skip choices receive their own classification using the actual offered cards.',
    ], choices: [
      { id: '@card-choice-access', text: 'Assess access to the remaining card choices after settling useful inventory rewards. Their specific acquisition payoff is evaluated in the revealed card task.' },
      { id: '@review-access', text: 'A specific fact about this reward-opening action is missing; record it through sts_feedback.' },
    ] }] : []);
}

function temporaryStrengthConditionBindings(observation, normalize) {
  const state = observation.position.observation.observation;
  const profile = preparationDeckComparison(observation, normalize);
  const ownsRelic = relic => (state.relics ?? []).some(row => row.relic === relic);
  return preparationCandidateGroups(observation, normalize).flatMap(group => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command);
    const acquisition = preparationCardAcquisition(observation, row);
    const definition = acquisition?.mechanics;
    const amount = definition?.temporary_strength_gain;
    if (!definition || definition.strength_gain_delivery !== 'on_play_with_separate_lose_strength'
        || !Number.isSafeInteger(amount) || amount <= 0 || definition.strength_gain !== amount
        || definition.strength_loss_at_end_of_turn !== amount || !Number.isSafeInteger(definition.cost)) return [];
    return [{ command: group.command, id: 'temporary-strength-payoff-v1', fixedMechanics: [
      `${definition.name}: printed cost ${definition.cost}; request ${amount} Strength on play and a separate ${amount} Lose Strength at turn end.`,
      'Its attack payoff uses a shared-Hand sequence: play this card, then useful attacks before the scheduled Strength loss. Compare the extra damage obtained from that sequence with the extra draw occupied by this permanent copy.',
      ...(profile ? [attackInventoryText(profile)] : []),
      ...(ownsRelic('Boot') ? ['Owned Boot changes qualifying low unblocked Attack hits. Apply its supplied rule to both the baseline and Strength-boosted hit, then compare those final damage amounts. A multi-hit payoff uses that difference for each actual hit.'] : []),
      ...(ownsRelic('MutagenicStrength') ? ['Owned MutagenicStrength supplies opening-turn Strength with its own first-turn loss. Stacking the two sources for damage uses this card and useful attacks in that opening Hand.'] : []),
    ], choices: [
      { id: '@same-hand-payoff', text: `Assess useful owned attacks (${profile?.attackCopies ?? 'the supplied inventory'}) accessed and paid after this temporary Strength card during the same turn, and their actual extra damage.` },
      ...(ownsRelic('Boot') ? [{ id: '@damage-threshold', text: 'Compare per-hit added damage after the owned Boot rule at both Strength levels, with the target Block and other combat modifiers.' }] : []),
      ...(ownsRelic('MutagenicStrength') ? [{ id: '@opening-stack', text: 'Assess the extra opening burst when this card shares the opening Hand with useful payable attacks and the owned temporary opening Strength.' }] : []),
      { id: '@draw-slot-tradeoff', text: 'Compare that temporary damage increment with skipping to preserve access to owned protection, draw and enabling cards.' },
      { id: '@review-access', text: 'A specific same-Hand, damage or persistence fact is needed; record it through sts_feedback.' },
    ] }];
  });
}

function statusDrawConditionBindings(observation, normalize) {
  const profile = preparationDeckComparison(observation, normalize);
  if (!profile) return [];
  return preparationCandidateGroups(observation, normalize).flatMap(group => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command);
    const acquisition = preparationCardAcquisition(observation, row);
    if (!['Evolve', 'FireBreathing'].includes(acquisition?.card) || acquisition.mechanics?.card_type !== 'Power') return [];
    const triggers = profile.statusInventory.filter(card => card.type === 'Status' || acquisition.card === 'FireBreathing');
    return [{ command: group.command, id: 'status-draw-route-v1', fixedMechanics: [
      `${acquisition.mechanics.name}: pay ${acquisition.mechanics.cost} Energy to deploy, then apply its supplied effect on each qualifying draw.`,
      `Owned generated Status routes: ${renderStatusRoutes(profile.statusGeneration) || 'none'}.`,
      `Owned original qualifying draws: ${triggers.map(card => `${card.card}${card.ethereal ? ' (Ethereal after drawing)' : ' (ordinary later redraw)'}`).join('; ') || 'none'}.`,
      'Count draws after deployment along these routes and the supplied encounter Status routes. A generated Hand card occupies a slot immediately; its later ordinary discard, reshuffle and draw can supply the trigger. Access, hand capacity, draw prevention and exhausting fuel determine actual triggers.',
    ], choices: [
      { id: '@owned-status-route', realizations: profile.statusGeneration.length || triggers.length ? ['sequence'] : ['future'],
        text: 'Assess the deployed payoff from the listed owned originals and generated Status routes, with their access and lifecycle.' },
      { id: '@encounter-status-route', text: 'Assess the deployed payoff from the next relevant encounter\'s supplied Status sources, receiving piles and later draws.' },
      { id: '@deployment-cost', text: 'Compare those reachable draw triggers with this Power\'s deployment Energy, extra original draw slot and needed protection.' },
      { id: '@review-access', text: 'A specific missing trigger, source or pile-changing rule needs maintainer review.' },
    ] }];
  });
}

function eventCombatConditionBindings(observation, normalize) {
  const state = observation.position.observation.observation;
  if (state.context?.kind !== 'event') return [];
  return observation.position.actions.flatMap(row => {
    const action = row.candidate?.action, semantics = action?.public_semantics;
    const structured = semantics?.structured_semantics;
    if (action?.kind !== 'choose_event_option' || !semantics.starts_combat
        || structured?.transition?.kind !== 'start_combat') return [];
    const command = normalize(row.semantic_action, observation);
    const hp = state.run?.current_hp, max = state.run?.max_hp;
    const recent = state.public_history;
    return [{ command, id: 'event-combat-risk-v1', fixedMechanics: [
      'This command enters combat using the current resources. Gold, relics and other listed combat rewards are received after victory. The event option\'s immediate HP delta describes the event transaction; combat HP cost is determined by the fight.',
      ...(Number.isSafeInteger(hp) && Number.isSafeInteger(max) ? [`Combat-entry HP is ${hp}/${max}.`] : []),
      ...(Number.isSafeInteger(recent?.recent_combat_hp_loss) ? [
        `The last adopted combat settled with net HP loss ${recent.recent_combat_hp_loss}. This is an observed past cost; the new encounter needs its own survival assessment.`,
      ] : []),
      'Combat-end healing resolves when combat settles; the HP needed to reach that boundary is paid first. Escape has its own potion cost and forfeits victory rewards.',
    ], choices: [
      { id: '@combat-risk', text: 'Weigh surviving the encounter at the actual entry HP with owned early protection, payable damage and usable potions against taking the safe event alternative.' },
      { id: '@victory-reward', text: 'Assess the listed reward after accounting for the fight needed to receive it, and the safe alternative\'s resource cost.' },
      { id: '@review-access', text: 'A concrete fact about encounter survival or reward timing is needed to rate this fight; record it through sts_feedback.' },
    ] }];
  });
}

function bossRelicConditionBindings(observation, normalize) {
  const evidence = observation.decision_evidence;
  const state = observation.position.observation.observation;
  if (state.context?.kind !== 'boss_relic' || evidence?.kind !== 'boss_relic_options') return [];
  const cardKeys = ['card', 'upgrades', 'misc_value', 'base_damage_override', 'base_block_override', 'cost_modifier', 'count'];
  const sources = (evidence.owned_hp_loss_sources ?? []).filter(source => (state.cards ?? []).some(card =>
    cardKeys.every(key => Object.hasOwn(card, key) && source.card?.[key] === card[key]) && card.count > 0));
  const label = source => `${source.card.count}x ${source.card.card}${source.card.upgrades > 0 ? '+' : ''}`;
  return preparationBossRelicOffers(observation, normalize).flatMap(candidate => {
    const { command, definition } = candidate;
    const relic = definition.relic;
    const potionRule = definition.potion_acquisition_rule;
    if (potionRule?.prevents_new_potions === true && potionRule.owned_potions_remain_usable === true
        && Number.isSafeInteger(definition.energy_per_turn_delta)) {
      const held = Array.isArray(state.potions) ? state.potions.filter(slot => slot.potion != null) : undefined;
      return [{ command, id: 'relic-energy-potion-acquisition-v1', fixedMechanics: [
        `${relic} adds ${definition.energy_per_turn_delta} base Energy each turn and prevents obtaining new potions for the rest of this run.`,
        ...(held ? [`Current carried potions: ${held.map(slot => `P${slot.slot} ${slot.potion.potion}`).join(', ') || 'none'}. These already-owned bottles remain usable and are spent according to their normal use rules.`]
          : ['Already-owned potions remain usable under their normal use rules.']),
        'Later reward, shop and event potion acquisitions are blocked. The current carried reserve and the lost later refill opportunities are separate resources.',
        'Compare the recurring payment improvement for accessed useful cards with covering later difficult fights using the retained potions and owned or newly acquired card/relic protection. A small carried reserve leaves fewer consumable fallback uses after those bottles are spent.',
      ], choices: [
        { id: '@recurring-energy', text: 'Compare useful additional plays funded every turn, including the owned setup and protection combinations payable together.' },
        { id: '@retained-potions', text: 'Assess the finite already-carried bottles and the difficult fights where each use can help.' },
        { id: '@future-potion-tradeoff', text: 'Weigh recurring Energy against all subsequent potion refill opportunities and the protection still needed for later hard fights.' },
        { id: '@review-access', text: 'A specific payment, protection or retained-potion fact is needed; record it through sts_feedback.' },
      ] }];
    }
    const cap = definition.card_play_limit_per_turn;
    if (Number.isSafeInteger(cap) && cap > 0 && Number.isSafeInteger(definition.energy_per_turn_delta)) {
      const profile = preparationDeckComparison(observation, normalize);
      return [{ command, id: 'relic-energy-play-cap-v1', fixedMechanics: [
        `${relic} adds ${definition.energy_per_turn_delta} base Energy every turn and permits at most ${cap} card plays in that turn; the allowance resets each turn.`,
        'Draw Skills, zero-cost cards, setup Powers, defensive cards and attacks share that play allowance. A played multi-hit attack consumes one card play; each subsequent card consumes another. Drawing a card alone uses no card play.',
        ...(profile ? [`Owned on-play draw: ${profile.onPlayDraw.map(row => row.card).join(', ') || 'none'}. These draw-card plays also use the allowance before the accessed cards are played.`] : []),
        'The limit remains in later fights after useful draws, lower costs or additional Energy become available. Actual useful card sequences determine when it binds.',
      ], choices: [
        { id: '@recurring-energy', text: 'Compare the extra payment for owned useful plays when they are accessed together.' },
        { id: '@shared-play-cap', text: 'Compare total useful draw, setup, protection and damage plays fitting the shared limit, including the access development still needed.' },
        { id: '@few-play-output', text: 'Strong protection and output delivered in few actual plays can make the extra payment worth the permanent limit.' },
        { id: '@review-access', text: 'A specific payment or play-limit fact is missing; record it through sts_feedback.' },
      ] }];
    }
    const replacement = definition.starter_card_replacement, targets = candidate.starterCardsReplaced;
    if (replacement?.replacements_per_card === 1 && Array.isArray(targets)
        && Array.isArray(replacement.pool_rarities) && replacement.pool_rarities.length
        && Number.isSafeInteger(replacement.initial_upgrades)) {
      const count = targets.reduce((n, card) => n + card.count, 0);
      return [{ command, id: 'relic-starter-replacement-v1', fixedMechanics: [
        `${relic} replaces ${count} owned starter cards on pickup: ${targets.map(card => `${card.count}x ${card.card}${card.upgrades > 0 ? '+' : ''}`).join(', ') || 'none'}. This includes the listed upgraded copies. Each is exchanged for one random class card from the ${replacement.pool_rarities.join('/')} pools, initially at upgrade ${replacement.initial_upgrades}.`,
        'The deck keeps its card count; the other owned card instances remain. The exchange gives up the listed starter damage and Block. Each acquired replacement is permanent and used in later combats; identities are revealed after acquisition.',
        ...(replacement.applies_obtain_effects ? ['The replacements receive normal obtain effects, including applicable owned Egg upgrades.'] : []),
      ], choices: [
        { id: '@replace-starters', text: 'Compare replacing these recurring starter draws using the declared pool with the retained owned engine, access and protection.' },
        { id: '@replacement-tradeoff', text: 'Weigh the starter damage and Block given up, random replacement mix and unchanged deck size against the strongest offered recurring resource and its restriction.' },
        { id: '@review-access', text: 'A specific replacement-pool or retained-function fact is missing; record it through sts_feedback.' },
      ] }];
    }
    const draw = definition.card_draw_on_hp_loss;
    if (Number.isSafeInteger(draw?.cards_drawn_per_event) && draw.cards_drawn_per_event > 0
        && Number.isSafeInteger(draw.minimum_hp_lost_per_event) && draw.minimum_hp_lost_per_event > 0) {
      const onPlay = sources.filter(source => source.on_play?.base_hp_loss > 0);
      const recurring = sources.filter(source => source.hp_loss_each_turn_after_deployment > 0
        || source.end_of_turn_after_deployment?.hp_loss_per_application > 0);
      return [{ command, id: 'relic-hp-loss-draw-v1', fixedMechanics: [
        `${relic} is active from combat start. Each independent event with at least ${draw.minimum_hp_lost_per_event} actual HP lost requests ${draw.cards_drawn_per_event} card(s) at that event. Prevented HP loss supplies zero draws; draw prevention and Hand capacity apply.`,
        ...onPlay.map(source => `${label(source)}: its own play requests ${source.on_play.base_hp_loss} HP loss; that play's positive HP-loss event activates the relic immediately. This trigger's required card is ${source.card.card} itself.`),
        ...recurring.map(source => `${label(source)}: first play this Power, then its recurring events request ${source.end_of_turn_after_deployment?.hp_loss_per_application ?? source.hp_loss_each_turn_after_deployment} HP loss per application. Each actual loss activates the already active relic.`),
        'Enemy damage reaching HP also activates the relic at that hit. Each source provides its own independent event; useful draw is weighed against HP spent and its timing.',
      ], choices: [
        ...(onPlay.length ? [{ id: '@on-play-draw', text: 'Assess the immediate extra access supplied by the listed owned on-play HP payments, their normal payment and subsequent useful plays.' }] : []),
        ...(recurring.length ? [{ id: '@recurring-draw', text: 'Assess additional repeated access after deploying a listed recurring Power, together with its later HP payments and protection.' }] : []),
        { id: '@enemy-loss-draw', text: 'Assess draw timing after an enemy hit reaches HP and the HP needed to survive that hit and the following turns.' },
        { id: '@draw-tradeoff', text: 'Compare recurring useful access and trigger frequency with the strongest feasible alternative resources.' },
        { id: '@review-access', text: 'A specific trigger or draw-timing fact is needed; record it through sts_feedback.' },
      ] }];
    }
    if (definition.gold_gain_rule === 'prevent_gold_gain'
        && Number.isSafeInteger(definition.energy_per_turn_delta) && definition.energy_per_turn_delta > 0
        && Number.isSafeInteger(state.run?.gold) && state.run.gold >= 0) {
      return [{ command, id: 'relic-energy-gold-v1', fixedMechanics: [
        `${relic} supplies ${definition.energy_per_turn_delta} additional base Energy each turn.`,
        `Owned Gold ${state.run.gold} remains available for purchases and removal after acquisition. Subsequent Gold income is prevented, so later shop budgets use the remaining owned balance.`,
      ], choices: [
        { id: '@recurring-energy', text: 'Compare the useful extra plays funded every turn with owned access and normal card costs.' },
        { id: '@retained-gold', text: 'Assess improvements affordable from the retained current Gold on a connected preparation route.' },
        { id: '@future-gold-tradeoff', text: 'Compare the recurring combat resource with giving up subsequent income for later purchases and removal.' },
        { id: '@review-access', text: 'A specific payment or connected-shop fact is needed; record it through sts_feedback.' },
      ] }];
    }
    return [];
  });
}

function routeConditionBindings(observation, normalize) {
  const evidence = observation.decision_evidence;
  const state = observation.position.observation.observation;
  if (state.context?.kind !== 'map' || evidence?.kind !== 'route_paths') return [];
  const commands = new Set(preparationCandidates(observation, normalize));
  const range = value => Number.isSafeInteger(value?.min) && Number.isSafeInteger(value?.max)
    && value.min >= 0 && value.max >= value.min ? `${value.min}..${value.max}` : undefined;
  return evidence.candidates.flatMap(candidate => {
    const command = normalize(candidate.semantic_action, observation), semantics = candidate.route_semantics;
    const first = semantics?.first_elite, path = candidate.path?.first_elite;
    const beforeShop = range(first?.shops_before?.count_per_observed_path);
    const beforeCamp = range(first?.campfires_before?.count_per_observed_path);
    const totalShop = range(semantics?.shops?.count_per_observed_path);
    const totalCamp = range(semantics?.campfires?.count_per_observed_path);
    if (!commands.has(command) || !path || !beforeShop || !beforeCamp || !totalShop || !totalCamp
        || !first.observed_paths_with_first_elite) return [];
    return [{ command, id: 'route-preparation-window-v1', fixedMechanics: [
      ...(Number.isSafeInteger(state.run?.current_hp) && Number.isSafeInteger(state.run?.max_hp)
        ? [`Current HP ${state.run.current_hp}/${state.run.max_hp}.`] : []),
      ...(Number.isSafeInteger(state.public_history?.recent_combat_hp_loss)
        ? [`Last adopted combat net HP loss ${state.public_history.recent_combat_hp_loss}; this observed cost is a pressure signal for route preparation.`] : []),
      `Before the first Elite on the supplied paths that reach it: Shops ${beforeShop}; campfires ${beforeCamp}. Ordinary fights ${path.min_hallway_fights_before}..${path.max_hallway_fights_before}; question-mark rooms ${path.min_unknowns_before}..${path.max_unknowns_before}.`,
      `Whole published horizon: Shops ${totalShop}; campfires ${totalCamp}. Coverage ${candidate.path.coverage}. These whole-horizon opportunities include later development; first-Elite preparation uses the before-first-Elite counts and a connected path example.`,
    ], choices: [
      { id: '@first-preparation', text: 'Compare recovery, one upgrade or shopping actually available before the first Elite on one connected path. A shop occurring afterward supports later development.' },
      { id: '@early-development', text: 'Ordinary fight victories supply card choices and permitted Gold rewards before the first Elite. Apply owned Gold-gain restrictions to the spending budget. Compare these development opportunities with their HP cost; question-mark outcomes are learned on arrival.' },
      { id: '@later-development', text: 'Compare the whole-horizon resources and future branching flexibility after accounting for the first-Elite preparation window.' },
      { id: '@review-access', text: 'A specific connected-path or preparation-window fact is needed; record it through sts_feedback.' },
    ] }];
  });
}

function duplicateTargetFacts(observation, normalize) {
  const state = observation.position.observation.observation;
  if (state.context?.reason !== 'Duplicate' || state.context?.kind !== 'run_choice') return [];
  const targets = new Map(preparationSelectionTargets(observation, normalize).map(row => [row.command, row]));
  return preparationCandidateGroups(observation, normalize).flatMap(group => {
    const target = targets.get(group.command), current = target?.reading?.current;
    const definition = current?.definition, cost = current?.persistent_cost;
    if (!definition || definition.card !== target.card.card || definition.upgrades !== target.card.upgrades
        || !Number.isSafeInteger(cost)) return [];
    const count = (state.cards ?? []).filter(card => card.card === definition.card).reduce((n, card) => n + card.count, 0);
    if (!Number.isSafeInteger(count) || count < 1) return [];
    const fixedMechanics = [`${definition.card}: owned copies ${count} -> ${count + 1}; copying starts from the selected upgrade and persistent instance values, then applies obtain-time effects. One extra copy occupies one ordinary draw and Hand slot when drawn.`];
    const energy = state.base_energy_per_turn;
    if (cost >= 0) fixedMechanics.push(`Its normal play costs ${cost} Energy before combat modifiers.`
      + (Number.isSafeInteger(energy) && cost <= energy ? ` Base Energy ${energy} leaves ${energy - cost} after this play.` : '')
      + (definition.card_type === 'Power' ? ` Playing two such copies normally costs ${cost * 2} Energy in total, with separate Hand access and deployment times.` : ''));
    else if (cost === -1) fixedMechanics.push('Its normal X-cost play spends the Energy available at use; X bonuses and free-play effects follow their own rules. Strength changes attack damage per hit, while Energy grants and instance cost changes come from their named effect sources.');
    else if (cost === -2) fixedMechanics.push('The copied card has the unplayable printed cost; assess its draw and other effect rules.');
    const effects = definition.reviewed_effects;
    const strength = effects?.player_strength_gain;
    const delayed = strength?.delivery === 'each_player_turn_start_post_draw';
    if (delayed) fixedMechanics.push(`Each application adds 0 immediate Strength on deployment, then ${strength.amount} at each later player turn start after draw. After n later triggers, that application has contributed ${strength.amount} * n Strength. An already deployed copy continues its own schedule. Additional deployment starts its own later schedule.`);
    const reduction = effects?.enemy_strength_reduction;
    if (reduction?.restoration === 'no_scheduled_restoration') fixedMechanics.push(`Each successful application reduces one enemy's Strength by ${reduction.amount}, including from zero into negative Strength. The reduction affects every later attack hit until an effect changes or clears it. Across h hits it removes ${reduction.amount} * h from base attack damage before multipliers and the zero-damage floor. Artifact can prevent the application. The card's Exhaust affects its reuse; the applied reduction continues.`);
    if (effects?.base_block_delivery === 'once_on_play') fixedMechanics.push(`The copy requests ${current.persistent_block} Block on play before combat modifiers.`);
    return [{ command: group.command, fixedMechanics,
      detailed: delayed || reduction?.restoration === 'no_scheduled_restoration' }];
  });
}

function duplicateConditionBindings(observation, normalize) {
  return duplicateTargetFacts(observation, normalize).filter(row => row.detailed).map(({ command, fixedMechanics }) => ({
    command, id: 'permanent-copy-increment-v1', fixedMechanics, choices: [
      { id: '@first-access', text: 'The extra copy helps reach the first useful play earlier across ordinary draws; assess that consistency gain against the extra draw occupied after the first application is active.' },
      { id: '@extra-use', text: 'An actual additional play supplies the copied effect again. Compare its payment, deployment time and useful duration with what the existing copies already supply.' },
      { id: '@setup-tradeoff', text: 'Compare protecting and using the existing engine through the next hard fight with another setup card. A full-base-Energy delayed application needs an owned way to cover its deployment and subsequent turns; cheap sustained mitigation can protect that setup.' },
      { id: '@review-access', text: 'A specific fact about the copied effect or its access/payment is needed; record it through sts_feedback.' },
    ],
  }));
}

// Derive payment thresholds from simulator facts, including grouped smith
// deltas. This is a base-Energy comparison, not an estimate of combat strength.
function preparationEnergyBindings(observation, normalize) {
  const state = observation.position.observation.observation;
  const energy = state.base_energy_per_turn;
  if (!Number.isSafeInteger(energy) || energy < 0) return [];
  return preparationCandidateGroups(observation, normalize).flatMap(group => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command);
    const action = row?.candidate?.action, acquisition = preparationCardAcquisition(observation, row);
    if (action?.kind !== 'smith' && !acquisition) return [];
    const evidence = observation.decision_evidence?.candidates?.find(row =>
      [row.semantic_action, ...(row.semantic_actions ?? [])].filter(Boolean).some(command => normalize(command, observation) === group.command));
    const before = action.kind === 'smith' ? evidence?.before : acquisition.mechanics;
    const after = action.kind === 'smith' ? evidence?.after : undefined;
    if (before?.card !== (action.kind === 'smith' ? action.card : acquisition.card) || !Number.isSafeInteger(before.cost) || before.cost < 0
        || (action.kind === 'smith' && (after?.card !== action.card || !Number.isSafeInteger(after.cost) || after.cost < 0))) return [];
    const crosses = before.cost > energy && after?.cost <= energy;
    if (before.cost <= energy && !(after && after.cost > energy)) return [];
    const cost = after?.cost ?? before.cost;
    const mechanics = [`Base Energy per turn is ${energy}. ${before.name ?? action.card} printed cost is ${before.cost}`
      + (after ? ` before this upgrade and ${cost} afterward.` : `; payment above base Energy needs ${cost - energy} additional Energy or an applicable cost reduction.`)];
    if (crosses) mechanics.push(`This upgrade crosses the base-Energy payment threshold: the upgraded card leaves ${energy - cost} base Energy after payment, before other plays or modifiers.`);
    const hpReduction = (after ?? before).cost_change_on_hp_loss;
    if (Number.isSafeInteger(hpReduction) && hpReduction < 0) {
      const events = Math.max(0, Math.ceil((cost - energy) / -hpReduction));
      mechanics.push(`Each positive HP-loss event lowers this card instance's cost by ${-hpReduction} for this combat. Paying from base Energy alone needs ${events} such events at the ${cost}-Energy printed cost. Fully blocked or prevented damage supplies zero HP-loss events. Intentional self-loss and enemy hits have their actual HP costs.`);
    }
    return [{ command: group.command, id: 'base-energy-payment-v1', fixedMechanics: mechanics, choices: [
      { id: '@payment-access', text: 'Compare first-draw payment, available extra Energy or deployed cost reduction, and the Energy left for the other needed plays. Existing setup is sequence; an additional acquisition is future.' },
      ...(crosses ? [{ id: '@upgrade-payment', text: 'The upgrade makes payment fit base Energy before any HP-loss event or extra-Energy setup; assess the earlier playable output and its remaining Energy together.' }] : []),
      ...(Number.isSafeInteger(hpReduction) && hpReduction < 0 ? [{ id: '@hp-loss-payment', text: 'HP-loss cost reduction requires the actual events before payment. Assess the time to those events, their HP cost and later access to the discounted card.' }] : []),
      { id: '@payment-tradeoff', text: 'Compare this output and its payment conditions with the strongest feasible alternative and the resource retained.' },
      { id: '@review-access', text: 'A specific access or payment fact is needed to assess this candidate; record it through sts_feedback.' },
    ] }];
  });
}

function armamentsConditionBindings(observation, normalize) {
  const state = observation.position.observation.observation;
  const owns = id => (state.cards ?? []).some(card => card.card === id && card.count > 0);
  if (!(state.cards ?? []).some(card => card.card === 'LimitBreak' && card.count > 0 && card.upgrades === 0)) return [];
  const corruption = owns('Corruption'), recovery = owns('Exhume');
  const spoon = (state.relics ?? []).some(row => row.relic === 'StrangeSpoon');
  return preparationCandidateGroups(observation, normalize).flatMap(group => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command);
    const acquisition = preparationCardAcquisition(observation, row);
    if (acquisition?.card !== 'Armaments') return [];
    const effect = acquisition.mechanics;
    if (effect?.card !== 'Armaments' || !Number.isSafeInteger(effect.cost) || !Number.isSafeInteger(effect.block)
        || !['selected_card_in_hand', 'whole_hand'].includes(effect.combat_upgrade_scope)) return [];
    const fixedMechanics = [
      `Adds one permanent ${effect.name} copy with printed cost ${effect.cost} and ${effect.block} Block; its combat upgrade affects ${effect.combat_upgrade_scope === 'whole_hand' ? 'the current Hand' : 'one selected card in the current Hand'}. The added copy occupies a future draw and its play needs access and payment.`,
      'Printed LimitBreak doubles current Strength and Exhausts; LimitBreak+ normally enters Discard for later redraws. The upgrade changes destination, while the doubling effect uses the same current Strength.',
    ];
    if (corruption) fixedMechanics.push('After Corruption deployment, Armaments still applies its upgrade effect before it is directed to Exhaust; Armaments and both LimitBreak versions cost 0 and are directed to Exhaust after play. Ordinary LimitBreak+ Draw/Discard cycling keeps Corruption awaiting deployment throughout the repeated uses.');
    if (recovery) fixedMechanics.push('One ordinary Exhume play returns one eligible card other than Exhume from Exhaust to Hand when the effect resolves with room in Hand, then Exhume itself is directed to Exhaust. Returning base LimitBreak already supplies another use.'
      + (corruption ? ' With Corruption deployed, both returned LimitBreak versions are directed to Exhaust again; this recovery benefit is shared by both versions.' : ' Compare that existing recovery with the extra ordinary redraws supplied by upgrading.'));
    if (spoon) fixedMechanics.push('StrangeSpoon gives a played non-Power card directed to Exhaust a 50% chance to enter Discard instead. A saved Skill can be redrawn under Corruption; the saving is probabilistic.');
    return [{ command: group.command, id: 'armaments-limitbreak-v1', fixedMechanics, choices: [
      { id: '@limit-redraw', text: 'Upgrade the unupgraded LimitBreak sharing Hand before its use, then double positive Strength on later redraws while protecting those turns.'
        + (corruption ? ' Keep Corruption awaiting deployment during this ordinary redraw sequence.' : '') },
      { id: '@block-access', text: 'Compare the supplied Block and same-Hand upgrade opportunity with owned protection, Energy and the extra draw occupied by this permanent copy.' },
      { id: '@review-access', text: 'A specific fact about shared-Hand access or setup protection is needed to rate the upgrade sequence; record that fact through sts_feedback.' },
    ] }];
  });
}

// Expert preferences supply default tradeoffs; shared Rust readings continue
// to own card effects and legal actions. Classification stays with the player.
export function preparationClassificationContext(observation, normalize, draft, locale = 'en') {
  const potionScope = shopPotionReplacementScope(observation, normalize);
  if (potionScope) return renderShopPotionComparison(potionScope, draft);
  const spendingScope = shopSpendingScope(observation, normalize);
  if (spendingScope) return renderShopSpendingComparison(observation, spendingScope, preparationCandidateGroups(observation, normalize), draft);
  const state = observation.position.observation.observation;
  const cards = state.cards ?? [];
  const owns = (id, upgrades) => cards.some(card => card.card === id && card.count > 0
    && (upgrades === undefined || card.upgrades === upgrades));
  const allGroups = preparationCandidateGroups(observation, normalize);
  const accepted = sameGameContextSource(draft?.source, observation) ? draft.rows : [];
  const groups = allGroups.filter(group => !accepted.some(row => row.command === group.command));
  const bindings = preparationConditionBindings(observation, normalize, locale);
  const copyFacts = new Map(duplicateTargetFacts(observation, normalize).filter(row => !row.detailed)
    .map(row => [row.command, row.fixedMechanics]));
  const armamentsBound = bindings.some(binding => binding.id === 'armaments-limitbreak-v1');
  const offered = id => observation.position.actions.some(row => row.candidate?.action?.card === id
    || preparationCardAcquisition(observation, row)?.card === id);
  const checks = [
    'Access and protection: a normal draw reaches a card before payment. A Power card supplies triggered draw after that card is played and its trigger occurs. An acquired passive relic is active from combat start and responds to each of its supplied triggers independently. A deployed Power can add another trigger source for that relic. Persistent mitigation protects later hits; an ordinary played card needs its own access and payment.',
    'Payment: track Gold for acquisition and Energy for combat play separately. Compare printed cost with base Energy per turn and owned Energy sources or cost reductions. An HP-loss cost reduction becomes available after the required actual HP-loss events; include that setup in the realization condition.',
  ];
  if (state.context?.kind === 'event') checks.push(
    'Event: compare the supplied resource changes and delivery method. Random rewards are evaluated before their identities are revealed; use the declared pool and known cost. Reward-screen potions can be inspected before choosing which bottles fit the slots. Gold retained plus income before a later shop supplies that shop\'s budget.');
  const selection = preparationSelectionFamily(observation)
    ?? (state.context?.kind === 'run_choice' ? state.context : undefined);
  if (selection) checks.push(
    `Card selection: ${selection.reason}; choose ${selection.min_choices}..${selection.max_choices} distinct targets. Classify each supplied target's incremental effect. For several targets, compare their combined change and submit one act select: command joining their exact card tokens with commas. The table rates targets individually.`,
    ...(selection.reason === 'Purge' ? ['Selected cards are permanently removed; compare first draw, later redraws and the functions given up.'] : []),
    ...(selection.reason === 'Transform' ? ['Selected cards are replaced by the supplied random transformation pool. Compare the functions given up and the declared replacement distribution.'] : []),
  );
  if (selection?.reason === 'Duplicate' || observation.position.actions.some(row => row.candidate?.action?.public_semantics
      ?.structured_semantics?.effects?.some(effect => effect.effect === 'duplicate_card'))) checks.push(defaultInstructions.text('duplication'));
  if (state.context?.kind === 'card_reward') checks.push(
    'Card reward: taking adds a permanent copy to the current deck; skipping preserves its current size. Both choices retain the same Gold. Assess the added card\'s function and draw cost alongside the existing copies.');
  if (state.context?.kind === 'boss_relic') checks.push(
    'Boss relic: compare once-on-pickup resources with recurring resources through the upcoming fights. For HP-loss draw, evaluate each owned on-play self-loss source, deployed recurring self-loss source and enemy HP-loss event separately; use the actual HP cost and the timing of those draws.');
  if (state.context?.kind === 'map') checks.push(
    'Route: compare each connected path\'s preparation before its first hard fight and its later development. HP on arrival depends on intervening fights and events. A shop\'s useful capacity depends on Gold; a campfire offers recovery or one upgrade. Use the displayed connected path to relate these opportunities. Missing Emerald can be collected from a later Act\'s burning elite.');
  if (state.run?.pending_act_entry) checks.push(
    `Act-entry preparation: this Act's Boss is completed. Current choices prepare for Act ${state.run.pending_act_entry}; Act-entry HP recovery follows the supplied transition rule. Evaluate setup protection, payable output and draw access for that upcoming Act.`);
  if (state.run?.act === 1 && !state.run?.pending_act_entry) checks.push(
    'Act 1 development: ordinary fights supply a card-choice opportunity and Gold after victory. Compare those development opportunities and their expected HP cost before the first elite with recovery, upgrades and shop spending on the same connected route. Question-mark outcomes remain unrevealed. Early damage depends on cards payable when drawn; a cost-lowering upgrade can make a previously gated attack available before taking damage.');
  if (state.public_map?.boss === 'TimeEater' || state.run?.act === 4) checks.push(
    'Played-card triggers: Time Warp and Beat of Death count actual card uses during combat. Permanent deck size affects draw access; the number of plays depends on which drawn cards are used and repeated. Assess a card\'s Block, damage and other effects together with the triggers from its actual uses.');
  if (owns('DemonForm') && owns('LimitBreak')) checks.push(
    'Strength plan: DemonForm supplies repeated Strength after deployment; LimitBreak doubles the Strength available when used. Protect the deployment turn and reach the damage cards afterward. Extra scaling competes with access and protection in this owned plan.');
  if (offered('Disarm')) checks.push(
    'Disarm: reducing Strength also works on a target starting at zero, taking it below zero. Its upgrade adds one point of reduction to every affected later attack hit until a debuff-clear effect removes negative Strength. Compare affected hits, duration and Artifact coverage.');
  if (offered('Armaments') && !armamentsBound) checks.push(
    'Armaments: compare actual unupgraded owned targets and their upgrade deltas. The target must share Hand with Armaments before its use; later campfire upgrades change the purchase\'s marginal value. Baseline Block plus an upgrade opportunity is compared with occupying another draw.');
  if (offered('Armaments') && owns('LimitBreak', 0) && !owns('Corruption') && !armamentsBound) checks.push(
    'Armaments with LimitBreak: upgrading LimitBreak before use allows it to return to the draw/discard cycle for repeated doubling. This needs shared Hand access, a positive Strength source, later redraws and protection while cycling.');
  if (owns('Corruption') && owns('LimitBreak') && !armamentsBound) checks.push(
    'Corruption with LimitBreak: compare deployment schedules. Before Corruption deployment, played LimitBreak normally Exhausts and played LimitBreak+ normally enters Discard for later redraws. Repeated ordinary LimitBreak+ uses cycle through Discard while Corruption awaits deployment. After Corruption deployment, both versions cost 0 and Exhaust when played. '
    + (offered('Armaments') ? 'Armaments changes the printed upgrade state; its upgrade payoff depends on the deployment schedule that preserves redraws. ' : '')
    + 'Evaluate the supplied recovery effects and their available uses separately from ordinary Draw/Discard cycling.');
  if (owns('Exhume') && owns('LimitBreak') && !armamentsBound) checks.push(
    'Exhume with LimitBreak: its printed play returns one eligible card other than Exhume from Exhaust to Hand, then Exhume itself Exhausts. Returning the unupgraded LimitBreak already supplies another use. '
    + (owns('Corruption') ? 'With Corruption deployed, the returned base or upgraded LimitBreak both cost 0 and Exhaust after that play. ' : '')
    + 'Compare this recovery use with the additional destination change supplied by upgrading LimitBreak.');
  if (offered('Evolve')) checks.push(
    'Evolve: use the supplied encounter links to identify Status sources and the pile receiving them. Deployment before the Status draw supplies extra draw; assess early setup and later cycles. Compare its deployment cost and access with the draw it unlocks in those encounters.');
  if (offered('Rupture') || owns('Rupture')) checks.push(
    'Rupture payoff: count actual positive self HP-loss events after deployment, multiplied by the active Rupture Strength per event. Use the owned source counts and their repetition rules in the mechanism links. A single on-play source supplies one event when that copy is used; later gains need its redraw, copying or recovery. Assess the resulting Strength, deployment cost and access through the next hard fight.');
  if (offered('DualWield') || owns('DualWield')) checks.push(
    'DualWield payoff: name the specific useful owned Attack or Power target. This target must share Hand with DualWield before copying, and the resulting plays need Hand room and total Energy for both the copying card and the copied uses. Assess that target\'s incremental output or triggers, then compare the whole sequence with keeping the draw slot.');
  if (offered('Clash')) checks.push(
    'Clash: its all-Attack Hand requirement interacts with the owned Skills, Powers, Curses and encounter Statuses. Evaluate the available ways to clear that Hand and the cost of draws where the condition is unmet.');
  const bindingByCommand = new Map(bindings.map(binding => [binding.command, binding]));
  const guidance = preparationExpertGuidance(observation, normalize);
  const adviceByCommand = new Map((guidance?.candidates ?? []).map(row => [row.command, row]));
  return [...(observation.declinedRewardItems?.length ? [
    `Completed card decisions for this inventory: reward items ${observation.declinedRewardItems.map(i => i + 1).join(', ')} were declined. The candidates below are the remaining preparation work.`
  ] : []), `Current classification candidates: ${groups.length} pending output rows of ${allGroups.length}. Fill one row per command below.`,
    `Pending commands: ${groups.map(group => group.command).join('; ')}.`,
    ...(sameGameContextSource(draft?.source, observation) && draft.pendingChoice ? [`Saved choice: ${draft.pendingChoice}. Complete the remaining rows through classify.`] : []),
    ...(accepted.length ? ['Accepted classification rows (recorded):', ...allGroups.flatMap(group => {
      const row = accepted.find(row => row.command === group.command);
      return row ? [`  ${row.command} | ${row.category} | ${row.realization} | ${row.conditionIds.join(',')}`] : [];
    })] : []),
    ...(sameGameContextSource(draft?.source, observation) && draft.issues?.length ? ['Last submission corrections:', ...draft.issues.map(issue => `  ${issue.message}`)] : []),
    'Row format: command | high/medium/low/review | direct/sequence/future/review | @reference[, @reference]. Use the references beside that command.',
    ...(groups.some(group => group.members.length > 1)
      ? ['For grouped copies, classify one copy on its command line. The host expands the row to the matching legal commands.'] : []),
    renderDeckComparison(preparationDeckComparison(observation, normalize, locale), { candidateCommands: allGroups.map(group => group.command), locale }),
    renderExpertGuidance(guidance, { includeCandidates: false }),
    renderShopBudget(observation.shopBudget, observation.shopPlan),
    ...(observation.shopRemovalPolicy ? [
      `Removal target strategy (maintainer preference): ${observation.shopRemovalPolicy.preference}`,
      ...observation.shopRemovalPolicy.conditions.map(text => `  Checked: ${text}`),
    ] : []),
    'Applicable mechanism and preparation checks:', ...checks.map(check => `  ${check}`),
    ...groups.flatMap(group => {
      const binding = bindingByCommand.get(group.command);
      return [
      `Condition references for ${binding.command}:`,
      ...(group.members.length > 1 ? [`  Shared one-copy evaluation: ${group.members.length} copies with identical public attributes.`] : []),
      ...(copyFacts.has(group.command) ? [`  ${copyFacts.get(group.command).join(' ')}`] : []),
      ...(observation.candidateReadings?.[binding.command]
        ? [observation.candidateReadings[binding.command].view] : []),
      ...[renderCandidateExpertGuidance(adviceByCommand.get(binding.command))].filter(Boolean).map(text => `  ${text}`),
      ...binding.fixedMechanics.map(text => `  Fixed mechanics: ${text}`),
      ...binding.choices.map(choice => `  ${choice.id}${choice.realizations ? ` [realization: ${choice.realizations.join('/')}]` : ''}: ${choice.text}`),
      ];
    })].join('\n');
}

export function parsePreparationClassification(basis, observation, normalize = command => command, locale = 'en') {
  const collected = collectPreparationClassification(basis, observation, normalize, undefined, undefined, locale);
  if (collected.issues.length) throw new Error(collected.issues.map(issue => issue.message).join('\n'));
  if (!collected.complete) throw new Error(`Classify remaining candidates: ${collected.pendingCommands.join(', ')}`);
  return collected.rows;
}

// Independent row validation preserves usable assessments from the same source.
// Drafts are host-owned; model prose stays in the raw submission journal only.
export function collectPreparationClassification(basis, observation, normalize = command => command, previous, submission = {}, locale = 'en') {
  const potionScope = shopPotionReplacementScope(observation, normalize);
  const spendingScope = shopSpendingScope(observation, normalize);
  const simpleScope = potionScope ?? spendingScope;
  if (typeof basis !== 'string' || !basis.trim()) throw new Error(`Supply basis. ${simpleScope ? 'One row per supplied command: command | high/medium/low/review.' : PREPARATION_BASIS}`);
  const groups = preparationCandidateGroups(observation, normalize), rows = new Map();
  const bindings = new Map(preparationConditionBindings(observation, normalize, locale).map(binding => [binding.command, binding]));
  const membership = new Map(groups.flatMap(group => group.members.map(command => [command, group])));
  if (sameGameContextSource(previous?.source, observation)) {
    for (const group of groups) {
      const row = previous.rows.find(row => row.command === group.command);
      if (row) rows.set(group.command, row);
    }
  }
  const text = basis.trim().replace(/^```(?:text)?\s*\r?\n([\s\S]*?)\r?\n```$/, '$1');
  const lines = text.split(/\r?\n/).filter(line => line.trim());
  const occurrences = new Map(), suppliedCommands = lines.map(line => {
    try {
      const group = membership.get(normalize(line.split('|')[0].trim(), observation));
      if (group) occurrences.set(group.command, (occurrences.get(group.command) ?? 0) + 1);
      return group?.command;
    } catch { return undefined; }
  });
  const issues = [], acceptedCommands = [];
  for (const [index, line] of lines.entries()) {
    try {
      const group = groups.find(group => group.command === suppliedCommands[index]);
      if (group && rows.has(group.command)) throw new Error(`Already accepted: ${group.command}. Continue with the pending rows.`);
      if (group && occurrences.get(group.command) > 1) {
        throw new Error(`Duplicate classification for ${group.command}. Its ${group.members.length} matching legal command(s) share one output row using ${group.command}.`);
      }
      // A clearly separated trailing explanation does not change the emitted
      // recommendation. Bind that grade to native facts; retain prose only in
      // the raw journal, avoiding a fresh decision to repair presentation.
      const simple = simpleScope && /^\s*([^|]+?)\s*\|\s*(high|medium|low|review)(?:[ \t]+[-–—][ \t]+[^|\r\n]+)?\s*$/.exec(line);
      const potion = potionScope?.candidates.find(row => row.command === group?.command);
      const spend = spendingScope?.candidates.find(row => row.command === group?.command);
      const match = simpleScope ? (simple && (potion || spend) ? [simple[0], simple[1], simple[2],
        simple[2] === 'review' ? 'review' : potion?.realization ?? null,
        simple[2] === 'review' ? '@review-access' : potion?.reference ?? '@shop-tradeoff'] : null)
        : /^\s*([^|]+?)\s*\|\s*(high|medium|low|review)\s*\|\s*(direct|sequence|future|review)\s*\|\s*(\S.*)\s*$/.exec(line);
      if (!match) throw new Error(`Classification row ${index + 1} format: ${simpleScope ? 'command | high/medium/low/review' : PREPARATION_BASIS}`);
      const [, , category, realization, reason] = match;
      if (!group) throw new Error(`Classification row ${index + 1} requires one of the supplied pending commands.`);
      const binding = bindings.get(group.command);
      const conditionIds = reason.split(',').map(id => id.trim());
      const conditions = conditionIds.map(id => binding.choices.find(choice => choice.id === id));
      if (conditions.some(choice => !choice) || new Set(conditionIds).size !== conditionIds.length) {
        const extraColumns = reason.split('|');
        const trailingIds = extraColumns.length === 2 ? extraColumns[1].split(',').map(id => id.trim()) : [];
        const replacement = trailingIds.length && new Set(trailingIds).size === trailingIds.length
          && trailingIds.every(id => binding.choices.some(choice => choice.id === id))
          ? ` Replacement row, preserving your ratings and references: ${group.command} | ${category} | ${realization} | ${trailingIds.join(',')}` : '';
        throw new Error(`Use supplied condition references for ${group.command}. Fourth column: comma-separated reference IDs. Row format: ${group.command} | ${category} | ${realization} | @reference[, @reference]. Supplied IDs: ${binding.choices.map(choice => choice.id).join(', ')}.${replacement}`);
      }
      if ((category === 'review' || realization === 'review') !== conditionIds.includes('@review-access')) {
        throw new Error(`For ${group.command}, pair @review-access with a review dimension; assessed dimensions use the benefit/tradeoff references.`);
      }
      // Validate typed benefit paths without changing the player's ratings.
      // A standalone Block play cannot be classified as needing acquisition.
      const paths = conditions.filter(choice => choice.realizations);
      if (realization !== 'review' && paths.some(choice => !choice.realizations.includes(realization))) {
        throw new Error(`For ${group.command}, select benefit references matching realization: ${paths.map(choice => `${choice.id} uses ${choice.realizations.join('/')}`).join('; ')}. Record the main benefit path and matching realization.`);
      }
      rows.set(group.command, { category, realization, reason: [...conditions.map(choice => choice.text), ...binding.fixedMechanics].join(' '),
        basisKind: potionScope ? 'host-bound-potion-tradeoff' : spendingScope ? 'host-bound-shop-tradeoff' : 'condition-references', bindingId: binding.id, conditionIds,
        conditions, fixedMechanics: binding.fixedMechanics });
      acceptedCommands.push(group.command);
    } catch (error) {
      issues.push({ line: index + 1, ...(suppliedCommands[index] ? { command: suppliedCommands[index] } : {}), message: error.message });
    }
  }
  const pendingCommands = groups.filter(group => !rows.has(group.command)).map(group => group.command);
  let pendingChoice = sameGameContextSource(previous?.source, observation) ? previous.pendingChoice : undefined;
  let pendingComparison = pendingChoice ? previous.pendingComparison : undefined;
  let choiceIssue = submission.choiceIssue;
  if (!choiceIssue && submission.choice?.trim()) {
    try {
      const selected = preparationChoiceMembers(submission.choice, observation, normalize).command;
      if (pendingChoice && (selected !== pendingChoice
          || (submission.comparison !== undefined && submission.comparison !== pendingComparison))) {
        throw new Error(`Saved choice: ${pendingChoice}. Classification corrections retain this choice and its comparison. After the table is complete, submit command choose with the intended choice to change it.`);
      }
      pendingChoice ??= selected;
      pendingComparison ??= submission.comparison;
    } catch (error) {
      choiceIssue = error.message;
    }
  }
  if (choiceIssue) issues.push({ field: 'choice', message: choiceIssue });
  return { source: gameContextSource(observation), complete: pendingCommands.length === 0,
    rows: preparationCandidates(observation, normalize).flatMap(command => {
      const row = rows.get(membership.get(command).command);
      return row ? [{ ...row, command }] : [];
    }), acceptedCommands, pendingCommands, issues,
    ...(pendingChoice ? { pendingChoice, ...(pendingComparison !== undefined ? { pendingComparison } : {}) } : {}),
    ...(choiceIssue ? { choiceIssue } : {}) };
}

export function preparationClassificationReceipt(draft) {
  return [`Classification rows recorded: ${draft.acceptedCommands.length} newly accepted; ${draft.rows.length} covered legal commands.`,
    ...draft.issues.map(issue => `Correction: ${issue.message}`),
    ...(draft.pendingChoice ? [`Saved choice: ${draft.pendingChoice}.`] : []),
    draft.complete ? 'Complete current table recorded.' : `Classify remaining candidates: ${draft.pendingCommands.join(', ')}`].join('\n');
}

export function preparationRecordedTable(classification, observation, normalize, locale = 'en') {
  const potionScope = shopPotionReplacementScope(observation, normalize);
  if (potionScope) return ['Recorded shop bottle recommendations:', ...potionScope.candidates.map(candidate => {
    const row = classification.rows.find(row => row.command === candidate.command);
    return `  ${row.command} | ${row.category}\n    ${candidate.fact}`;
  }), 'Submit command choose and the selected complete command in choice.'].join('\n');
  const spendingScope = shopSpendingScope(observation, normalize);
  if (spendingScope) return renderShopSpendingComparison(observation, spendingScope,
    preparationCandidateGroups(observation, normalize), classification);
  const representatives = preparationCandidateGroups(observation, normalize);
  const rows = representatives.map(group => classification.rows.find(row => row.command === group.command));
  // Preserve every recorded condition and fact, including old reason-only
  // rows. Exact shared text is rendered once with its owning commands.
  const notes = new Map();
  for (const row of rows) {
    const texts = row.conditions && row.fixedMechanics
      ? [...row.conditions.map(condition => condition.text), ...row.fixedMechanics] : [row.reason];
    for (const text of new Set(texts.filter(Boolean))) {
      const commands = notes.get(text) ?? [];
      commands.push(row.command);
      notes.set(text, commands);
    }
  }
  const shared = new Map();
  for (const [text, commands] of notes) {
    const key = commands.join('; ');
    const texts = shared.get(key) ?? [];
    texts.push(text);
    shared.set(key, texts);
  }
  return ['Recorded classifications: benefit | realization | basis',
    ...(classification.pendingChoice ? [`Saved choice: ${classification.pendingChoice}.`] : []),
    'This table records your classify submission for the current decision.', ...representatives.map(group => {
    const row = classification.rows.find(row => row.command === group.command);
    return `  ${row.command} | ${row.category} | ${row.realization} | ${row.conditionIds?.join(',') ?? 'recorded basis below'}`;
  }), renderDeckComparison(preparationDeckComparison(observation, normalize, locale), { candidateCommands: representatives.map(group => group.command), locale }),
  renderExpertGuidance(preparationExpertGuidance(observation, normalize)),
  'Recorded basis facts and conditions:', ...[...shared].flatMap(([commands, texts]) => [
    `  Applies to ${commands}:`, ...texts.map(text => `    ${text}`),
  ]),
  renderShopBudget(observation.shopBudget, observation.shopPlan),
  ...(observation.position?.observation?.observation?.context?.kind === 'map'
    ? ['Map adoption: choose from the highest-rated assessed routes. Close tradeoffs may share a rating.'] : []),
  ...(preparationSelectionFamily(observation) ? [
    `Selection adoption: choose ${preparationSelectionFamily(observation).min_choices}..${preparationSelectionFamily(observation).max_choices} distinct card tokens in one act select: command. Several targets use comma-separated tokens; each selected target binds its recorded row.`,
  ] : representatives.length === 1 ? [`Sole candidate adoption: command ${representatives[0].command}. The host supplies its sole recorded row as basis.`] : [])].join('\n');
}

// One immutable task root, independent of model conclusions and message history.
// Preparation tools can only read or adopt gameplay. The controller head also
// changes on an adopted choice in an evidence workspace whose state revision
// stays at the original external checkpoint.
export function preparationTaskRoot(observation) {
  const task = gameDecisionTask(observation);
  if (!families.has(task.family)) throw new Error('A preparation task starts at a strategic decision: shop, campfire, event, reward, card choice, map or run transition.');
  const gold = observation.position?.observation?.observation?.run?.gold;
  return { source: gameContextSource(observation), task,
    ...(Number.isSafeInteger(gold) ? { startingGold: gold } : {}) };
}

export function preparationTaskProgress(root, observation) {
  const source = gameContextSource(observation);
  const task = gameDecisionTask(observation, root.task);
  if (source.session_id !== root.source.session_id || source.state_revision < root.source.state_revision) {
    throw new Error('Preparation task root differs from the bound game; start a new task from its actual state.');
  }
  if (sameGameContextSource(root.source, observation)) return 'pending';
  const site = observation.position?.observation?.observation?.context?.kind;
  // Only an execution target stays attached (e.g. a bottle or transform).
  // Opening a reward starts a fresh classification of its revealed choices.
  if (task.location === root.task.location && site === 'run_choice') return 'selection';
  return 'complete';
}

export function preparationProgressText(progress, decision, root, classification, draft, locale = 'en') {
  const { text } = preparationInstructions(locale);
  const budget = Number.isSafeInteger(root?.startingGold) ? text('progressBudget', { gold: root.startingGold }) : '';
  const adopted = decision?.command ? text('progressAdopted', { command: decision.command })
    + (decision.basis ? text('progressBasis', { basis: decision.basis }) : '') : '';
  return budget + (progress === 'complete' ? adopted + text('progressComplete')
    : progress === 'selection' ? adopted + text('progressSelection', { instruction: text(classification ? 'progressTargetComplete' : 'progressTargetPending') })
    : classification ? text('progressAdoption')
    : text('progressClassification', { instruction: draft?.rows.length
      ? text('progressRows', { count: draft.pendingCommands.length, instruction: draft.pendingChoice ? text('progressSavedChoice', { choice: draft.pendingChoice }) : text('progressRemaining') })
      : text('progressRate') }));
}

export function restoredPreparationDecision(session) {
  for (const event of session.snapshotEvents()) {
    if (event.type !== 'tool/result') continue;
    const call = session.eventAt(event.sourceEventSeqs?.[0]);
    if (call?.type !== 'tool/call' || call.data.name !== 'sts') continue;
    const decision = event.data.meta?.preparationDecision;
    if (event.data.meta?.schema === 'sts-preparation-result-v1'
        && typeof decision?.command === 'string' && decision.command.trim()
        && (decision.basis === undefined || typeof decision.basis === 'string')) {
      return { command: decision.command, basis: decision.basis };
    }
  }
}

function preparationChoiceMembers(choice, observation, normalize) {
  const command = normalize(choice.trim(), observation);
  const candidates = preparationCandidates(observation, normalize);
  const family = preparationSelectionFamily(observation);
  const members = family && command.startsWith('act select:') && command !== 'act select:none'
    ? command.slice('act select:'.length).split(',').map(token => `act select:${token}`) : [command];
  if (family && command.startsWith('act ')) {
    const count = command === 'act select:none' ? 0 : members.length;
    if (new Set(members).size !== members.length || count < family.min_choices || count > family.max_choices) {
      throw new Error(`Select ${family.min_choices}..${family.max_choices} distinct classified targets in one act select: command.`);
    }
  }
  if (members.some(member => !candidates.includes(member))) {
    throw new Error('Select classified current candidates using their supplied commands. A complete card selection starts with act select: and joins the classified card tokens with commas.');
  }
  return { command, members };
}

export function preparationCommandBasis([command, args], progress, basis, observation, classification, normalize = command => command) {
  if (!['pending', 'selection'].includes(progress) || queries.has(command)) return;
  const candidates = preparationCandidates(observation, normalize);
  if (!classification || !sameGameContextSource(classification.source, observation)
      || !Array.isArray(classification.rows) || classification.rows.length !== candidates.length
      || candidates.some(candidate => !classification.rows.some(row => row.command === candidate))) {
    throw new Error('Call classify with the complete current table, then compare and adopt a preparation.');
  }
  const { command: selected, members } = preparationChoiceMembers([command, ...args].join(' '), observation, normalize);
  const family = preparationSelectionFamily(observation);
  const selectedRows = members.map(member => classification.rows.find(row => row.command === member));
  if (selectedRows.some(row => row.category === 'review' || row.realization === 'review')) {
    throw Object.assign(new Error('Record the selected candidate\'s missing fact through sts_feedback for maintainer review.'),
      { preparationReviewCandidate: selected });
  }
  const groups = preparationCandidateGroups(observation, normalize);
  const renderRow = item => `${item.command} | ${item.category} | ${item.realization} | ${item.reason.replace(/\s+/g, ' ')}`;
  const selectedGroups = groups.filter(group => group.members.some(member => members.includes(member)));
  const alternatives = groups.filter(group => !selectedGroups.includes(group));
  if (observation.position?.observation?.observation?.context?.kind === 'map') {
    const rank = { high: 3, medium: 2, low: 1 };
    const higher = classification.rows.filter(row => row.realization !== 'review'
      && rank[row.category] > rank[selectedRows[0].category]);
    if (higher.length) throw new Error(`Map choice ${selected} is rated ${selectedRows[0].category}; higher recorded recommendations: ${higher.map(row => `${row.command} (${row.category})`).join('; ')}. Submit choose with a highest-rated assessed route, or record the unresolved route tradeoff through sts_feedback.`);
  }
  const multi = family && family.max_choices > 1 && selected.startsWith('act select:');
  const bound = { command: selected, ...(selectedRows.length === 1 ? { selectedRow: selectedRows[0] } : {}), selectedRows };
  const rendered = selectedRows.map(renderRow).join('; ');
  if (shopSpendingScope(observation, normalize)) {
    const keep = /^(?:then\s+)?keep the balance$/.test(basis?.trim() ?? '');
    const match = basis?.trim() && !keep ? /^then\s+([^|\r\n]+)$/.exec(basis.trim()) : undefined;
    if (basis?.trim() && !match && !keep) throw new Error('An optional second spend uses comparison: then FOLLOW_UP_COMMAND from the selected first spend\'s budget row; keeping the balance uses keep the balance.');
    const continuation = match ? shopContinuation(observation, selected, match[1].trim()) : undefined;
    return { ...bound, basisKind: 'shop-recommendations', alternativeRow: null,
      comparisonIds: [], comparisons: [], ...(continuation ? { shopContinuation: continuation } : {}),
      renderedBasis: `${selected} | ${selectedRows[0].category}${continuation ? `; then ${continuation.next}` : ''}; native budget bound by host.` };
  }
  if (multi) return { ...bound, basisKind: 'selection-set',
    unselectedRows: classification.rows.filter(row => !members.includes(row.command)),
    comparisonIds: [], comparisons: [], renderedBasis: `${rendered}; Selected targets bound to the complete current classification.` };
  if (!alternatives.length) return { ...bound, alternativeRow: null,
    comparisonIds: [], comparisons: [], basisKind: 'sole-candidate', renderedBasis: `${rendered}; ${groups.length === 1 ? 'Sole classified candidate.' : 'All classification groups selected.'}` };
  const priorityComparisons = preparationPriorityBaselines(observation, normalize);
  let comparisonBasis = basis, basisKind = 'player-comparison';
  if (!basis?.trim()) {
    const required = priorityComparisons.filter(baseline => !members.some(member => baseline.commands.includes(member)));
    const eligible = alternatives.filter(group => required.every(baseline => group.members.some(member => baseline.commands.includes(member))));
    if (!eligible.length) throw new Error('The current comparison priorities require maintainer review of their shared legal alternatives.');
    const rank = { high: 3, medium: 2, low: 1, review: 0 };
    const alternative = eligible.map(group => classification.rows.find(row => row.command === group.command))
      .sort((left, right) => rank[right.category] - rank[left.category])[0];
    const references = [...new Set(['@function', '@access', '@resources', ...priorityComparisons.flatMap(row => row.comparisonIds)])];
    comparisonBasis = `${alternative.command} | ${references.join(',')}`;
    basisKind = 'host-comparison';
  }
  const match = /^\s*([^|]+?)\s*\|\s*([^|\r\n]+?)(?:\s*\|\s*then\s+([^|\r\n]+))?\s*$/.exec(comparisonBasis);
  if (!match) throw new Error(`Supply comparison basis: ${PREPARATION_COMPARISON_BASIS}`);
  const alternative = normalize(match[1].trim(), observation);
  const alternativeRow = classification.rows.find(row => row.command === alternative);
  if (!alternativeRow || !alternatives.some(group => group.members.includes(alternative))) {
    throw new Error(`Compare with another current legal target or candidate: ${alternatives.map(group => group.command).join('; ')}. Use its command before the | separator.`);
  }
  const comparisonIds = match[2].split(',').map(id => id.trim());
  const comparisons = comparisonIds.map(id => PREPARATION_COMPARISONS.find(item => item.id === id));
  if (comparisons.some(item => !item) || new Set(comparisonIds).size !== comparisonIds.length) {
    throw new Error(`Use supplied comparison references: ${PREPARATION_COMPARISONS.map(item => item.id).join(', ')}.`);
  }
  for (const baseline of priorityComparisons) {
    if (!members.some(member => baseline.commands.includes(member)) && !baseline.commands.includes(alternative)) {
      throw new Error(`Compare this choice with a current ${baseline.id} candidate: ${baseline.commands.join('; ')}. Use its command as the alternative and include ${baseline.comparisonIds.join(',')}.`);
    }
    if (baseline.comparisonIds.some(id => !comparisonIds.includes(id))) {
      throw new Error(`Include ${baseline.comparisonIds.join(',')} when comparing this choice under ${baseline.id}; weigh access with the actual function, protection and resource tradeoff.`);
    }
  }
  const continuation = match[3] ? shopContinuation(observation, selected, match[3].trim()) : undefined;
  return { ...bound, alternativeRow, comparisonIds, comparisons, basisKind,
    ...(priorityComparisons.length ? { priorityComparisons } : {}), ...(continuation ? { shopContinuation: continuation } : {}),
    renderedBasis: `${rendered}; Compared with ${renderRow(alternativeRow)}; Emphasis: ${comparisonIds.join(',')}` };
}

function preparationPriorityBaselines(observation, normalize) {
  const legal = new Set(preparationCandidates(observation, normalize));
  return (preparationExpertGuidance(observation, normalize)?.construction?.priorities ?? []).flatMap(priority => {
    const baseline = priority.comparisonBaseline;
    const commands = baseline?.commands.filter(command => legal.has(command));
    return commands?.length ? [{ id: priority.id, commands, comparisonIds: baseline.comparisonIds }] : [];
  });
}

export function assertPreparationCommand([command, args], progress, observation, normalize = command => command) {
  if (queries.has(command) && args.length === 0) return;
  if (progress === 'complete') throw new Error(preparationProgressText(progress));
  if (command === 'classify' && !args.length && ['pending', 'selection'].includes(progress)) return;
  if (command === 'act') {
    const site = observation.position?.observation?.observation?.context?.kind;
    if (site !== 'run_choice' || !args.length || !/^select(?::|$)/.test(args[0])
        || args.some(token => token.includes(':') && !token.startsWith('select:'))) {
      throw new Error('Use a displayed preparation choice; act select:<card> is for its attached card selection.');
    }
    return;
  }
  const candidates = preparationCandidates(observation, normalize);
  if (!candidates.includes(normalize([command, ...args].join(' '), observation))) {
    const labelSeparator = args.indexOf('|');
    const commandOnly = labelSeparator >= 0 ? normalize([command, ...args.slice(0, labelSeparator)].join(' '), observation) : undefined;
    if (candidates.includes(commandOnly)) {
      throw new Error(`Copy the command ${commandOnly} into choice. Its display label is separate text.`);
    }
    if (command === 'buy' || command.startsWith('buy:')) {
      throw new Error('Copy a complete command ID from the first column of the current candidate table into choice, such as buy:Card#0. Use read to recover the current table.');
    }
    throw new Error('This task completes one preparation. Use a supplied candidate, read for recovery, options for attached choices, or sts_feedback for a missing fact.');
  }
}
