// Structural comparison of public inventory. Rust owns instance numbers and
// card mechanics; this module counts copies and renders their payment/timing.
const attributes = ['card', 'upgrades', 'misc_value', 'base_damage_override', 'base_block_override', 'cost_modifier', 'count'];
const positive = value => Number.isSafeInteger(value) && value > 0;
const label = row => `${row.card.count}x ${row.card.card}${row.card.upgrades > 0 ? '+' : ''}`;
const sum = rows => rows.reduce((total, row) => total + row.card.count, 0);
const list = rows => rows.map(label).join(', ') || 'none';
const inventoryError = message => Object.assign(new Error(message), {
  preparationGap: { kind: 'facts', reason: 'owned-mechanics-mismatch', message },
});
const fixedBlock = row => row.mechanics.base_block_delivery === 'once_on_play' && positive(row.persistent_block);
const persistentReduction = row => positive(row.mechanics.enemy_strength_down)
  && row.mechanics.enemy_strength_restoration === 'no_scheduled_restoration';
const percent = value => `${(value * 100).toFixed(1)}%`;
const accessText = access => `${access.description}: copies ${access.copiesBefore}->${access.copiesAfter} (owned ${access.owned});`
  + ` inventory sample ${access.windows.map(window => `${window.draws}: ${percent(window.before)}->${percent(window.after)}`).join(', ')}.`;

export function enduringStrengthGain(mechanics) {
  if (!positive(mechanics?.strength_gain) || positive(mechanics.temporary_strength_gain)
      || !['on_play', 'if_target_intent_base_damage_nonnegative_at_resolution', 'each_player_turn_start_post_draw']
        .includes(mechanics.strength_gain_delivery)) return;
  return { amount: mechanics.strength_gain, delivery: mechanics.strength_gain_delivery };
}

export const strengthTiming = delivery => ({
  on_play: 'on play',
  if_target_intent_base_damage_nonnegative_at_resolution: 'on play when the target has attack intent at resolution',
  each_player_turn_start_post_draw: 'at each player turn start after draw, following deployment',
})[delivery];

export function playedCardLifecycle({ type, exhausts, ethereal }) {
  if (type === 'Power') return 'Power leaves ordinary circulation after deployment';
  if (exhausts) return 'Exhaust after play';
  return 'ordinary redraw permits later uses'
    + (ethereal ? '; unplayed Ethereal follows the supplied turn-end rules' : '');
}

export const OTHER_CARD_EXHAUST_COMPARISON = 'Exhaust moves the selected card out of ordinary future reshuffles. '
  + 'Compare losing a useful function with removing a weak or obstructive future draw, with the supplied output and draw effects at their actual timing.';

export const attackInventoryText = profile => `Owned Attack-type inventory (${profile.counts.Attack} copies): `
  + (profile.attacks.map(row => `${row.card}: ${row.cost === -1 ? 'X' : row.cost} Energy; `
    + (row.delivery?.kind === 'attack'
      ? `${row.delivery.hits?.kind === 'fixed' ? `${row.delivery.hits.count} hit(s)` : `hits use ${row.delivery.hits?.kind ?? 'the supplied rule'}`}; targets ${row.delivery.targets}`
        + (row.delivery.amount?.kind === 'base' && Number.isSafeInteger(row.baseDamagePerHit)
          ? `; ${row.baseDamagePerHit} base damage/hit before combat modifiers` : '')
        + (row.damageGrowthOnPlay > 0 ? `; this copy's damage grows by ${row.damageGrowthOnPlay} after each play` : '')
      : 'output uses its full supplied effect')
    + `; ${row.exhausts ? 'Exhaust after play' : 'ordinary redraw before lifecycle modifiers'}`).join('; ') || 'none')
  + '. Damage amounts, Strength factors and additional effects use each supplied full card rule.';

// Rust owns each injection's destination and persistence. Explain the later
// ordinary route as well as the insertion; a Hand insertion is not a draw.
export function renderStatusRoutes(sources) {
  return sources.flatMap(source => source.injections.map(effect => {
    const entry = { hand: 'create in Hand -> ordinary end-turn discard -> Discard -> reshuffle -> Draw -> draw into Hand',
      discard_pile: 'create in Discard -> reshuffle -> Draw -> draw into Hand',
      draw_pile: 'create in Draw -> draw into Hand' }[effect.destination];
    const later = { persistent: 'After drawing, ordinary end-turn discard permits later reshuffles and draws.',
      ethereal: 'After drawing, Ethereal exhausts it if it remains in Hand at turn end under the supplied Retain rules. Discarding it earlier permits a later reshuffle and draw.' }[effect.persistence];
    if (!entry || !later) throw Object.assign(new Error('Status route needs its typed destination and persistence.'), {
      preparationGap: { kind: typeof effect.destination === 'string' && typeof effect.persistence === 'string' ? 'relations' : 'facts',
        reason: typeof effect.destination === 'string' && typeof effect.persistence === 'string' ? 'unsupported-status-route' : 'missing-status-route-facts', card: source.card,
        destination: effect.destination, persistence: effect.persistence },
    });
    return `${source.card}: ${effect.count} ${effect.card}; ${entry}. ${later} Draw-triggered effects apply at draw into Hand. Exhausting the Status cuts this ordinary redraw route; Retain or a skipped hand discard keeps it in Hand until a later pile-changing effect.`;
  })).join('\n');
}

// Interpret the reviewed Rust amount rule; the inventory has no typed tag
// census, so retain N rather than guessing tags from a CardId or name.
export function strikeScaledAttack(damage) {
  const delivery = damage?.delivery;
  if (delivery?.kind !== 'attack' || delivery.amount?.kind !== 'base_plus_strike_cards'
      || delivery.hits?.kind !== 'fixed' || delivery.hits.count !== 1 || damage.hit_count !== 1
      || !Number.isSafeInteger(damage.damage_per_hit) || damage.damage_per_hit < 0
      || !positive(delivery.amount.per_card)) return;
  return { base: damage.damage_per_hit, perCard: delivery.amount.per_card,
    countRule: 'N counts Strike-tagged copies across Hand, Draw and Discard together, including the played copy exactly once. Each counted copy entering Exhaust lowers N by one; a counted copy returning to those piles raises N by one.' };
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

function accessIncrement(kind, description, rows, size) {
  const count = sum(rows);
  return { kind, description, owned: list(rows), copiesBefore: count, copiesAfter: count + 1,
    windows: [...new Set([Math.min(5, size), Math.min(10, size)])].filter(positive).map(draws => ({ draws,
      before: atLeastOne(size, count, draws), after: atLeastOne(size + 1, count + 1, draws) })) };
}

function removalAccess(cards, removed, size) {
  const retained = cards.filter(row => row !== removed);
  return [
    ['retained_setup', 'other owned Power groups', retained.filter(row => row.mechanics.card_type === 'Power')],
    ['retained_draw', 'other owned on-play draw groups', retained.filter(row => positive(row.mechanics.draw_cards))],
    ['retained_protection', 'other owned fixed on-play Block or persistent enemy Strength reduction groups',
      retained.filter(row => fixedBlock(row) || persistentReduction(row))],
  ].flatMap(([kind, description, rows]) => {
    const count = sum(rows);
    if (!count) return [];
    return [{ kind, description, owned: list(rows), copiesBefore: count, copiesAfter: count,
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

export function battleTranceDrawOrder(profile) {
  const sources = [
    ...profile.onPlayDraw.map(source => `${source.card} (${source.requested} draws, ${source.cost} Energy)`),
    ...profile.exhaustDraw.map(source => `${source.card} (${source.requested} draws per Exhaust after deployment)`),
  ];
  return 'Use other needed on-play or Exhaust-triggered draws before BattleTrance. '
    + 'Draws requested afterward during that same turn face NoDraw, so compare that order with the useful actions the new draw reaches.'
    + (sources.length ? ` Owned draw sources affected by this order: ${sources.join('; ')}.` : '');
}

export function renderOnPlayDrawCoverage(profile) {
  const coverage = profile.onPlayDrawCoverage;
  return `Reviewed on-play multi-draw inventory: ${coverage.copies} of ${profile.size} original cards (${coverage.owned}); `
    + `unordered inventory sample ${coverage.windows.map(window => `${window.draws}: ${percent(window.probability)}`).join(', ') || 'empty inventory'}.`;
}

function cardIncrement(action, mechanics, counts, size, cards) {
  const type = mechanics.card_type;
  const strikeScaling = strikeScaledAttack(mechanics.damage);
  const row = { card: { card: action.card, upgrades: action.upgrades, count: 1 }, mechanics,
    persistent_cost: mechanics.cost };
  const matching = cards.filter(row => row.card.card === action.card);
  const protection = mechanics.base_block_delivery === 'once_on_play' && positive(mechanics.block);
  const persistent = persistentReduction({ mechanics });
  const access = [
    ...(matching.length && !protection && !persistent
      ? [accessIncrement('same_card', `${action.card} across owned upgrade states`, matching, size)] : []),
    ...(protection ? [accessIncrement('fixed_block', `original cards requesting at least ${mechanics.block} Block on play`,
      cards.filter(row => fixedBlock(row) && row.persistent_block >= mechanics.block), size)] : []),
    ...(persistent ? [accessIncrement('persistent_reduction', 'original cards requesting persistent enemy Strength reduction',
      cards.filter(persistentReduction), size)] : []),
    ...(type === 'Attack' ? [accessIncrement('attack_type', 'original Attack-type cards, each with its supplied output and conditions',
      cards.filter(row => row.mechanics.card_type === 'Attack'), size)] : []),
    ...(expandsHandAccess(draw(row)) ? [accessIncrement('multi_draw', 'original on-play multi-draw cards, with their own payment and selection rules',
      cards.filter(owned => expandsHandAccess(draw(owned))), size)] : []),
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

export function preparationDeckComparison(observation, normalize = command => command) {
  const state = observation.position?.observation?.observation;
  const evidence = observation.deck_evidence;
  if (!state?.cards || !evidence) return;
  if (evidence.schema !== 'sts-agent-deck-evidence-v1'
      || evidence.public_snapshot_id !== observation.position.public_snapshot_id
      || evidence.cards?.length !== state.cards.length) {
    throw inventoryError('Deck comparison requires mechanics bound to the current public inventory.');
  }
  const remaining = [...evidence.cards];
  const cards = state.cards.map(card => {
    const index = remaining.findIndex(row => attributes.every(key => Object.hasOwn(card, key)
      && row.card?.[key] === card[key]));
    if (index < 0) throw inventoryError('Deck comparison mechanics differ from the current owned card attributes.');
    const [row] = remaining.splice(index, 1);
    if (row.mechanics?.card !== card.card || row.mechanics?.upgrades !== card.upgrades
        || !positive(card.count) || !Number.isSafeInteger(row.persistent_cost)) {
      throw inventoryError('Deck comparison lacks matching instance mechanics.');
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
      const increment = cardIncrement(acquisition, mechanics, counts, size, cards);
      const nextAct = state.run?.pending_act_entry ?? state.run?.act;
      if (positive(mechanics.exhaust_draw_power) && reusableExhaust.length
          && nextAct >= 2 && mechanics.cost >= 0 && mechanics.cost <= state.base_energy_per_turn) {
        increment.preference = { id: 'owned-reusable-exhaust-access', origin: 'maintainer_comparison_preference',
          text: (multiDrawCopies
            ? `Compare this deployed access engine's additional draws from ${list(reusableExhaust)} with the owned ${multiDrawCopies} on-play multi-draw copies (${list(multiDraw)}). Each route uses its own access, fuel, payment and lifecycle. `
            : `Prioritize this access engine over another ordinary paid attack or skipping in this owned plan: ${list(reusableExhaust)} supply reusable Exhaust routes, while owned reviewed on-play draws provide at most self-replacement. `)
            + 'Deploying this Power enables those routes to reach existing useful cards. Compare deployment protection and the listed Energy sources as its realization conditions.' };
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
        access: removalAccess(cards, owned, size) }];
    }
    if (action?.kind === 'skip_card_reward') return [{ command, kind: 'preserve', sizeBefore: size, sizeAfter: size }];
    if (action?.kind === 'singing_bowl') {
      const resource = observation.decision_evidence?.kind === 'card_acquisition_options'
        ? observation.decision_evidence.resource_options?.find(item => item.semantic_action === row.semantic_action) : undefined;
      if (!resource || !Number.isSafeInteger(resource.hp_gain) || resource.hp_gain < 0
          || !Number.isSafeInteger(resource.max_hp_gain) || resource.max_hp_gain <= 0
          || typeof resource.native_rule !== 'string' || !resource.native_rule.trim()) {
        throw inventoryError('Singing Bowl needs its matching native immediate resource outcome.');
      }
      return [{ command, kind: 'preserve', sizeBefore: size, sizeAfter: size,
        resource: { kind: 'singing_bowl', hpGain: resource.hp_gain, maxHpGain: resource.max_hp_gain,
          nativeRule: resource.native_rule } }];
    }
    return [];
  });
  return { schema: 'sts-preparation-deck-comparison-v1', basis: 'current_public_inventory_and_shared_native_mechanics',
    size, baseEnergy: state.base_energy_per_turn, counts, attackCosts: costs,
    attackCopies: list(attacks), playedExhaustAttacks: list(attacks.filter(row => row.mechanics.exhausts)),
    attacks: attacks.map(row => ({ card: label(row), copies: row.card.count, cost: row.persistent_cost,
      delivery: row.mechanics.damage?.delivery, baseDamagePerHit: row.persistent_damage,
      damageGrowthOnPlay: row.mechanics.damage_growth_on_play, exhausts: row.mechanics.exhausts })),
    strikeTags: cards.every(row => Array.isArray(row.mechanics.tags))
      ? { copies: sum(cards.filter(row => row.mechanics.tags.includes('Strike'))),
        owned: list(cards.filter(row => row.mechanics.tags.includes('Strike'))) } : undefined,
    strengthSources: cards.flatMap(row => enduringStrengthGain(row.mechanics)
      ? [{ card: label(row), type: row.mechanics.card_type, cost: row.persistent_cost,
        exhausts: row.mechanics.exhausts, ethereal: row.mechanics.ethereal,
        ...enduringStrengthGain(row.mechanics) }] : []),
    onPlayDraw: cards.flatMap(row => draw(row) ? [draw(row)] : []),
    onPlayDrawCoverage: { copies: multiDrawCopies, owned: list(multiDraw),
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
    playedExhaust: list(cards.filter(row => row.mechanics.exhausts)),
    ethereal: list(cards.filter(row => row.mechanics.ethereal)),
    otherCardExhaust: list(cards.filter(row => row.mechanics.exhausts_other_cards)),
    skillExhaustPowers: cards.filter(row => row.card.card === 'Corruption').map(row =>
      ({ card: label(row), cost: row.persistent_cost })),
    reusableOtherCardExhaust: list(reusableExhaust),
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
      'Compare additions with the current whole deck and keeping this draw slot. Taking a card retains every original copy; removing a weak recurring draw changes the original pool.',
      'For an additional attack, compare the useful output payable within the owned Energy and attack pool with access to existing output and protection. Its damage above one Strike describes the card; its recommendation depends on the whole-deck increment.',
      'An access engine gains value from its listed owned triggers and payment sources. Compare its deployment cost with reaching and paying useful actions after deployment. Protection can buy that deployment time.',
      'For another copy of an owned function, compare its actual additional quantity, earlier access and combined applications. A recurring Exhaust effect can supply triggers on later uses after ordinary redraws; engine deployment is paid before those triggers.',
      'Additional protection earns value through another payable protective draw and, for persistent reduction, extra successful applications. Compare that quantity and access gain with its draw slot and Energy cost. Single-card Block/Energy efficiency compares those cards when reached; whole-deck coverage depends on their copy counts and access as well.',
      'Engine deployment protection uses the combined Energy and access of the engine and protective action. Later attack turns need their own protection. Independent draw, retained protection and deployed passive effects can cover those needs under their supplied triggers; already abundant usable coverage favors preserving access to the engine.',
    ] };
}

export function preparationDeckConditionBindings(observation, normalize = command => command) {
  const profile = preparationDeckComparison(observation, normalize);
  if (!profile) return [];
  return profile.candidates.flatMap(row => {
    const reduction = row.enemyStrengthReduction;
    const persistent = reduction?.restoration === 'no_scheduled_restoration';
    if (row.kind === 'removal') {
      const shop = observation.position.observation.observation.context?.kind === 'shop';
      const payment = row.cost === -2 ? 'Its supplied normal-play rule is unplayable'
        : row.cost === -1 ? 'Its normal X-cost play uses the Energy available at use'
          : `Its normal play costs ${row.cost} Energy`;
      const lifecycle = row.type === 'Power' ? 'leaves ordinary card circulation after deployment'
        : row.exhausts ? 'Exhausts after play' : row.ethereal ? 'Ethereal' : 'ordinary redraw permits later uses';
      return [{ command: row.command, id: 'removal-retained-access-v1', fixedMechanics: [
        `${row.card}${row.upgrades > 0 ? '+' : ''}: remove one permanent copy; deck ${row.sizeBefore}->${row.sizeAfter}. ${payment}; instance base damage ${row.damage}, Block ${row.block}; ${lifecycle}. Its full supplied effects define the function forgone.`,
        ...row.access.map(accessText),
      ], choices: [
        { id: '@retained-access', text: 'Removing a weak recurring draw improves access to retained setup, useful damage and protection in upcoming fights. Compare its draw burden with the owned alternatives and their payment.' },
        { id: '@foregone-function', text: 'Preserve a target whose damage, protection, access or enabling function remains useful in the owned plan. Its lifecycle determines how long its draw burden persists during combat.' },
        shop ? { id: '@gold-opportunity', text: 'Compare this removal with current useful purchases and retaining Gold for a connected later shop. Leaving keeps the target through intervening fights and gives up this shop\'s removal opportunity; the next price increase follows an actual removal purchase.' }
          : { id: '@preparation-opportunity', text: 'Compare this cleanup with the useful upgrade, recovery or other preparation available at this node. Keeping the target preserves its function and draw burden through the next fights.' },
        { id: '@review-access', text: 'A specific target-function or budget fact needs maintainer review.' },
      ] }];
    }
    if (row.kind === 'preserve') {
      const additions = profile.candidates.filter(candidate => candidate.kind === 'addition');
      if (!additions.length) return [];
      return [{ command: row.command, id: 'reward-preservation-comparison-v1', fixedMechanics: [
        `Skipping keeps the ${profile.size}-card original pool and the same Gold. Each offered addition retains those originals and adds one permanent copy; its play follows the supplied access, payment and lifecycle rules.`,
        `Actual competing additions: ${additions.map(candidate => candidate.command).join(', ')}. Use the strongest relevant addition as the comparison, with its recorded benefit and realization conditions.`,
      ], choices: additions.flatMap((candidate, index) => [
        { id: `@keep-vs-${index + 1}`, alternativeCommand: candidate.command, emphasis: 'preserved_access',
          text: `Compared with ${candidate.command}, favor preserving access to owned useful cards when that addition's useful increment is outweighed by its extra draw and payment burden.` },
        { id: `@gain-vs-${index + 1}`, alternativeCommand: candidate.command, emphasis: 'foregone_gain',
          text: `Compared with ${candidate.command}, assess the useful function or output forgone by skipping when that addition improves the current preparation plan enough to justify its extra draw and payment.` },
      ]).concat({ id: '@review-access', text: 'A specific missing fact about an offered addition or the owned plan needs maintainer review for this comparison.' }) }];
    }
    if (row.kind === 'addition' && row.card === 'BattleTrance' && row.onPlayDraw) {
      return [{ command: row.command, id: 'draw-order-access-v1', fixedMechanics: [
        `${row.card}${row.upgrades > 0 ? '+' : ''}: deck ${row.sizeBefore}->${row.sizeAfter}; costs ${row.cost} Energy; while draw is allowed, requests ${row.onPlayDraw.requested} cards before applying NoDraw for the rest of that turn. Spending this card and drawing contributes ${row.onPlayDraw.handChange} to Hand before other effects and capacity.`,
        'Normal turn-start draw and deployed Brutality turn-start draw resolve before the player can play BattleTrance. Its NoDraw is removed at that turn\'s end; the next turn starts with ordinary drawing again.',
        `Owned on-play draw: ${profile.onPlayDraw.map(source => `${source.card}: cost ${source.cost}, request ${source.requested}`).join('; ') || 'none'}. Owned per-Exhaust draw after deployment: ${profile.exhaustDraw.map(source => `${source.card}: cost ${source.cost}, request ${source.requested}/event`).join('; ') || 'none'}.`,
      ], choices: [
        { id: '@independent-access', text: 'Assess additional access to owned payable output, protection and enabling cards during this turn. The net Hand gain costs the printed Energy and one original draw slot.' },
        { id: '@same-turn-order', text: battleTranceDrawOrder(profile) },
        { id: '@draw-slot-cost', text: 'Compare keeping access to already sufficient owned draw and useful actions with this extra copy when the additional Hand access mainly reaches unusable or redundant actions.' },
        { id: '@review-access', text: 'A specific missing access or payment fact needs maintainer review for this comparison.' },
      ] }];
    }
    if (row.kind === 'addition' && row.type === 'Skill' && row.onPlayDraw && !row.blockRequest && !persistent) {
      return [{ command: row.command, id: 'on-play-access-v1', fixedMechanics: [
        `${row.card}${row.upgrades > 0 ? '+' : ''}: deck ${row.sizeBefore}->${row.sizeAfter}; costs ${row.cost} Energy; on-play draw request ${row.onPlayDraw.requested}; Hand contribution ${row.onPlayDraw.handChange ?? 'selection-dependent'} before other effects and capacity.`,
        `Played copy: ${row.exhausts ? 'Exhaust after play' : 'ordinary redraw permits later uses'} before other deployed effects. Hand selection and pile-changing effects follow this candidate's supplied full rule.`,
        `Owned on-play draw: ${profile.onPlayDraw.map(source => `${source.card}: ${source.cost} Energy, draw ${source.requested}, Hand contribution ${source.handChange ?? 'selection-dependent'}`).join('; ') || 'none'}.`,
        ...row.access.filter(access => access.kind === 'multi_draw').map(accessText),
      ], choices: [
        { id: '@independent-access', text: 'Assess earlier and repeated access to owned output, protection and enabling cards. Compare the added multi-draw or Hand turnover, its copy access and usable payment with another component that needs those cards.' },
        ...(row.onPlayDraw.handChange === null ? [{ id: '@selection-turnover', text: row.onPlayDraw.exhaustsOther
          ? OTHER_CARD_EXHAUST_COMPARISON
          : 'Use the supplied selection rule to exchange a current Hand card for fresh draws. Compare sacrificing a low-value or obstructive card, the newly reachable plays and later circulation.' }] : []),
        { id: '@access-cost', text: 'Weigh useful access against this copy\'s draw slot, Energy, Gold, HP and other printed costs, including the protection or recovery needed on the connected route.' },
        { id: '@review-access', text: 'A specific missing draw, selection or payment fact needs maintainer review.' },
      ] }];
    }
    if (row.kind === 'addition' && row.enduringStrength) {
      const source = row.enduringStrength;
      return [{ command: row.command, id: 'enduring-strength-increment-v1', fixedMechanics: [
        `${row.card}${row.upgrades > 0 ? '+' : ''}: deck ${row.sizeBefore}->${row.sizeAfter}; costs ${row.cost} Energy; request ${source.amount} Strength ${strengthTiming(source.delivery)}.`,
        `Played copy: ${row.exhausts ? 'Exhaust after play' : row.type === 'Power' ? 'Power leaves ordinary circulation after deployment' : 'ordinary redraw permits later paid uses under its target condition'} before lifecycle modifiers.`,
        `Owned enduring Strength sources: ${profile.strengthSources.map(owned => `${owned.card}: ${owned.cost} Energy, request ${owned.amount} ${strengthTiming(owned.delivery)}`).join('; ') || 'none'}.`,
        ...row.access.filter(access => access.kind === 'same_card').map(accessText),
        attackInventoryText(profile),
      ], choices: [
        { id: '@strength-output', text: `Assess the added Strength through the owned attacks (${profile.attackCopies}) reached and paid after the gain, with each attack's supplied hit, target and Strength rules.` },
        { id: '@growth-access', text: 'An additional source can reach Hand earlier and supply another successful gain. Compare that increment with the owned source amounts, copy access, deployment or redraw conditions, and timely attack access.' },
        { id: '@setup-cost', text: 'Weigh the additional source\'s draw slot, acquisition cost and play payment against accessing and protecting the already-owned useful actions.' },
        { id: '@review-access', text: 'A specific source, attack or timing fact needs maintainer review.' },
      ] }];
    }
    if (row.kind === 'addition' && row.attackDelivery?.amount?.kind === 'player_block') {
      const support = profile.blockSupport;
      return [{ command: row.command, id: 'current-block-output-v1', fixedMechanics: [
        `${row.card}${row.upgrades > 0 ? '+' : ''}: deck ${row.sizeBefore}->${row.sizeAfter}; costs ${row.cost} Energy. Current Block at attack resolution supplies base damage. ${row.card === 'BodySlam' ? 'BodySlam preserves that Block. ' : ''}Strength, Weak, Vulnerable, target, relic modifiers and other effects follow the full supplied Attack rule.`,
        `Owned fixed on-play Block requests: ${profile.fixedPlayedBlock.map(source => `${source.card}: ${source.cost} Energy, ${source.block} Block`).join('; ') || 'none'}.`,
        ...profile.fixedPlayedBlock.filter(source => source.cost >= 0 && row.cost >= 0
          && source.cost + row.cost <= profile.baseEnergy).map(source =>
          `Same-Hand payment: ${source.card} -> ${row.card}; ${source.cost + row.cost} of ${profile.baseEnergy} base Energy; Block request ${source.block} before modifiers. Access to both cards and other needed actions determines usable output.`),
        `Owned retention: ${support.retention.map(source => `${source.card}: ${source.cost} Energy to deploy`).join('; ') || 'none'}. Cross-turn output uses Block remaining after the intervening enemy actions and the supplied retention rules.`,
        `Owned multiplication: ${support.multiplication.map(source => `${source.card}: ${source.cost} Energy; ${source.exhausts ? 'Exhaust after play' : 'ordinary redraw permits later use'}`).join('; ') || 'none'}. Each multiplication uses the Block present when that card resolves, with access and payment for that use.`,
        `Owned damage-cap protection: ${support.damageCaps.map(source => `${source.card}: ${source.cost} Energy, request ${source.amount} Intangible with damage cap ${source.cap}; ${source.exhausts ? 'Exhaust after play' : 'ordinary redraw permits later use'}`).join('; ') || 'none'}. Apply its actual active duration to the enemy hits before evaluating surviving Block.`,
        `Owned Block-based attacks: ${profile.attacks.filter(source => source.delivery?.amount?.kind === 'player_block').map(source => `${source.card}: ${source.cost} Energy`).join('; ') || 'none'}.`,
        renderOnPlayDrawCoverage(profile),
      ], choices: [
        { id: '@same-turn-output', text: 'Compare useful damage from owned payable Block generation followed by this attack in the same turn, together with the protection that Block supplies.' },
        ...(support.retention.length ? [{ id: '@retained-output', text: `Compare deploying ${support.retention.map(source => source.card).join(', ')}, building surplus through enemy turns using owned Block sources (${profile.fixedPlayedBlock.map(source => source.card).join(', ')}), then reaching and paying this attack. Owned multiplication (${support.multiplication.map(source => source.card).join(', ') || 'none'}) and protection use their actual access and timing.` }] : []),
        { id: '@output-access-cost', text: 'Compare the extra damage conversion with improving access to the existing protection, growth and attacks. Rate the conversion using reachable Block and the payment for the whole sequence.' },
        { id: '@review-access', text: 'A specific Block, timing or payment fact needs maintainer review.' },
      ] }];
    }
    if (row.kind === 'addition' && row.type === 'Attack' && !row.blockRequest && !persistent) {
      return [{ command: row.command, id: 'attack-whole-deck-increment-v1', fixedMechanics: [
        `${row.card}${row.upgrades > 0 ? '+' : ''}: deck ${row.sizeBefore}->${row.sizeAfter}; Attack copies ${row.typeBefore}->${row.typeAfter}; retains the owned attack suite; ${row.cost === -1 ? 'normal X-cost play uses available Energy at use' : `normal play costs ${row.cost} Energy`}; base Energy ${profile.baseEnergy}/turn.`,
        ...(Number.isSafeInteger(row.baseDamagePerHit) && Number.isSafeInteger(row.attackDelivery?.hits?.count)
          ? [`Base damage per hit ${row.baseDamagePerHit}; hits per play ${row.attackDelivery.hits.count}. Resolved Strength applies on each actual hit before the other attack modifiers.`] : []),
        ...(row.strikeScaling ? [
          `Damage request before Strength and other combat modifiers: ${row.strikeScaling.base} + ${row.strikeScaling.perCard} * N. ${row.strikeScaling.countRule}`,
        ] : []),
        ...(row.attackDelivery?.targets === 'random_enemy_per_hit'
          ? ['Each hit chooses among current enemies. With one enemy, every hit goes to that enemy.'] : []),
        ...row.access.map(accessText),
        ...(row.onPlayDraw ? [`Its on-play draw requests ${row.onPlayDraw.requested} cards; the supplied draw/topdeck rule contributes ${row.onPlayDraw.handChange ?? 'selection-dependent'} to Hand before other effects and capacity.`] : []),
        ...(row.exhausts ? ['The printed card Exhausts after play; further uses follow owned recovery and lifecycle modifiers.'] : []),
        `Owned draw payoffs per Exhaust event after deployment: ${profile.exhaustDraw.map(source => `${source.card}: pay ${source.cost} Energy to deploy, request ${source.requested}/event`).join('; ') || 'none'}.`,
      ], choices: [
        { id: '@output-access', text: 'Assess earlier access to useful payable damage against the whole owned attack suite. Weak early attacks or a concrete damage coverage gap can make another efficient attack valuable.' },
        { id: '@extra-output', text: 'Assess the extra useful attack play and its damage beyond the attacks already payable in that turn or fight, including the named owned relic or Strength interaction and protection still needed.' },
        { id: '@added-function', text: 'Assess the supplied draw, retrieval, debuff, cleanup, resource or deployed Exhaust payoff as this addition\'s increment. Use its named owned sources and access/payment order together.' },
        { id: '@draw-payment-cost', text: 'Favor access to owned protection, draw and enabling cards when this addition mainly repeats damage already reachable and payable. Compare that draw and Energy burden with the extra useful output or function.' },
        { id: '@review-access', text: 'A specific missing output, access, payment or encounter fact needs maintainer review for this comparison.' },
      ] }];
    }
    if (row.kind !== 'addition' || (!row.blockRequest && !persistent)) return [];
    const fixedMechanics = [
      `${row.card}${row.upgrades > 0 ? '+' : ''}: deck ${row.sizeBefore}->${row.sizeAfter}; normal play costs ${row.cost} Energy; the new copy occupies an original draw slot.`,
      ...row.access.map(accessText),
    ];
    if (row.blockRequest) fixedMechanics.push(`On play request ${row.blockRequest} Block before combat modifiers.`);
    const acquisition = observation.position.actions.find(action => normalize(action.semantic_action, observation) === row.command);
    const mechanics = preparationCardAcquisition(observation, acquisition)?.mechanics;
    const exhaustPaths = [
      ...(profile.otherCardExhaust !== 'none' ? [profile.otherCardExhaust] : []),
      ...profile.skillExhaustPowers.map(source => `${source.card}: ${source.cost} Energy to deploy, then played Skills Exhaust`),
    ].join('; ') || 'none';
    if (positive(mechanics?.energy_gain_on_exhaust)) fixedMechanics.push(
      `First usable benefit: the normal paid play supplies ${row.blockRequest} Block directly. Additional ${mechanics.energy_gain_on_exhaust} Energy triggers when this card itself enters Exhaust. Owned applicable Exhaust routes: ${exhaustPaths}. Compare the direct Block and the additional triggered Energy separately.`);
    if (row.onPlayDraw) fixedMechanics.push(`On play request ${row.onPlayDraw.requested} draw; the supplied draw/topdeck rule contributes ${row.onPlayDraw.handChange ?? 'selection-dependent'} to Hand before other effects and capacity.`);
    if (persistent) {
      const before = reduction.owned.filter(owned => owned.restoration === 'no_scheduled_restoration')
        .reduce((total, owned) => total + owned.amount * owned.count, 0);
      fixedMechanics.push(`Each successful play requests ${reduction.amount} Strength reduction on ${reduction.target}. If all owned persistent-reduction originals and this copy apply to the same target, the requested total changes ${before}->${before + reduction.amount} before intervening clears or Strength changes. Each amount affects subsequent attack hits while present. The played card's Exhaust controls its reuse; the applied reduction remains. Artifact and target effects use the supplied rules.`);
    }
    return [{ command: row.command, id: 'protection-quantity-access-v1', fixedMechanics,
      choices: [
        ...(row.blockRequest ? [{ id: '@protective-draw', realizations: row.cost <= profile.baseEnergy ? ['direct'] : ['sequence', 'future'],
          text: 'An additional payable protective draw improves coverage when existing protective copies arrive sparsely. Compare its quantity and sample access with its Energy cost, setup protection and later attack turns; include its other supplied on-play effects.' }] : []),
        ...(positive(mechanics?.energy_gain_on_exhaust) ? [{ id: '@self-exhaust-energy',
          realizations: exhaustPaths === 'none' && !mechanics.ethereal ? ['future'] : ['sequence'],
          text: `The additional Energy uses a route exhausting this source card itself. Owned applicable Exhaust routes: ${exhaustPaths}; its Ethereal flag is ${mechanics.ethereal === true}. Assess that trigger separately from the direct paid Block.` }] : []),
        ...(persistent ? [{ id: '@extra-application',
          text: 'An additional successful application deepens mitigation on one target or covers another eligible target. Compare actual affected hits, copy access, Artifact and target Strength changes with spending another draw and its payment.' }] : []),
        { id: '@coverage-cost', text: 'Keeping this draw slot favors access to existing useful actions when usable protection is already abundant, independent access reaches it, or this extra payment competes with more useful setup and actions.' },
        { id: '@review-access', text: 'A specific missing access, payment or encounter fact needs maintainer review for this comparison.' },
      ] }];
  });
}

export function renderDeckComparison(profile, { candidateCommands } = {}) {
  if (!profile) return '';
  const commandSet = candidateCommands ? new Set(candidateCommands) : undefined;
  const candidates = commandSet ? profile.candidates.filter(row => commandSet.has(row.command)) : profile.candidates;
  const onPlayDraw = row => `${row.card}: cost ${row.cost}, draw request ${row.requested},`
    + (row.topdeck > 0 ? ` then topdeck ${row.topdeck},` : '')
    + (row.handChange === null ? ' additional Hand removal/selection uses the supplied rule'
      : ` Hand contribution ${row.handChange >= 0 ? '+' : ''}${row.handChange} from spending this card and its draw/topdeck, before other Hand effects, capacity or draw prevention`);
  return ['Whole-deck comparison baseline:',
    `  Inventory: ${profile.size} cards; ${Object.entries(profile.counts).map(([type, count]) => `${count} ${type}`).join(', ')}; base Energy ${profile.baseEnergy}/turn.`,
    `  Owned attacks: ${profile.attackCopies}. Persistent attack costs: ${Object.entries(profile.attackCosts).map(([cost, count]) => `${count}x ${cost}c`).join(', ') || 'none'}.`,
    `  ${attackInventoryText(profile)}`,
    `  Owned enduring Strength sources: ${profile.strengthSources.map(row => `${row.card}: ${row.cost} Energy, request ${row.amount} ${strengthTiming(row.delivery)}`).join('; ') || 'none'}.`,
    `  Attacks that Exhaust when played: ${profile.playedExhaustAttacks}. Their original copies supply one ordinary play before recovery effects.`,
    `  Reviewed on-play draw: ${profile.onPlayDraw.map(onPlayDraw).join('; ') || 'none'}.`,
    `  ${renderOnPlayDrawCoverage(profile)}`,
    `  Draw on later turns after Power deployment: ${profile.laterTurnDraw.map(row => `${row.card}: cost ${row.cost}, request ${row.requested}/turn`).join('; ') || 'none'}.`,
    `  Draw per Exhaust event after Power deployment: ${profile.exhaustDraw.map(row => `${row.card}: cost ${row.cost}, request ${row.requested}/event`).join('; ') || 'none'}.`,
    `  Owned Exhaust event sources: on-play ${profile.playedExhaust}; unplayed Ethereal at turn end ${profile.ethereal}; effects exhausting other cards ${profile.otherCardExhaust}. Printed reusable other-card Exhaust effects: ${profile.reusableOtherCardExhaust}; later uses need ordinary redraw and payment, with combat lifecycle modifiers applied.`,
    `  Owned Status generation and later routes: ${renderStatusRoutes(profile.statusGeneration) || 'none'}.`,
    `  Owned Energy on play: ${profile.onPlayEnergy.map(row => `${row.card}: pay ${row.cost}c${row.hpLoss ? ` and request ${row.hpLoss} HP loss` : ''}, gain ${row.amount} Energy; ${row.exhausts ? 'Exhausts' : 'ordinary redraw permits later use'}`).join('; ') || 'none'}.`,
    `  Reviewed fixed Block on play: ${profile.fixedPlayedBlock.map(row => `${row.card}: ${row.block} Block for ${row.cost}c`).join('; ') || 'none'}. Compare mitigation, potions and recurring protection from their supplied rules as well.`,
    `  Owned persistent enemy Strength reduction: ${profile.persistentReduction.map(row => `${row.card}: ${row.amount} reduction on ${row.target} for ${row.cost}c; ${row.exhausts ? 'the played card Exhausts, the applied reduction remains' : 'ordinary redraw permits another application'}`).join('; ') || 'none'}. Target Strength changes, Artifact and debuff clears follow their supplied rules.`,
    ...(candidates.some(row => row.access?.length) ? [
      '  Inventory access reference: uniform sampling without replacement from original permanent copies. Each percentage is the chance of at least one matching original in a sample of the stated size. Actual combat draw uses its relics, extra draws, generated cards and pile changes; play additionally requires payment and applicable effect conditions.',
    ] : []),
    ...candidates.map(row => row.kind === 'addition'
      ? `  ${row.command}: deck ${row.sizeBefore}->${row.sizeAfter}; ${row.type} copies ${row.typeBefore}->${row.typeAfter}; original copies retained; cost ${row.cost}c.`
        + (row.strikeScaling ? ` Damage before Strength and other combat modifiers: ${row.strikeScaling.base} + ${row.strikeScaling.perCard} * N. ${row.strikeScaling.countRule}` : '')
        + (row.onPlayDraw ? ` ${onPlayDraw(row.onPlayDraw)}.` : '')
        + (row.laterTurnDraw ? ` After deployment, request ${row.laterTurnDraw} on later turns.` : '')
        + (row.drawPerExhaustAfterDeployment ? ` After deployment, request ${row.drawPerExhaustAfterDeployment} per Exhaust event; owned trigger sources are listed above.` : '')
        + (row.blockRequest ? ` On play request ${row.blockRequest} Block, before combat modifiers.` : '')
        + (row.access?.map(access => ` ${accessText(access)}`).join('') || '')
        + (row.enemyStrengthReduction ? ` On play request ${row.enemyStrengthReduction.amount} Strength reduction on ${row.enemyStrengthReduction.target}; restoration ${row.enemyStrengthReduction.restoration}.`
          + ` Owned reduction uses: ${row.enemyStrengthReduction.owned.map(owned => `${owned.card}: ${owned.amount}, ${owned.target}, ${owned.restoration}`).join('; ') || 'none'}.`
          + ' Each successful application subtracts its amount from the target\'s current Strength; successive applications on the same enemy add their reductions while present.'
          + (row.enemyStrengthReduction.restoration === 'no_scheduled_restoration'
            ? ' The played card\'s Exhaust governs card reuse; the applied reduction remains until a target effect changes or clears it.' : '')
          + ' Artifact, debuff clears and scheduled restoration use the supplied rules.' : '')
        + (row.preference ? ` Candidate strategy: ${row.preference.text}` : '')
      : row.kind === 'removal'
        ? `  ${row.command}: deck ${row.sizeBefore}->${row.sizeAfter}; ${row.type} copies ${row.typeBefore}->${row.typeAfter}.`
          + row.access.map(access => ` ${accessText(access)}`).join('')
        : `  ${row.command}: preserve the ${row.sizeBefore}-card original pool.`),
    ...profile.priorities.map(text => `  Maintainer comparison: ${text}`),
  ].join('\n');
}
