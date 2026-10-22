import { createPresentation, resolveLocale } from '@sts-simulator/presentation';
import { gameContextSource, sameGameContextSource, gameDecisionTask } from './context.mjs';
import { preparationExpertGuidance, renderExpertGuidance, renderCandidateExpertGuidance, apparitionPreparationOffers, preparationBossRelicOffers, preparationShopRemovalPolicy } from './strategy.mjs';
import { preparationDeckComparison, preparationDeckConditionBindings, preparationCardAcquisition, strikeScaledAttack, renderDeckComparison, attackInventoryText, renderStatusRoutes } from './deck.mjs';
import { shopInstructions, renderShopBudget, shopContinuation, shopNeedsPotionSlot, shopPotionReplacementScope,
  renderShopPotionComparison, shopPotionComparisonReading, shopPotionConditionBindings,
  shopSpendingScope, shopSpendingReading, shopSpendingConditionBindings, renderShopSpendingComparison } from './shop.mjs';

const families = new Set(['shop', 'campfire', 'event', 'rewards', 'treasure', 'boss_relic', 'run_choice', 'map', 'run_transition']);
const queries = new Set(['read', 'options']);

const preparationViews = Object.fromEntries(['en', 'zh-CN'].map(locale => [locale, createPresentation({ locale })]));
const preparationText = (locale, key, parameters = {}) => preparationViews[resolveLocale(locale)].text(`scripted:preparation.${key}`,
  Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));

export function preparationInstructions(locale = 'en') {
  const text = (key, params) => preparationText(locale, key, params);
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
export function preparationReadingSupply(observation, normalize = command => command, locale = 'en') {
  const native = observation.preparation_reading;
  if (native?.schema !== 'sts-preparation-reading-v1'
      || !native.public_snapshot_id || native.public_snapshot_id !== observation.position?.public_snapshot_id) {
    throw Object.assign(new Error(preparationText(locale, 'readingSourceMismatch')),
      { preparationGap: { kind: 'facts', reason: 'preparation-reading-source' } });
  }
  const commands = new Set(preparationCandidates(observation, normalize, locale));
  const omissions = sameGameContextSource(observation.preparationOmissions?.source, observation)
    ? new Set(observation.preparationOmissions.commands.map(command => normalize(command, observation))) : new Set();
  const candidates = {}, reading = [];
  reading.push(native.overview, native.selections);
  for (const block of native.candidates) {
    const command = normalize(block.command, observation);
    if (omissions.has(command)) continue;
    if (commands.has(command)) {
      if (Object.hasOwn(candidates, command)) throw new Error(preparationText(locale, 'duplicateRenderedCommand', { command: command }));
      candidates[command] = block;
    } else reading.push(`${command}  |  ${block.label}\n${block.view}`);
  }
  reading.push(...native.stock.filter(row => !row.command).map(row => row.view), native.inventory.view, native.routes);
  return { reading: shopPotionComparisonReading(native, observation, locale) ?? shopSpendingReading(native, observation, normalize, locale)
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
function preparationSelectionFamily(observation, locale = 'en') {
  const rows = (observation.position?.actions ?? []).filter(row => row.run_selection_family);
  if (!rows.length) return;
  if (rows.length !== 1) throw new Error(preparationText(locale, 'selectionFamilyRequired'));
  const family = rows[0].run_selection_family;
  if (!Array.isArray(family.card_tokens) || family.card_tokens.length !== family.selectable_cards?.length
      || new Set(family.card_tokens).size !== family.card_tokens.length
      || family.card_tokens.some(token => typeof token !== 'string' || !token || /[\s,]/.test(token))
      || !Number.isSafeInteger(family.min_choices) || !Number.isSafeInteger(family.max_choices)
      || family.min_choices < 0 || family.min_choices > family.max_choices) {
    throw new Error(preparationText(locale, 'selectionTokensMissing'));
  }
  return family;
}

function preparationSelectionTargets(observation, normalize = command => command, locale = 'en') {
  const family = preparationSelectionFamily(observation, locale);
  if (family) return family.selectable_cards.map((card, index) => ({ card,
    command: normalize(`act select:${family.card_tokens[index]}`, observation),
    reading: family.card_readings?.[index] }));
  return (observation.position?.actions ?? []).flatMap(row => row.run_selection_target
    ? [{ ...row.run_selection_target, command: normalize(`act ${row.semantic_action}`, observation) }] : []);
}

export function preparationCandidates(observation, normalize = command => command, locale = 'en') {
  const actions = observation.position?.actions;
  if (!Array.isArray(actions) || !actions.length) throw new Error(preparationText(locale, 'legalCandidatesRequired'));
  return actions.flatMap(action => {
    if (action.run_selection_family) {
      const family = preparationSelectionFamily(observation, locale);
      return [...family.card_tokens.map(token => normalize(`act select:${token}`, observation)),
        ...(family.min_choices === 0 ? ['act select:none'] : [])];
    }
    const semantic = action.candidate?.action?.kind === 'submit_run_selection'
      ? `act ${action.semantic_action}` : action.semantic_action;
    const command = normalize(semantic, observation);
    if (typeof command !== 'string' || !command.trim()) throw new Error(preparationText(locale, 'semanticCommandMissing'));
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
export function preparationCandidateGroups(observation, normalize = command => command, locale = 'en') {
  const commands = preparationCandidates(observation, normalize, locale);
  if (new Set(commands).size !== commands.length) throw new Error(preparationText(locale, 'duplicateCandidateCommands'));
  const state = observation.position.observation.observation;
  // Bottles bind a particular lineage through relic state. The grouped public
  // cards omit that identity; preserve individual choices while it is present.
  if ((state.relics ?? []).some(row => ['BottledFlame', 'BottledLightning', 'BottledTornado'].includes(row.relic))) {
    return commands.map(command => ({ command, members: [command] }));
  }
  const cards = state.cards ?? [];
  const attributes = ['misc_value', 'base_damage_override', 'base_block_override', 'cost_modifier'];
  const targets = preparationSelectionTargets(observation, normalize, locale);
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
  const potionScope = shopPotionReplacementScope(observation, normalize, locale);
  if (potionScope) return shopPotionConditionBindings(potionScope, locale);
  const spendingScope = shopSpendingScope(observation, normalize, locale);
  if (spendingScope) return shopSpendingConditionBindings(spendingScope, locale);
  const specific = [...preparationEnergyBindings(observation, normalize, locale), ...armamentsConditionBindings(observation, normalize, locale),
    ...etherealUpgradeConditionBindings(observation, normalize, locale),
    ...attackUpgradeConditionBindings(observation, normalize, locale),
    ...duplicateConditionBindings(observation, normalize, locale), ...routeConditionBindings(observation, normalize, locale),
    ...bossRelicConditionBindings(observation, normalize, locale), ...eventCombatConditionBindings(observation, normalize, locale),
    ...apparitionConditionBindings(observation, normalize, locale),
    ...rewardRevealConditionBindings(observation, normalize, locale),
    ...statusDrawConditionBindings(observation, normalize, locale),
    ...temporaryStrengthConditionBindings(observation, normalize, locale)];
  const bindings = [...specific, ...preparationDeckConditionBindings(observation, normalize, locale)
    .filter(row => !specific.some(binding => binding.command === row.command))];
  const profile = preparationDeckComparison(observation, normalize, locale);
  return [...bindings, ...preparationCandidateGroups(observation, normalize, locale)
    .filter(group => !bindings.some(binding => binding.command === group.command))
    .map(group => ({ command: group.command, id: 'preparation-effect-tradeoff-v1',
      fixedMechanics: [],
      choices: [
        { id: '@effect', text: preparationText(locale, 'conditionEffect') },
        { id: '@owned-support', text: preparationText(locale, 'conditionRealization') },
        { id: '@resource-tradeoff', text: preparationText(locale, 'conditionTradeoff') },
        { id: '@review-access', text: preparationText(locale, 'conditionReview') },
      ] }))].map(binding => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === binding.command);
    const acquisition = preparationCardAcquisition(observation, row);
    if (!acquisition || !profile) return binding;
    const state = observation.position.observation.observation;
    const copies = state.cards.filter(card => card.card === acquisition.card).reduce((sum, card) => sum + card.count, 0);
    const injections = acquisition.mechanics?.status_injections ?? [];
    return { ...binding, fixedMechanics: [
      preparationText(locale, 'evaluationPool', { size: profile.size, card: acquisition.card, upgradeSuffix: acquisition.upgrades > 0 ? '+' : '', ownedCard: acquisition.card, copiesBefore: copies, copiesAfter: copies + 1 }),
      ...(injections.length ? [
        preparationText(locale, 'additionStatusRoutes', { renderStatusRoutes: renderStatusRoutes([{ card: acquisition.card, injections }], locale) }),
        preparationText(locale, 'ownedStatusPayoffs', { statusPayoffs: profile.statusPayoffs.map(source => preparationText(locale, 'deploymentCost', { card: source.card, cost: source.cost })).join('; ') || preparationText(locale, 'none') }),
      ] : []), ...binding.fixedMechanics,
    ] };
  });
}

function attackUpgradeConditionBindings(observation, normalize, locale = 'en') {
  return preparationCandidateGroups(observation, normalize, locale).flatMap(group => {
    const action = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command)?.candidate?.action;
    if (action?.kind !== 'smith') return [];
    const evidence = observation.decision_evidence?.candidates?.find(row =>
      [row.semantic_action, ...(row.semantic_actions ?? [])].filter(Boolean).some(command => normalize(command, observation) === group.command));
    const before = evidence?.before, after = evidence?.after;
    if (before?.card !== action.card || before.upgrades !== action.upgrades
        || after?.card !== action.card || after.upgrades !== before.upgrades + 1
        || !Number.isSafeInteger(before.cost) || before.cost < 0 || before.cost !== after.cost) return [];
    const attacks = [before, after].map(card => card.damage);
    const [oldScaling, newScaling] = attacks.map(damage => strikeScaledAttack(damage, locale));
    if (oldScaling && newScaling && attacks[0].delivery.targets === attacks[1].delivery.targets
        && (oldScaling.perCard !== newScaling.perCard || oldScaling.base !== newScaling.base)) {
      return [{ command: group.command, id: 'strike-scaled-attack-upgrade-v1', fixedMechanics: [
        preparationText(locale, 'strikeUpgrade', { card: action.card, costBefore: before.cost, costAfter: after.cost, baseBefore: oldScaling.base, perCardBefore: oldScaling.perCard, baseAfter: newScaling.base, perCardAfter: newScaling.perCard }),
        oldScaling.countRule,
        preparationText(locale, 'strikeUpgradeIncrement', { baseIncrement: newScaling.base - oldScaling.base, perCardIncrement: newScaling.perCard - oldScaling.perCard }),
      ], choices: [
        { id: '@strike-count', text: preparationText(locale, 'strikeUpgradeCount') },
        { id: '@upgrade-opportunity', text: preparationText(locale, 'attackUpgradeOpportunity') },
        { id: '@review-access', text: preparationText(locale, 'strikeUpgradeReview') },
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
      preparationText(locale, 'attackUpgrade', { card: action.card, costBefore: before.cost, costAfter: after.cost, damageBefore: oldDamage.damage_per_hit, damageAfter: newDamage.damage_per_hit, hitsBefore: oldDamage.hit_count, hitsAfter: newDamage.hit_count }),
      preparationText(locale, 'attackUpgradeTotal', { totalBefore: oldDamage.damage_per_hit * oldDamage.hit_count, totalAfter: newDamage.damage_per_hit * newDamage.hit_count }),
      ...(drawChanges ? [preparationText(locale, 'attackUpgradeDraw', { drawBefore: before.draw_cards, drawAfter: after.draw_cards, cost: after.cost })] : []),
      preparationText(locale, 'attackTargeting', { targets: newDamage.delivery.targets })
        + (newDamage.delivery.targets === 'random_enemy_per_hit'
          ? preparationText(locale, 'randomAttackTargeting') : ''),
      preparationText(locale, 'attackUpgradeLifecycle'),
    ], choices: [
      ...(extraHits > 0 ? [{ id: '@extra-hit', text: preparationText(locale, 'attackExtraHits', { extraHits: extraHits }) }] : []),
      ...(perHitChanges ? [{ id: '@base-damage', text: preparationText(locale, 'attackBaseChange') }] : []),
      ...(drawChanges ? [{ id: '@draw-increment', text: preparationText(locale, 'attackExtraDraw') }] : []),
      { id: '@upgrade-opportunity', text: preparationText(locale, 'attackUpgradeOpportunity') },
      { id: '@review-access', text: preparationText(locale, 'attackUpgradeReview') },
    ] }];
  });
}

function etherealUpgradeConditionBindings(observation, normalize, locale = 'en') {
  const state = observation.position.observation.observation;
  return preparationCandidateGroups(observation, normalize, locale).flatMap(group => {
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
      preparationText(locale, 'etherealUpgrade', { card: action.card }),
      pyramid ? preparationText(locale, 'pyramidEtherealUpgrade')
        : preparationText(locale, 'ordinaryEtherealUpgrade'),
      preparationText(locale, 'etherealUpgradeCosts', { costBefore: before.cost, costAfter: after.cost, exhaustBefore: before.exhausts, exhaustAfter: after.exhausts }),
      ...(Number.isSafeInteger(before.intangible_gain) && Number.isSafeInteger(after.intangible_gain)
        && after.intangible_gain > 0 ? [preparationText(locale, 'etherealUpgradeProtection', { intangibleBefore: before.intangible_gain, intangibleAfter: after.intangible_gain })] : []),
      preparationText(locale, 'etherealUpgradeOpportunity'),
    ], choices: [
      { id: '@preserve-for-redraw', text: preparationText(locale, 'etherealRedraw') },
      ...(pyramid ? [{ id: '@pyramid-hand', text: preparationText(locale, 'pyramidDelayedUse') }] : []),
      { id: '@upgrade-opportunity', text: preparationText(locale, 'etherealPreparationTradeoff') },
      { id: '@review-access', text: preparationText(locale, 'etherealTimingReview') },
    ] }];
  });
}

function apparitionConditionBindings(observation, normalize, locale = 'en') {
  const state = observation.position.observation.observation;
  const offers = apparitionPreparationOffers(observation, normalize);
  return offers.flatMap(offer => {
    const card = offer.current;
    const fixedMechanics = [
      preparationText(locale, 'apparitionPackage', { command: offer.command, copies: offer.count, hpBefore: offer.hpBefore, hpAfter: offer.hpAfter, hpCost: offer.hpBefore - offer.hpAfter, maxHpBefore: offer.maxHpBefore, maxHpAfter: offer.maxHpAfter, maxHpReduction: offer.maxHpBefore - offer.maxHpAfter }),
      preparationText(locale, 'apparitionProtection', { cost: card.cost, intangible: card.intangible_gain, damageCap: card.intangible_damage_cap })
        + (card.intangible_caps_direct_hp_loss_to_one ? preparationText(locale, 'apparitionHpLossCap') : ''),
      preparationText(locale, 'apparitionLifecycle', { exhausts: card.exhausts ? preparationText(locale, 'apparitionExhaust') : preparationText(locale, 'apparitionDestination') })
        + (card.ethereal ? preparationText(locale, 'apparitionEthereal') : '')
        + (offer.upgraded?.card === card.card && offer.upgraded.upgrades === card.upgrades + 1
          && card.ethereal && offer.upgraded.ethereal === false ? preparationText(locale, 'apparitionUpgrade') : ''),
      preparationText(locale, 'apparitionCoverage'),
      preparationText(locale, 'apparitionHealing', { maxHpAfter: offer.maxHpAfter }),
    ];
    const decline = observation.position.actions.filter(row => {
      const action = row.candidate?.action, semantics = action?.public_semantics;
      return action?.kind === 'choose_event_option' && semantics?.hp_delta === 0 && semantics.max_hp_delta === 0
        && semantics.gold_delta_min === 0 && semantics.gold_delta_max === 0
        && semantics.structured_semantics?.effects?.length === 0 && !semantics.starts_combat;
    });
    return [{ command: offer.command, id: 'apparition-package-v1', fixedMechanics, choices: [
      { id: '@protected-turns', text: preparationText(locale, 'apparitionProtectedTurns') },
      { id: '@hp-cap-tradeoff', text: preparationText(locale, 'apparitionHpTradeoff') },
      { id: '@access-timing', text: preparationText(locale, 'apparitionAccessTiming') },
      { id: '@review-access', text: preparationText(locale, 'apparitionReview') },
    ] }, ...decline.map(row => ({ command: normalize(row.semantic_action, observation), id: 'apparition-decline-v1',
      fixedMechanics: [preparationText(locale, 'apparitionDecline', { hp: state.run.current_hp, maxHp: state.run.max_hp }), ...fixedMechanics], choices: [
        { id: '@retain-hp-cap', text: preparationText(locale, 'apparitionRetainHp') },
        { id: '@forgone-protection', text: preparationText(locale, 'apparitionForegoneProtection') },
        { id: '@review-access', text: preparationText(locale, 'apparitionDeclineReview') },
      ] }))];
  });
}

function rewardRevealConditionBindings(observation, normalize, locale = 'en') {
  if (observation.position.observation.observation.context?.kind !== 'reward') return [];
  return observation.position.actions.flatMap(row => row.candidate?.action?.kind === 'open_card_reward'
    ? [{ command: normalize(row.semantic_action, observation), id: 'reward-card-reveal-v1', fixedMechanics: [
      preparationText(locale, 'openCardReward'),
    ], choices: [
      { id: '@card-choice-access', text: preparationText(locale, 'rewardRevealAccess') },
      { id: '@review-access', text: preparationText(locale, 'rewardRevealReview') },
    ] }] : []);
}

function temporaryStrengthConditionBindings(observation, normalize, locale = 'en') {
  const state = observation.position.observation.observation;
  const profile = preparationDeckComparison(observation, normalize, locale);
  const ownsRelic = relic => (state.relics ?? []).some(row => row.relic === relic);
  return preparationCandidateGroups(observation, normalize, locale).flatMap(group => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command);
    const acquisition = preparationCardAcquisition(observation, row);
    const definition = acquisition?.mechanics;
    const amount = definition?.temporary_strength_gain;
    if (!definition || definition.strength_gain_delivery !== 'on_play_with_separate_lose_strength'
        || !Number.isSafeInteger(amount) || amount <= 0 || definition.strength_gain !== amount
        || definition.strength_loss_at_end_of_turn !== amount || !Number.isSafeInteger(definition.cost)) return [];
    return [{ command: group.command, id: 'temporary-strength-payoff-v1', fixedMechanics: [
      preparationText(locale, 'temporaryStrengthMechanics', { card: definition.name, cost: definition.cost, strength: amount, strengthLoss: amount }),
      preparationText(locale, 'temporaryStrengthSequence'),
      ...(profile ? [attackInventoryText(profile, locale)] : []),
      ...(ownsRelic('Boot') ? [preparationText(locale, 'bootStrengthDamage')] : []),
      ...(ownsRelic('MutagenicStrength') ? [preparationText(locale, 'mutagenicOpening')] : []),
    ], choices: [
      { id: '@same-hand-payoff', text: preparationText(locale, 'temporaryStrengthOutput', { value: profile?.attackCopies ?? preparationText(locale, 'suppliedInventory') }) },
      ...(ownsRelic('Boot') ? [{ id: '@damage-threshold', text: preparationText(locale, 'bootStrengthComparison') }] : []),
      ...(ownsRelic('MutagenicStrength') ? [{ id: '@opening-stack', text: preparationText(locale, 'openingBurst') }] : []),
      { id: '@draw-slot-tradeoff', text: preparationText(locale, 'temporaryStrengthCost') },
      { id: '@review-access', text: preparationText(locale, 'temporaryStrengthReview') },
    ] }];
  });
}

function statusDrawConditionBindings(observation, normalize, locale = 'en') {
  const profile = preparationDeckComparison(observation, normalize, locale);
  if (!profile) return [];
  return preparationCandidateGroups(observation, normalize, locale).flatMap(group => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command);
    const acquisition = preparationCardAcquisition(observation, row);
    if (!['Evolve', 'FireBreathing'].includes(acquisition?.card) || acquisition.mechanics?.card_type !== 'Power') return [];
    const triggers = profile.statusInventory.filter(card => card.type === 'Status' || acquisition.card === 'FireBreathing');
    return [{ command: group.command, id: 'status-draw-route-v1', fixedMechanics: [
      preparationText(locale, 'statusPayoffDeployment', { name: acquisition.mechanics.name, cost: acquisition.mechanics.cost }),
      preparationText(locale, 'ownedStatusRoutes', { renderStatusRoutes: renderStatusRoutes(profile.statusGeneration, locale) || preparationText(locale, 'none') }),
      preparationText(locale, 'qualifyingOriginals', { triggers: triggers.map(card => `${card.card}${card.ethereal ? preparationText(locale, 'drawnEthereal') : preparationText(locale, 'ordinaryLaterRedraw')}`).join('; ') || preparationText(locale, 'none') }),
      preparationText(locale, 'statusTriggerCounting'),
    ], choices: [
      { id: '@owned-status-route', realizations: profile.statusGeneration.length || triggers.length ? ['sequence'] : ['future'],
        text: preparationText(locale, 'ownedStatusPayoff') },
      { id: '@encounter-status-route', text: preparationText(locale, 'encounterStatusPayoff') },
      { id: '@deployment-cost', text: preparationText(locale, 'statusDeploymentCost') },
      { id: '@review-access', text: preparationText(locale, 'statusRuleReview') },
    ] }];
  });
}

function eventCombatConditionBindings(observation, normalize, locale = 'en') {
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
      preparationText(locale, 'eventCombatEntry'),
      ...(Number.isSafeInteger(hp) && Number.isSafeInteger(max) ? [preparationText(locale, 'eventEntryHp', { hp: hp, maxHp: max })] : []),
      ...(Number.isSafeInteger(recent?.recent_combat_hp_loss) ? [
        preparationText(locale, 'previousCombatCost', { recent_combat_hp_loss: recent.recent_combat_hp_loss }),
      ] : []),
      preparationText(locale, 'eventCombatHealing'),
    ], choices: [
      { id: '@combat-risk', text: preparationText(locale, 'eventCombatRisk') },
      { id: '@victory-reward', text: preparationText(locale, 'eventVictoryReward') },
      { id: '@review-access', text: preparationText(locale, 'eventCombatReview') },
    ] }];
  });
}

function bossRelicConditionBindings(observation, normalize, locale = 'en') {
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
        preparationText(locale, 'sozuGain', { relic: relic, energy_per_turn_delta: definition.energy_per_turn_delta }),
        ...(held ? [preparationText(locale, 'sozuHeldPotions', { held: held.map(slot => `P${slot.slot} ${slot.potion.potion}`).join(', ') || preparationText(locale, 'none') })]
          : [preparationText(locale, 'sozuHeldUse')]),
        preparationText(locale, 'sozuRefills'),
        preparationText(locale, 'sozuProtection'),
      ], choices: [
        { id: '@recurring-energy', text: preparationText(locale, 'sozuExtraPlays') },
        { id: '@retained-potions', text: preparationText(locale, 'sozuReserve') },
        { id: '@future-potion-tradeoff', text: preparationText(locale, 'sozuTradeoff') },
        { id: '@review-access', text: preparationText(locale, 'sozuReview') },
      ] }];
    }
    const cap = definition.card_play_limit_per_turn;
    if (Number.isSafeInteger(cap) && cap > 0 && Number.isSafeInteger(definition.energy_per_turn_delta)) {
      const profile = preparationDeckComparison(observation, normalize, locale);
      return [{ command, id: 'relic-energy-play-cap-v1', fixedMechanics: [
        preparationText(locale, 'chokerGain', { relic: relic, energy: definition.energy_per_turn_delta, playLimit: cap }),
        preparationText(locale, 'chokerSharedAllowance'),
        ...(profile ? [preparationText(locale, 'chokerDrawPlays', { onPlayDraw: profile.onPlayDraw.map(row => row.card).join(', ') || preparationText(locale, 'none') })] : []),
        preparationText(locale, 'chokerLaterLimit'),
      ], choices: [
        { id: '@recurring-energy', text: preparationText(locale, 'chokerPayment') },
        { id: '@shared-play-cap', text: preparationText(locale, 'chokerSequence') },
        { id: '@few-play-output', text: preparationText(locale, 'chokerCompactOutput') },
        { id: '@review-access', text: preparationText(locale, 'chokerReview') },
      ] }];
    }
    const replacement = definition.starter_card_replacement, targets = candidate.starterCardsReplaced;
    if (replacement?.replacements_per_card === 1 && Array.isArray(targets)
        && Array.isArray(replacement.pool_rarities) && replacement.pool_rarities.length
        && Number.isSafeInteger(replacement.initial_upgrades)) {
      const count = targets.reduce((n, card) => n + card.count, 0);
      return [{ command, id: 'relic-starter-replacement-v1', fixedMechanics: [
        preparationText(locale, 'starterReplacement', { relic: relic, count: count, cards: targets.map(card => `${card.count}x ${card.card}${card.upgrades > 0 ? '+' : ''}`).join(', ') || preparationText(locale, 'none'), pools: replacement.pool_rarities.join('/'), upgrades: replacement.initial_upgrades }),
        preparationText(locale, 'replacementDeck'),
        ...(replacement.applies_obtain_effects ? [preparationText(locale, 'replacementObtainEffects')] : []),
      ], choices: [
        { id: '@replace-starters', text: preparationText(locale, 'replacementPool') },
        { id: '@replacement-tradeoff', text: preparationText(locale, 'replacementTradeoff') },
        { id: '@review-access', text: preparationText(locale, 'replacementReview') },
      ] }];
    }
    const draw = definition.card_draw_on_hp_loss;
    if (Number.isSafeInteger(draw?.cards_drawn_per_event) && draw.cards_drawn_per_event > 0
        && Number.isSafeInteger(draw.minimum_hp_lost_per_event) && draw.minimum_hp_lost_per_event > 0) {
      const onPlay = sources.filter(source => source.on_play?.base_hp_loss > 0);
      const recurring = sources.filter(source => source.hp_loss_each_turn_after_deployment > 0
        || source.end_of_turn_after_deployment?.hp_loss_per_application > 0);
      return [{ command, id: 'relic-hp-loss-draw-v1', fixedMechanics: [
        preparationText(locale, 'hpLossRelic', { relic: relic, minimumHpLoss: draw.minimum_hp_lost_per_event, draws: draw.cards_drawn_per_event }),
        ...onPlay.map(source => preparationText(locale, 'onPlayHpLossSource', { source: label(source), hpLoss: source.on_play.base_hp_loss, requiredCard: source.card.card })),
        ...recurring.map(source => preparationText(locale, 'recurringHpLossSource', { label: label(source), value: source.end_of_turn_after_deployment?.hp_loss_per_application ?? source.hp_loss_each_turn_after_deployment })),
        preparationText(locale, 'enemyHpLossTrigger'),
      ], choices: [
        ...(onPlay.length ? [{ id: '@on-play-draw', text: preparationText(locale, 'immediateHpLossDraw') }] : []),
        ...(recurring.length ? [{ id: '@recurring-draw', text: preparationText(locale, 'recurringHpLossDraw') }] : []),
        { id: '@enemy-loss-draw', text: preparationText(locale, 'enemyHitDraw') },
        { id: '@draw-tradeoff', text: preparationText(locale, 'hpLossRelicTradeoff') },
        { id: '@review-access', text: preparationText(locale, 'hpLossRelicReview') },
      ] }];
    }
    if (definition.gold_gain_rule === 'prevent_gold_gain'
        && Number.isSafeInteger(definition.energy_per_turn_delta) && definition.energy_per_turn_delta > 0
        && Number.isSafeInteger(state.run?.gold) && state.run.gold >= 0) {
      return [{ command, id: 'relic-energy-gold-v1', fixedMechanics: [
        preparationText(locale, 'energyRelicGain', { relic: relic, energy_per_turn_delta: definition.energy_per_turn_delta }),
        preparationText(locale, 'ectoplasmGold', { gold: state.run.gold }),
      ], choices: [
        { id: '@recurring-energy', text: preparationText(locale, 'energyRelicPayment') },
        { id: '@retained-gold', text: preparationText(locale, 'retainedGoldPurchases') },
        { id: '@future-gold-tradeoff', text: preparationText(locale, 'ectoplasmTradeoff') },
        { id: '@review-access', text: preparationText(locale, 'ectoplasmReview') },
      ] }];
    }
    return [];
  });
}

function routeConditionBindings(observation, normalize, locale = 'en') {
  const evidence = observation.decision_evidence;
  const state = observation.position.observation.observation;
  if (state.context?.kind !== 'map' || evidence?.kind !== 'route_paths') return [];
  const commands = new Set(preparationCandidates(observation, normalize, locale));
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
        ? [preparationText(locale, 'routeCurrentHp', { hp: state.run.current_hp, maxHp: state.run.max_hp })] : []),
      ...(Number.isSafeInteger(state.public_history?.recent_combat_hp_loss)
        ? [preparationText(locale, 'routeRecentCost', { recent_combat_hp_loss: state.public_history.recent_combat_hp_loss })] : []),
      preparationText(locale, 'firstEliteWindow', { shops: beforeShop, rests: beforeCamp, fightsMin: path.min_hallway_fights_before, fightsMax: path.max_hallway_fights_before, questionsMin: path.min_unknowns_before, questionsMax: path.max_unknowns_before }),
      preparationText(locale, 'routeHorizon', { shops: totalShop, rests: totalCamp, coverage: candidate.path.coverage }),
    ], choices: [
      { id: '@first-preparation', text: preparationText(locale, 'firstElitePreparation') },
      { id: '@early-development', text: preparationText(locale, 'routeDevelopment') },
      { id: '@later-development', text: preparationText(locale, 'routeFutureFlexibility') },
      { id: '@review-access', text: preparationText(locale, 'routeWindowReview') },
    ] }];
  });
}

function duplicateTargetFacts(observation, normalize, locale = 'en') {
  const state = observation.position.observation.observation;
  if (state.context?.reason !== 'Duplicate' || state.context?.kind !== 'run_choice') return [];
  const targets = new Map(preparationSelectionTargets(observation, normalize, locale).map(row => [row.command, row]));
  return preparationCandidateGroups(observation, normalize, locale).flatMap(group => {
    const target = targets.get(group.command), current = target?.reading?.current;
    const definition = current?.definition, cost = current?.persistent_cost;
    if (!definition || definition.card !== target.card.card || definition.upgrades !== target.card.upgrades
        || !Number.isSafeInteger(cost)) return [];
    const count = (state.cards ?? []).filter(card => card.card === definition.card).reduce((n, card) => n + card.count, 0);
    if (!Number.isSafeInteger(count) || count < 1) return [];
    const fixedMechanics = [preparationText(locale, 'duplicateCopies', { card: definition.card, copiesBefore: count, copiesAfter: count + 1 })];
    const energy = state.base_energy_per_turn;
    if (cost >= 0) fixedMechanics.push(preparationText(locale, 'duplicateCost', { cost: cost })
      + (Number.isSafeInteger(energy) && cost <= energy ? preparationText(locale, 'duplicateRemainingEnergy', { baseEnergy: energy, remainingEnergy: energy - cost }) : '')
      + (definition.card_type === 'Power' ? preparationText(locale, 'duplicateCombinedCost', { cost: cost * 2 }) : ''));
    else if (cost === -1) fixedMechanics.push(preparationText(locale, 'duplicateXCost'));
    else if (cost === -2) fixedMechanics.push(preparationText(locale, 'duplicateUnplayable'));
    const effects = definition.reviewed_effects;
    const strength = effects?.player_strength_gain;
    const delayed = strength?.delivery === 'each_player_turn_start_post_draw';
    if (delayed) fixedMechanics.push(preparationText(locale, 'duplicateDelayedStrength', { strengthPerTurn: strength.amount, strengthFactor: strength.amount }));
    const reduction = effects?.enemy_strength_reduction;
    if (reduction?.restoration === 'no_scheduled_restoration') fixedMechanics.push(preparationText(locale, 'duplicateReduction', { reduction: reduction.amount, reductionFactor: reduction.amount }));
    if (effects?.base_block_delivery === 'once_on_play') fixedMechanics.push(preparationText(locale, 'duplicateBlock', { persistent_block: current.persistent_block }));
    return [{ command: group.command, fixedMechanics,
      detailed: delayed || reduction?.restoration === 'no_scheduled_restoration' }];
  });
}

function duplicateConditionBindings(observation, normalize, locale = 'en') {
  return duplicateTargetFacts(observation, normalize, locale).filter(row => row.detailed).map(({ command, fixedMechanics }) => ({
    command, id: 'permanent-copy-increment-v1', fixedMechanics, choices: [
      { id: '@first-access', text: preparationText(locale, 'duplicateFirstAccess') },
      { id: '@extra-use', text: preparationText(locale, 'duplicateExtraPlay') },
      { id: '@setup-tradeoff', text: preparationText(locale, 'duplicateSetupProtection') },
      { id: '@review-access', text: preparationText(locale, 'duplicateReview') },
    ],
  }));
}

// Derive payment thresholds from simulator facts, including grouped smith
// deltas. This is a base-Energy comparison, not an estimate of combat strength.
function preparationEnergyBindings(observation, normalize, locale = 'en') {
  const state = observation.position.observation.observation;
  const energy = state.base_energy_per_turn;
  if (!Number.isSafeInteger(energy) || energy < 0) return [];
  return preparationCandidateGroups(observation, normalize, locale).flatMap(group => {
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
    const mechanics = [preparationText(locale, 'paymentThreshold', { baseEnergy: energy, card: before.name ?? action.card, cost: before.cost })
      + (after ? preparationText(locale, 'upgradePaymentAfter', { cost: cost }) : preparationText(locale, 'extraEnergyNeeded', { cost: cost - energy }))];
    if (crosses) mechanics.push(preparationText(locale, 'upgradeCrossesThreshold', { energy: energy - cost }));
    const hpReduction = (after ?? before).cost_change_on_hp_loss;
    if (Number.isSafeInteger(hpReduction) && hpReduction < 0) {
      const events = Math.max(0, Math.ceil((cost - energy) / -hpReduction));
      mechanics.push(preparationText(locale, 'hpLossCostReduction', { costReduction: -hpReduction, eventsNeeded: events, printedCost: cost }));
    }
    return [{ command: group.command, id: 'base-energy-payment-v1', fixedMechanics: mechanics, choices: [
      { id: '@payment-access', text: preparationText(locale, 'paymentAccess') },
      ...(crosses ? [{ id: '@upgrade-payment', text: preparationText(locale, 'upgradePayment') }] : []),
      ...(Number.isSafeInteger(hpReduction) && hpReduction < 0 ? [{ id: '@hp-loss-payment', text: preparationText(locale, 'hpLossPayment') }] : []),
      { id: '@payment-tradeoff', text: preparationText(locale, 'paymentTradeoff') },
      { id: '@review-access', text: preparationText(locale, 'paymentReview') },
    ] }];
  });
}

function armamentsConditionBindings(observation, normalize, locale = 'en') {
  const state = observation.position.observation.observation;
  const owns = id => (state.cards ?? []).some(card => card.card === id && card.count > 0);
  if (!(state.cards ?? []).some(card => card.card === 'LimitBreak' && card.count > 0 && card.upgrades === 0)) return [];
  const corruption = owns('Corruption'), recovery = owns('Exhume');
  const spoon = (state.relics ?? []).some(row => row.relic === 'StrangeSpoon');
  return preparationCandidateGroups(observation, normalize, locale).flatMap(group => {
    const row = observation.position.actions.find(row => normalize(row.semantic_action, observation) === group.command);
    const acquisition = preparationCardAcquisition(observation, row);
    if (acquisition?.card !== 'Armaments') return [];
    const effect = acquisition.mechanics;
    if (effect?.card !== 'Armaments' || !Number.isSafeInteger(effect.cost) || !Number.isSafeInteger(effect.block)
        || !['selected_card_in_hand', 'whole_hand'].includes(effect.combat_upgrade_scope)) return [];
    const fixedMechanics = [
      preparationText(locale, 'armamentsAddition', { card: effect.name, cost: effect.cost, block: effect.block, scope: effect.combat_upgrade_scope === 'whole_hand' ? preparationText(locale, 'wholeHand') : preparationText(locale, 'selectedHandCard') }),
      preparationText(locale, 'limitBreakUpgrade'),
    ];
    if (corruption) fixedMechanics.push(preparationText(locale, 'corruptionUpgradeOrder'));
    if (recovery) fixedMechanics.push(preparationText(locale, 'exhumeRecovery')
      + (corruption ? preparationText(locale, 'corruptionRecovery') : preparationText(locale, 'recoveryVersusUpgrade')));
    if (spoon) fixedMechanics.push(preparationText(locale, 'spoonRecovery'));
    return [{ command: group.command, id: 'armaments-limitbreak-v1', fixedMechanics, choices: [
      { id: '@limit-redraw', text: preparationText(locale, 'limitBreakRedraw')
        + (corruption ? preparationText(locale, 'delayCorruption') : '') },
      { id: '@block-access', text: preparationText(locale, 'armamentsTradeoff') },
      { id: '@review-access', text: preparationText(locale, 'armamentsReview') },
    ] }];
  });
}

// Expert preferences supply default tradeoffs; shared Rust readings continue
// to own card effects and legal actions. Classification stays with the player.
export function preparationClassificationContext(observation, normalize, draft, locale = 'en') {
  const potionScope = shopPotionReplacementScope(observation, normalize, locale);
  if (potionScope) return renderShopPotionComparison(potionScope, draft, locale);
  const spendingScope = shopSpendingScope(observation, normalize, locale);
  if (spendingScope) return renderShopSpendingComparison(observation, spendingScope, preparationCandidateGroups(observation, normalize, locale), draft, locale);
  const state = observation.position.observation.observation;
  const cards = state.cards ?? [];
  const owns = (id, upgrades) => cards.some(card => card.card === id && card.count > 0
    && (upgrades === undefined || card.upgrades === upgrades));
  const allGroups = preparationCandidateGroups(observation, normalize, locale);
  const accepted = sameGameContextSource(draft?.source, observation) ? draft.rows : [];
  const groups = allGroups.filter(group => !accepted.some(row => row.command === group.command));
  const bindings = preparationConditionBindings(observation, normalize, locale);
  const copyFacts = new Map(duplicateTargetFacts(observation, normalize, locale).filter(row => !row.detailed)
    .map(row => [row.command, row.fixedMechanics]));
  const armamentsBound = bindings.some(binding => binding.id === 'armaments-limitbreak-v1');
  const offered = id => observation.position.actions.some(row => row.candidate?.action?.card === id
    || preparationCardAcquisition(observation, row)?.card === id);
  const checks = [
    preparationText(locale, 'checkAccessProtection'),
    preparationText(locale, 'checkPayment'),
  ];
  if (state.context?.kind === 'event') checks.push(
    preparationText(locale, 'checkEvent'));
  const selection = preparationSelectionFamily(observation, locale)
    ?? (state.context?.kind === 'run_choice' ? state.context : undefined);
  if (selection) checks.push(
    preparationText(locale, 'selectionInstruction', { reason: selection.reason, minimum: selection.min_choices, maximum: selection.max_choices }),
    ...(selection.reason === 'Purge' ? [preparationText(locale, 'purgeInstruction')] : []),
    ...(selection.reason === 'Transform' ? [preparationText(locale, 'transformInstruction')] : []),
  );
  if (selection?.reason === 'Duplicate' || observation.position.actions.some(row => row.candidate?.action?.public_semantics
      ?.structured_semantics?.effects?.some(effect => effect.effect === 'duplicate_card'))) checks.push(preparationText(locale, 'duplication'));
  if (state.context?.kind === 'card_reward') checks.push(
    preparationText(locale, 'checkCardReward'));
  if (state.context?.kind === 'boss_relic') checks.push(
    preparationText(locale, 'checkBossRelic'));
  if (state.context?.kind === 'map') checks.push(
    preparationText(locale, 'checkRoute'));
  if (state.run?.pending_act_entry) checks.push(
    preparationText(locale, 'checkActEntry', { pending_act_entry: state.run.pending_act_entry }));
  if (state.run?.act === 1 && !state.run?.pending_act_entry) checks.push(
    preparationText(locale, 'checkActOne'));
  if (state.public_map?.boss === 'TimeEater' || state.run?.act === 4) checks.push(
    preparationText(locale, 'checkCardPlayTriggers'));
  if (owns('DemonForm') && owns('LimitBreak')) checks.push(
    preparationText(locale, 'checkStrengthPlan'));
  if (offered('Disarm')) checks.push(
    preparationText(locale, 'checkDisarm'));
  if (offered('Armaments') && !armamentsBound) checks.push(
    preparationText(locale, 'checkArmaments'));
  if (offered('Armaments') && owns('LimitBreak', 0) && !owns('Corruption') && !armamentsBound) checks.push(
    preparationText(locale, 'checkArmamentsLimit'));
  if (owns('Corruption') && owns('LimitBreak') && !armamentsBound) checks.push(
    preparationText(locale, 'checkCorruptionLimit')
    + (offered('Armaments') ? preparationText(locale, 'checkArmamentsSchedule') : '')
    + preparationText(locale, 'checkRecoveryVersusCycling'));
  if (owns('Exhume') && owns('LimitBreak') && !armamentsBound) checks.push(
    preparationText(locale, 'checkExhumeLimit')
    + (owns('Corruption') ? preparationText(locale, 'checkCorruptionRecoveredLimit') : '')
    + preparationText(locale, 'checkRecoveryDestination'));
  if (offered('Evolve')) checks.push(
    preparationText(locale, 'checkEvolve'));
  if (offered('Rupture') || owns('Rupture')) checks.push(
    preparationText(locale, 'checkRupture'));
  if (offered('DualWield') || owns('DualWield')) checks.push(
    preparationText(locale, 'checkDualWield'));
  if (offered('Clash')) checks.push(
    preparationText(locale, 'checkClash'));
  const bindingByCommand = new Map(bindings.map(binding => [binding.command, binding]));
  const guidance = preparationExpertGuidance(observation, normalize, locale);
  const adviceByCommand = new Map((guidance?.candidates ?? []).map(row => [row.command, row]));
  return [...(observation.declinedRewardItems?.length ? [
    preparationText(locale, 'declinedRewards', { declinedRewardItems: observation.declinedRewardItems.map(i => i + 1).join(', ') })
  ] : []), preparationText(locale, 'pendingClassificationCount', { pending: groups.length, total: allGroups.length }),
    preparationText(locale, 'pendingCommands', { groups: groups.map(group => group.command).join('; ') }),
    ...(sameGameContextSource(draft?.source, observation) && draft.pendingChoice ? [preparationText(locale, 'savedClassificationChoice', { pendingChoice: draft.pendingChoice })] : []),
    ...(accepted.length ? [preparationText(locale, 'acceptedRows'), ...allGroups.flatMap(group => {
      const row = accepted.find(row => row.command === group.command);
      return row ? [`  ${row.command} | ${row.category} | ${row.realization} | ${row.conditionIds.join(',')}`] : [];
    })] : []),
    ...(sameGameContextSource(draft?.source, observation) && draft.issues?.length ? [preparationText(locale, 'submissionCorrections'), ...draft.issues.map(issue => `  ${issue.message}`)] : []),
    preparationText(locale, 'classificationRowFormat'),
    ...(groups.some(group => group.members.length > 1)
      ? [preparationText(locale, 'groupedRowInstruction')] : []),
    renderDeckComparison(preparationDeckComparison(observation, normalize, locale), { candidateCommands: allGroups.map(group => group.command), locale }),
    renderExpertGuidance(guidance, { includeCandidates: false }),
    renderShopBudget(observation.shopBudget, observation.shopPlan, locale),
    ...(observation.shopRemovalPolicy ? [
      preparationText(locale, 'removalTargetStrategy', { preference: observation.shopRemovalPolicy.preference }),
      ...observation.shopRemovalPolicy.conditions.map(text => preparationText(locale, 'checkedFact', { text: text })),
    ] : []),
    preparationText(locale, 'applicableChecks'), ...checks.map(check => `  ${check}`),
    ...groups.flatMap(group => {
      const binding = bindingByCommand.get(group.command);
      return [
      preparationText(locale, 'conditionReferences', { command: binding.command }),
      ...(group.members.length > 1 ? [preparationText(locale, 'sharedCopyEvaluation', { length: group.members.length })] : []),
      ...(copyFacts.has(group.command) ? [`  ${copyFacts.get(group.command).join(' ')}`] : []),
      ...(observation.candidateReadings?.[binding.command]
        ? [observation.candidateReadings[binding.command].view] : []),
      ...[renderCandidateExpertGuidance(adviceByCommand.get(binding.command))].filter(Boolean).map(text => `  ${text}`),
      ...binding.fixedMechanics.map(text => preparationText(locale, 'fixedMechanics', { text: text })),
      ...binding.choices.map(choice => `  ${choice.id}${choice.realizations ? preparationText(locale, 'realizationLabel', { realizations: choice.realizations.join('/') }) : ''}: ${choice.text}`),
      ];
    })].join('\n');
}

export function parsePreparationClassification(basis, observation, normalize = command => command, locale = 'en') {
  const collected = collectPreparationClassification(basis, observation, normalize, undefined, undefined, locale);
  if (collected.issues.length) throw new Error(collected.issues.map(issue => issue.message).join('\n'));
  if (!collected.complete) throw new Error(preparationText(locale, 'classifyRemaining', { pendingCommands: collected.pendingCommands.join(', ') }));
  return collected.rows;
}

// Independent row validation preserves usable assessments from the same source.
// Drafts are host-owned; model prose stays in the raw submission journal only.
export function collectPreparationClassification(basis, observation, normalize = command => command, previous, submission = {}, locale = 'en') {
  const potionScope = shopPotionReplacementScope(observation, normalize, locale);
  const spendingScope = shopSpendingScope(observation, normalize, locale);
  const simpleScope = potionScope ?? spendingScope;
  if (typeof basis !== 'string' || !basis.trim()) throw new Error(preparationText(locale, 'supplyBasis', { format: simpleScope ? preparationText(locale, 'simpleBasisFormat') : preparationInstructions(locale).basis }));
  const groups = preparationCandidateGroups(observation, normalize, locale), rows = new Map();
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
      if (group && rows.has(group.command)) throw new Error(preparationText(locale, 'alreadyAccepted', { command: group.command }));
      if (group && occurrences.get(group.command) > 1) {
        throw new Error(preparationText(locale, 'duplicateClassification', { command: group.command, copies: group.members.length, groupCommand: group.command }));
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
      if (!match) throw new Error(preparationText(locale, 'invalidRowFormat', { line: index + 1, format: simpleScope ? preparationText(locale, 'simpleRowGrammar') : preparationInstructions(locale).basis }));
      const [, , category, realization, reason] = match;
      if (!group) throw new Error(preparationText(locale, 'pendingCommandRequired', { index: index + 1 }));
      const binding = bindings.get(group.command);
      const conditionIds = reason.split(',').map(id => id.trim());
      const conditions = conditionIds.map(id => binding.choices.find(choice => choice.id === id));
      if (conditions.some(choice => !choice) || new Set(conditionIds).size !== conditionIds.length) {
        const extraColumns = reason.split('|');
        const trailingIds = extraColumns.length === 2 ? extraColumns[1].split(',').map(id => id.trim()) : [];
        const replacement = trailingIds.length && new Set(trailingIds).size === trailingIds.length
          && trailingIds.every(id => binding.choices.some(choice => choice.id === id))
          ? preparationText(locale, 'replacementRow', { command: group.command, category: category, realization: realization, references: trailingIds.join(',') }) : '';
        throw new Error(preparationText(locale, 'invalidConditionReferences', { command: group.command, rowCommand: group.command, category: category, realization: realization, references: binding.choices.map(choice => choice.id).join(', '), replacement: replacement }));
      }
      if ((category === 'review' || realization === 'review') !== conditionIds.includes('@review-access')) {
        throw new Error(preparationText(locale, 'reviewDimensionRequired', { command: group.command }));
      }
      // Validate typed benefit paths without changing the player's ratings.
      // A standalone Block play cannot be classified as needing acquisition.
      const paths = conditions.filter(choice => choice.realizations);
      if (realization !== 'review' && paths.some(choice => !choice.realizations.includes(realization))) {
        throw new Error(preparationText(locale, 'realizationReferenceMismatch', { command: group.command, paths: paths.map(choice => preparationText(locale, 'referenceRealizations', { id: choice.id, realizations: choice.realizations.join('/') })).join('; ') }));
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
      const selected = preparationChoiceMembers(submission.choice, observation, normalize, locale).command;
      if (pendingChoice && (selected !== pendingChoice
          || (submission.comparison !== undefined && submission.comparison !== pendingComparison))) {
        throw new Error(preparationText(locale, 'savedChoiceImmutable', { pendingChoice: pendingChoice }));
      }
      pendingChoice ??= selected;
      pendingComparison ??= submission.comparison;
    } catch (error) {
      choiceIssue = error.message;
    }
  }
  if (choiceIssue) issues.push({ field: 'choice', message: choiceIssue });
  return { source: gameContextSource(observation), complete: pendingCommands.length === 0,
    rows: preparationCandidates(observation, normalize, locale).flatMap(command => {
      const row = rows.get(membership.get(command).command);
      return row ? [{ ...row, command }] : [];
    }), acceptedCommands, pendingCommands, issues,
    ...(pendingChoice ? { pendingChoice, ...(pendingComparison !== undefined ? { pendingComparison } : {}) } : {}),
    ...(choiceIssue ? { choiceIssue } : {}) };
}

export function preparationClassificationReceipt(draft, locale = 'en') {
  return [preparationText(locale, 'classificationReceipt', { accepted: draft.acceptedCommands.length, covered: draft.rows.length }),
    ...draft.issues.map(issue => preparationText(locale, 'correction', { message: issue.message })),
    ...(draft.pendingChoice ? [preparationText(locale, 'savedChoiceShort', { pendingChoice: draft.pendingChoice })] : []),
    draft.complete ? preparationText(locale, 'completeTableRecorded') : preparationText(locale, 'classifyRemaining', { pendingCommands: draft.pendingCommands.join(', ') })].join('\n');
}

export function preparationRecordedTable(classification, observation, normalize, locale = 'en') {
  const potionScope = shopPotionReplacementScope(observation, normalize, locale);
  if (potionScope) return [preparationText(locale, 'recordedBottleRecommendations'), ...potionScope.candidates.map(candidate => {
    const row = classification.rows.find(row => row.command === candidate.command);
    return `  ${row.command} | ${row.category}\n    ${candidate.fact}`;
  }), preparationText(locale, 'chooseRecordedCommand')].join('\n');
  const spendingScope = shopSpendingScope(observation, normalize, locale);
  if (spendingScope) return renderShopSpendingComparison(observation, spendingScope, preparationCandidateGroups(observation, normalize, locale), classification, locale);
  const representatives = preparationCandidateGroups(observation, normalize, locale);
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
  return [preparationText(locale, 'recordedClassificationHeading'),
    ...(classification.pendingChoice ? [preparationText(locale, 'savedChoiceShort', { pendingChoice: classification.pendingChoice })] : []),
    preparationText(locale, 'recordedSubmission'), ...representatives.map(group => {
    const row = classification.rows.find(row => row.command === group.command);
    return `  ${row.command} | ${row.category} | ${row.realization} | ${row.conditionIds?.join(',') ?? preparationText(locale, 'basisBelow')}`;
  }), renderDeckComparison(preparationDeckComparison(observation, normalize, locale), { candidateCommands: representatives.map(group => group.command), locale }),
  renderExpertGuidance(preparationExpertGuidance(observation, normalize, locale)),
  preparationText(locale, 'recordedFacts'), ...[...shared].flatMap(([commands, texts]) => [
    preparationText(locale, 'appliesTo', { commands: commands }), ...texts.map(text => `    ${text}`),
  ]),
  renderShopBudget(observation.shopBudget, observation.shopPlan, locale),
  ...(observation.position?.observation?.observation?.context?.kind === 'map'
    ? [preparationText(locale, 'mapAdoption')] : []),
  ...(preparationSelectionFamily(observation, locale) ? [
    preparationText(locale, 'selectionAdoption', { minimum: preparationSelectionFamily(observation, locale).min_choices, maximum: preparationSelectionFamily(observation, locale).max_choices }),
  ] : representatives.length === 1 ? [preparationText(locale, 'soleCandidateAdoption', { command: representatives[0].command })] : [])].join('\n');
}

// One immutable task root, independent of model conclusions and message history.
// Preparation tools can only read or adopt gameplay. The controller head also
// changes on an adopted choice in an evidence workspace whose state revision
// stays at the original external checkpoint.
export function preparationTaskRoot(observation, locale = 'en') {
  const task = gameDecisionTask(observation);
  if (!families.has(task.family)) throw new Error(preparationText(locale, 'strategicTaskRequired'));
  const gold = observation.position?.observation?.observation?.run?.gold;
  return { source: gameContextSource(observation), task,
    ...(Number.isSafeInteger(gold) ? { startingGold: gold } : {}) };
}

export function preparationTaskProgress(root, observation, locale = 'en') {
  const source = gameContextSource(observation);
  const task = gameDecisionTask(observation, root.task);
  if (source.session_id !== root.source.session_id || source.state_revision < root.source.state_revision) {
    throw new Error(preparationText(locale, 'taskRootMismatch'));
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

function preparationChoiceMembers(choice, observation, normalize, locale = 'en') {
  const command = normalize(choice.trim(), observation);
  const candidates = preparationCandidates(observation, normalize, locale);
  const family = preparationSelectionFamily(observation, locale);
  const members = family && command.startsWith('act select:') && command !== 'act select:none'
    ? command.slice('act select:'.length).split(',').map(token => `act select:${token}`) : [command];
  if (family && command.startsWith('act ')) {
    const count = command === 'act select:none' ? 0 : members.length;
    if (new Set(members).size !== members.length || count < family.min_choices || count > family.max_choices) {
      throw new Error(preparationText(locale, 'selectTargetCount', { minimum: family.min_choices, maximum: family.max_choices }));
    }
  }
  if (members.some(member => !candidates.includes(member))) {
    throw new Error(preparationText(locale, 'selectCurrentTargets'));
  }
  return { command, members };
}

export function preparationCommandBasis([command, args], progress, basis, observation, classification, normalize = command => command, locale = 'en') {
  if (!['pending', 'selection'].includes(progress) || queries.has(command)) return;
  const candidates = preparationCandidates(observation, normalize, locale);
  if (!classification || !sameGameContextSource(classification.source, observation)
      || !Array.isArray(classification.rows) || classification.rows.length !== candidates.length
      || candidates.some(candidate => !classification.rows.some(row => row.command === candidate))) {
    throw new Error(preparationText(locale, 'completeClassificationRequired'));
  }
  const { command: selected, members } = preparationChoiceMembers([command, ...args].join(' '), observation, normalize, locale);
  const family = preparationSelectionFamily(observation, locale);
  const selectedRows = members.map(member => classification.rows.find(row => row.command === member));
  if (selectedRows.some(row => row.category === 'review' || row.realization === 'review')) {
    throw Object.assign(new Error(preparationText(locale, 'selectedReviewRequired')),
      { preparationReviewCandidate: selected });
  }
  const groups = preparationCandidateGroups(observation, normalize, locale);
  const renderRow = item => `${item.command} | ${item.category} | ${item.realization} | ${item.reason.replace(/\s+/g, ' ')}`;
  const selectedGroups = groups.filter(group => group.members.some(member => members.includes(member)));
  const alternatives = groups.filter(group => !selectedGroups.includes(group));
  if (observation.position?.observation?.observation?.context?.kind === 'map') {
    const rank = { high: 3, medium: 2, low: 1 };
    const higher = classification.rows.filter(row => row.realization !== 'review'
      && rank[row.category] > rank[selectedRows[0].category]);
    if (higher.length) throw new Error(preparationText(locale, 'higherRatedRoutes', { command: selected, category: selectedRows[0].category, higher: higher.map(row => `${row.command} (${row.category})`).join('; ') }));
  }
  const multi = family && family.max_choices > 1 && selected.startsWith('act select:');
  const bound = { command: selected, ...(selectedRows.length === 1 ? { selectedRow: selectedRows[0] } : {}), selectedRows };
  const rendered = selectedRows.map(renderRow).join('; ');
  if (shopSpendingScope(observation, normalize, locale)) {
    const keep = /^(?:then\s+)?keep the balance$/.test(basis?.trim() ?? '');
    const match = basis?.trim() && !keep ? /^then\s+([^|\r\n]+)$/.exec(basis.trim()) : undefined;
    if (basis?.trim() && !match && !keep) throw new Error(preparationText(locale, 'followupGrammar'));
    const continuation = match ? shopContinuation(observation, selected, match[1].trim(), locale) : undefined;
    return { ...bound, basisKind: 'shop-recommendations', alternativeRow: null,
      comparisonIds: [], comparisons: [], ...(continuation ? { shopContinuation: continuation } : {}),
      renderedBasis: preparationText(locale, 'shopRenderedBasis', { command: selected, category: selectedRows[0].category, continuation: continuation ? preparationText(locale, 'renderedFollowup', { next: continuation.next }) : '' }) };
  }
  if (multi) return { ...bound, basisKind: 'selection-set',
    unselectedRows: classification.rows.filter(row => !members.includes(row.command)),
    comparisonIds: [], comparisons: [], renderedBasis: preparationText(locale, 'selectionRenderedBasis', { rendered: rendered }) };
  if (!alternatives.length) return { ...bound, alternativeRow: null,
    comparisonIds: [], comparisons: [], basisKind: 'sole-candidate', renderedBasis: `${rendered}; ${groups.length === 1 ? preparationText(locale, 'soleClassifiedCandidate') : preparationText(locale, 'allGroupsSelected')}` };
  const priorityComparisons = preparationPriorityBaselines(observation, normalize, locale);
  let comparisonBasis = basis, basisKind = 'player-comparison';
  if (!basis?.trim()) {
    const required = priorityComparisons.filter(baseline => !members.some(member => baseline.commands.includes(member)));
    const eligible = alternatives.filter(group => required.every(baseline => group.members.some(member => baseline.commands.includes(member))));
    if (!eligible.length) throw new Error(preparationText(locale, 'priorityReviewRequired'));
    const rank = { high: 3, medium: 2, low: 1, review: 0 };
    const alternative = eligible.map(group => classification.rows.find(row => row.command === group.command))
      .sort((left, right) => rank[right.category] - rank[left.category])[0];
    const references = [...new Set(['@function', '@access', '@resources', ...priorityComparisons.flatMap(row => row.comparisonIds)])];
    comparisonBasis = `${alternative.command} | ${references.join(',')}`;
    basisKind = 'host-comparison';
  }
  const match = /^\s*([^|]+?)\s*\|\s*([^|\r\n]+?)(?:\s*\|\s*then\s+([^|\r\n]+))?\s*$/.exec(comparisonBasis);
  if (!match) throw new Error(preparationText(locale, 'supplyComparison', { format: preparationInstructions(locale).comparisonBasis }));
  const alternative = normalize(match[1].trim(), observation);
  const alternativeRow = classification.rows.find(row => row.command === alternative);
  if (!alternativeRow || !alternatives.some(group => group.members.includes(alternative))) {
    throw new Error(preparationText(locale, 'alternativeRequired', { alternatives: alternatives.map(group => group.command).join('; ') }));
  }
  const comparisonIds = match[2].split(',').map(id => id.trim());
  const comparisonDefinitions = preparationInstructions(locale).comparisons;
  const comparisons = comparisonIds.map(id => comparisonDefinitions.find(item => item.id === id));
  if (comparisons.some(item => !item) || new Set(comparisonIds).size !== comparisonIds.length) {
    throw new Error(preparationText(locale, 'comparisonReferencesRequired', { pREPARATION_COMPARISONS: comparisonDefinitions.map(item => item.id).join(', ') }));
  }
  for (const baseline of priorityComparisons) {
    if (!members.some(member => baseline.commands.includes(member)) && !baseline.commands.includes(alternative)) {
      throw new Error(preparationText(locale, 'priorityAlternativeRequired', { priority: baseline.id, commands: baseline.commands.join('; '), references: baseline.comparisonIds.join(',') }));
    }
    if (baseline.comparisonIds.some(id => !comparisonIds.includes(id))) {
      throw new Error(preparationText(locale, 'priorityReferencesRequired', { references: baseline.comparisonIds.join(','), priority: baseline.id }));
    }
  }
  const continuation = match[3] ? shopContinuation(observation, selected, match[3].trim(), locale) : undefined;
  return { ...bound, alternativeRow, comparisonIds, comparisons, basisKind,
    ...(priorityComparisons.length ? { priorityComparisons } : {}), ...(continuation ? { shopContinuation: continuation } : {}),
    renderedBasis: preparationText(locale, 'comparedBasis', { selected: rendered, alternative: renderRow(alternativeRow), emphasis: comparisonIds.join(',') }) };
}

function preparationPriorityBaselines(observation, normalize, locale = 'en') {
  const legal = new Set(preparationCandidates(observation, normalize, locale));
  return (preparationExpertGuidance(observation, normalize, locale)?.construction?.priorities ?? []).flatMap(priority => {
    const baseline = priority.comparisonBaseline;
    const commands = baseline?.commands.filter(command => legal.has(command));
    return commands?.length ? [{ id: priority.id, commands, comparisonIds: baseline.comparisonIds }] : [];
  });
}

export function assertPreparationCommand([command, args], progress, observation, normalize = command => command, locale = 'en') {
  if (queries.has(command) && args.length === 0) return;
  if (progress === 'complete') throw new Error(preparationProgressText(progress));
  if (command === 'classify' && !args.length && ['pending', 'selection'].includes(progress)) return;
  if (command === 'act') {
    const site = observation.position?.observation?.observation?.context?.kind;
    if (site !== 'run_choice' || !args.length || !/^select(?::|$)/.test(args[0])
        || args.some(token => token.includes(':') && !token.startsWith('select:'))) {
      throw new Error(preparationText(locale, 'displayedPreparationRequired'));
    }
    return;
  }
  const candidates = preparationCandidates(observation, normalize, locale);
  if (!candidates.includes(normalize([command, ...args].join(' '), observation))) {
    const labelSeparator = args.indexOf('|');
    const commandOnly = labelSeparator >= 0 ? normalize([command, ...args.slice(0, labelSeparator)].join(' '), observation) : undefined;
    if (candidates.includes(commandOnly)) {
      throw new Error(preparationText(locale, 'copyIntoChoice', { commandOnly: commandOnly }));
    }
    if (command === 'buy' || command.startsWith('buy:')) {
      throw new Error(preparationText(locale, 'copyCompleteCommand'));
    }
    throw new Error(preparationText(locale, 'onePreparationTask'));
  }
}
