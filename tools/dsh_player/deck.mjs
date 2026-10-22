import { createPresentation, resolveLocale } from '@sts-simulator/presentation';

const deckViews = Object.fromEntries(['en', 'zh-CN'].map(locale => [locale, createPresentation({ locale })]));
const deckText = (locale, key, parameters = {}) => deckViews[resolveLocale(locale)].text(`scripted:deck.${key}`,
  Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));

// Structural comparison of public inventory. Rust owns instance numbers and
// card mechanics; this module counts copies and renders their payment/timing.
const attributes = ['card', 'upgrades', 'misc_value', 'base_damage_override', 'base_block_override', 'cost_modifier', 'count'];
const positive = value => Number.isSafeInteger(value) && value > 0;
const label = row => `${row.card.count}x ${row.card.card}${row.card.upgrades > 0 ? '+' : ''}`;
const sum = rows => rows.reduce((total, row) => total + row.card.count, 0);
const list = (rows, locale = 'en') => rows.map(label).join(', ') || deckText(locale, 'none');
const inventoryError = message => Object.assign(new Error(message), {
  preparationGap: { kind: 'facts', reason: 'owned-mechanics-mismatch', message },
});
const fixedBlock = row => row.mechanics.base_block_delivery === 'once_on_play' && positive(row.persistent_block);
const persistentReduction = row => positive(row.mechanics.enemy_strength_down)
  && row.mechanics.enemy_strength_restoration === 'no_scheduled_restoration';
const percent = value => `${(value * 100).toFixed(1)}%`;
const accessText = (access, locale = 'en') => deckText(locale, 'accessCopies', { description: access.description, copiesBefore: access.copiesBefore, copiesAfter: access.copiesAfter, owned: access.owned })
  + deckText(locale, 'accessSamples', { windows: access.windows.map(window => `${window.draws}: ${percent(window.before)}->${percent(window.after)}`).join(', ') });

export function enduringStrengthGain(mechanics) {
  if (!positive(mechanics?.strength_gain) || positive(mechanics.temporary_strength_gain)
      || !['on_play', 'if_target_intent_base_damage_nonnegative_at_resolution', 'each_player_turn_start_post_draw']
        .includes(mechanics.strength_gain_delivery)) return;
  return { amount: mechanics.strength_gain, delivery: mechanics.strength_gain_delivery };
}

export const strengthTiming = (delivery, locale = 'en') => ({
  on_play: deckText(locale, 'strengthOnPlay'),
  if_target_intent_base_damage_nonnegative_at_resolution: deckText(locale, 'strengthOnAttackIntent'),
  each_player_turn_start_post_draw: deckText(locale, 'strengthEachTurn'),
})[delivery];

export function playedCardLifecycle({ type, exhausts, ethereal }, locale = 'en') {
  if (type === 'Power') return deckText(locale, 'powerLifecycle');
  if (exhausts) return deckText(locale, 'exhaustAfterPlay');
  return deckText(locale, 'ordinaryRedraw')
    + (ethereal ? deckText(locale, 'etherealLifecycle') : '');
}

export const otherCardExhaustComparison = (locale = 'en') => deckText(locale, 'otherExhaustRemoval')
  + deckText(locale, 'otherExhaustTradeoff');

export const OTHER_CARD_EXHAUST_COMPARISON = otherCardExhaustComparison();

export const attackInventoryText = (profile, locale = 'en') => deckText(locale, 'attackInventory', { attack: profile.counts.Attack })
  + (profile.attacks.map(row => deckText(locale, 'attackCost', { card: row.card, cost: row.cost === -1 ? 'X' : row.cost })
    + (row.delivery?.kind === 'attack'
      ? deckText(locale, 'attackTargets', { value: row.delivery.hits?.kind === 'fixed' ? deckText(locale, 'fixedHits', { count: row.delivery.hits.count }) : deckText(locale, 'variableHits', { value: row.delivery.hits?.kind ?? deckText(locale, 'suppliedRule') }), targets: row.delivery.targets })
        + (row.delivery.amount?.kind === 'base' && Number.isSafeInteger(row.baseDamagePerHit)
          ? deckText(locale, 'baseDamage', { baseDamagePerHit: row.baseDamagePerHit }) : '')
        + (row.damageGrowthOnPlay > 0 ? deckText(locale, 'damageGrowth', { damageGrowthOnPlay: row.damageGrowthOnPlay }) : '')
      : deckText(locale, 'fullOutputRule'))
    + `; ${row.exhausts ? deckText(locale, 'exhaustAfterPlay') : deckText(locale, 'ordinaryBeforeModifiers')}`).join('; ') || deckText(locale, 'none'))
  + deckText(locale, 'attackRuleDetails');

// Rust owns each injection's destination and persistence. Explain the later
// ordinary route as well as the insertion; a Hand insertion is not a draw.
export function renderStatusRoutes(sources, locale = 'en') {
  return sources.flatMap(source => source.injections.map(effect => {
    const entry = { hand: deckText(locale, 'statusHandRoute'),
      discard_pile: deckText(locale, 'statusDiscardRoute'),
      draw_pile: deckText(locale, 'statusDrawRoute') }[effect.destination];
    const later = { persistent: deckText(locale, 'statusPersistent'),
      ethereal: deckText(locale, 'statusEthereal') }[effect.persistence];
    if (!entry || !later) throw Object.assign(new Error(deckText(locale, 'statusRouteMissing')), {
      preparationGap: { kind: typeof effect.destination === 'string' && typeof effect.persistence === 'string' ? 'relations' : 'facts',
        reason: typeof effect.destination === 'string' && typeof effect.persistence === 'string' ? 'unsupported-status-route' : 'missing-status-route-facts', card: source.card,
        destination: effect.destination, persistence: effect.persistence },
    });
    return deckText(locale, 'statusRoute', { card: source.card, count: effect.count, card2: effect.card, entry: entry, later: later });
  })).join('\n');
}

// Interpret the reviewed Rust amount rule; the inventory has no typed tag
// census, so retain N rather than guessing tags from a CardId or name.
export function strikeScaledAttack(damage, locale = 'en') {
  const delivery = damage?.delivery;
  if (delivery?.kind !== 'attack' || delivery.amount?.kind !== 'base_plus_strike_cards'
      || delivery.hits?.kind !== 'fixed' || delivery.hits.count !== 1 || damage.hit_count !== 1
      || !Number.isSafeInteger(damage.damage_per_hit) || damage.damage_per_hit < 0
      || !positive(delivery.amount.per_card)) return;
  return { base: damage.damage_per_hit, perCard: delivery.amount.per_card,
    countRule: deckText(locale, 'strikeCountRule') };
}

// A known single-card event grant uses its actual event command. Rust's
// acquisition definition includes obtain-time upgrades such as Toxic Egg.
export function preparationCardAcquisition(observation, row) {
  const action = row?.candidate?.action;
  const evidence = observation.decision_evidence?.candidates?.find(candidate => candidate.semantic_action === row?.semantic_action);
  const mechanics = evidence?.cards?.length === 1 ? evidence.cards[0].current : undefined;
  if (['take_card', 'buy_card'].includes(action?.kind)) return { card: action.card, upgrades: action.upgrades,
    ...(mechanics?.card === action.card && mechanics.upgrades === action.upgrades ? { mechanics } : {}) };
  if (action?.kind !== 'choose_event_option' || observation.decision_evidence?.kind !== 'card_acquisition_options') return;
  const effects = action.public_semantics?.structured_semantics?.effects;
  const grant = effects?.length === 1 ? effects[0] : undefined;
  if (grant?.effect !== 'obtain_card' || grant.count !== 1 || grant.pool?.kind !== 'specific'
      || mechanics?.card !== grant.pool.card || !Number.isSafeInteger(mechanics.upgrades) || mechanics.upgrades < 0) return;
  return { card: mechanics.card, upgrades: mechanics.upgrades, mechanics };
}

// An unordered inventory reference, independent of combat draw rules and RNG.
// Count access to originals; payment, generated cards and deployment are separate.
function atLeastOne(size, copies, draws) {
  let miss = 1;
  for (let i = 0; i < draws; i++) miss *= Math.max(0, size - copies - i) / (size - i);
  return 1 - miss;
}

function accessIncrement(kind, description, rows, size, locale = 'en') {
  const count = sum(rows);
  return { kind, description, owned: list(rows, locale), copiesBefore: count, copiesAfter: count + 1,
    windows: [...new Set([Math.min(5, size), Math.min(10, size)])].filter(positive).map(draws => ({ draws,
      before: atLeastOne(size, count, draws), after: atLeastOne(size + 1, count + 1, draws) })) };
}

function removalAccess(cards, removed, size, locale = 'en') {
  const retained = cards.filter(row => row !== removed);
  return [
    ['retained_setup', deckText(locale, 'retainedPowers'), retained.filter(row => row.mechanics.card_type === 'Power')],
    ['retained_draw', deckText(locale, 'retainedDraw'), retained.filter(row => positive(row.mechanics.draw_cards))],
    ['retained_protection', deckText(locale, 'retainedProtection'),
      retained.filter(row => fixedBlock(row) || persistentReduction(row))],
  ].flatMap(([kind, description, rows]) => {
    const count = sum(rows);
    if (!count) return [];
    return [{ kind, description, owned: list(rows, locale), copiesBefore: count, copiesAfter: count,
      windows: [...new Set([Math.min(5, size - 1), Math.min(10, size - 1)])].filter(positive).map(draws => ({ draws,
        before: atLeastOne(size, count, draws), after: atLeastOne(size - 1, count, draws) })) }];
  });
}

function draw(row) {
  const requested = row.mechanics.draw_cards;
  if (!positive(requested)) return;
  const topdeck = row.mechanics.hand_topdeck_after_draw ?? 0;
  return { card: label(row), type: row.mechanics.card_type, cost: row.persistent_cost, requested, topdeck,
    exhausts: row.mechanics.exhausts, ethereal: row.mechanics.ethereal,
    exhaustsOther: row.mechanics.exhausts_other_cards === true,
    handChange: row.mechanics.exhausts_other_cards || (row.mechanics.selects_from_hand && !topdeck)
      ? null : requested - 1 - topdeck };
}

export const expandsHandAccess = request => request?.requested >= 2
  && (request.handChange === null || request.handChange > 0);

export function battleTranceDrawOrder(profile, locale = 'en') {
  const sources = [
    ...profile.onPlayDraw.map(source => deckText(locale, 'drawSource', { card: source.card, requested: source.requested, cost: source.cost })),
    ...profile.exhaustDraw.map(source => deckText(locale, 'exhaustDrawSource', { card: source.card, requested: source.requested })),
  ];
  return deckText(locale, 'battleTranceFirst')
    + deckText(locale, 'battleTranceNoDraw')
    + (sources.length ? deckText(locale, 'battleTranceSources', { sources: sources.join('; ') }) : '');
}

export function renderOnPlayDrawCoverage(profile, locale = 'en') {
  const coverage = profile.onPlayDrawCoverage;
  return deckText(locale, 'drawCoverage', { copies: coverage.copies, size: profile.size, owned: coverage.owned })
    + deckText(locale, 'drawCoverageSamples', { windows: coverage.windows.map(window => `${window.draws}: ${percent(window.probability)}`).join(', ') || deckText(locale, 'emptyInventory') });
}

function cardIncrement(action, mechanics, counts, size, cards, locale = 'en') {
  const type = mechanics.card_type;
  const strikeScaling = strikeScaledAttack(mechanics.damage, locale);
  const row = { card: { card: action.card, upgrades: action.upgrades, count: 1 }, mechanics,
    persistent_cost: mechanics.cost };
  const matching = cards.filter(row => row.card.card === action.card);
  const protection = mechanics.base_block_delivery === 'once_on_play' && positive(mechanics.block);
  const persistent = persistentReduction({ mechanics });
  const access = [
    ...(matching.length && !protection && !persistent
      ? [accessIncrement('same_card', deckText(locale, 'sameCardAccess', { card: action.card }), matching, size, locale)] : []),
    ...(protection ? [accessIncrement('fixed_block', deckText(locale, 'fixedBlockAccess', { block: mechanics.block }), cards.filter(row => fixedBlock(row) && row.persistent_block >= mechanics.block), size, locale)] : []),
    ...(persistent ? [accessIncrement('persistent_reduction', deckText(locale, 'reductionAccess'), cards.filter(persistentReduction), size, locale)] : []),
    ...(type === 'Attack' ? [accessIncrement('attack_type', deckText(locale, 'attackAccess'), cards.filter(row => row.mechanics.card_type === 'Attack'), size, locale)] : []),
    ...(expandsHandAccess(draw(row)) ? [accessIncrement('multi_draw', deckText(locale, 'multiDrawAccess'), cards.filter(owned => expandsHandAccess(draw(owned))), size, locale)] : []),
  ];
  return { kind: 'addition', card: action.card, upgrades: action.upgrades, type,
    sizeBefore: size, sizeAfter: size + 1, typeBefore: counts[type] ?? 0, typeAfter: (counts[type] ?? 0) + 1,
    cost: mechanics.cost, exhausts: mechanics.exhausts, ethereal: mechanics.ethereal, tags: mechanics.tags,
    combatUpgradeScope: mechanics.combat_upgrade_scope,
    combatUpgradePersistence: mechanics.combat_upgrade_persistence,
    targetedHandExhaust: mechanics.exhausts_other_cards === true && mechanics.selects_from_hand === true,
    exhaustRecovery: mechanics.exhaust_pile_to_hand,
    enemyWeak: mechanics.weak,
    enemyWeakTarget: mechanics.play_target,
    enemyVulnerable: mechanics.vulnerable,
    onPlayEnergy: mechanics.energy_gain_rule?.delivery === 'on_play' ? mechanics.energy_gain_rule : undefined,
    selfHpLoss: mechanics.self_hp_loss_on_play,
    endOfTurnHpLossAndDamage: mechanics.end_of_turn_hp_loss_and_damage,
    costChangeOnHpLoss: mechanics.cost_change_on_hp_loss,
    topDeckPlay: mechanics.top_deck_play,
    exhaustsOtherCards: mechanics.exhausts_other_cards,
    statusInjections: mechanics.status_injections,
    temporaryStrength: mechanics.temporary_strength_gain,
    strengthLossAtTurnEnd: mechanics.strength_loss_at_end_of_turn,
    randomAttackToHand: mechanics.random_attack_to_hand,
    copyToDiscardAfterDamage: mechanics.copy_to_discard_after_damage,
    ...(enduringStrengthGain(mechanics) ? { enduringStrength: enduringStrengthGain(mechanics) } : {}),
    ...(mechanics.damage?.delivery?.kind === 'attack' ? { attackDelivery: mechanics.damage.delivery,
      ...(mechanics.damage.delivery.amount?.kind === 'base'
        && Number.isSafeInteger(mechanics.damage.damage_per_hit) && mechanics.damage.damage_per_hit >= 0
        ? { baseDamagePerHit: mechanics.damage.damage_per_hit } : {}) } : {}),
    ...(strikeScaling ? { strikeScaling } : {}),
    access,
    ...(mechanics.base_block_delivery === 'once_on_play' && positive(mechanics.block) ? { blockRequest: mechanics.block } : {}),
    ...(draw(row) ? { onPlayDraw: draw(row) } : {}),
    ...(positive(mechanics.draw_cards_each_turn) ? { laterTurnDraw: mechanics.draw_cards_each_turn } : {}),
    ...(positive(mechanics.exhaust_draw_power) ? { drawPerExhaustAfterDeployment: mechanics.exhaust_draw_power } : {}),
    ...(positive(mechanics.exhaust_block_power) ? { blockPerExhaustAfterDeployment: mechanics.exhaust_block_power } : {}),
    ...(positive(mechanics.enemy_strength_down) ? { enemyStrengthReduction: {
      amount: mechanics.enemy_strength_down, restoration: mechanics.enemy_strength_restoration,
      target: mechanics.play_target,
      owned: cards.filter(row => positive(row.mechanics.enemy_strength_down)).map(row => ({
        card: label(row), amount: row.mechanics.enemy_strength_down,
        restoration: row.mechanics.enemy_strength_restoration, target: row.mechanics.play_target,
        count: row.card.count })),
    } } : {}),
  };
}

export function preparationDeckComparison(observation, normalize = command => command, locale = 'en') {
  const state = observation.position?.observation?.observation;
  const evidence = observation.deck_evidence;
  if (!state?.cards || !evidence) return;
  if (evidence.schema !== 'sts-agent-deck-evidence-v1'
      || evidence.public_snapshot_id !== observation.position.public_snapshot_id
      || evidence.cards?.length !== state.cards.length) {
    throw inventoryError(deckText(locale, 'inventorySourceMismatch'));
  }
  const remaining = [...evidence.cards];
  const cards = state.cards.map(card => {
    const index = remaining.findIndex(row => attributes.every(key => Object.hasOwn(card, key)
      && row.card?.[key] === card[key]));
    if (index < 0) throw inventoryError(deckText(locale, 'inventoryAttributesMismatch'));
    const [row] = remaining.splice(index, 1);
    if (row.mechanics?.card !== card.card || row.mechanics?.upgrades !== card.upgrades
        || !positive(card.count) || !Number.isSafeInteger(row.persistent_cost)) {
      throw inventoryError(deckText(locale, 'inventoryMechanicsMissing'));
    }
    return row;
  });
  const size = sum(cards);
  const counts = Object.fromEntries(['Attack', 'Skill', 'Power', 'Curse', 'Status'].map(type =>
    [type, sum(cards.filter(row => row.mechanics.card_type === type))]));
  const attacks = cards.filter(row => row.mechanics.card_type === 'Attack');
  const reusableExhaust = cards.filter(row => row.mechanics.exhausts_other_cards && !row.mechanics.exhausts && !row.mechanics.ethereal);
  const multiDraw = cards.filter(row => expandsHandAccess(draw(row)));
  const multiDrawCopies = sum(multiDraw);
  const costs = Object.fromEntries([...new Set(attacks.map(row => row.persistent_cost))].sort((a, b) => a - b)
    .map(cost => [cost, sum(attacks.filter(row => row.persistent_cost === cost))]));
  const candidates = (observation.position.actions ?? []).flatMap(row => {
    const action = row.candidate?.action;
    const command = normalize(row.semantic_action, observation);
    const acquisition = preparationCardAcquisition(observation, row);
    if (acquisition) {
      const mechanics = acquisition.mechanics;
      if (!mechanics) return [];
      const increment = cardIncrement(acquisition, mechanics, counts, size, cards, locale);
      const nextAct = state.run?.pending_act_entry ?? state.run?.act;
      if (positive(mechanics.exhaust_draw_power) && reusableExhaust.length
          && nextAct >= 2 && mechanics.cost >= 0 && mechanics.cost <= state.base_energy_per_turn) {
        increment.preference = { id: 'owned-reusable-exhaust-access', origin: 'maintainer_comparison_preference',
          text: (multiDrawCopies
            ? deckText(locale, 'compareExhaustEngine', { list: list(reusableExhaust, locale), multiDrawCopies: multiDrawCopies, list2: list(multiDraw, locale) })
            : deckText(locale, 'preferExhaustEngine', { list: list(reusableExhaust, locale) }))
            + deckText(locale, 'exhaustEngineSetup') };
      }
      return [{ command, ...increment }];
    }
    if (['remove_card', 'toke'].includes(action?.kind)) {
      const matching = cards.filter(card => card.card.card === action.card && card.card.upgrades === action.upgrades);
      if (matching.length !== 1) return [];
      const owned = matching[0], type = owned.mechanics.card_type;
      return [{ command, kind: 'removal', card: action.card, upgrades: action.upgrades,
        sizeBefore: size, sizeAfter: size - 1, type, typeBefore: counts[type], typeAfter: counts[type] - 1,
        cost: owned.persistent_cost, damage: owned.persistent_damage, block: owned.persistent_block,
        exhausts: owned.mechanics.exhausts, ethereal: owned.mechanics.ethereal,
        access: removalAccess(cards, owned, size, locale) }];
    }
    if (action?.kind === 'skip_card_reward') return [{ command, kind: 'preserve', sizeBefore: size, sizeAfter: size }];
    if (action?.kind === 'singing_bowl') {
      const resource = observation.decision_evidence?.kind === 'card_acquisition_options'
        ? observation.decision_evidence.resource_options?.find(item => item.semantic_action === row.semantic_action) : undefined;
      if (!resource || !Number.isSafeInteger(resource.hp_gain) || resource.hp_gain < 0
          || !Number.isSafeInteger(resource.max_hp_gain) || resource.max_hp_gain <= 0
          || typeof resource.native_rule !== 'string' || !resource.native_rule.trim()) {
        throw inventoryError(deckText(locale, 'bowlOutcomeMissing'));
      }
      return [{ command, kind: 'preserve', sizeBefore: size, sizeAfter: size,
        resource: { kind: 'singing_bowl', hpGain: resource.hp_gain, maxHpGain: resource.max_hp_gain,
          nativeRule: resource.native_rule } }];
    }
    return [];
  });
  return { schema: 'sts-preparation-deck-comparison-v1', basis: 'current_public_inventory_and_shared_native_mechanics',
    size, baseEnergy: state.base_energy_per_turn, counts, attackCosts: costs,
    attackCopies: list(attacks, locale), playedExhaustAttacks: list(attacks.filter(row => row.mechanics.exhausts), locale),
    attacks: attacks.map(row => ({ card: label(row), copies: row.card.count, cost: row.persistent_cost,
      delivery: row.mechanics.damage?.delivery, baseDamagePerHit: row.persistent_damage,
      damageGrowthOnPlay: row.mechanics.damage_growth_on_play, exhausts: row.mechanics.exhausts })),
    strikeTags: cards.every(row => Array.isArray(row.mechanics.tags))
      ? { copies: sum(cards.filter(row => row.mechanics.tags.includes('Strike'))),
        owned: list(cards.filter(row => row.mechanics.tags.includes('Strike')), locale) } : undefined,
    strengthSources: cards.flatMap(row => enduringStrengthGain(row.mechanics)
      ? [{ card: label(row), type: row.mechanics.card_type, cost: row.persistent_cost,
        exhausts: row.mechanics.exhausts, ethereal: row.mechanics.ethereal,
        ...enduringStrengthGain(row.mechanics) }] : []),
    onPlayDraw: cards.flatMap(row => draw(row) ? [draw(row)] : []),
    onPlayDrawCoverage: { copies: multiDrawCopies, owned: list(multiDraw, locale),
      windows: [...new Set([Math.min(5, size), Math.min(10, size)])].filter(positive).map(draws =>
        ({ draws, probability: atLeastOne(size, multiDrawCopies, draws) })) },
    blockSupport: {
      retention: cards.filter(row => row.card.card === 'Barricade').map(row => ({ card: label(row), type: row.mechanics.card_type, cost: row.persistent_cost })),
      multiplication: cards.filter(row => row.card.card === 'Entrench').map(row =>
        ({ card: label(row), cost: row.persistent_cost, exhausts: row.mechanics.exhausts })),
      damageCaps: cards.filter(row => positive(row.mechanics.intangible_gain) && positive(row.mechanics.intangible_damage_cap))
        .map(row => ({ card: label(row), cost: row.persistent_cost, amount: row.mechanics.intangible_gain,
          cap: row.mechanics.intangible_damage_cap, exhausts: row.mechanics.exhausts })),
    },
    laterTurnDraw: cards.filter(row => positive(row.mechanics.draw_cards_each_turn)).map(row =>
      ({ card: label(row), cost: row.persistent_cost, requested: row.mechanics.draw_cards_each_turn })),
    exhaustDraw: cards.filter(row => positive(row.mechanics.exhaust_draw_power)).map(row =>
      ({ card: label(row), cost: row.persistent_cost, requested: row.mechanics.exhaust_draw_power })),
    playedExhaust: list(cards.filter(row => row.mechanics.exhausts), locale),
    ethereal: list(cards.filter(row => row.mechanics.ethereal), locale),
    otherCardExhaust: list(cards.filter(row => row.mechanics.exhausts_other_cards), locale),
    otherCardExhaustCopies: sum(cards.filter(row => row.mechanics.exhausts_other_cards)),
    skillExhaustPowers: cards.filter(row => row.card.card === 'Corruption').map(row =>
      ({ card: label(row), cost: row.persistent_cost })),
    reusableOtherCardExhaust: list(reusableExhaust, locale),
    reusableOtherCardExhaustCopies: sum(reusableExhaust),
    statusGeneration: cards.filter(row => row.mechanics.status_injections?.length).map(row => ({ card: label(row),
      injections: row.mechanics.status_injections })),
    statusInventory: cards.filter(row => ['Status', 'Curse'].includes(row.mechanics.card_type)).map(row =>
      ({ card: label(row), type: row.mechanics.card_type, ethereal: row.mechanics.ethereal })),
    statusPayoffs: cards.filter(row => ['Evolve', 'FireBreathing'].includes(row.card.card)).map(row =>
      ({ card: label(row), cost: row.persistent_cost })),
    onPlayEnergy: cards.filter(row => row.mechanics.energy_gain_rule?.delivery === 'on_play' && positive(row.mechanics.energy_gain_rule.amount))
      .map(row => ({ card: label(row), cost: row.persistent_cost, amount: row.mechanics.energy_gain_rule.amount,
        hpLoss: row.mechanics.self_hp_loss_on_play?.base_hp_loss ?? 0, exhausts: row.mechanics.exhausts })),
    fixedPlayedBlock: cards.filter(row => row.mechanics.base_block_delivery === 'once_on_play' && positive(row.persistent_block))
      .map(row => ({ card: label(row), cost: row.persistent_cost, block: row.persistent_block })),
    persistentReduction: cards.filter(persistentReduction).map(row => ({ card: label(row), cost: row.persistent_cost,
      amount: row.mechanics.enemy_strength_down, target: row.mechanics.play_target, exhausts: row.mechanics.exhausts })),
    candidates,
    priorities: [
      deckText(locale, 'priorityWholeDeck'),
      deckText(locale, 'priorityAttack'),
      deckText(locale, 'priorityAccessEngine'),
      deckText(locale, 'priorityAdditionalCopy'),
      deckText(locale, 'priorityProtection'),
      deckText(locale, 'prioritySetupProtection'),
    ] };
}

export function preparationDeckConditionBindings(observation, normalize = command => command, locale = 'en') {
  const profile = preparationDeckComparison(observation, normalize, locale);
  if (!profile) return [];
  return profile.candidates.flatMap(row => {
    const reduction = row.enemyStrengthReduction;
    const persistent = reduction?.restoration === 'no_scheduled_restoration';
    if (row.kind === 'removal') {
      const shop = observation.position.observation.observation.context?.kind === 'shop';
      const payment = row.cost === -2 ? deckText(locale, 'normalUnplayable')
        : row.cost === -1 ? deckText(locale, 'normalXCost')
          : deckText(locale, 'normalCost', { cost: row.cost });
      const lifecycle = row.type === 'Power' ? deckText(locale, 'deployedLifecycle')
        : row.exhausts ? deckText(locale, 'exhaustsAfterPlay') : row.ethereal ? deckText(locale, 'etherealLabel') : deckText(locale, 'ordinaryRedraw');
      return [{ command: row.command, id: 'removal-retained-access-v1', fixedMechanics: [
        deckText(locale, 'removalMechanics', { card: row.card, upgradeSuffix: row.upgrades > 0 ? '+' : '', sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, payment: payment, damage: row.damage, block: row.block, lifecycle: lifecycle }),
        ...row.access.map(access => accessText(access, locale)),
      ], choices: [
        { id: '@retained-access', text: deckText(locale, 'removalRetainedAccess') },
        { id: '@foregone-function', text: deckText(locale, 'removalForegoneFunction') },
        shop ? { id: '@gold-opportunity', text: deckText(locale, 'removalGoldOpportunity') }
          : { id: '@preparation-opportunity', text: deckText(locale, 'removalPreparationOpportunity') },
        { id: '@review-access', text: deckText(locale, 'reviewTargetBudget') },
      ] }];
    }
    if (row.kind === 'preserve') {
      const additions = profile.candidates.filter(candidate => candidate.kind === 'addition');
      if (!additions.length) return [];
      return [{ command: row.command, id: 'reward-preservation-comparison-v1', fixedMechanics: [
        deckText(locale, 'skipMechanics', { size: profile.size }),
        deckText(locale, 'competingAdditions', { additions: additions.map(candidate => candidate.command).join(', ') }),
      ], choices: additions.flatMap((candidate, index) => [
        { id: `@keep-vs-${index + 1}`, alternativeCommand: candidate.command, emphasis: 'preserved_access',
          text: deckText(locale, 'keepVersusAddition', { command: candidate.command }) },
        { id: `@gain-vs-${index + 1}`, alternativeCommand: candidate.command, emphasis: 'foregone_gain',
          text: deckText(locale, 'gainVersusSkip', { command: candidate.command }) },
      ]).concat({ id: '@review-access', text: deckText(locale, 'reviewAdditionPlan') }) }];
    }
    if (row.kind === 'addition' && row.card === 'BattleTrance' && row.onPlayDraw) {
      return [{ command: row.command, id: 'draw-order-access-v1', fixedMechanics: [
        deckText(locale, 'battleTranceMechanics', { card: row.card, upgradeSuffix: row.upgrades > 0 ? '+' : '', sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, cost: row.cost, requested: row.onPlayDraw.requested, handChange: row.onPlayDraw.handChange }),
        deckText(locale, 'battleTranceTurnBoundary'),
        deckText(locale, 'drawAndExhaustSources', { onPlayDraw: profile.onPlayDraw.map(source => deckText(locale, 'costRequest', { card: source.card, cost: source.cost, requested: source.requested })).join('; ') || deckText(locale, 'none'), exhaustDraw: profile.exhaustDraw.map(source => deckText(locale, 'costRequestPerEvent', { card: source.card, cost: source.cost, requested: source.requested })).join('; ') || deckText(locale, 'none') }),
      ], choices: [
        { id: '@independent-access', text: deckText(locale, 'battleTranceAccess') },
        { id: '@same-turn-order', text: battleTranceDrawOrder(profile, locale) },
        { id: '@draw-slot-cost', text: deckText(locale, 'battleTranceDrawCost') },
        { id: '@review-access', text: deckText(locale, 'reviewAccessPayment') },
      ] }];
    }
    if (row.kind === 'addition' && row.type === 'Skill' && row.onPlayDraw && !row.blockRequest && !persistent) {
      return [{ command: row.command, id: 'on-play-access-v1', fixedMechanics: [
        deckText(locale, 'drawAdditionMechanics', { card: row.card, upgradeSuffix: row.upgrades > 0 ? '+' : '', sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, cost: row.cost, requested: row.onPlayDraw.requested, handChange: row.onPlayDraw.handChange ?? deckText(locale, 'selectionDependent') }),
        deckText(locale, 'drawLifecycle', { exhausts: row.exhausts ? deckText(locale, 'exhaustAfterPlay') : deckText(locale, 'ordinaryRedraw') }),
        deckText(locale, 'ownedOnPlayDraw', { onPlayDraw: profile.onPlayDraw.map(source => deckText(locale, 'drawHandContribution', { card: source.card, cost: source.cost, requested: source.requested, handChange: source.handChange ?? deckText(locale, 'selectionDependent') })).join('; ') || deckText(locale, 'none') }),
        ...row.access.filter(access => access.kind === 'multi_draw').map(access => accessText(access, locale)),
      ], choices: [
        { id: '@independent-access', text: deckText(locale, 'independentAccess') },
        ...(row.onPlayDraw.handChange === null ? [{ id: '@selection-turnover', text: row.onPlayDraw.exhaustsOther
          ? otherCardExhaustComparison(locale)
          : deckText(locale, 'selectionTurnover') }] : []),
        { id: '@access-cost', text: deckText(locale, 'accessCosts') },
        { id: '@review-access', text: deckText(locale, 'reviewDrawSelectionPayment') },
      ] }];
    }
    if (row.kind === 'addition' && row.enduringStrength) {
      const source = row.enduringStrength;
      return [{ command: row.command, id: 'enduring-strength-increment-v1', fixedMechanics: [
        deckText(locale, 'strengthAdditionMechanics', { card: row.card, upgradeSuffix: row.upgrades > 0 ? '+' : '', sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, cost: row.cost, amount: source.amount, strengthTiming: strengthTiming(source.delivery, locale) }),
        deckText(locale, 'lifecycleBeforeModifiers', { exhausts: row.exhausts ? deckText(locale, 'exhaustAfterPlay') : row.type === 'Power' ? deckText(locale, 'powerLifecycle') : deckText(locale, 'conditionalRedraw') }),
        deckText(locale, 'ownedStrength', { strengthSources: profile.strengthSources.map(owned => deckText(locale, 'strengthSource', { card: owned.card, cost: owned.cost, amount: owned.amount, strengthTiming: strengthTiming(owned.delivery, locale) })).join('; ') || deckText(locale, 'none') }),
        ...row.access.filter(access => access.kind === 'same_card').map(access => accessText(access, locale)),
        attackInventoryText(profile, locale),
      ], choices: [
        { id: '@strength-output', text: deckText(locale, 'strengthOutput', { attackCopies: profile.attackCopies }) },
        { id: '@growth-access', text: deckText(locale, 'additionalStrength') },
        { id: '@setup-cost', text: deckText(locale, 'strengthCost') },
        { id: '@review-access', text: deckText(locale, 'reviewSourceAttackTiming') },
      ] }];
    }
    if (row.kind === 'addition' && row.attackDelivery?.amount?.kind === 'player_block') {
      const support = profile.blockSupport;
      return [{ command: row.command, id: 'current-block-output-v1', fixedMechanics: [
        deckText(locale, 'blockAttackMechanics', { card: row.card, upgradeSuffix: row.upgrades > 0 ? '+' : '', sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, cost: row.cost, card2: row.card === 'BodySlam' ? deckText(locale, 'bodySlamPreservesBlock') : '' }),
        deckText(locale, 'ownedFixedBlock', { fixedPlayedBlock: profile.fixedPlayedBlock.map(source => deckText(locale, 'blockSource', { card: source.card, cost: source.cost, block: source.block })).join('; ') || deckText(locale, 'none') }),
        ...profile.fixedPlayedBlock.filter(source => source.cost >= 0 && row.cost >= 0
          && source.cost + row.cost <= profile.baseEnergy).map(source =>
          deckText(locale, 'sameHandBlockPayment', { card: source.card, card2: row.card, cost: source.cost + row.cost, baseEnergy: profile.baseEnergy, block: source.block })),
        deckText(locale, 'ownedRetention', { retention: support.retention.map(source => deckText(locale, 'deploymentCost', { card: source.card, cost: source.cost })).join('; ') || deckText(locale, 'none') }),
        deckText(locale, 'ownedMultiplication', { multiplication: support.multiplication.map(source => deckText(locale, 'costLifecycle', { card: source.card, cost: source.cost, exhausts: source.exhausts ? deckText(locale, 'exhaustAfterPlay') : deckText(locale, 'ordinaryLaterUse') })).join('; ') || deckText(locale, 'none') }),
        deckText(locale, 'ownedDamageCaps', { damageCaps: support.damageCaps.map(source => deckText(locale, 'damageCapSource', { card: source.card, cost: source.cost, amount: source.amount, cap: source.cap, exhausts: source.exhausts ? deckText(locale, 'exhaustAfterPlay') : deckText(locale, 'ordinaryLaterUse') })).join('; ') || deckText(locale, 'none') }),
        deckText(locale, 'ownedBlockAttacks', { attacks: profile.attacks.filter(source => source.delivery?.amount?.kind === 'player_block').map(source => deckText(locale, 'energySource', { card: source.card, cost: source.cost })).join('; ') || deckText(locale, 'none') }),
        renderOnPlayDrawCoverage(profile, locale),
      ], choices: [
        { id: '@same-turn-output', text: deckText(locale, 'sameTurnBlockOutput') },
        ...(support.retention.length ? [{ id: '@retained-output', text: deckText(locale, 'retainedBlockOutput', { retention: support.retention.map(source => source.card).join(', '), fixedPlayedBlock: profile.fixedPlayedBlock.map(source => source.card).join(', '), multiplication: support.multiplication.map(source => source.card).join(', ') || deckText(locale, 'none') }) }] : []),
        { id: '@output-access-cost', text: deckText(locale, 'blockOutputCost') },
        { id: '@review-access', text: deckText(locale, 'reviewBlockTimingPayment') },
      ] }];
    }
    if (row.kind === 'addition' && row.type === 'Attack' && !row.blockRequest && !persistent) {
      return [{ command: row.command, id: 'attack-whole-deck-increment-v1', fixedMechanics: [
        deckText(locale, 'attackAdditionMechanics', { card: row.card, upgradeSuffix: row.upgrades > 0 ? '+' : '', sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, typeBefore: row.typeBefore, typeAfter: row.typeAfter, cost: row.cost === -1 ? deckText(locale, 'attackXCost') : deckText(locale, 'attackNormalCost', { cost: row.cost }), baseEnergy: profile.baseEnergy }),
        ...(Number.isSafeInteger(row.baseDamagePerHit) && Number.isSafeInteger(row.attackDelivery?.hits?.count)
          ? [deckText(locale, 'baseHits', { baseDamagePerHit: row.baseDamagePerHit, count: row.attackDelivery.hits.count })] : []),
        ...(row.strikeScaling ? [
          deckText(locale, 'strikeDamageRequest', { base: row.strikeScaling.base, perCard: row.strikeScaling.perCard, countRule: row.strikeScaling.countRule }),
        ] : []),
        ...(row.attackDelivery?.targets === 'random_enemy_per_hit'
          ? [deckText(locale, 'randomHits')] : []),
        ...row.access.map(access => accessText(access, locale)),
        ...(row.onPlayDraw ? [deckText(locale, 'attackDrawContribution', { requested: row.onPlayDraw.requested, handChange: row.onPlayDraw.handChange ?? deckText(locale, 'selectionDependent') })] : []),
        ...(row.exhausts ? [deckText(locale, 'attackExhaustLifecycle')] : []),
        deckText(locale, 'ownedExhaustDrawPayoffs', { exhaustDraw: profile.exhaustDraw.map(source => deckText(locale, 'exhaustDrawDeployment', { card: source.card, cost: source.cost, requested: source.requested })).join('; ') || deckText(locale, 'none') }),
      ], choices: [
        { id: '@output-access', text: deckText(locale, 'attackOutputAccess') },
        { id: '@extra-output', text: deckText(locale, 'attackExtraOutput') },
        { id: '@added-function', text: deckText(locale, 'attackAddedFunction') },
        { id: '@draw-payment-cost', text: deckText(locale, 'attackDrawPaymentCost') },
        { id: '@review-access', text: deckText(locale, 'reviewOutputAccessEncounter') },
      ] }];
    }
    if (row.kind !== 'addition' || (!row.blockRequest && !persistent)) return [];
    const fixedMechanics = [
      deckText(locale, 'protectionAdditionMechanics', { card: row.card, upgradeSuffix: row.upgrades > 0 ? '+' : '', sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, cost: row.cost }),
      ...row.access.map(access => accessText(access, locale)),
    ];
    if (row.blockRequest) fixedMechanics.push(deckText(locale, 'blockOnPlay', { blockRequest: row.blockRequest }));
    const acquisition = observation.position.actions.find(action => normalize(action.semantic_action, observation) === row.command);
    const mechanics = preparationCardAcquisition(observation, acquisition)?.mechanics;
    const exhaustPaths = [
      ...(profile.otherCardExhaustCopies > 0 ? [profile.otherCardExhaust] : []),
      ...profile.skillExhaustPowers.map(source => deckText(locale, 'skillExhaustDeployment', { card: source.card, cost: source.cost })),
    ].join('; ') || deckText(locale, 'none');
    if (positive(mechanics?.energy_gain_on_exhaust)) fixedMechanics.push(
      deckText(locale, 'blockSelfExhaustEnergy', { blockRequest: row.blockRequest, energy_gain_on_exhaust: mechanics.energy_gain_on_exhaust, exhaustPaths: exhaustPaths }));
    if (row.onPlayDraw) fixedMechanics.push(deckText(locale, 'protectionDrawContribution', { requested: row.onPlayDraw.requested, handChange: row.onPlayDraw.handChange ?? deckText(locale, 'selectionDependent') }));
    if (persistent) {
      const before = reduction.owned.filter(owned => owned.restoration === 'no_scheduled_restoration')
        .reduce((total, owned) => total + owned.amount * owned.count, 0);
      fixedMechanics.push(deckText(locale, 'persistentReductionApplications', { amount: reduction.amount, target: reduction.target, before: before, before2: before + reduction.amount }));
    }
    return [{ command: row.command, id: 'protection-quantity-access-v1', fixedMechanics,
      choices: [
        ...(row.blockRequest ? [{ id: '@protective-draw', realizations: row.cost <= profile.baseEnergy ? ['direct'] : ['sequence', 'future'],
          text: deckText(locale, 'protectiveDraw') }] : []),
        ...(positive(mechanics?.energy_gain_on_exhaust) ? [{ id: '@self-exhaust-energy',
          realizations: profile.otherCardExhaustCopies === 0 && profile.skillExhaustPowers.length === 0 && !mechanics.ethereal ? ['future'] : ['sequence'],
          text: deckText(locale, 'selfExhaustEnergy', { exhaustPaths: exhaustPaths, ethereal: mechanics.ethereal === true }) }] : []),
        ...(persistent ? [{ id: '@extra-application',
          text: deckText(locale, 'extraReductionApplication') }] : []),
        { id: '@coverage-cost', text: deckText(locale, 'protectionCoverageCost') },
        { id: '@review-access', text: deckText(locale, 'reviewAccessPaymentEncounter') },
      ] }];
  });
}

export function renderDeckComparison(profile, { candidateCommands, locale = 'en' } = {}) {
  if (!profile) return '';
  const commandSet = candidateCommands ? new Set(candidateCommands) : undefined;
  const candidates = commandSet ? profile.candidates.filter(row => commandSet.has(row.command)) : profile.candidates;
  const onPlayDraw = row => deckText(locale, 'onPlayDrawRow', { card: row.card, cost: row.cost, requested: row.requested })
    + (row.topdeck > 0 ? deckText(locale, 'topdeckAfterDraw', { topdeck: row.topdeck }) : '')
    + (row.handChange === null ? deckText(locale, 'additionalHandSelection')
      : deckText(locale, 'onPlayHandContribution', { handChange: row.handChange >= 0 ? '+' : '', handChange2: row.handChange }));
  return [deckText(locale, 'comparisonHeading'),
    deckText(locale, 'inventorySummary', { size: profile.size, object: Object.entries(profile.counts).map(([type, count]) => `${count} ${type}`).join(', '), baseEnergy: profile.baseEnergy }),
    deckText(locale, 'attacksSummary', { attackCopies: profile.attackCopies, object: Object.entries(profile.attackCosts).map(([cost, count]) => `${count}x ${cost}c`).join(', ') || deckText(locale, 'none') }),
    `  ${attackInventoryText(profile, locale)}`,
    deckText(locale, 'strengthSummary', { strengthSources: profile.strengthSources.map(row => deckText(locale, 'strengthSource', { card: row.card, cost: row.cost, amount: row.amount, strengthTiming: strengthTiming(row.delivery, locale) })).join('; ') || deckText(locale, 'none') }),
    deckText(locale, 'exhaustAttacksSummary', { playedExhaustAttacks: profile.playedExhaustAttacks }),
    deckText(locale, 'onPlayDrawSummary', { onPlayDraw: profile.onPlayDraw.map(onPlayDraw).join('; ') || deckText(locale, 'none') }),
    `  ${renderOnPlayDrawCoverage(profile, locale)}`,
    deckText(locale, 'laterDrawSummary', { laterTurnDraw: profile.laterTurnDraw.map(row => deckText(locale, 'requestPerTurn', { card: row.card, cost: row.cost, requested: row.requested })).join('; ') || deckText(locale, 'none') }),
    deckText(locale, 'exhaustDrawSummary', { exhaustDraw: profile.exhaustDraw.map(row => deckText(locale, 'costRequestPerEvent', { card: row.card, cost: row.cost, requested: row.requested })).join('; ') || deckText(locale, 'none') }),
    deckText(locale, 'exhaustSourcesSummary', { playedExhaust: profile.playedExhaust, ethereal: profile.ethereal, otherCardExhaust: profile.otherCardExhaust, reusableOtherCardExhaust: profile.reusableOtherCardExhaust }),
    deckText(locale, 'statusSummary', { renderStatusRoutes: renderStatusRoutes(profile.statusGeneration, locale) || deckText(locale, 'none') }),
    deckText(locale, 'energySummary', { onPlayEnergy: profile.onPlayEnergy.map(row => deckText(locale, 'energyOnPlayRow', { card: row.card, cost: row.cost, hpLoss: row.hpLoss ? deckText(locale, 'hpLossPayment', { hpLoss: row.hpLoss }) : '', amount: row.amount, exhausts: row.exhausts ? deckText(locale, 'exhaustsLabel') : deckText(locale, 'ordinaryLaterUse') })).join('; ') || deckText(locale, 'none') }),
    deckText(locale, 'fixedBlockSummary', { fixedPlayedBlock: profile.fixedPlayedBlock.map(row => deckText(locale, 'fixedBlockRow', { card: row.card, block: row.block, cost: row.cost })).join('; ') || deckText(locale, 'none') }),
    deckText(locale, 'reductionSummary', { persistentReduction: profile.persistentReduction.map(row => deckText(locale, 'reductionRow', { card: row.card, amount: row.amount, target: row.target, cost: row.cost, exhausts: row.exhausts ? deckText(locale, 'exhaustReductionPersists') : deckText(locale, 'reductionReapply') })).join('; ') || deckText(locale, 'none') }),
    ...(candidates.some(row => row.access?.length) ? [
      deckText(locale, 'inventorySamplingRule'),
    ] : []),
    ...candidates.map(row => row.kind === 'addition'
      ? deckText(locale, 'additionSummary', { command: row.command, sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, type: row.type, typeBefore: row.typeBefore, typeAfter: row.typeAfter, cost: row.cost })
        + (row.strikeScaling ? deckText(locale, 'strikeDamageSummary', { base: row.strikeScaling.base, perCard: row.strikeScaling.perCard, countRule: row.strikeScaling.countRule }) : '')
        + (row.onPlayDraw ? ` ${onPlayDraw(row.onPlayDraw)}.` : '')
        + (row.laterTurnDraw ? deckText(locale, 'laterDrawIncrement', { laterTurnDraw: row.laterTurnDraw }) : '')
        + (row.drawPerExhaustAfterDeployment ? deckText(locale, 'exhaustDrawIncrement', { drawPerExhaustAfterDeployment: row.drawPerExhaustAfterDeployment }) : '')
        + (row.blockRequest ? deckText(locale, 'blockIncrement', { blockRequest: row.blockRequest }) : '')
        + (row.access?.map(access => ` ${accessText(access)}`).join('') || '')
        + (row.enemyStrengthReduction ? deckText(locale, 'reductionIncrement', { amount: row.enemyStrengthReduction.amount, target: row.enemyStrengthReduction.target, restoration: row.enemyStrengthReduction.restoration })
          + deckText(locale, 'ownedReductionUses', { owned: row.enemyStrengthReduction.owned.map(owned => `${owned.card}: ${owned.amount}, ${owned.target}, ${owned.restoration}`).join('; ') || deckText(locale, 'none') })
          + deckText(locale, 'reductionAdds')
          + (row.enemyStrengthReduction.restoration === 'no_scheduled_restoration'
            ? deckText(locale, 'reductionLifetime') : '')
          + deckText(locale, 'reductionOtherEffects') : '')
        + (row.preference ? deckText(locale, 'candidateStrategy', { text: row.preference.text }) : '')
      : row.kind === 'removal'
        ? deckText(locale, 'removalSummary', { command: row.command, sizeBefore: row.sizeBefore, sizeAfter: row.sizeAfter, type: row.type, typeBefore: row.typeBefore, typeAfter: row.typeAfter })
          + row.access.map(access => ` ${accessText(access)}`).join('')
        : deckText(locale, 'preserveSummary', { command: row.command, sizeBefore: row.sizeBefore })),
    ...profile.priorities.map(text => deckText(locale, 'maintainerComparison', { text: text })),
  ].join('\n');
}
