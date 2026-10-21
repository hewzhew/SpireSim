import { gameContextSource, sameGameContextSource } from './context.mjs';
import { preparationCardAcquisition, preparationDeckComparison, renderStatusRoutes, battleTranceDrawOrder,
  strengthTiming, playedCardLifecycle, OTHER_CARD_EXHAUST_COMPARISON } from './deck.mjs';
import { preparationCardPolicies, STRATEGY_SOURCES } from './strategy.mjs';

export const CARD_CHOICE_GUIDE = `Apply the supplied comparison question to the listed options. Submit one complete option command in choice with command choose. Use sts_feedback for a concrete missing fact or policy.`;
export const CARD_CHOICE_AUTOMATIC_GUIDE = 'The host supplied one policy choice. Submit that complete option command in choice with command choose.';
export const CARD_CHOICE_REVIEW_GUIDE = 'The host retained this reward for policy review. Record the supplied strategy gap through sts_feedback.';
export const CARD_CHOICE_TOOL_DESCRIPTION = 'Choose one supplied reward option. read recovers the current step.';
export const MAX_CARD_CHOICE_OPTIONS = 4;
// Bytes of the decision reading plus its guide. Tool schemas and provider
// framing have separate token overhead, measured by the runtime token meter.
export const TARGET_CARD_CHOICE_INPUT_BYTES = 4096;
export const CARD_CHOICE_TOOL_PARAMETERS = {
  command: { type: 'string', required: true, description: 'choose to submit; read to recover.' },
  choice: { type: 'string', description: 'For choose: copy one complete supplied option command.' },
};

const label = row => `${row.card.card}${row.card.upgrades > 0 ? '+' : ''}`;

function gapError(kind, reason, message, details = {}) {
  const error = new Error(message);
  error.preparationGap = { kind, reason, message, ...details };
  return error;
}

// Resolve policy compatibility before asking the player to compare costs.
// Keep the full legal set in the frame; an oversized comparison is a policy
// gap rather than an arbitrary truncation of that set.
export function preparationCardChoiceRoute(frame) {
  if (!frame) return;
  const gap = frame.rules.find(rule => rule.gap)?.gap;
  if (gap) return { kind: 'review', commands: [], gap: { ...gap, legalOptionCount: frame.candidates.length } };
  const legal = new Set(frame.candidates.map(row => row.command));
  const policies = frame.rules.map(rule => ({ id: rule.id, commands: [...new Set(rule.commands)] }));
  if (policies.some(rule => !rule.commands.length || rule.commands.some(command => !legal.has(command)))) {
    throw gapError('strategy', 'invalid-policy-command', 'Card-choice policies require current legal commands.');
  }
  const review = (reason, message) => ({ kind: 'review', commands: [],
    gap: { kind: 'strategy', reason, message, policies, legalOptionCount: legal.size } });
  if (!policies.length) return review('missing-policy', 'This card choice needs a supplied strategy policy.');
  const common = policies[0].commands.filter(command => policies.every(rule => rule.commands.includes(command)));
  if (!common.length) return review('policy-conflict', 'The matched strategy policies need a priority decision.');
  if (common.length > MAX_CARD_CHOICE_OPTIONS) return review('comparison-too-large',
    `The matched strategy policies need a smaller comparison: ${common.length} options, limit ${MAX_CARD_CHOICE_OPTIONS}.`);
  const commands = frame.candidates.filter(row => common.includes(row.command)).map(row => row.command);
  if (commands.length > 1 && !frame.rules.some(rule => rule.question)) return review('missing-comparison-question',
    'This comparison needs a supplied policy question.');
  return { kind: commands.length === 1 ? 'automatic' : 'model', commands,
    policyIds: policies.map(rule => rule.id), legalOptionCount: legal.size };
}

// This replaces the preparation table only for a pure one-card acquisition
// window. Shop spending, random grants and composite event costs keep their
// existing owner; they cannot be silently projected into a free card choice.
export function preparationCardChoice(observation, normalize = command => command) {
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
      throw gapError('facts', 'missing-candidate-mechanics', 'Card choice needs the offered card\'s matching native mechanics.',
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
  const profile = preparationDeckComparison(observation, normalize);
  if (!profile) throw gapError('facts', 'missing-owned-mechanics', 'Card choice needs native mechanics for the current owned inventory.');
  const owned = observation.deck_evidence.cards;
  const exhausters = owned.filter(row => row.mechanics.exhausts_other_cards);
  const names = rows => rows.map(row => `${row.card.count}x ${label(row)}`).join(', ') || 'none';
  const rules = preparationCardPolicies(observation, profile, normalize);
  const dependencies = new Set(rules.flatMap(rule => rule.dependencies ?? []));
  const relicRules = [];
  if (dependencies.size && (state.relics ?? []).some(row => row.relic === 'MummifiedHand')
      && owned.some(row => row.mechanics.card_type === 'Power')) {
    // Select the complete native display block for this known payment relic.
    // Its random eligible-Hand condition remains part of the supplied rule.
    const native = /^  Mummified Hand\r?\n(?: {4}[^\r\n]+(?:\r?\n|$))+/m.exec(observation.ownedResourceReading ?? '')?.[0]?.trim();
    relicRules.push({ relic: 'MummifiedHand', native });
  }
  if (dependencies.has('attack-play')) {
    for (const [relic, name] of Object.entries({ Kunai: 'Kunai', Shuriken: 'Shuriken',
      OrnamentalFan: 'Ornamental Fan', Nunchaku: 'Nunchaku', PenNib: 'Pen Nib' })) {
      if (!(state.relics ?? []).some(row => row.relic === relic)) continue;
      const native = new RegExp(`^  ${name}\\r?\\n(?: {4}[^\\r\\n]+(?:\\r?\\n|$))+`, 'm')
        .exec(observation.ownedResourceReading ?? '')?.[0]?.trim();
      relicRules.push({ relic, native });
    }
  }
  const candidates = actions.map(row => {
    const command = normalize(row.semantic_action, observation);
    const grant = grants.find(item => item.command === command);
    if (!grant) {
      const resource = profile.candidates.find(item => item.command === command)?.resource;
      return { command, kind: resource ? 'resource' : 'skip',
        facts: [`Keep the ${profile.size}-card permanent deck.`, ...(resource ? [
          `Immediate native outcome: HP ${state.run.current_hp}->${state.run.current_hp + resource.hpGain}; Max HP ${state.run.max_hp}->${state.run.max_hp + resource.maxHpGain}. This consumes the card reward.`,
          `Native rule: ${resource.nativeRule}`,
        ] : [])] };
    }
    const m = grant.mechanics;
    const comparisonGuidance = m.exhausts_other_cards ? [OTHER_CARD_EXHAUST_COMPARISON] : [];
    const facts = [`Gain one ${grant.card}${grant.upgrades > 0 ? '+' : ''}; deck ${profile.size}->${profile.size + 1}; play cost ${m.cost === -1 ? 'X' : m.cost} Energy.`];
    facts.push(`Played copy: ${m.card_type}; ${playedCardLifecycle({ type: m.card_type, exhausts: m.exhausts, ethereal: m.ethereal })} before other deployed effects.`);
    if (Array.isArray(m.tags)) facts.push(`Native card tags: ${m.tags.join(', ') || 'none'}.`);
    if (m.damage?.delivery?.amount?.kind === 'base_plus_strike_cards') {
      if (!profile.strikeTags || !Array.isArray(m.tags)) throw gapError('facts', 'missing-strike-tags',
        'Perfected Strike comparison needs native tags for the owned originals and offered card.');
      const added = m.tags.includes('Strike') ? 1 : 0;
      facts.push(`Original Strike-tagged pool: ${profile.strikeTags.copies} copies (${profile.strikeTags.owned}). Adding this card brings it to ${profile.strikeTags.copies + added}. Counted cards use Hand, Draw and Discard; creation, Exhaust and recovery change that pool during combat.`);
    }
    if (m.base_block_delivery === 'once_on_play' && m.block > 0) facts.push(`On play: ${m.block} Block before combat modifiers.`);
    if (m.draw_cards > 0) facts.push(`On play: request ${m.draw_cards} draws${m.exhausts_other_cards ? ' with the printed other-card selection' : ''}.`);
    if (m.energy_gain_rule?.delivery === 'on_play' && m.energy_gain_rule.amount > 0) {
      facts.push(`On play: gain ${m.energy_gain_rule.amount} Energy${m.self_hp_loss_on_play
        ? `; base HP loss ${m.self_hp_loss_on_play.base_hp_loss} before the supplied combat modifiers` : ''}.`);
    }
    if (m.hand_topdeck_after_draw > 0) facts.push(`After drawing, place ${m.hand_topdeck_after_draw} Hand card on top of Draw. Spending this card, drawing and placing back contributes ${m.draw_cards - 1 - m.hand_topdeck_after_draw} cards to Hand before other effects and capacity. The top card awaits a later draw.`);
    if (m.top_deck_play) {
      if (m.top_deck_play.bypass_fixed_energy_cost === true) facts.push('Top-card play bypasses fixed Energy cost.');
      if (m.top_deck_play.x_cost_uses_current_energy === true) facts.push('X-cost uses current Energy and the target card\'s own Energy spending rule.');
      if (m.top_deck_play.exhaust_after_play === true) facts.push('Played Powers deploy and remain active; other targets follow the supplied Exhaust-after-play rule.');
      const handPlanners = owned.filter(source => source.mechanics.hand_topdeck_after_draw > 0);
      const discardPlanners = owned.filter(source => source.card.card === 'Headbutt');
      facts.push(`Owned Hand-to-top-of-Draw planning: ${names(handPlanners)}; use their draw first, then select the Hand card to put on top. Owned Discard-to-top-of-Draw planning: ${names(discardPlanners)}; the selected target must already be in Discard. Use Havoc after placing the intended target and before another draw removes it.`);
    }
    if (m.card === 'BattleTrance' && m.draw_cards > 0) facts.push(battleTranceDrawOrder(profile));
    if (m.energy_gain_on_exhaust > 0) {
      const routes = [...exhausters.map(row => `${row.card.count}x ${label(row)}`),
        ...(m.card_type === 'Skill' ? profile.skillExhaustPowers.map(row => `${row.card} after deployment`) : []),
        ...(m.exhausts ? ['this card\'s Exhaust on play'] : []), ...(m.ethereal ? ['this card\'s Ethereal at turn end'] : [])];
      facts.push(`When this card itself Exhausts: ${m.energy_gain_on_exhaust} Energy. Owned trigger routes: ${routes.join(', ') || 'none'}.`);
    }
    if (['Evolve', 'FireBreathing'].includes(m.card)) {
      facts.push(`Owned generated Status routes: ${renderStatusRoutes(profile.statusGeneration) || 'none'}.`);
      const original = profile.statusInventory.filter(row => m.card === 'FireBreathing' || row.type === 'Status');
      facts.push(`Owned original qualifying cards: ${original.map(row => row.card).join(', ') || 'none'}. Deploy this Power before the triggering draw.`);
    }
    if (m.temporary_strength_gain > 0) facts.push(`Same-turn attack support uses only owned attacks: ${profile.attackCopies}.`);
    return { command, kind: 'gain', card: grant.card, upgrades: grant.upgrades, facts,
      comparisonGuidance,
      nativeReading: observation.candidateReadings?.[command] };
  });
  const factGroups = {
    strength: [`Enduring Strength: ${profile.strengthSources.map(row => `${row.card}: ${row.type}, ${row.cost} Energy; gain ${row.amount} Strength ${strengthTiming(row.delivery)}; ${playedCardLifecycle(row)}`).join('; ') || 'none from the supplied native cards'}.`],
    retention: [`Block retention: ${profile.blockSupport.retention.map(row => `${row.card}: ${row.type}, ${row.cost} Energy; preserves surviving Block across turns after deployment`).join('; ') || 'none'}.`],
    draw: [`Owned on-play draw requests: ${profile.onPlayDraw.map(row => `${row.card}: ${row.requested} draws, ${row.cost} Energy; ${row.type}; ${playedCardLifecycle(row)}`).join('; ') || 'none'}.`],
    exhaust: [`Other-card Exhaust: ${names(exhausters)}. Skill-Exhaust Power: ${profile.skillExhaustPowers.map(row => `${row.card}, ${row.cost} Energy`).join(', ') || 'none'}.`],
    'exhaust-draw': [`Owned Exhaust-draw Powers: ${profile.exhaustDraw.map(row => `${row.card}, ${row.cost} Energy; ${row.requested} draws per Exhaust after deployment`).join('; ') || 'none'}.`],
    payment: [`Owned on-play Energy: ${profile.onPlayEnergy.map(row => `${row.card}, ${row.cost} Energy; gain ${row.amount} Energy, lose ${row.hpLoss} HP`).join('; ') || 'none'}. Skill-Exhaust Power: ${profile.skillExhaustPowers.map(row => `${row.card}, ${row.cost} Energy`).join(', ') || 'none'}.`],
    'multi-hit': [`Owned multi-hit attacks: ${profile.attacks.filter(row => row.delivery?.hits?.count > 1 || row.delivery?.hits?.kind === 'per_effective_x').map(row => `${row.card}, ${row.cost === -1 ? 'X' : row.cost} Energy; ${row.delivery.hits.count ?? row.delivery.hits.kind} hits; ${row.delivery.targets}`).join('; ') || 'none'}.`],
    'attack-play': [], // Complete native relic blocks supply these counters.
  };
  const frame = { schema: 'sts-preparation-card-choice-v1', source: gameContextSource(observation),
    run: state.run, baseEnergy: profile.baseEnergy, size: profile.size,
    inventory: names(owned), rules, candidates, dependencies: [...dependencies],
    defaultCommands: [...new Set(rules.filter(rule => rule.origin === 'maintainer_strategy_default').flatMap(rule => rule.defaultCommands ?? rule.commands))],
    ownedFacts: [...dependencies].flatMap(id => factGroups[id]), relicRules,
    sources: Object.fromEntries([...new Set(rules.flatMap(rule => rule.sources))].map(id => [id, STRATEGY_SOURCES[id]])) };
  frame.route = preparationCardChoiceRoute(frame);
  frame.defaultCommands = frame.defaultCommands.filter(command => frame.route.commands.includes(command));
  return frame;
}

function nativeRuleLines(text) {
  // Keep the original full printed rule and exact upgraded requests. Remove
  // repeated instructions for interpreting it, not parts of the card effect.
  return (text ?? '').split('\n').filter(line => /^\s*Card |^\s*Printed rule|^\s*Printed (Block|draw|base damage|damage) request/.test(line)).map(line => line.trim());
}

export function renderPreparationCardChoice(frame) {
  if (!frame) return '';
  const route = preparationCardChoiceRoute(frame);
  const options = frame.candidates.filter(row => route.commands.includes(row.command));
  const missingRelics = (frame.relicRules ?? []).filter(row => !row.native);
  if (route.kind === 'model' && missingRelics.length) throw gapError('facts', 'missing-payment-relic-rules',
    'Card comparison needs the native rules of the owned supporting relics.',
    { relics: missingRelics.map(row => row.relic) });
  const missingRules = options.filter(row => row.kind === 'gain'
    && !nativeRuleLines(row.nativeReading).some(line => /^Printed rule/.test(line)));
  if (missingRules.length) throw gapError('facts', 'missing-candidate-rules',
    'Card comparison needs the full native printed rule for each supplied gain.',
    { commands: missingRules.map(row => row.command) });
  const reading = [
    `Card choice | Act ${frame.run.act} F${frame.run.floor} | HP ${frame.run.current_hp}/${frame.run.max_hp} | ${frame.baseEnergy} Energy/turn`,
    `Permanent deck size: ${frame.size}.`,
    ...(route.kind === 'model' && frame.defaultCommands.length
      ? [`Maintainer default: ${frame.defaultCommands.join('; ')}.`] : []),
    'Checked owned facts:', ...frame.ownedFacts.map(text => `  ${text}`),
    ...(frame.relicRules ?? []).filter(row => row.native).map(row => `  Owned relic: ${row.native}`),
    'Applied strategy (maintainer preferences):',
    ...(frame.rules.length ? frame.rules.flatMap(rule => [
      `  ${rule.id}: ${rule.preference}`,
      ...rule.conditions.map(text => `    Checked: ${text}`),
    ]) : ['  This window needs a supplied strategy policy. Record the missing policy through sts_feedback.']),
    ...(route.kind === 'review' ? [`Strategy gap: ${route.gap.message}`,
      ...(route.gap.candidates ?? []).map(row => `  ${row.command}: ${row.policy}`)]
      : [`Policy comparison options (${options.length} of ${frame.candidates.length} legal options; full set retained by host):`]),
    ...(route.kind === 'model' ? [`Comparison question: ${frame.rules.findLast(rule => rule.question).question}`] : []),
    ...options.flatMap(row => [
      `  ${row.command}`, ...row.facts.map(text => `    ${text}`),
      ...nativeRuleLines(row.nativeReading).map(text => `    ${text}`),
      ...(row.comparisonGuidance ?? []).map(text => `    Selection comparison: ${text}`),
    ]),
    `Current step: ${route.kind === 'review' ? 'record the strategy gap' : 'choose one supplied comparison option'}.`,
  ].join('\n');
  const guide = { automatic: CARD_CHOICE_AUTOMATIC_GUIDE, model: CARD_CHOICE_GUIDE,
    review: CARD_CHOICE_REVIEW_GUIDE }[route.kind];
  const bytes = Buffer.byteLength(reading) + Buffer.byteLength(guide);
  frame.inputBudget = { bytes, target: TARGET_CARD_CHOICE_INPUT_BYTES, exceeded: bytes > TARGET_CARD_CHOICE_INPUT_BYTES,
    includes: ['reading', 'card-choice-guide'], componentBytes: [Buffer.byteLength(reading), Buffer.byteLength(guide)] };
  return reading;
}

export function bindPreparationCardChoice(choice, frame, observation, owner = 'flash-card-choice') {
  if (!frame || !sameGameContextSource(frame.source, observation)) throw new Error('Card-choice source changed; read the current step.');
  const route = preparationCardChoiceRoute(frame);
  if (route.kind === 'review') throw gapError(route.gap.kind, route.gap.reason, route.gap.message, { ...route.gap });
  const selected = frame.candidates.find(row => row.command === choice?.trim());
  if (!selected) throw new Error(`Copy one current command into choice: ${frame.candidates.map(row => row.command).join('; ')}`);
  if (!route.commands.includes(selected.command)) throw new Error(`Choose one supplied policy comparison option: ${route.commands.join('; ')}`);
  if (!['flash-card-choice', 'host-card-policy'].includes(owner)
      || (owner === 'host-card-policy' && route.kind !== 'automatic')) {
    throw new Error('Automatic card adoption requires one compatible policy command.');
  }
  return { schema: 'sts-preparation-card-choice-adoption-v1', source: frame.source, command: selected.command,
    owner, route,
    selected, followsDefault: frame.defaultCommands.includes(selected.command),
    suppliedPolicies: frame.rules, defaultCommands: frame.defaultCommands,
    basisKind: 'host-checked-card-choice', renderedBasis: `Selected ${selected.command}; supplied policies: ${route.policyIds.join('; ')}.` };
}

// Both the flow's automatic choice and the player's bounded choice use this
// source check and intent/applied journal around the real workbench execution.
export async function adoptPreparationCardChoice(game, frame, observation, { choice, owner, signal, record }) {
  const adopted = bindPreparationCardChoice(choice, frame, observation, owner);
  if (!sameGameContextSource(frame.source, await game.observe({ signal }))) {
    throw new Error('Game position changed before the card choice; read the current step.');
  }
  await record({ ...adopted, phase: 'intent', at: new Date().toISOString(), frame });
  if (!sameGameContextSource(frame.source, await game.observe({ signal }))) {
    throw new Error('Game position changed while recording the card choice; read the current step.');
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
