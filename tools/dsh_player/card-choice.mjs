import { createPresentation, resolveLocale } from '@sts-simulator/presentation';
import { gameContextSource, sameGameContextSource } from './context.mjs';
import { preparationCardAcquisition, preparationDeckComparison, renderStatusRoutes, battleTranceDrawOrder,
  strengthTiming, playedCardLifecycle, otherCardExhaustComparison } from './deck.mjs';
import { preparationCardPolicies, STRATEGY_SOURCES } from './strategy.mjs';

const cardChoiceViews = Object.fromEntries(['en', 'zh-CN'].map(locale => [locale, createPresentation({ locale })]));
const cardChoiceText = (locale, key, parameters = {}) => cardChoiceViews[resolveLocale(locale)].text(`scripted:cardChoice.${key}`,
  Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));

export function cardChoiceInstructions(locale = 'en') {
  const text = key => cardChoiceText(locale, key);
  return { guide: text('guide'), automatic: text('automatic'), review: text('review'), description: text('description'),
    parameters: { command: { type: 'string', required: true, description: text('parameterCommand') },
      choice: { type: 'string', description: text('parameterChoice') } } };
}
const defaultInstructions = cardChoiceInstructions();
export const CARD_CHOICE_GUIDE = defaultInstructions.guide;
export const CARD_CHOICE_AUTOMATIC_GUIDE = defaultInstructions.automatic;
export const CARD_CHOICE_REVIEW_GUIDE = defaultInstructions.review;
export const CARD_CHOICE_TOOL_DESCRIPTION = defaultInstructions.description;
export const CARD_CHOICE_TOOL_PARAMETERS = defaultInstructions.parameters;
export const MAX_CARD_CHOICE_OPTIONS = 4;
// Bytes of the decision reading plus its guide. Tool schemas and provider
// framing have separate token overhead, measured by the runtime token meter.
export const TARGET_CARD_CHOICE_INPUT_BYTES = 4096;
const label = row => `${row.card.card}${row.card.upgrades > 0 ? '+' : ''}`;

function gapError(kind, reason, message, details = {}) {
  const error = new Error(message);
  error.preparationGap = { kind, reason, message, ...details };
  return error;
}

// Resolve policy compatibility before asking the player to compare costs.
// Keep the full legal set in the frame; an oversized comparison is a policy
// gap rather than an arbitrary truncation of that set.
export function preparationCardChoiceRoute(frame, locale = 'en') {
  if (!frame) return;
  const gap = frame.rules.find(rule => rule.gap)?.gap;
  if (gap) return { kind: 'review', commands: [], gap: { ...gap, legalOptionCount: frame.candidates.length } };
  const legal = new Set(frame.candidates.map(row => row.command));
  const policies = frame.rules.map(rule => ({ id: rule.id, commands: [...new Set(rule.commands)] }));
  if (policies.some(rule => !rule.commands.length || rule.commands.some(command => !legal.has(command)))) {
    throw gapError('strategy', 'invalid-policy-command', cardChoiceText(locale, 'invalidPolicyCommands'));
  }
  const review = (reason, message) => ({ kind: 'review', commands: [],
    gap: { kind: 'strategy', reason, message, policies, legalOptionCount: legal.size } });
  if (!policies.length) return review('missing-policy', cardChoiceText(locale, 'missingPolicy'));
  const common = policies[0].commands.filter(command => policies.every(rule => rule.commands.includes(command)));
  if (!common.length) return review('policy-conflict', cardChoiceText(locale, 'conflictingPolicies'));
  if (common.length > MAX_CARD_CHOICE_OPTIONS) return review('comparison-too-large',
    cardChoiceText(locale, 'comparisonTooLarge', { optionCount: common.length, limit: MAX_CARD_CHOICE_OPTIONS }));
  const commands = frame.candidates.filter(row => common.includes(row.command)).map(row => row.command);
  if (commands.length > 1 && !frame.rules.some(rule => rule.question)) return review('missing-comparison-question',
    cardChoiceText(locale, 'missingQuestion'));
  return { kind: commands.length === 1 ? 'automatic' : 'model', commands,
    policyIds: policies.map(rule => rule.id), legalOptionCount: legal.size };
}

// This replaces the preparation table only for a pure one-card acquisition
// window. Shop spending, random grants and composite event costs keep their
// existing owner; they cannot be silently projected into a free card choice.
export function preparationCardChoice(observation, normalize = command => command, locale = 'en') {
  const state = observation.position?.observation?.observation;
  if (!['card_reward', 'event'].includes(state?.context?.kind)) return;
  const actions = observation.position.actions;
  if (!actions?.length) return;
  const grants = [];
  for (const row of actions) {
    const action = row.candidate?.action;
    if (['skip_card_reward', 'singing_bowl'].includes(action?.kind)) continue;
    const acquisition = preparationCardAcquisition(observation, row);
    if (action?.kind === 'take_card' && !acquisition?.mechanics) {
      throw gapError('facts', 'missing-candidate-mechanics', cardChoiceText(locale, 'missingCandidateMechanics'),
        { command: row.semantic_action });
    }
    if (!acquisition || !acquisition.mechanics || action.kind === 'buy_card') return;
    if (action.kind === 'choose_event_option') {
      const semantics = action.public_semantics;
      if (semantics.hp_delta !== 0 || semantics.max_hp_delta !== 0
          || semantics.gold_delta_min !== 0 || semantics.gold_delta_max !== 0
          || semantics.starts_combat || semantics.random_outcome) return;
    }
    grants.push({ command: normalize(row.semantic_action, observation), ...acquisition });
  }
  if (!grants.length) return;
  const profile = preparationDeckComparison(observation, normalize, locale);
  if (!profile) throw gapError('facts', 'missing-owned-mechanics', cardChoiceText(locale, 'missingOwnedMechanics'));
  const owned = observation.deck_evidence.cards;
  const exhausters = owned.filter(row => row.mechanics.exhausts_other_cards);
  const names = rows => rows.map(row => `${row.card.count}x ${label(row)}`).join(', ') || cardChoiceText(locale, 'none');
  const rules = preparationCardPolicies(observation, profile, normalize, locale);
  const dependencies = new Set(rules.flatMap(rule => rule.dependencies ?? []));
  const relicRules = [];
  if (dependencies.size && (state.relics ?? []).some(row => row.relic === 'MummifiedHand')
      && owned.some(row => row.mechanics.card_type === 'Power')) {
    const native = observation.ownedRelicReadings?.MummifiedHand?.trim();
    relicRules.push({ relic: 'MummifiedHand', native });
  }
  if (dependencies.has('attack-play')) {
    for (const relic of ['Kunai', 'Shuriken', 'OrnamentalFan', 'Nunchaku', 'PenNib']) {
      if (!(state.relics ?? []).some(row => row.relic === relic)) continue;
      const native = observation.ownedRelicReadings?.[relic]?.trim();
      relicRules.push({ relic, native });
    }
  }
  const candidates = actions.map(row => {
    const command = normalize(row.semantic_action, observation);
    const grant = grants.find(item => item.command === command);
    if (!grant) {
      const resource = profile.candidates.find(item => item.command === command)?.resource;
      return { command, kind: resource ? 'resource' : 'skip',
        facts: [cardChoiceText(locale, 'keepDeck', { size: profile.size }), ...(resource ? [
          cardChoiceText(locale, 'resourceOutcome', { hpBefore: state.run.current_hp, hpAfter: state.run.current_hp + resource.hpGain, maxHpBefore: state.run.max_hp, maxHpAfter: state.run.max_hp + resource.maxHpGain }),
          cardChoiceText(locale, 'nativeRule', { nativeRule: resource.nativeRule }),
        ] : [])] };
    }
    const m = grant.mechanics;
    const comparisonGuidance = m.exhausts_other_cards ? [otherCardExhaustComparison(locale)] : [];
    const facts = [cardChoiceText(locale, 'gainCard', { card: grant.card, upgradeSuffix: grant.upgrades > 0 ? '+' : '', sizeBefore: profile.size, sizeAfter: profile.size + 1, cost: m.cost === -1 ? 'X' : m.cost })];
    facts.push(cardChoiceText(locale, 'playedCopy', { type: m.card_type, lifecycle: playedCardLifecycle({ type: m.card_type, exhausts: m.exhausts, ethereal: m.ethereal }, locale) }));
    if (Array.isArray(m.tags)) facts.push(cardChoiceText(locale, 'cardTags', { tags: m.tags.join(', ') || cardChoiceText(locale, 'none') }));
    if (m.damage?.delivery?.amount?.kind === 'base_plus_strike_cards') {
      if (!profile.strikeTags || !Array.isArray(m.tags)) throw gapError('facts', 'missing-strike-tags',
        cardChoiceText(locale, 'missingStrikeTags'));
      const added = m.tags.includes('Strike') ? 1 : 0;
      facts.push(cardChoiceText(locale, 'strikePool', { copiesBefore: profile.strikeTags.copies, owned: profile.strikeTags.owned, copiesAfter: profile.strikeTags.copies + added }));
    }
    if (m.base_block_delivery === 'once_on_play' && m.block > 0) facts.push(cardChoiceText(locale, 'blockOnPlay', { block: m.block }));
    if (m.draw_cards > 0) facts.push(cardChoiceText(locale, 'drawOnPlay', { draws: m.draw_cards, selection: m.exhausts_other_cards ? cardChoiceText(locale, 'otherCardSelection') : '' }));
    if (m.energy_gain_rule?.delivery === 'on_play' && m.energy_gain_rule.amount > 0) {
      facts.push(cardChoiceText(locale, 'energyOnPlay', { energy: m.energy_gain_rule.amount, hpLoss: m.self_hp_loss_on_play
        ? cardChoiceText(locale, 'baseHpLoss', { base_hp_loss: m.self_hp_loss_on_play.base_hp_loss }) : '' }));
    }
    if (m.hand_topdeck_after_draw > 0) facts.push(cardChoiceText(locale, 'handTopdeck', { topdeckCount: m.hand_topdeck_after_draw, handChange: m.draw_cards - 1 - m.hand_topdeck_after_draw }));
    if (m.top_deck_play) {
      if (m.top_deck_play.bypass_fixed_energy_cost === true) facts.push(cardChoiceText(locale, 'topPlayBypassCost'));
      if (m.top_deck_play.x_cost_uses_current_energy === true) facts.push(cardChoiceText(locale, 'topPlayXCost'));
      if (m.top_deck_play.exhaust_after_play === true) facts.push(cardChoiceText(locale, 'topPlayLifecycle'));
      const handPlanners = owned.filter(source => source.mechanics.hand_topdeck_after_draw > 0);
      const discardPlanners = owned.filter(source => source.card.card === 'Headbutt');
      facts.push(cardChoiceText(locale, 'topdeckPlanning', { handPlanners: names(handPlanners), discardPlanners: names(discardPlanners) }));
    }
    if (m.card === 'BattleTrance' && m.draw_cards > 0) facts.push(battleTranceDrawOrder(profile, locale));
    if (m.energy_gain_on_exhaust > 0) {
      const routes = [...exhausters.map(row => `${row.card.count}x ${label(row)}`),
        ...(m.card_type === 'Skill' ? profile.skillExhaustPowers.map(row => cardChoiceText(locale, 'afterDeployment', { card: row.card })) : []),
        ...(m.exhausts ? [cardChoiceText(locale, 'selfExhaustRoute')] : []), ...(m.ethereal ? [cardChoiceText(locale, 'selfEtherealRoute')] : [])];
      facts.push(cardChoiceText(locale, 'selfExhaustEnergy', { energy: m.energy_gain_on_exhaust, routes: routes.join(', ') || cardChoiceText(locale, 'none') }));
    }
    if (['Evolve', 'FireBreathing'].includes(m.card)) {
      facts.push(cardChoiceText(locale, 'statusRoutes', { renderStatusRoutes: renderStatusRoutes(profile.statusGeneration, locale) || cardChoiceText(locale, 'none') }));
      const original = profile.statusInventory.filter(row => m.card === 'FireBreathing' || row.type === 'Status');
      facts.push(cardChoiceText(locale, 'originalQualifyingCards', { original: original.map(row => row.card).join(', ') || cardChoiceText(locale, 'none') }));
    }
    if (m.temporary_strength_gain > 0) facts.push(cardChoiceText(locale, 'temporaryStrengthAttacks', { attackCopies: profile.attackCopies }));
    return { command, kind: 'gain', card: grant.card, upgrades: grant.upgrades, facts,
      comparisonGuidance,
      nativeReading: observation.candidateReadings?.[command]?.card_rules
        ?.find(rule => rule.card === grant.card && rule.upgrades === grant.upgrades)?.view,
      nativeRuleAvailable: observation.candidateReadings?.[command]?.card_rules
        ?.some(rule => rule.card === grant.card && rule.upgrades === grant.upgrades && rule.available) === true };
  });
  const factGroups = {
    strength: [cardChoiceText(locale, 'enduringStrength', { strengthSources: profile.strengthSources.map(row => cardChoiceText(locale, 'strengthSource', { card: row.card, type: row.type, cost: row.cost, strength: row.amount, timing: strengthTiming(row.delivery, locale), lifecycle: playedCardLifecycle(row, locale) })).join('; ') || cardChoiceText(locale, 'noNativeSource') })],
    retention: [cardChoiceText(locale, 'blockRetention', { retention: profile.blockSupport.retention.map(row => cardChoiceText(locale, 'retentionSource', { card: row.card, type: row.type, cost: row.cost })).join('; ') || cardChoiceText(locale, 'none') })],
    draw: [cardChoiceText(locale, 'onPlayDrawSources', { onPlayDraw: profile.onPlayDraw.map(row => cardChoiceText(locale, 'drawSource', { card: row.card, draws: row.requested, cost: row.cost, type: row.type, lifecycle: playedCardLifecycle(row, locale) })).join('; ') || cardChoiceText(locale, 'none') })],
    exhaust: [cardChoiceText(locale, 'exhaustSources', { exhausters: names(exhausters), skillExhaustPowers: profile.skillExhaustPowers.map(row => cardChoiceText(locale, 'energyCost', { card: row.card, cost: row.cost })).join(', ') || cardChoiceText(locale, 'none') })],
    'exhaust-draw': [cardChoiceText(locale, 'exhaustDrawPowers', { exhaustDraw: profile.exhaustDraw.map(row => cardChoiceText(locale, 'exhaustDrawSource', { card: row.card, cost: row.cost, draws: row.requested })).join('; ') || cardChoiceText(locale, 'none') })],
    payment: [cardChoiceText(locale, 'onPlayEnergySources', { energySources: profile.onPlayEnergy.map(row => cardChoiceText(locale, 'onPlayEnergySource', { card: row.card, cost: row.cost, gain: row.amount, hpLoss: row.hpLoss })).join('; ') || cardChoiceText(locale, 'none'), skillExhaustPowers: profile.skillExhaustPowers.map(row => cardChoiceText(locale, 'energyCost', { card: row.card, cost: row.cost })).join(', ') || cardChoiceText(locale, 'none') })],
    'multi-hit': [cardChoiceText(locale, 'multiHitAttacks', { attacks: profile.attacks.filter(row => row.delivery?.hits?.count > 1 || row.delivery?.hits?.kind === 'per_effective_x').map(row => cardChoiceText(locale, 'multiHitSource', { card: row.card, cost: row.cost === -1 ? 'X' : row.cost, hits: row.delivery.hits.count ?? row.delivery.hits.kind, targets: row.delivery.targets })).join('; ') || cardChoiceText(locale, 'none') })],
    'attack-play': [], // Complete native relic blocks supply these counters.
  };
  const frame = { schema: 'sts-preparation-card-choice-v1', source: gameContextSource(observation),
    run: state.run, baseEnergy: profile.baseEnergy, size: profile.size,
    inventory: names(owned), rules, candidates, dependencies: [...dependencies],
    defaultCommands: [...new Set(rules.filter(rule => rule.origin === 'maintainer_strategy_default').flatMap(rule => rule.defaultCommands ?? rule.commands))],
    ownedFacts: [...dependencies].flatMap(id => factGroups[id]), relicRules,
    sources: Object.fromEntries([...new Set(rules.flatMap(rule => rule.sources))].map(id => [id, STRATEGY_SOURCES[id]])) };
  frame.route = preparationCardChoiceRoute(frame, locale);
  frame.defaultCommands = frame.defaultCommands.filter(command => frame.route.commands.includes(command));
  return frame;
}

function nativeRuleLines(text) {
  // Align the complete source block; no heading or rule-language recognition.
  return (text ?? '').split('\n').map(line => line.trim()).filter(Boolean);
}

export function renderPreparationCardChoice(frame, locale = 'en') {
  if (!frame) return '';
  const route = preparationCardChoiceRoute(frame, locale);
  const options = frame.candidates.filter(row => route.commands.includes(row.command));
  const missingRelics = (frame.relicRules ?? []).filter(row => !row.native);
  if (route.kind === 'model' && missingRelics.length) throw gapError('facts', 'missing-payment-relic-rules',
    cardChoiceText(locale, 'missingRelicRules'),
    { relics: missingRelics.map(row => row.relic) });
  const missingRules = options.filter(row => row.kind === 'gain'
    && (!row.nativeRuleAvailable || !row.nativeReading?.trim()));
  if (missingRules.length) throw gapError('facts', 'missing-candidate-rules',
    cardChoiceText(locale, 'missingPrintedRules'),
    { commands: missingRules.map(row => row.command) });
  const reading = [
    cardChoiceText(locale, 'heading', { act: frame.run.act, floor: frame.run.floor, hp: frame.run.current_hp, maxHp: frame.run.max_hp, energy: frame.baseEnergy }),
    cardChoiceText(locale, 'deckSize', { size: frame.size }),
    ...(route.kind === 'model' && frame.defaultCommands.length
      ? [cardChoiceText(locale, 'maintainerDefault', { defaultCommands: frame.defaultCommands.join('; ') })] : []),
    cardChoiceText(locale, 'checkedFacts'), ...frame.ownedFacts.map(text => `  ${text}`),
    ...(frame.relicRules ?? []).filter(row => row.native).map(row => cardChoiceText(locale, 'ownedRelic', { native: row.native })),
    cardChoiceText(locale, 'appliedStrategy'),
    ...(frame.rules.length ? frame.rules.flatMap(rule => [
      `  ${rule.id}: ${rule.preference}`,
      ...rule.conditions.map(text => cardChoiceText(locale, 'checkedCondition', { text: text })),
    ]) : [cardChoiceText(locale, 'policyNeeded')]),
    ...(route.kind === 'review' ? [cardChoiceText(locale, 'strategyGap', { message: route.gap.message }),
      ...(route.gap.candidates ?? []).map(row => `  ${row.command}: ${row.policy}`)]
      : [cardChoiceText(locale, 'comparisonOptions', { shownCount: options.length, legalCount: frame.candidates.length })]),
    ...(route.kind === 'model' ? [cardChoiceText(locale, 'comparisonQuestion', { question: frame.rules.findLast(rule => rule.question).question })] : []),
    ...options.flatMap(row => [
      `  ${row.command}`, ...row.facts.map(text => `    ${text}`),
      ...nativeRuleLines(row.nativeReading).map(text => `    ${text}`),
      ...(row.comparisonGuidance ?? []).map(text => cardChoiceText(locale, 'selectionComparison', { text: text })),
    ]),
    cardChoiceText(locale, 'currentStep', { kind: route.kind === 'review' ? cardChoiceText(locale, 'recordGap') : cardChoiceText(locale, 'chooseOption') }),
  ].join('\n');
  const instructions = cardChoiceInstructions(locale);
  const guide = { automatic: instructions.automatic, model: instructions.guide, review: instructions.review }[route.kind];
  const bytes = Buffer.byteLength(reading) + Buffer.byteLength(guide);
  frame.inputBudget = { bytes, target: TARGET_CARD_CHOICE_INPUT_BYTES, exceeded: bytes > TARGET_CARD_CHOICE_INPUT_BYTES,
    includes: ['reading', 'card-choice-guide'], componentBytes: [Buffer.byteLength(reading), Buffer.byteLength(guide)] };
  return reading;
}

export function bindPreparationCardChoice(choice, frame, observation, owner = 'flash-card-choice', locale = 'en') {
  if (!frame || !sameGameContextSource(frame.source, observation)) throw new Error(cardChoiceText(locale, 'sourceChanged'));
  const route = preparationCardChoiceRoute(frame, locale);
  if (route.kind === 'review') throw gapError(route.gap.kind, route.gap.reason, route.gap.message, { ...route.gap });
  const selected = frame.candidates.find(row => row.command === choice?.trim());
  if (!selected) throw new Error(cardChoiceText(locale, 'copyCommand', { candidates: frame.candidates.map(row => row.command).join('; ') }));
  if (!route.commands.includes(selected.command)) throw new Error(cardChoiceText(locale, 'choosePolicyOption', { commands: route.commands.join('; ') }));
  if (!['flash-card-choice', 'host-card-policy'].includes(owner)
      || (owner === 'host-card-policy' && route.kind !== 'automatic')) {
    throw new Error(cardChoiceText(locale, 'automaticPolicyRequired'));
  }
  return { schema: 'sts-preparation-card-choice-adoption-v1', source: frame.source, command: selected.command,
    owner, route,
    selected, followsDefault: frame.defaultCommands.includes(selected.command),
    suppliedPolicies: frame.rules, defaultCommands: frame.defaultCommands,
    basisKind: 'host-checked-card-choice', renderedBasis: cardChoiceText(locale, 'adoptedBasis', { command: selected.command, policyIds: route.policyIds.join('; ') }) };
}

// Both the flow's automatic choice and the player's bounded choice use this
// source check and intent/applied journal around the real workbench execution.
export async function adoptPreparationCardChoice(game, frame, observation, { choice, owner, signal, record, locale = 'en' }) {
  const adopted = bindPreparationCardChoice(choice, frame, observation, owner, locale);
  if (!sameGameContextSource(frame.source, await game.observe({ signal }))) {
    throw new Error(cardChoiceText(locale, 'positionChangedBeforeChoice'));
  }
  await record({ ...adopted, phase: 'intent', at: new Date().toISOString(), frame });
  if (!sameGameContextSource(frame.source, await game.observe({ signal }))) {
    throw new Error(cardChoiceText(locale, 'positionChangedDuringRecord'));
  }
  let receipt, successor;
  try {
    receipt = await game.execute(adopted.command, { signal });
    successor = await game.observe({ signal });
    await record({ schema: adopted.schema, owner: adopted.owner,
      phase: 'applied', at: new Date().toISOString(), source: adopted.source,
      command: adopted.command, successor: gameContextSource(successor) });
    return { adopted, successor, receipt };
  } catch (error) {
    // A returned receipt acknowledges execution. A changed revision alone
    // cannot establish that an interrupted request applied this command.
    if (receipt) error.preparationExecution = { adopted, receipt, successor };
    throw error;
  }
}
