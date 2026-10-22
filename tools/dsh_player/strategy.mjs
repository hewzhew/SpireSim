// Maintainer-authored policy, grounded in the tracked expert notes. These
// preferences are distinct from the simulator's card effects and legal actions.
import catalogue from './strategy-roles.json' with { type: 'json' };
import { isDeepStrictEqual } from 'node:util';
import { createPresentation, resolveLocale } from '@sts-simulator/presentation';
import { preparationDeckComparison, preparationCardAcquisition, enduringStrengthGain, expandsHandAccess, renderOnPlayDrawCoverage } from './deck.mjs';
export const STRATEGY_SOURCES = {
  jorbs: { author: 'Jorbs', reference: 'reference/expert/notes/extra-expert-reading-20260923.md',
    locator: 'A20H retrospective: General Advice and Ironclad',
    url: 'https://www.reddit.com/r/slaythespire/comments/xhkiws/how_i_won_80_of_my_a20h_spire_runs_for_a_month/' },
  je: { author: 'JapaneseExport', reference: 'reference/expert/notes/extra-expert-reading-20260923.md',
    locator: 'Ironclad card replies: Dark Embrace, Metallicize, Clothesline, Dual Wield, Sword Boomerang',
    url: 'https://www.reddit.com/r/slaythespire/comments/1e1tete/70_winrate_a20h_ironclad_tier_list/' },
  cx: { author: '漅汐', reference: 'reference/expert/notes/tieba-10072740344-card-index.md',
    locator: 'Role catalogue; floors 7-15 and card entries',
    url: 'https://tieba.baidu.com/p/10072740344?fr=frs' },
  panacea: { author: 'Panacea', reference: 'reference/expert/outlines/reddit-ironclad-10-streak-guide.jsonl',
    locator: 'Act 1 pathing, early drafting, Act 2+ planning, shops and events',
    url: 'https://www.reddit.com/r/slaythespire/comments/rlcxra/ironclad_in_depth_guide_for_a20_heart_kills_10/' },
  equation: { author: '我要吃人1', reference: 'reference/expert/notes/tieba-7709780592.md',
    locator: 'Transition, card-slot concentration, fees and blood circulation',
    url: 'https://tieba.baidu.com/p/7709780592?fr=pb' },
};

const names = ids => new Set(ids.split(' '));
const strength = names('Inflame SpotWeakness Flex DemonForm');
const selfLoss = names('Bloodletting Hemokinesis Offering Combust Brutality Pain');
const strengthRelics = names('Vajra Girya Shuriken DuVuDoll SlingOfCourage RedSkull MutagenicStrength Brimstone');
const access = names('BattleTrance PommelStrike ShrugItOff BurningPact Offering Brutality Warcry');
const triggeredAccess = names('Headbutt Exhume DarkEmbrace Evolve Dropkick Havoc');
const damage = names('Anger TwinStrike Headbutt WildStrike HeavyBlade PerfectedStrike SwordBoomerang Clothesline IronWave ThunderClap Cleave Uppercut BloodForBlood Hemokinesis SeverSoul Whirlwind Carnage RecklessCharge Pummel Rampage Immolate Bludgeon FiendFire');
const attackPlayRelics = names('Kunai Shuriken OrnamentalFan Nunchaku PenNib');

const strategyViews = Object.fromEntries(['en', 'zh-CN'].map(locale => [locale, createPresentation({ locale })]));
const strategyText = (locale, key, parameters = {}) => strategyViews[resolveLocale(locale)].text(`scripted:strategy.${key}`,
  Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));

const policy = (id, sources, preference, change) => ({ id, sources, preference, change });
// Maintained applicability index. The source role text is retained separately:
// a draw payoff is not a draw source, and terminal protection is not growth.
function maintainedAdvice(locale) {
  const families = [
    { id: 'transition', cards: names('BodySlam PommelStrike TwinStrike Headbutt PerfectedStrike Anger WildStrike HeavyBlade Clothesline IronWave ThunderClap Cleave SwordBoomerang Uppercut BloodForBlood InfernalBlade Hemokinesis SeverSoul Whirlwind RecklessCharge Rampage Carnage SearingBlow Combust Pummel Dropkick FiendFire Immolate Bludgeon'),
      text: strategyText(locale, 'familyTransition') },
    { id: 'scaling', cards: names('BodySlam DualWield Entrench Rupture Rampage SearingBlow Inflame SpotWeakness Combust Juggernaut DoubleTap Barricade DemonForm LimitBreak'),
      text: strategyText(locale, 'familyScaling') },
    { id: 'strength', cards: names('TwinStrike Anger Flex HeavyBlade SwordBoomerang Rupture Whirlwind Inflame SpotWeakness Pummel FiendFire DemonForm LimitBreak'),
      text: strategyText(locale, 'familyStrength') },
    { id: 'access', cards: names('PommelStrike Headbutt Warcry Havoc ShrugItOff BattleTrance BurningPact Evolve Dropkick DarkEmbrace Exhume Brutality Offering'),
      text: strategyText(locale, 'familyAccess') },
    { id: 'payment', cards: names('Clash BodySlam Anger Havoc BloodForBlood Sentinel InfernalBlade Intimidate Bloodletting Whirlwind RecklessCharge Rage SeeingRed Dropkick Berserk Offering Corruption'),
      text: strategyText(locale, 'familyPayment') },
    { id: 'protection', cards: names('TrueGrit Armaments ShrugItOff IronWave Sentinel Entrench GhostlyArmor FeelNoPain FlameBarrier Rage PowerThrough Disarm SecondWind Metallicize Barricade Impervious'),
      text: strategyText(locale, 'familyProtection') },
    { id: 'debuff', cards: names('Clothesline ThunderClap Uppercut Intimidate Disarm Shockwave'),
      text: strategyText(locale, 'familyDebuff') },
    { id: 'matchup', cards: names('Clash WildStrike ThunderClap Cleave Whirlwind RecklessCharge FireBreathing FlameBarrier PowerThrough Disarm Evolve Immolate'),
      text: strategyText(locale, 'familyMatchup') },
    { id: 'exhaust', cards: names('TrueGrit Havoc Sentinel SeverSoul FeelNoPain BurningPact SecondWind DarkEmbrace Exhume Impervious FiendFire Corruption'),
      text: strategyText(locale, 'familyExhaust') },
    { id: 'hp-resource', cards: names('Reaper Feed'),
      text: strategyText(locale, 'familyHpResource') },
  ];
  const sitePolicies = {
    card_reward: policy('draft', ['jorbs', 'panacea', 'cx'],
      strategyText(locale, 'siteCardRewardPreference'),
      strategyText(locale, 'siteCardRewardChange')),
    shop: policy('shop', ['panacea', 'cx'],
      strategyText(locale, 'siteShopPreference'),
      strategyText(locale, 'siteShopChange')),
    campfire: policy('campfire', ['panacea', 'cx'],
      strategyText(locale, 'siteCampfirePreference'),
      strategyText(locale, 'siteCampfireChange')),
    map: policy('route', ['panacea', 'jorbs'],
      strategyText(locale, 'siteMapPreference'),
      strategyText(locale, 'siteMapChange')),
    event: policy('event', ['panacea', 'equation'],
      strategyText(locale, 'siteEventPreference'),
      strategyText(locale, 'siteEventChange')),
    boss_relic: policy('boss-relic', ['panacea', 'cx', 'jorbs'],
      strategyText(locale, 'siteBossRelicPreference'),
      strategyText(locale, 'siteBossRelicChange')),
    treasure: policy('treasure', ['panacea'],
      strategyText(locale, 'siteTreasurePreference'),
      strategyText(locale, 'siteTreasureChange')),
    reward: policy('reward', ['panacea'],
      strategyText(locale, 'siteRewardPreference'),
      strategyText(locale, 'siteRewardChange')),
    run_transition: policy('act-entry', ['cx', 'panacea'],
      strategyText(locale, 'siteRunTransitionPreference'),
      strategyText(locale, 'siteRunTransitionChange')),
  };

  // Per-card advice retains the expert's default preference and its reversal
  // conditions. It supplies opinions, never a new runtime implementation.
  const cardPolicies = {
    SwordBoomerang: policy('sword-boomerang', ['je'],
      strategyText(locale, 'cardSwordBoomerangPreference'),
      strategyText(locale, 'cardSwordBoomerangChange')),
    Combust: policy('combust', ['cx', 'equation'],
      strategyText(locale, 'cardCombustPreference'),
      strategyText(locale, 'cardCombustChange')),
    Headbutt: policy('headbutt', ['cx', 'equation'],
      strategyText(locale, 'cardHeadbuttPreference'),
      strategyText(locale, 'cardHeadbuttChange')),
    PerfectedStrike: policy('perfected-strike', ['cx', 'equation'],
      strategyText(locale, 'cardPerfectedStrikePreference'),
      strategyText(locale, 'cardPerfectedStrikeChange')),
    SeverSoul: policy('sever-soul', ['cx', 'equation'],
      strategyText(locale, 'cardSeverSoulPreference'),
      strategyText(locale, 'cardSeverSoulChange')),
    Bloodletting: policy('bloodletting', ['cx'],
      strategyText(locale, 'cardBloodlettingPreference'),
      strategyText(locale, 'cardBloodlettingChange')),
    Flex: policy('flex', ['cx'],
      strategyText(locale, 'cardFlexPreference'),
      strategyText(locale, 'cardFlexChange')),
    Clash: policy('clash', ['cx'],
      strategyText(locale, 'cardClashPreference'),
      strategyText(locale, 'cardClashChange')),
    DemonForm: policy('demon-form', ['cx', 'equation'],
      strategyText(locale, 'cardDemonFormPreference'),
      strategyText(locale, 'cardDemonFormChange')),
    LimitBreak: policy('limit-break', ['cx'],
      strategyText(locale, 'cardLimitBreakPreference'),
      strategyText(locale, 'cardLimitBreakChange')),
    DarkEmbrace: policy('dark-embrace', ['je', 'cx'],
      strategyText(locale, 'cardDarkEmbracePreference'),
      strategyText(locale, 'cardDarkEmbraceChange')),
    Corruption: policy('corruption', ['cx', 'panacea'],
      strategyText(locale, 'cardCorruptionPreference'),
      strategyText(locale, 'cardCorruptionChange')),
    Sentinel: policy('sentinel', ['cx'],
      strategyText(locale, 'cardSentinelPreference'),
      strategyText(locale, 'cardSentinelChange')),
    BattleTrance: policy('battle-trance', ['cx', 'equation'],
      strategyText(locale, 'cardBattleTrancePreference'),
      strategyText(locale, 'cardBattleTranceChange')),
    Metallicize: policy('metallicize', ['je'],
      strategyText(locale, 'cardMetallicizePreference'),
      strategyText(locale, 'cardMetallicizeChange')),
    Clothesline: policy('clothesline', ['je'],
      strategyText(locale, 'cardClotheslinePreference'),
      strategyText(locale, 'cardClotheslineChange')),
    DualWield: policy('dual-wield', ['je', 'cx'],
      strategyText(locale, 'cardDualWieldPreference'),
      strategyText(locale, 'cardDualWieldChange')),
    Evolve: policy('evolve', ['cx'],
      strategyText(locale, 'cardEvolvePreference'),
      strategyText(locale, 'cardEvolveChange')),
    Armaments: policy('armaments', ['cx'],
      strategyText(locale, 'cardArmamentsPreference'),
      strategyText(locale, 'cardArmamentsChange')),
    TrueGrit: policy('true-grit', ['cx', 'equation'],
      strategyText(locale, 'cardTrueGritPreference'),
      strategyText(locale, 'cardTrueGritChange')),
    FeelNoPain: policy('feel-no-pain', ['cx', 'panacea'],
      strategyText(locale, 'cardFeelNoPainPreference'),
      strategyText(locale, 'cardFeelNoPainChange')),
    Barricade: policy('barricade', [],
      strategyText(locale, 'cardBarricadePreference'),
      strategyText(locale, 'cardBarricadeChange')),
    Entrench: policy('entrench', [],
      strategyText(locale, 'cardEntrenchPreference'),
      strategyText(locale, 'cardEntrenchChange')),
    BodySlam: policy('body-slam', [],
      strategyText(locale, 'cardBodySlamPreference'),
      strategyText(locale, 'cardBodySlamChange')),
  };
  return { families, sitePolicies, cardPolicies };
}
const maintainedViews = Object.fromEntries(['en', 'zh-CN'].map(locale => [locale, maintainedAdvice(locale)]));

// Inventory-bound strategy priorities. The deck owner validates instance facts;
// these reviewed preferences organize development without predicting combat HP.
export function apparitionPreparationOffers(observation, normalize = command => command) {
  const state = observation.position?.observation?.observation;
  if (state?.context?.kind !== 'event' || !Number.isSafeInteger(state.run?.current_hp)
      || !Number.isSafeInteger(state.run?.max_hp)) return [];
  return (observation.position.actions ?? []).flatMap(row => {
    const action = row.candidate?.action, semantics = action?.public_semantics;
    const grants = semantics?.structured_semantics?.effects?.filter(effect => effect.effect === 'obtain_card'
      && effect.pool?.kind === 'specific' && effect.pool.card === 'Apparition'
      && Number.isSafeInteger(effect.count) && effect.count > 0) ?? [];
    const cards = observation.decision_evidence?.candidates?.find(candidate => candidate.semantic_action === row.semantic_action)?.cards;
    const current = cards?.length === 1 ? cards[0].current : undefined;
    if (action?.kind !== 'choose_event_option' || !grants.length
        || current?.card !== 'Apparition' || grants.some(grant => grant.pool.upgrades !== current.upgrades)
        || !Number.isSafeInteger(current.cost) || current.cost < 0
        || !Number.isSafeInteger(current.intangible_gain) || current.intangible_gain <= 0
        || !Number.isSafeInteger(current.intangible_damage_cap) || current.intangible_damage_cap <= 0
        || !Number.isSafeInteger(semantics.max_hp_delta) || semantics.max_hp_delta >= 0
        || !Number.isSafeInteger(semantics.hp_delta)) return [];
    return [{ command: normalize(row.semantic_action, observation), current, upgraded: cards[0].upgraded,
      count: grants.reduce((sum, grant) => sum + grant.count, 0),
      hpBefore: state.run.current_hp, hpAfter: state.run.current_hp + semantics.hp_delta,
      maxHpBefore: state.run.max_hp, maxHpAfter: state.run.max_hp + semantics.max_hp_delta }];
  });
}

export function preparationBossRelicOffers(observation, normalize = command => command) {
  const state = observation.position?.observation?.observation;
  if (state?.context?.kind !== 'boss_relic') return [];
  return (observation.decision_evidence?.candidates ?? []).flatMap(candidate => {
    const action = observation.position.actions.find(row => row.semantic_action === candidate.semantic_action)?.candidate?.action;
    if (action?.kind !== 'take_boss_relic' || candidate.definition?.relic !== action.relic) return [];
    const rule = candidate.definition.starter_card_replacement;
    const targets = Array.isArray(rule?.target_cards) ? state.cards.filter(card => card.count > 0 && rule.target_cards.includes(card.card)) : undefined;
    return [{ command: normalize(candidate.semantic_action, observation), definition: candidate.definition,
      ...(targets ? { starterCardsReplaced: targets } : {}) }];
  });
}

function constructionPriorities(observation, normalize, locale = 'en') {
  const profile = preparationDeckComparison(observation, normalize, locale);
  if (!profile) return;
  const state = observation.position.observation.observation;
  const cards = observation.deck_evidence.cards;
  const positive = n => Number.isSafeInteger(n) && n > 0;
  const owns = id => cards.some(row => row.card.card === id);
  const labels = rows => rows.map(row => `${row.card.count}x ${row.card.card}${row.card.upgrades > 0 ? '+' : ''}`);
  const act = state.run?.pending_act_entry ?? state.run?.act;
  if (!Number.isSafeInteger(act) || act < 1 || act > 4) return;
  const priorities = [];
  const add = (id, sources, ownedFacts, preference, supportFamilies, comparisonBaseline) =>
    priorities.push({ id, sources, ownedFacts, preference, supportFamilies,
      ...(comparisonBaseline ? { comparisonBaseline } : {}) });
  add('development-window', ['panacea', 'cx'], [], act === 1
    ? strategyText(locale, 'developmentActOne')
    : strategyText(locale, 'developmentLaterActs'),
  ['transition', 'access', 'payment', 'protection', 'scaling', 'hp-resource']);

  const baseEnergy = state.base_energy_per_turn;
  const cardOffers = (observation.position.actions ?? []).flatMap(row => {
    const acquisition = preparationCardAcquisition(observation, row);
    return acquisition?.mechanics ? [{ semantic_action: row.semantic_action, ...acquisition }] : [];
  });
  const relicOffers = preparationBossRelicOffers(observation, normalize);
  const energyRelics = relicOffers.filter(candidate => positive(candidate.definition.energy_per_turn_delta));
  const costly = cards.filter(row => ['Attack', 'Skill', 'Power'].includes(row.mechanics.card_type)
    && row.persistent_cost >= 2 && row.persistent_cost <= baseEnergy);
  const opened = Number.isSafeInteger(baseEnergy) ? energyRelics.flatMap(candidate => {
    const after = baseEnergy + candidate.definition.energy_per_turn_delta;
    const pairs = costly.flatMap((left, i) => costly.slice(i).flatMap(right => {
      const total = left.persistent_cost + right.persistent_cost;
      if ((left === right && left.card.count < 2) || total <= baseEnergy || total > after) return [];
      return [strategyText(locale, 'payablePair', { left: left.mechanics.name ?? left.card.card, leftCost: left.persistent_cost, right: right.mechanics.name ?? right.card.card, rightCost: right.persistent_cost })];
    }));
    return pairs.length ? [strategyText(locale, 'openedPaymentPairs', { relic: candidate.definition.relic, before: baseEnergy, after: after, pairs: pairs.slice(0, 3).join('; ') })] : [];
  }) : [];
  if (opened.length) add('recurring-payment-window', ['panacea', 'cx'],
    [...labels(costly), ...opened,
      strategyText(locale, 'currentGoldRemoval', { gold: state.run?.gold, removal: (state.relics ?? []).some(row => row.relic === 'PeacePipe') ? strategyText(locale, 'peacePipeCampfire') : strategyText(locale, 'none') })],
    strategyText(locale, 'recurringPaymentPlan'),
    ['payment', 'access', 'protection']);

  const capped = relicOffers.filter(row => positive(row.definition.card_play_limit_per_turn));
  if (capped.length) add('card-play-cap-window', [], [
    ...capped.map(row => strategyText(locale, 'offeredPlayCap', { command: row.command, energy: row.definition.energy_per_turn_delta ?? 0, limit: row.definition.card_play_limit_per_turn })),
    strategyText(locale, 'ownedZeroCost', { cards: labels(cards.filter(row => row.persistent_cost === 0)).join(', ') || strategyText(locale, 'none') }),
    strategyText(locale, 'ownedOnPlayDraw', { cards: profile.onPlayDraw.map(row => row.card).join(', ') || strategyText(locale, 'none') }),
    strategyText(locale, 'ownedDeployedDraw', { cards: [...profile.laterTurnDraw, ...profile.exhaustDraw].map(row => row.card).join(', ') || strategyText(locale, 'none') }),
  ],
  strategyText(locale, 'permanentPlayCapPlan'),
  ['payment', 'access', 'protection']);

  const replacements = relicOffers.filter(row => row.definition.starter_card_replacement?.replacements_per_card === 1
    && row.starterCardsReplaced?.some(card => card.count > 0));
  if (replacements.length) add('starter-replacement-window', [], replacements.map(row => {
    const targets = row.starterCardsReplaced;
    return strategyText(locale, 'starterReplacement', { command: row.command, replaced: targets.reduce((n, card) => n + card.count, 0), deckSize: profile.size })
      + targets.map(card => `${card.count}x ${card.card}${card.upgrades > 0 ? '+' : ''}`).join(', ');
  }),
  strategyText(locale, 'starterReplacementPlan'),
  ['access', 'payment', 'protection']);

  const enduringStrength = cards.filter(row => enduringStrengthGain(row.mechanics));
  const strengthOwned = cards.some(row => positive(row.mechanics.strength_gain)
    && !positive(row.mechanics.temporary_strength_gain))
    || (owns('Rupture') && cards.some(row => selfLoss.has(row.card.card)))
    || (state.relics ?? []).some(row => strengthRelics.has(row.relic));
  const converters = cards.filter(row => row.mechanics.damage?.delivery?.kind === 'attack'
    && (row.mechanics.damage.delivery.hits?.kind === 'per_effective_x'
      || row.mechanics.damage.delivery.hits?.count > 1));
  const offeredGrowth = act >= 2 && !strengthOwned && converters.length
    ? cardOffers.flatMap(candidate => {
      const definition = candidate.mechanics;
      if (!positive(definition.strength_gain)
          || definition.strength_gain_delivery !== 'each_player_turn_start_post_draw'
          || !Number.isSafeInteger(definition.cost) || definition.cost < 0 || definition.cost > baseEnergy) return [];
      const remaining = baseEnergy - definition.cost;
      const protection = cards.filter(row => row.persistent_cost >= 0 && row.persistent_cost <= remaining
        && (row.persistent_block > 0 || positive(row.mechanics.weak) || positive(row.mechanics.enemy_strength_down)));
      return [strategyText(locale, 'firstRecurringStrength', { command: normalize(candidate.semantic_action, observation) })
        + strategyText(locale, 'strengthDeployment', { cost: definition.cost, energy: baseEnergy, remaining: remaining, strength: definition.strength_gain }),
        strategyText(locale, 'ownedMultiHitConverters', { cards: labels(converters).join(', ') }),
        strategyText(locale, 'protectionAlongsideDeployment', { cards: labels(protection).join(', ') || strategyText(locale, 'noneListed') })];
    }) : [];
  if (offeredGrowth.length) add('first-strength-growth', ['cx', 'panacea'], offeredGrowth,
    strategyText(locale, 'firstStrengthGrowthPlan'),
    ['scaling', 'strength', 'access', 'payment', 'protection']);
  if (enduringStrength.length) add('deliver-owned-growth', ['cx', 'equation'],
    enduringStrength.map(row => strategyText(locale, 'ownedStrengthRequest', { card: labels([row])[0], cost: row.persistent_cost, strength: row.mechanics.strength_gain, timing: row.mechanics.strength_gain_delivery })),
    strategyText(locale, 'deliverOwnedGrowthPlan'),
    ['access', 'payment', 'protection', 'strength']);

  const retention = cards.filter(row => row.card.card === 'Barricade');
  const multiplication = cards.filter(row => row.card.card === 'Entrench');
  const blockAttacks = cards.filter(row => row.mechanics.damage?.delivery?.kind === 'attack'
    && row.mechanics.damage.delivery.amount?.kind === 'player_block');
  if (retention.length) add('develop-owned-block-reserve', [], [
    ...retention.map(row => strategyText(locale, 'ownedBlockRetention', { card: labels([row])[0], cost: row.persistent_cost })),
    strategyText(locale, 'ownedFixedBlock', { cards: profile.fixedPlayedBlock.map(row => strategyText(locale, 'fixedBlockCard', { card: row.card, cost: row.cost, block: row.block })).join(', ') || strategyText(locale, 'otherProtectionTriggers') }),
    ...multiplication.map(row => strategyText(locale, 'ownedBlockMultiplier', { card: labels([row])[0], cost: row.persistent_cost, lifecycle: row.mechanics.exhausts ? strategyText(locale, 'exhaustAfterPlay') : strategyText(locale, 'ordinaryRedrawAfterPlay') })),
    ...blockAttacks.map(row => strategyText(locale, 'ownedBlockAttack', { card: labels([row])[0], cost: row.persistent_cost })),
    strategyText(locale, 'ownedOnPlayAccess', { cards: profile.onPlayDraw.map(row => row.card).join(', ') || strategyText(locale, 'noneListed') }),
  ],
  strategyText(locale, 'ownedBlockStagesPlan'),
  ['protection', 'scaling', 'access', 'payment', 'exhaust']);

  const offeredAmplifier = !owns('LimitBreak') && enduringStrength.length && converters.length
    ? cardOffers.flatMap(candidate => {
      const definition = candidate.mechanics;
      if (candidate.card !== 'LimitBreak'
          || !Number.isSafeInteger(definition.cost) || definition.cost < 0 || definition.cost > baseEnergy) return [];
      const exhaust = definition.exhausts || owns('Corruption');
      return [strategyText(locale, 'firstStrengthAmplifier', { command: normalize(candidate.semantic_action, observation), cost: definition.cost })
        + (exhaust ? strategyText(locale, 'singleDoublingLifecycle')
          : strategyText(locale, 'repeatedDoublingLifecycle'))];
    }) : [];
  if (offeredAmplifier.length) add('first-strength-amplifier', [], [
    ...offeredAmplifier,
    ...enduringStrength.map(row => strategyText(locale, 'ownedStrengthRequest', { card: labels([row])[0], cost: row.persistent_cost, strength: row.mechanics.strength_gain, timing: row.mechanics.strength_gain_delivery })),
    ...converters.map(row => strategyText(locale, 'ownedCardEnergy', { card: labels([row])[0], cost: row.persistent_cost === -1 ? 'X' : row.persistent_cost })
      + (row.mechanics.damage.delivery.hits.kind === 'per_effective_x' ? strategyText(locale, 'strengthPerEffectiveX')
        : strategyText(locale, 'strengthApplicationsPerPlay', { hits: row.mechanics.damage.delivery.hits.count }))
      + (row.mechanics.exhausts ? strategyText(locale, 'exhaustSuffix') : '')),
    strategyText(locale, 'ownedOnPlayAccess', { cards: profile.onPlayDraw.map(row => row.card).join(', ') || strategyText(locale, 'noneListed') }),
  ],
  strategyText(locale, 'firstStrengthAmplifierPlan'),
  ['scaling', 'strength', 'access', 'payment', 'protection']);

  const fuel = cards.filter(row => row.mechanics.exhausts_other_cards || row.mechanics.exhausts || row.mechanics.ethereal);
  if (profile.exhaustDraw.length && fuel.length) add('start-owned-exhaust-access', ['je', 'cx'],
    [...profile.exhaustDraw.map(row => strategyText(locale, 'ownedExhaustDrawDeployment', { card: row.card, cost: row.cost, draw: row.requested })), ...labels(fuel)],
    strategyText(locale, 'ownedExhaustAccessPlan'),
    ['access', 'payment', 'protection', 'exhaust']);

  const extraDraw = profile.onPlayDraw.some(expandsHandAccess);
  const offeredAccess = act >= 2 && (enduringStrength.length || retention.length)
    ? profile.candidates.filter(row => row.kind === 'addition' && expandsHandAccess(row.onPlayDraw)
      && row.cost >= 0 && row.cost <= baseEnergy) : [];
  if (offeredAccess.length) add('independent-access-window', ['cx', 'equation'], [
    strategyText(locale, 'ownedFunctionsToReach', { cards: labels([...enduringStrength, ...retention]).join(', ') }),
    strategyText(locale, 'reviewedOnPlayAccess', { cards: profile.onPlayDraw.map(row => strategyText(locale, 'ownedDrawHandContribution', { card: row.card, cost: row.cost, draw: row.requested, handChange: row.handChange ?? strategyText(locale, 'selectionDependent') })).join('; ') || strategyText(locale, 'noneListed') }),
    renderOnPlayDrawCoverage(profile, locale),
    ...offeredAccess.map(row => strategyText(locale, 'offeredDrawHandContribution', { command: row.command, cost: row.cost, draw: row.onPlayDraw.requested, handChange: row.onPlayDraw.handChange ?? strategyText(locale, 'selectionDependent'), lifecycle: row.exhausts ? strategyText(locale, 'exhaustAfterPlay') : strategyText(locale, 'ordinaryRedrawLaterUse') })),
  ],
  (extraDraw
    ? strategyText(locale, 'additionalAccessPlan')
    : strategyText(locale, 'firstIndependentAccessPlan'))
    + strategyText(locale, 'accessOpportunityPlan'),
  ['access'], { commands: offeredAccess.map(row => row.command), comparisonIds: ['@access'] });
  const energyFuel = cards.filter(row => positive(row.mechanics.energy_gain_on_exhaust));
  const payment = [...profile.onPlayEnergy.map(row => row.card), ...labels(energyFuel),
    ...(owns('Corruption') ? labels(cards.filter(row => row.card.card === 'Corruption')) : [])];
  if (payment.length) add('access-for-owned-payment', ['cx', 'equation'],
    [...payment, strategyText(locale, 'reviewedOnPlayDraw', { cards: profile.onPlayDraw.map(row => row.card).join(', ') || strategyText(locale, 'none') }), renderOnPlayDrawCoverage(profile, locale)],
    (extraDraw
      ? strategyText(locale, 'additionalAccessPaymentPlan')
      : strategyText(locale, 'accessBeforeMoreEnergyPlan'))
      + strategyText(locale, 'ownedPaymentNeedsCards')
      + (energyFuel.length ? strategyText(locale, 'exhaustEnergyNeedsSource') : '')
      + strategyText(locale, 'laterDrawSetupPlan'),
    ['access', 'protection']);

  const recentLoss = state.public_history?.recent_combat_hp_loss;
  const routes = state.context?.kind === 'map' && observation.decision_evidence?.kind === 'route_paths'
    ? observation.decision_evidence.candidates.filter(row => row.path?.coverage === 'complete_within_horizon'
      && row.path.first_elite?.paths_with_first_elite > 0
      && (row.path.first_elite.can_bail_to_rest_before || row.path.first_elite.can_bail_to_shop_before)
      && observation.position.actions.some(action => action.semantic_action === row.semantic_action)) : [];
  if (positive(recentLoss) && Number.isSafeInteger(state.run?.current_hp)
      && Number.isSafeInteger(state.run?.max_hp) && state.run.current_hp < state.run.max_hp && routes.length) {
    add('preserve-first-elite-preparation', ['panacea', 'jorbs'],
      [strategyText(locale, 'recentCombatLoss', { hp: state.run.current_hp, maxHp: state.run.max_hp, loss: recentLoss }),
        strategyText(locale, 'reviewedOnPlayDraw', { cards: profile.onPlayDraw.map(row => row.card).join(', ') || strategyText(locale, 'none') }),
        ...routes.map(row => strategyText(locale, 'retainedElitePreparation', { command: normalize(row.semantic_action, observation) })
          + [row.path.first_elite.can_bail_to_rest_before ? strategyText(locale, 'campfire') : '',
            row.path.first_elite.can_bail_to_shop_before ? strategyText(locale, 'shop') : ''].filter(Boolean).join(', '))],
      strategyText(locale, 'preserveElitePreparationPlan'),
      []);
  }

  if (owns('Corruption')) add('skill-lifecycle', ['cx', 'panacea'],
    [...labels(cards.filter(row => row.card.card === 'Corruption')), ...labels(energyFuel)],
    strategyText(locale, 'skillPaymentSequence')
      + (energyFuel.length ? strategyText(locale, 'exhaustEnergyPaysFromSource') : '')
      + strategyText(locale, 'supportSkillSetup'),
    ['access', 'protection', 'exhaust', 'strength']);

  if (profile.statusGeneration.length) add('generated-status-support', ['cx'],
    profile.statusGeneration.map(row => `${row.card}: ${row.injections.map(effect => strategyText(locale, 'statusInjection', { count: effect.count, card: effect.card, destination: effect.destination })).join(', ')}`),
    strategyText(locale, 'generatedStatusSupportPlan'),
    ['access', 'exhaust', 'protection', 'matchup']);

  return { basis: 'current_public_inventory_and_maintainer_strategy', publicSnapshotId: observation.position.public_snapshot_id,
    preparationAct: act, priorities };
}

export function preparationExpertGuidance(observation, normalize = command => command, locale = 'en') {
  const { sitePolicies } = maintainedViews[resolveLocale(locale)];
  const state = observation.position?.observation?.observation;
  if (!state || !Array.isArray(state.cards)) return undefined;
  const owned = state.cards.filter(row => row.count > 0);
  const format = rows => rows.map(row => `${row.count}x ${row.card}${row.upgrades > 0 ? '+' : ''}`).join(', ') || strategyText(locale, 'none');
  const owns = id => owned.some(row => row.card === id);
  const occupied = (state.potions ?? []).filter(slot => slot?.potion != null);
  const strengthCards = owned.filter(row => strength.has(row.card) || (row.card === 'Rupture' && owned.some(row => selfLoss.has(row.card))));
  const strengthItems = (state.relics ?? []).filter(row => strengthRelics.has(row.relic)).map(row => row.relic);
  const inventory = [
    strategyText(locale, 'inventoryAttacks', { starters: format(owned.filter(row => row.card === 'Strike' || row.card === 'Bash')), damage: format(owned.filter(row => damage.has(row.card))) }),
    strategyText(locale, 'inventoryStrength', { cards: format(strengthCards), relics: strengthItems.join(', ') || strategyText(locale, 'none') }),
    strategyText(locale, 'inventoryAccess', { cards: format(owned.filter(row => access.has(row.card))), triggered: format(owned.filter(row => triggeredAccess.has(row.card))) }),
  ];
  if ((state.relics ?? []).some(row => row.relic === 'ToyOrnithopter')) {
    inventory.push(strategyText(locale, 'ornithopterRecovery', { occupiedSlots: occupied.length }));
  }
  if ((state.relics ?? []).some(row => row.relic === 'Ectoplasm')) {
    inventory.push(strategyText(locale, 'ectoplasmBudget', { gold: state.run?.gold ?? 'Unknown' }));
  }
  const selected = [];
  const site = state.context?.kind;
  const removalPrice = state.context?.inventory?.purge?.price;
  if (site === 'shop' && Number.isSafeInteger(removalPrice) && removalPrice >= 0) {
    inventory.push(strategyText(locale, 'offeredRemovalPrice', { price: removalPrice }));
  }
  if (sitePolicies[site]) selected.push(sitePolicies[site]);
  const apparitionOffer = apparitionPreparationOffers(observation, normalize);
  if (apparitionOffer.length) {
    inventory.push(...apparitionOffer.map(row => strategyText(locale, 'apparitionAcquisition', { command: row.command, count: row.count })
      + strategyText(locale, 'hpAndMaxHpChange', { hpBefore: row.hpBefore, hpAfter: row.hpAfter, maxHpBefore: row.maxHpBefore, maxHpAfter: row.maxHpAfter })));
    const profile = preparationDeckComparison(observation, undefined, locale);
    const intangible = profile ? observation.deck_evidence.cards.filter(row => row.mechanics.intangible_gain > 0) : [];
    const act = state.run?.act;
    selected.push(policy('apparition-protection-window', [],
      strategyText(locale, 'apparitionPreference', { preference: act === 2 && profile && !intangible.length
        ? strategyText(locale, 'firstIntangibleActTwo')
        : strategyText(locale, 'compareIntangibleCoverage') })
        + strategyText(locale, 'apparitionLongTermPlan'),
      strategyText(locale, 'apparitionConstraints')));
  }
  if (site === 'run_choice') {
    const reason = state.context.reason;
    selected.push(reason === 'Duplicate'
      ? policy('copy', ['cx', 'jorbs'], strategyText(locale, 'copyPreference'), strategyText(locale, 'copyConditions'))
      : policy('cleanup', ['cx', 'equation'], strategyText(locale, 'cleanupPreference'), strategyText(locale, 'cleanupConditions')));
  }
  const candidates = preparationCardExpertGuidance(observation, normalize, locale);
  if (!selected.length && !candidates.length) return undefined;
  const construction = constructionPriorities(observation, normalize, locale);
  for (const candidate of candidates) candidate.priorityIds = (construction?.priorities ?? [])
    .filter(row => row.supportFamilies.some(id => candidate.familyIds?.includes(id))).map(row => row.id);
  return { schema: 'sts-preparation-expert-guidance-v1', origin: 'maintainer_synthesis_of_tracked_expert_notes',
    inventory, policies: selected, candidates, ...(construction ? { construction } : {}),
    families: preparationCardFamilyGuidance(candidates, locale),
    sources: Object.fromEntries([...new Set([...selected, ...candidates, ...(construction?.priorities ?? [])].flatMap(row => row.sources))].map(id => [id, STRATEGY_SOURCES[id]])) };
}

// The small card-choice input uses the same maintained advice without building
// the full preparation plan or asking the player to discover handbook entries.
export function preparationCardExpertGuidance(observation, normalize = command => command, locale = 'en') {
  const { cardPolicies, families } = maintainedViews[resolveLocale(locale)];
  const state = observation.position.observation.observation;
  return (observation.position.actions ?? []).flatMap(row => {
    const acquisition = preparationCardAcquisition(observation, row);
    const card = acquisition?.card, role = catalogue.cards[card];
    if (!acquisition || (!cardPolicies[card] && !role)) return [];
    const advice = cardPolicies[card] ?? { id: `role-${card}`, sources: ['cx'] };
    const combust = card === 'Combust' ? repeatedCombustAdvice(observation, acquisition, locale) : undefined;
    let conversion;
    if (card === 'BodySlam' && acquisition.mechanics && observation.deck_evidence) {
      const profile = preparationDeckComparison(observation, normalize, locale);
      const offer = profile?.candidates.find(candidate => candidate.command === normalize(row.semantic_action, observation));
      const checked = offer && paidBlockConversionDefault(state, observation.deck_evidence.cards, profile, offer, locale);
      if (checked) conversion = { preference: checked.preference, change: checked.facts.join(' ') };
    }
    return [{ command: normalize(row.semantic_action, observation), card, ...advice,
      ...(combust ?? {}),
      ...(conversion ?? {}),
      ...(role ? { sourceRole: role, familyIds: families.filter(family => family.cards.has(card)).map(family => family.id) } : {}),
      ...(card === 'Evolve' && state.run?.act === 4 ? {
        preference: strategyText(locale, 'evolveHeartAccess', { preference: advice.preference }),
      } : {}) }];
  });
}

function repeatedCombustAdvice(observation, acquisition, locale = 'en') {
  if (observation.deck_evidence?.public_snapshot_id !== observation.position?.public_snapshot_id) return;
  const effect = acquisition.mechanics?.end_of_turn_hp_loss_and_damage;
  const owned = observation.deck_evidence?.cards?.filter(row => row.card.card === 'Combust'
    && row.mechanics.card_type === 'Power' && row.persistent_cost >= 0
    && Number.isSafeInteger(row.card.count) && row.card.count > 0
    && row.mechanics.end_of_turn_hp_loss_and_damage?.hp_loss_per_application > 0
    && row.mechanics.end_of_turn_hp_loss_and_damage.damage_per_enemy_per_application > 0);
  if (!owned?.length || !effect || !(effect.hp_loss_per_application > 0)
      || !(effect.damage_per_enemy_per_application > 0)) return;
  const copies = owned.reduce((n, row) => n + row.card.count, 0);
  const hpLoss = owned.reduce((n, row) => n + row.card.count * row.mechanics.end_of_turn_hp_loss_and_damage.hp_loss_per_application, 0);
  const damage = owned.reduce((n, row) => n + row.card.count * row.mechanics.end_of_turn_hp_loss_and_damage.damage_per_enemy_per_application, 0);
  return {
    preference: strategyText(locale, 'repeatedCombustPreference'),
    change: strategyText(locale, 'repeatedCombustConditions', { beforeCopies: copies, afterCopies: copies + 1, beforeHpLoss: hpLoss, afterHpLoss: hpLoss + effect.hp_loss_per_application, beforeDamage: damage, afterDamage: damage + effect.damage_per_enemy_per_application }),
  };
}

export function preparationCardFamilyGuidance(candidates, locale = 'en') {
  const { families } = maintainedViews[resolveLocale(locale)];
  return families.filter(family => candidates.some(row => row.familyIds?.includes(family.id)))
    .map(({ id, text }) => ({ id, text }));
}

// A deliberately bounded drafting domain. Thresholds and dispositions are
// maintainer preferences; the checked inventory remains native evidence.
// Uncovered cards/conditions retain the legal set for offline policy review.
const payableTransitions = (cards, energy) => cards.filter(row => damage.has(row.card.card)
  && row.persistent_cost >= 0 && row.persistent_cost <= energy);

// A starting-inventory request, not a combat forecast. Strike counts change
// when cards are created or leave Hand/Draw/Discard during the actual fight.
function retrievalOutput(source, profile, locale = 'en') {
  const delivery = source.mechanics.damage?.delivery;
  if (delivery?.kind !== 'attack' || delivery.targets !== 'selected_enemy'
      || delivery.hits?.kind !== 'fixed' || !Number.isSafeInteger(delivery.hits.count)
      || delivery.hits.count <= 0 || !Number.isSafeInteger(source.persistent_damage)
      || delivery.play_condition) return;
  let perHit = source.persistent_damage;
  let basis = strategyText(locale, 'retrievalBaseDamage', { damage: perHit });
  if (delivery.amount?.kind === 'base_plus_strike_cards') {
    if (!profile.strikeTags || !Number.isSafeInteger(delivery.amount.per_card)) return;
    perHit += delivery.amount.per_card * profile.strikeTags.copies;
    basis += strategyText(locale, 'retrievalStrikeBonus', { perCard: delivery.amount.per_card, copies: profile.strikeTags.copies });
  } else if (delivery.amount?.kind !== 'base') return;
  return { total: perHit * delivery.hits.count,
    fact: strategyText(locale, 'retrievalOutputFact', { card: labelCard(source), basis: basis, hits: delivery.hits.count, cost: source.persistent_cost }) };
}

function laterOutputCoverage(state, cards, profile, locale = 'en') {
  const transitions = payableTransitions(cards, profile.baseEnergy);
  const focused = cards.flatMap(source => {
    if (source.persistent_cost < 0 || source.persistent_cost > profile.baseEnergy
        || source.mechanics.exhausts !== false) return [];
    const output = retrievalOutput(source, profile, locale);
    return output?.total >= 18 ? [{ source, ...output }] : [];
  });
  const count = transitions.reduce((n, row) => n + row.card.count, 0);
  return { supported: state.run.act >= 2 && state.run.act <= 3 && profile.baseEnergy >= 3
    && !(state.relics ?? []).some(row => ['SneckoEye', 'VelvetChoker'].includes(row.relic))
    && count >= 4 && focused.length > 0,
    facts: [strategyText(locale, 'ownedPayableAttackCount', { count: count }), ...focused.map(row => row.fact)] };
}

// Shared by free rewards and purchases. The output default concerns adding
// this paid conversion; shop Gold and survival resources still compete.
function paidBlockConversionDefault(state, cards, profile, offer, locale = 'en') {
  const coverage = laterOutputCoverage(state, cards, profile, locale);
  const block = profile.fixedPlayedBlock;
  if (!coverage.supported || offer.card !== 'BodySlam' || offer.upgrades !== 0
      || offer.type !== 'Attack' || offer.cost !== 1 || offer.exhausts !== false
      || offer.attackDelivery?.amount?.kind !== 'player_block'
      || !block.length || block.some(source => source.cost < 1 || source.block > 16)
      || cards.some(source => (source.mechanics.block > 0 && source.mechanics.base_block_delivery !== 'once_on_play')
        || source.mechanics.exhaust_block_power > 0 || source.mechanics.end_of_turn_block_each_turn
        || source.card.card === 'Rage')
      || profile.blockSupport.retention.length || profile.blockSupport.multiplication.length
      || profile.blockSupport.damageCaps.length || profile.skillExhaustPowers.length
      || (state.relics ?? []).some(row => ['Calipers', 'OrnamentalFan', 'Abacus', 'Anchor', 'HornCleat', 'CaptainsWheel'].includes(row.relic))) return;
  const example = [...block].filter(source => source.cost + offer.cost <= profile.baseEnergy)
    .sort((left, right) => right.block - left.block || left.cost - right.cost)[0];
  if (!example) return;
  return { policy: 'covered-output-paid-block-conversion', disposition: 'preserve', facts: [
    ...coverage.facts,
    strategyText(locale, 'ownedFixedBlockBeforeModifiers', { cards: block.map(source => strategyText(locale, 'ownedBlockAndCost', { card: source.card, block: source.block, cost: source.cost })).join('; ') }),
    strategyText(locale, 'sameHandBlockConversion', { card: example.card, cost: example.cost + offer.cost, block: example.block }),
    strategyText(locale, 'ordinaryBlockLifetime'),
  ], preference: strategyText(locale, 'paidBlockConversionPreference') };
}

function ownedDrawPaymentBundle(cards, profile, extraEnergy) {
  const payable = source => source.persistent_cost >= 0 && source.persistent_cost <= profile.baseEnergy;
  const draws = cards.filter(source => payable(source) && source.mechanics.draw_cards >= 2
    && !source.mechanics.exhausts_other_cards && !source.mechanics.selects_from_hand
    && !(source.mechanics.hand_topdeck_after_draw > 0));
  const attacks = cards.filter(source => payable(source) && source.persistent_cost > 0
    && source.mechanics.card_type === 'Attack' && source.mechanics.exhausts === false
    && retrievalOutput(source, profile)?.total >= 9);
  for (const draw of draws) for (let i = 0; i < attacks.length; i++) for (let j = i + 1; j < attacks.length; j++) {
    if (draw === attacks[i] || draw === attacks[j]) continue;
    const sources = [draw, attacks[i], attacks[j]];
    const cost = sources.reduce((sum, source) => sum + source.persistent_cost, 0);
    if (cost > profile.baseEnergy && cost <= profile.baseEnergy + extraEnergy) return { sources, cost };
  }
}

// Use the native detached upgrade of the owned instance. These are possible
// changes for a card that actually shares Hand with the combat upgrader.
function handUpgradeTargets(cards, energy) {
  const basics = [], access = [];
  for (const source of cards) {
    const next = source.next_upgrade, definition = next?.definition;
    if (source.card.upgrades !== 0 || definition?.card !== source.card.card
        || definition.upgrades !== source.card.upgrades + 1
        || !Number.isSafeInteger(next.persistent_cost) || next.persistent_cost < 0
        || next.persistent_cost > energy || next.persistent_cost > source.persistent_cost) continue;
    if (['Strike', 'Defend'].includes(source.card.card) && source.persistent_cost === 1
        && next.persistent_cost === 1
        && (source.card.card === 'Strike' ? next.persistent_damage - source.persistent_damage >= 3
          : next.persistent_block - source.persistent_block >= 3)) basics.push(source);
    const draws = definition.reviewed_effects?.unconditional_draw_request;
    const moreDraw = Number.isSafeInteger(draws) && draws > source.mechanics.draw_cards;
    const lessCost = source.persistent_cost > next.persistent_cost;
    if (moreDraw || lessCost) access.push({ source,
      fact: `${labelCard(source)} own upgrade: ${source.persistent_cost}->${next.persistent_cost} Energy`
        + (moreDraw ? `; on-play draw request ${source.mechanics.draw_cards}->${draws}` : '') + '.' });
  }
  return { basics, access };
}

// Pick a removal target before comparing the shop's spending alternatives.
// The rule concerns the target only; buying removal still competes with the
// other purchases and keeping Gold in the current classification.
export function preparationShopRemovalPolicy(observation, locale = 'en') {
  const state = observation.position?.observation?.observation;
  const purge = state?.context?.inventory?.purge;
  if (state?.context?.kind !== 'shop' || !observation.deck_evidence
      || purge?.purchase_status?.status !== 'purchasable' || !Number.isSafeInteger(purge.price)
      || purge.price < 0 || !Number.isSafeInteger(state.run?.gold) || purge.price > state.run.gold
      || !Number.isSafeInteger(state.run?.act) || state.run.act < 2) return;
  const profile = preparationDeckComparison(observation, undefined, locale);
  if (!profile) return;
  const cards = observation.deck_evidence.cards;
  const strength = profile.strengthSources.filter(row => row.cost >= 0 && row.cost <= profile.baseEnergy);
  const transitions = payableTransitions(cards, profile.baseEnergy);
  const count = transitions.reduce((n, row) => n + row.card.count, 0);
  const strikes = cards.filter(row => row.card.card === 'PerfectedStrike');
  const covered = laterOutputCoverage(state, cards, profile, locale);
  if ((!strength.length && !covered.supported) || count < 3
      || (strikes.length && (!profile.strikeTags
        || transitions.filter(row => row.card.card !== 'PerfectedStrike').reduce((n, row) => n + row.card.count, 0) < 3))
      || (state.relics ?? []).some(row => ['StrikeDummy', 'BottledFlame', 'BottledLightning', 'BottledTornado'].includes(row.relic))
      || profile.candidates.some(row => row.kind === 'removal' && row.type === 'Curse')) return;
  const upgrades = cards.some(row => row.card.card === 'Strike' && row.card.upgrades === 0) ? 0 : 1;
  const originals = cards.filter(row => row.card.card === 'Strike' && row.card.upgrades === upgrades);
  if (originals.length !== 1) return;
  const original = originals[0], m = original.mechanics, attributes = original.card;
  if (attributes.misc_value !== 0 || attributes.cost_modifier !== 0
      || attributes.base_damage_override !== null || attributes.base_block_override !== null
      || original.persistent_cost !== 1 || original.persistent_damage !== 6 + 3 * upgrades || original.persistent_block !== 0
      || m.card_type !== 'Attack' || m.exhausts !== false || m.ethereal !== false || m.draw_cards !== 0) return;
  const commands = (observation.position.actions ?? []).filter(row => {
    const action = row.candidate?.action;
    return action?.kind === 'remove_card' && action.card === 'Strike' && action.upgrades === upgrades
      && action.price === purge.price;
  }).map(row => row.semantic_action);
  if (!commands.length) return;
  return { id: strength.length ? 'established-strength-basic-removal' : 'covered-output-basic-removal', origin: 'maintainer_strategy_default',
    sources: ['cx', 'equation'], commands,
    conditions: [strategyText(locale, 'removalPreparationAct', { act: state.run.act }),
      strategyText(locale, 'removalOwnedStrength', { cards: strength.map(row => row.card).join(', ') || strategyText(locale, 'none') }),
      ...(!strength.length ? covered.facts : []),
      strategyText(locale, 'removalOwnedAttacks', { count: count, cards: transitions.map(labelCard).join(', ') }),
      strategyText(locale, 'ordinaryStrikeRemoval', { upgradeSuffix: upgrades ? '+' : '', copies: original.card.count, damage: original.persistent_damage, price: purge.price }),
      ...(strikes.length ? [strategyText(locale, 'removalStrikePool', { before: profile.strikeTags.copies, after: profile.strikeTags.copies - 1, otherAttacks: transitions.filter(row => row.card.card !== 'PerfectedStrike').reduce((n, row) => n + row.card.count, 0) })] : [])],
    preference: strategyText(locale, 'ordinaryStrikeRemovalPreference', { upgradeSuffix: upgrades ? '+' : '' }) };
}

function ordinaryRewardPolicy(state, cards, offers, profile) {
  const act = state.run?.pending_act_entry ?? state.run?.act;
  const copies = rows => rows.reduce((n, row) => n + row.card.count, 0);
  const owned = id => cards.filter(row => row.card.card === id);
  const payable = row => row.persistent_cost >= 0 && row.persistent_cost <= profile.baseEnergy;
  const transitions = payableTransitions(cards, profile.baseEnergy);
  const focused = transitions.filter(row => row.mechanics.damage?.delivery?.targets === 'selected_enemy');
  const multiHit = transitions.filter(row => row.mechanics.damage?.delivery?.hits?.kind === 'fixed'
    && row.mechanics.damage.delivery.hits.count > 1);
  const established = act >= 2 && profile.strengthSources.length > 0 && copies(transitions) >= 3;
  const clearsHand = cards.filter(row => ['SecondWind', 'SeverSoul'].includes(row.card.card)
    && row.mechanics.exhausts_other_cards && payable(row));
  const strengthAmplifiers = owned('LimitBreak');
  const setup = [...profile.strengthSources, ...profile.blockSupport.retention, ...profile.exhaustDraw]
    .filter(row => row.cost >= 0 && row.cost <= profile.baseEnergy);
  const smallSkillWindow = act >= 2 && act <= 3 && profile.baseEnergy >= 3 && setup.length > 0
    && !(state.relics ?? []).some(row => ['SneckoEye', 'VelvetChoker'].includes(row.relic));
  const firstReduction = offers.filter(row => row.card === 'Disarm' && row.cost >= 0 && row.cost <= profile.baseEnergy
    && row.enemyStrengthReduction?.amount > 0
    && row.enemyStrengthReduction.restoration === 'no_scheduled_restoration' && !profile.persistentReduction.length);
  const early = act === 1 && profile.baseEnergy >= 3
    && !(state.relics ?? []).some(row => ['SneckoEye', 'VelvetChoker'].includes(row.relic));
  const retrievalTargets = early ? cards.flatMap(source => {
    if (source.mechanics.exhausts !== false || source.persistent_cost < 0
        || source.persistent_cost + 1 > profile.baseEnergy) return [];
    const output = retrievalOutput(source, profile);
    return output?.total >= 18 ? [{ source, fact: output.fact }] : [];
  }) : [];
  const earlyRetrieval = offers.filter(row => early && row.card === 'Headbutt' && !owned('Headbutt').length
    && row.type === 'Attack' && row.cost === 1 && row.exhausts === false && row.baseDamagePerHit >= 9
    && retrievalTargets.length && profile.onPlayDraw.some(source => source.cost >= 0
      && source.cost + 1 <= profile.baseEnergy && source.topdeck === 0));
  const upgradeTargets = handUpgradeTargets(cards, profile.baseEnergy);
  const earlyHandUpgrade = offers.filter(row => early && row.card === 'Armaments' && !owned('Armaments').length
    && !owned('Apotheosis').length && !(state.relics ?? []).some(relic => relic.relic === 'WarpedTongs')
    && copies(upgradeTargets.basics) >= 6 && upgradeTargets.access.length
    && row.type === 'Skill' && row.cost === 1 && row.blockRequest >= 5 && row.exhausts === false
    && row.combatUpgradeScope === 'whole_hand' && row.combatUpgradePersistence === 'combat_only');
  const area = cards.filter(source => payable(source) && source.mechanics.damage?.delivery?.kind === 'attack'
    && source.mechanics.damage.delivery.targets === 'each_enemy'
    && source.mechanics.damage.delivery.amount?.kind === 'base'
    && source.mechanics.damage.delivery.hits?.kind === 'fixed'
    && !source.mechanics.damage.delivery.play_condition);
  const earlyArea = offers.filter(row => early && row.card === 'Cleave' && !owned('Cleave').length && row.type === 'Attack'
    && row.cost === 1 && row.exhausts === false && row.baseDamagePerHit >= 8
    && row.attackDelivery?.targets === 'each_enemy' && row.attackDelivery.hits?.count === 1
    && !row.attackDelivery.play_condition
    && !area.some(source => source.persistent_damage * source.mechanics.damage.delivery.hits.count >= row.baseDamagePerHit));
  const strikeIncrement = cards.reduce((total, source) => total
    + (source.mechanics.damage?.delivery?.amount?.kind === 'base_plus_strike_cards'
      ? source.mechanics.damage.delivery.amount.per_card * source.card.count : 0), 0);
  const reusableExhaust = cards.filter(source => source.mechanics.exhausts_other_cards === true
    && source.mechanics.exhausts === false && payable(source));
  const weakSources = cards.filter(source => source.mechanics.weak > 0 && payable(source));
  const singleHitPool = profile.attacks.length > 0 && profile.attacks.every(source =>
    source.delivery?.kind === 'attack' && source.delivery.hits?.kind === 'fixed'
    && source.delivery.hits.count === 1
    && ['base', 'base_plus_strike_cards'].includes(source.delivery.amount?.kind));
  const coveredEarlyOutput = early && copies(transitions) >= 4 && retrievalTargets.length > 0;
  const outputCoverage = laterOutputCoverage(state, cards, profile);
  const coveredLaterOutput = outputCoverage.supported;
  const outputCoverageFacts = outputCoverage.facts;
  const attackRelics = (state.relics ?? []).filter(source => attackPlayRelics.has(source.relic));
  const earlyFunctionWindow = state.run.act === 1 && profile.baseEnergy >= 3
    && !(state.relics ?? []).some(row => ['SneckoEye', 'VelvetChoker'].includes(row.relic));
  const growthProtection = [...weakSources.map(labelCard),
    ...[...profile.persistentReduction, ...profile.blockSupport.damageCaps]
      .filter(source => source.cost >= 0 && source.cost <= profile.baseEnergy).map(source => source.card)];
  const basicCleanup = cards.filter(source => ['Strike', 'Defend', 'AscendersBane'].includes(source.card.card));
  const usefulPaidSkills = cards.filter(source => source.mechanics.card_type === 'Skill'
    && source.persistent_cost > 0 && payable(source)
    && (source.persistent_block > 0 || source.mechanics.draw_cards > 0
      || source.mechanics.enemy_strength_down > 0 || source.mechanics.weak > 0));
  const strengthSupport = cards.some(source => source.mechanics.strength_gain > 0
    || source.mechanics.temporary_strength_gain > 0 || source.card.card === 'Rupture')
    || (state.relics ?? []).some(source => strengthRelics.has(source.relic))
    || (state.potions ?? []).some(source => ['StrengthPotion', 'SteroidPotion'].includes(source.potion?.potion));
  const smallBlock = profile.fixedPlayedBlock.length > 0
    && profile.fixedPlayedBlock.every(source => source.cost >= 1 && source.block <= 8)
    && cards.every(source => !(source.mechanics.block > 0)
      || source.mechanics.base_block_delivery === 'once_on_play')
    && !cards.some(source => source.mechanics.exhaust_block_power > 0
      || source.mechanics.end_of_turn_block_each_turn || source.card.card === 'Rage')
    && !profile.blockSupport.retention.length && !profile.blockSupport.multiplication.length
    && !profile.blockSupport.damageCaps.length && !profile.skillExhaustPowers.length
    && !profile.onPlayEnergy.length;
  const areaRoutes = cards.filter(source => payable(source)
    && source.mechanics.damage?.delivery?.targets === 'each_enemy');
  const thinArea = areaRoutes.every(source => source.mechanics.damage.delivery.amount?.kind === 'base'
    && source.mechanics.damage.delivery.hits?.kind === 'fixed'
    && source.persistent_damage * source.mechanics.damage.delivery.hits.count < 8)
    && !cards.some(source => source.mechanics.end_of_turn_hp_loss_and_damage
      || source.card.card === 'FireBreathing');
  const assessments = offers.map(row => {
    const id = row.card;
    const result = (policy, disposition, facts, preference) => ({ command: row.command, card: id,
      policy, disposition, facts, preference });
    const review = (policy, facts) => result(policy, 'review', facts,
      'Supply a reviewed drafting condition for this offered card.');
    if (id === 'Inflame') {
      if (coveredLaterOutput && !strengthSupport && !owned(id).length && row.type === 'Power'
          && row.cost === 1 && row.enduringStrength?.delivery === 'on_play'
          && row.enduringStrength.amount >= 2) {
        return result('first-low-cost-enduring-strength', 'take', [
          ...outputCoverageFacts,
          `Owned Strength support: 0. Inflame costs ${row.cost} Energy and grants ${row.enduringStrength.amount} Strength on play for the rest of combat; later attacks use it after deployment.`,
          `Owned payable attacks: ${profile.attacks.filter(source => source.cost >= 0 && source.cost <= profile.baseEnergy).map(source => source.card).join(', ')}. Each subsequent hit gains the supplied Strength before combat modifiers.`,
          ...owned('FiendFire').filter(payable).map(source => `After prior Inflame deployment, ${labelCard(source)} gains ${row.enduringStrength.amount} requested damage per actually Exhausted Hand card. Playing both in the same Hand spends Inflame itself before counting FiendFire fuel.`),
        ], 'Take the first low-cost lasting Strength source for existing attacks. Pay deployment once; its benefit continues on subsequent attack turns and redraws.');
      }
      return review('enduring-strength-increment', [`Owned lasting Strength sources: ${profile.strengthSources.map(source => source.card).join(', ') || 'none'}; reviewed output coverage: ${coveredLaterOutput}.`]);
    }
    if (id === 'Anger') {
      if (coveredLaterOutput && profile.onPlayDrawCoverage.copies <= 2 && !profile.exhaustDraw.length
          && !profile.skillExhaustPowers.length && row.type === 'Attack' && row.cost === 0 && row.exhausts === false
          && row.copyToDiscardAfterDamage === 1 && row.attackDelivery?.amount?.kind === 'base'
          && row.attackDelivery.hits?.kind === 'fixed' && row.attackDelivery.hits.count === 1
          && row.baseDamagePerHit === (row.upgrades > 0 ? 8 : 6)) {
        return result('covered-output-copying-free-attack', attackRelics.length && !owned(id).length ? 'compare' : 'preserve', [
          ...outputCoverageFacts,
          `Anger supplies ${row.baseDamagePerHit} base damage for 0 Energy and one Attack play. Each play adds one combat-only copy to Discard alongside the original; later reshuffles can draw these additional attacks.`,
          `Owned on-play multi-draw copies: ${profile.onPlayDrawCoverage.copies}. The free payment still uses a draw and Hand slot; copied attacks compete with existing protection and access.`,
          ...(attackRelics.length ? [`Owned Attack-play relics: ${attackRelics.map(source => source.relic).join(', ')}. Their actual play thresholds and benefits come from the supplied native rules.`] : []),
        ], attackRelics.length && !owned(id).length
          ? 'Compare a first free Attack toward the owned play-count relic with preserving access or adding a lasting function. Include the copies entering later draws.'
          : 'Preserve access to existing output and protection over additional self-copying attacks in this limited-draw foundation.');
      }
      return review('copying-free-attack-increment', [`Owned Attack-play relics: ${attackRelics.map(source => source.relic).join(', ') || 'none'}.`]);
    }
    if (id === 'Bloodletting') {
      const hpInteractions = owned('Rupture').length || owned('Reaper').length
        || cards.some(source => source.mechanics.cost_change_on_hp_loss)
        || profile.blockSupport.damageCaps.length
        || (state.relics ?? []).some(source => ['TungstenRod', 'RunicCube', 'CentennialPuzzle', 'SelfFormingClay', 'RedSkull'].includes(source.relic));
      const existingPayment = profile.onPlayEnergy.filter(source => source.cost === 0 && source.hpLoss === 0 && source.amount >= 2);
      if (coveredLaterOutput && state.run.current_hp <= Math.floor(state.run.max_hp / 3)
          && profile.onPlayDrawCoverage.copies <= 2 && existingPayment.length && !hpInteractions
          && !profile.exhaustDraw.length && !profile.skillExhaustPowers.length
          && row.type === 'Skill' && row.cost === 0 && row.exhausts === false
          && row.selfHpLoss?.base_hp_loss === 3 && row.selfHpLoss.bypasses_block === true
          && [2, 3].includes(row.onPlayEnergy?.amount)) {
        return result('low-hp-covered-payment', 'preserve', [
          `Current HP ${state.run.current_hp}/${state.run.max_hp}; at most one third remains (maintainer survival threshold). Bloodletting pays 3 HP before granting ${row.onPlayEnergy.amount} Energy; ordinary redraw permits another paid use.`,
          `Owned zero-Energy, zero-HP payment: ${existingPayment.map(source => `${source.card} grants ${source.amount} Energy`).join('; ')}. These copies need their own Hand access.`,
          `Owned on-play multi-draw copies: ${profile.onPlayDrawCoverage.copies}. Extra Energy needs useful cards reached in the same turn.`,
        ], 'Preserve scarce HP and access over another HP-funded payment source with existing free payment and limited draw.');
      }
      return review('hp-funded-payment-increment', [`Current HP ${state.run.current_hp}/${state.run.max_hp}; owned free payment sources: ${existingPayment.length}; HP interactions: ${hpInteractions ? 'present' : 'absent'}.`]);
    }
    if (id === 'SeeingRed') {
      const bundle = row.onPlayEnergy?.amount > 0 ? ownedDrawPaymentBundle(cards, profile, row.onPlayEnergy.amount) : undefined;
      if (coveredLaterOutput && !owned(id).length && !profile.onPlayEnergy.length && !profile.skillExhaustPowers.length
          && row.type === 'Skill' && row.cost === 0 && row.exhausts === true && row.onPlayEnergy?.amount === 2 && bundle) {
        return result('first-free-payment-for-owned-draw-output', 'take', [
          `SeeingRed costs 0, gains 2 Energy on play, then Exhausts. Base Energy ${profile.baseEnergy}->${profile.baseEnergy + 2} for that turn after actual use.`,
          `Owned payment bundle: ${bundle.sources.map(source => `${labelCard(source)} (${source.persistent_cost} Energy)`).join(' + ')} = ${bundle.cost}, crossing the ${profile.baseEnergy}-Energy baseline. This is a payment example; reach the cards and use their actual effects in order.`,
          'The owned multi-draw can reach further useful plays. Each extra play needs its actual Hand card; unused Energy is lost at turn end. Owned on-play Energy sources: 0.',
        ], 'Take a first free Energy source for the checked owned draw and output payment. Its extra draw slot pays off through useful cards actually accessed that turn.');
      }
      return review('draw-output-payment-coverage', [`Owned on-play Energy sources: ${profile.onPlayEnergy.length}; current draw/output payment bundle: ${bundle ? bundle.cost : 'none'}.`]);
    }
    if (id === 'SeverSoul') {
      const burst = owned('FiendFire').filter(payable);
      const targeted = cards.filter(source => payable(source) && source.mechanics.exhausts_other_cards === true
        && source.mechanics.selects_from_hand === true && source.mechanics.exhausts === false);
      if (coveredLaterOutput && burst.length && targeted.length
          && !profile.exhaustDraw.length && !cards.some(source => source.mechanics.exhaust_block_power > 0
            || source.mechanics.energy_gain_on_exhaust > 0)
          && row.upgrades === 0 && row.type === 'Attack' && row.cost === 2 && row.exhausts === false
          && row.exhaustsOtherCards === true && row.baseDamagePerHit === 16
          && row.attackDelivery?.amount?.kind === 'base' && row.attackDelivery.targets === 'selected_enemy'
          && row.attackDelivery.hits?.kind === 'fixed' && row.attackDelivery.hits.count === 1) {
        return result('covered-output-and-cleanup', 'preserve', [...outputCoverageFacts,
          `Owned broad burst cleanup: ${burst.map(labelCard).join(', ')}; reusable selected-card cleanup: ${targeted.map(labelCard).join(', ')}.`,
          'SeverSoul adds a repeatable 16-base-damage hit and clears only remaining non-Attacks; useful Skills and Powers share that pool. FiendFire also clears Attacks but Exhausts itself. These are different clearing functions.',
        ], 'Preserve existing output and targeted cleanup over another two-Energy attack in this pool. Repeatable non-Attack clearing or actual Exhaust payoffs can justify a separate comparison.');
      }
      return review('non-attack-clearing-coverage', [`Owned broad burst cleanup: ${burst.map(labelCard).join(', ') || 'none'}; reusable selected-card cleanup: ${targeted.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'FiendFire') {
      if (earlyFunctionWindow && !owned(id).length && copies(basicCleanup) >= 6 && profile.onPlayDrawCoverage.copies > 0
          && row.type === 'Attack' && row.cost === 2 && row.exhausts === true && row.exhaustsOtherCards === true
          && row.baseDamagePerHit >= 7 && row.attackDelivery?.amount?.kind === 'base'
          && row.attackDelivery.targets === 'selected_enemy'
          && row.attackDelivery.hits?.kind === 'per_hand_card_at_resolution_before_exhaust') {
        return result('first-hand-exhaust-burst', 'take', [
          `Owned basic cleanup pool: ${copies(basicCleanup)} copies (${basicCleanup.map(labelCard).join(', ')}). Use the actual Hand to count targets.`,
          `With k other Hand cards at resolution, base damage is ${row.baseDamagePerHit} x k to one target; each hit uses resolved Strength and modifiers.`,
          'Play needed protection/setup first. Remaining weak cards become burst and leave reshuffles; sacrificed useful functions need recovery to return.',
          ...cards.filter(source => source.mechanics.combat_upgrade_scope === 'whole_hand'
            && payable(source) && source.persistent_cost + row.cost <= profile.baseEnergy).map(source =>
            `${labelCard(source)} can upgrade FiendFire in the same actual Hand: pay ${source.persistent_cost} + ${row.cost} Energy, then resolve the upgraded burst.`),
          ...(profile.strikeTags ? [`Owned Strike-tagged pool: ${profile.strikeTags.copies}. Exhausting counted cards also lowers later PerfectedStrike requests until those cards return to Hand, Draw or Discard.`] : []),
        ], 'Take a first broad Hand-clearing burst with owned draw and a substantial basic-card pool. Check the actual Hand before using it.');
      }
      return review('hand-exhaust-burst-coverage', [`Owned FiendFire: ${copies(owned(id))}; basic cleanup copies: ${copies(basicCleanup)}; owned multi-draw copies: ${profile.onPlayDrawCoverage.copies}.`]);
    }
    if (id === 'Corruption') {
      if (earlyFunctionWindow && !owned(id).length && row.type === 'Power' && row.cost >= 2 && row.cost <= profile.baseEnergy
          && copies(usefulPaidSkills) >= 6 && usefulPaidSkills.some(source => source.mechanics.draw_cards >= 2)
          && copies(transitions) >= 3) {
        return result('first-skill-payment-engine', 'take', [
          `Owned useful paid Skills: ${copies(usefulPaidSkills)} (${usefulPaidSkills.map(source => `${labelCard(source)}: ${source.persistent_cost} Energy`).join('; ')}).`,
          `After ${row.cost}-Energy deployment, reached Skills then cost 0 and Exhaust when played; their payments become available for other plays.`,
          'Schedule deployment while available Skill protection/access can cover the fight. Spent basic Skills improve later draws; spent useful Skills need recovery for reuse.',
          `Owned Exhaust-draw Powers: ${profile.exhaustDraw.length}; owned Exhaust-block Powers: ${copies(owned('FeelNoPain'))}.`,
        ], 'Take a first Skill-payment engine supported by the owned protection, draw and payable output. Existing Exhaust payoffs improve it; the payment and cleanup benefits already exist without new cards.');
      }
      return review('skill-payment-engine-coverage', [`Owned Corruption: ${copies(owned(id))}; useful paid Skill copies: ${copies(usefulPaidSkills)}.`]);
    }
    if (id === 'DemonForm') {
      if (earlyFunctionWindow && !profile.strengthSources.length && !strengthSupport && copies(transitions) >= 4
          && growthProtection.length
          && row.type === 'Power' && row.cost >= 2 && row.cost <= profile.baseEnergy
          && row.enduringStrength?.amount >= 2 && row.enduringStrength.delivery === 'each_player_turn_start_post_draw') {
        return result('first-protected-delayed-strength', 'take', [
          `Owned enduring Strength sources: 0; payable transition attacks: ${copies(transitions)}.`,
          `DemonForm: ${row.cost} Energy, gain ${row.enduringStrength.amount} Strength each later turn start; supplies no Strength on its deployment turn.`,
          `Owned protection: ${growthProtection.join(', ')}. Prior-turn debuffs can protect setup; same-turn plays still share ${profile.baseEnergy} base Energy.`,
        ], 'A first protected long-fight growth source is useful. Compare its deployment and later paid attacks with the immediate burst or Skill-payment alternatives.');
      }
      return review('delayed-strength-setup-coverage', [`Owned enduring Strength sources: ${profile.strengthSources.length}; payable transition attacks: ${copies(transitions)}.`]);
    }
    if (id === 'BodySlam') {
      const checked = paidBlockConversionDefault(state, cards, profile, row);
      if (checked) return result(checked.policy, checked.disposition, checked.facts, checked.preference);
      if (coveredEarlyOutput && smallBlock && row.cost === 1 && row.exhausts === false
          && row.attackDelivery?.amount?.kind === 'player_block') return result('small-block-output-conversion', 'preserve', [
        `Owned fixed Block requests: ${profile.fixedPlayedBlock.map(source => `${source.card}: ${source.block} Block for ${source.cost} Energy`).join('; ')} before combat modifiers.`,
        `BodySlam converts current Block into base Attack damage and preserves that Block; its current play costs ${row.cost} Energy after paying the Block sources. An upgrade lowers its own payment under the native rule; combat upgrades still require actual shared Hand.`,
        `Owned payable transition attacks: ${copies(transitions)}; stronger single-target requests already exist. Current small Block production gives this additional conversion a low drafting priority.`,
      ], 'Preserve the draw slot over another output conversion in this small-Block foundation. Larger Block production, payment or retention deserves its own comparison.');
      return review('block-output-foundation', [`Owned fixed Block requests: ${profile.fixedPlayedBlock.map(source => `${source.card}: ${source.block}`).join('; ') || 'none'}.`]);
    }
    if (id === 'Combust') {
      const effect = row.endOfTurnHpLossAndDamage;
      if (early && !owned(id).length && thinArea && row.type === 'Power' && row.cost === 1
          && effect?.damage_per_enemy_per_application >= 5 && effect.hp_loss_per_application === 1
          && Number.isSafeInteger(state.run.current_hp) && Number.isSafeInteger(state.run.max_hp)
          && state.run.max_hp > 0 && state.run.current_hp >= Math.ceil(state.run.max_hp * 2 / 3)) {
        return result('first-early-recurring-area-output', 'take', [
          `Owned payable area Attack requests: ${areaRoutes.map(source => `${labelCard(source)}: ${source.persistent_damage * source.mechanics.damage.delivery.hits.count} per enemy`).join('; ') || 'none'}. Current area output is thin.`,
          `Combust costs ${row.cost} Energy to deploy, then each actual player turn-end trigger loses ${effect.hp_loss_per_application} HP before requesting ${effect.damage_per_enemy_per_application} non-Attack damage to each enemy. Its Power stays deployed; the played copy leaves ordinary circulation.`,
          `HP ${state.run.current_hp}/${state.run.max_hp} meets this policy's two-thirds reserve preference. Self HP loss and enemy damage resolve under their native modifiers; longer deployment increases both damage and HP expense.`,
        ], 'Take a first repeat area-damage function while the checked HP reserve can fund its recurring cost. Use actual combat HP and enemy state to decide deployment; the threshold is a maintainer preference.');
      }
      return review('hp-funded-area-coverage', [`HP ${state.run.current_hp}/${state.run.max_hp}; owned Combust: ${copies(owned(id))}; payable area sources: ${areaRoutes.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'Uppercut') {
      if (early && !weakSources.length && copies(transitions) >= 3
          && row.type === 'Attack' && row.cost === 2 && row.exhausts === false
          && row.enemyWeak >= 1 && row.enemyVulnerable >= 1 && row.baseDamagePerHit >= 13
          && row.attackDelivery?.targets === 'selected_enemy' && row.attackDelivery.hits?.count === 1) {
        const upgrades = owned('Armaments').filter(source => payable(source)
          && source.persistent_cost + row.cost <= profile.baseEnergy
          && source.mechanics.combat_upgrade_scope === 'whole_hand'
          && source.mechanics.combat_upgrade_persistence === 'combat_only');
        return result('first-early-weak-attack', 'take', [
          `Owned payable Weak sources: 0; transition attacks: ${copies(transitions)}. Uppercut adds ${row.enemyWeak} Weak turn(s) and ${row.enemyVulnerable} Vulnerable after its hit; ${row.cost} Energy leaves ${profile.baseEnergy - row.cost} base Energy.`,
          ...upgrades.map(source => `${labelCard(source)} can upgrade Uppercut when both actually share Hand; paying both fits ${profile.baseEnergy} base Energy. The upgraded effect follows the native rule and lasts this combat.`),
        ], 'Take a first recurring Weak source for protection with useful transition damage; its current duration and two-Energy payment remain part of the choice.');
      }
      return review('weak-attack-coverage', [`Owned payable Weak: ${weakSources.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'FeelNoPain') {
      if (early && !owned(id).length && row.type === 'Power' && row.cost === 1
          && row.blockPerExhaustAfterDeployment > 0 && reusableExhaust.length) {
        const once = cards.filter(source => source.mechanics.exhausts === true || source.mechanics.ethereal === true);
        return result('first-early-exhaust-block', 'take', [
          `FeelNoPain: deploy for ${row.cost} Energy, then gain ${row.blockPerExhaustAfterDeployment} Block per actual card Exhaust. That Block protects the trigger turn and follows ordinary Block loss.`,
          ...reusableExhaust.map(source => source.card.card === 'BurningPact'
            ? `${labelCard(source)} costs ${source.persistent_cost} Energy: select one other Hand card to Exhaust, then request ${source.mechanics.draw_cards} draws. Each later use needs redraw, payment and a Hand target; each selected card gives one Block trigger after deployment.`
            : `${labelCard(source)} costs ${source.persistent_cost} Energy and Exhausts other Hand cards under its native selection rule; each actual Exhaust gives one Block trigger after deployment.`),
          ...once.map(source => `${labelCard(source)}: ${source.mechanics.exhausts === true ? 'Exhaust after play' : 'Ethereal Exhaust if still in Hand at turn end'}. This gives one Block trigger after deployment; the exhausted copy leaves ordinary redraws, with a separate recovery effect needed for another use.`),
        ], 'A first deployed Exhaust-block function has owned fuel. Compare its actual trigger quantity and timing with protection delivered directly by another offered card.');
      }
      return review('exhaust-block-fuel-coverage', [`Owned payable reusable other-card Exhaust: ${reusableExhaust.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'Cleave') {
      if (earlyArea.some(offer => offer.command === row.command)) return result('first-early-area-output', 'take',
        [`Owned payable fixed area attacks: ${area.map(source => `${labelCard(source)}, ${source.persistent_damage} base damage per hit`).join('; ') || 'none'}.`,
          `Cleave requests ${row.baseDamagePerHit} damage to each enemy for 1 Energy before modifiers.`],
        'Take a first stronger standalone area hit for early multi-enemy fights.');
      return review('area-output-coverage', [`Owned payable fixed area attacks: ${area.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'Hemokinesis') {
      if (early && (earlyHandUpgrade.length || earlyArea.length) && copies(transitions) >= 4
          && retrievalTargets.some(target => retrievalOutput(target.source, profile).total >= 20)
          && row.type === 'Attack' && row.cost === 1 && row.exhausts === false && row.baseDamagePerHit === 15
          && row.attackDelivery?.targets === 'selected_enemy' && row.attackDelivery.hits?.count === 1
          && !row.attackDelivery.play_condition && row.selfHpLoss?.base_hp_loss === 2
          && !cards.some(source => ['Rupture', 'BloodForBlood'].includes(source.card.card))
          && !(state.relics ?? []).some(relic => ['RunicCube', 'CentennialPuzzle', 'RedSkull'].includes(relic.relic))) {
        return result('existing-output-over-hp-transition', 'preserve',
          [`Owned payable transition attacks: ${copies(transitions)} (${transitions.map(labelCard).join(', ')}).`,
            ...retrievalTargets.map(target => target.fact),
            'Hemokinesis adds a 15-damage single-target request and 2 base HP loss per play; this reward supplies a first upgrade or area function.'],
          'Prefer the supplied new function over another HP-funded single-target transition attack in this covered output inventory.');
      }
      return review('hp-transition-coverage', [`Owned payable transition attacks: ${copies(transitions)}.`]);
    }
    if (id === 'Barricade') {
      const basicBlock = cards.filter(source => source.persistent_block > 0);
      const basicFoundation = basicBlock.length && basicBlock.every(source =>
        source.mechanics.base_block_delivery === 'once_on_play' && source.persistent_block <= 5
        && source.persistent_cost >= 1)
        && !cards.some(source => source.mechanics.card_type === 'Power' || source.mechanics.weak > 0
          || ['Entrench', 'BodySlam'].includes(source.card.card))
        && !profile.blockSupport.damageCaps.length && !profile.persistentReduction.length
        && !profile.onPlayEnergy.length
        && (state.relics ?? []).every(relic => ['BurningBlood', 'SingingBowl'].includes(relic.relic));
      if (earlyRetrieval.length && basicFoundation && row.type === 'Power' && row.cost === profile.baseEnergy) {
        return result('early-retrieval-over-unfunded-retention', 'preserve',
          [`Owned Block requests: ${basicBlock.map(labelCard).join(', ')}; each at most 5 Block for at least 1 Energy.`,
            `Barricade deployment costs all ${profile.baseEnergy} base Energy. This reward also supplies a first useful Headbutt.`],
          'Prefer the supplied retrieval of existing strong output here. Retention development needs larger Block production or deployment support; keep that future investment for its supported comparison.');
      }
      return review('retention-foundation', [`Owned Block production: ${basicBlock.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'WildStrike') {
      if (coveredLaterOutput && row.type === 'Attack' && row.cost === 1 && row.exhausts === false
          && row.baseDamagePerHit === 12 && row.tags?.includes('Strike')
          && row.attackDelivery?.amount?.kind === 'base' && row.attackDelivery.targets === 'selected_enemy'
          && row.attackDelivery.hits?.kind === 'fixed' && row.attackDelivery.hits.count === 1
          && row.statusInjections?.length === 1 && row.statusInjections[0].card === 'Wound'
          && row.statusInjections[0].count === 1 && row.statusInjections[0].destination === 'draw_pile'
          && row.statusInjections[0].persistence === 'persistent' && !profile.statusPayoffs.length
          && strikeIncrement <= 3) return result('covered-output-over-wound-transition', 'preserve', [
        ...outputCoverageFacts,
        `WildStrike adds a 12-base-damage hit and one Wound into Draw per play. Its Strike tag adds ${strikeIncrement} base damage summed across original Strike-scaling copies, before modifiers and combat pile changes.`,
        'Owned cleanup can remove an accessed Wound through its own selection and payment; the inserted Wound may obstruct a draw before that cleanup.',
      ], 'Preserve access to existing output, protection and draw over another ordinary attack with a Draw-pile Wound in this covered output pool.');
      if (earlyRetrieval.length && copies(transitions) >= 3 && row.type === 'Attack' && row.cost === 1
          && row.baseDamagePerHit === 12 && row.attackDelivery?.targets === 'selected_enemy'
          && row.attackDelivery.hits?.count === 1 && row.statusInjections?.length === 1
          && row.statusInjections[0].card === 'Wound' && row.statusInjections[0].count === 1
          && row.statusInjections[0].destination === 'draw_pile' && !profile.statusPayoffs.length
          && strikeIncrement <= 3) return result('early-retrieval-over-wound-transition', 'preserve',
        [`Owned payable transition attacks: ${copies(transitions)} (${transitions.map(labelCard).join(', ')}).`,
          `Adding one Strike-tagged copy changes each original Strike-scaling attack by its own coefficient; summed across original copies: ${strikeIncrement} base damage before pile changes. WildStrike inserts one Wound into Draw per play.`],
        'Prefer the first useful retrieval of owned strong output over another ordinary transition attack that adds Draw-pile obstruction. Existing cleanup uses its own access, selection and payment.');
      return review('wound-transition-increment', [`Owned payable transition attacks: ${copies(transitions)}; owned Status payoff Powers: ${profile.statusPayoffs.map(source => source.card).join(', ') || 'none'}.`]);
    }
    if (id === 'InfernalBlade') {
      const exhaustPayoff = profile.exhaustDraw.length || cards.some(source => source.mechanics.exhaust_block_power > 0)
        || (state.relics ?? []).some(source => source.relic === 'DeadBranch');
      if (coveredLaterOutput && row.type === 'Skill' && row.cost === 0 && row.exhausts === true
          && row.randomAttackToHand?.count === 1 && row.randomAttackToHand.cost_for_turn === 0
          && !exhaustPayoff && !profile.skillExhaustPowers.length && !owned('MasterReality').length) {
        const facts = [...outputCoverageFacts,
          'InfernalBlade+ uses one permanent draw slot to generate one random class Attack for Hand, then Exhausts. The generated card costs 0 this turn and follows its own play conditions; Hand overflow goes to Discard. It remains combat-only, using its ordinary cost after turn end.',
          ...(attackRelics.length ? [
            `Owned 0/1-Energy Attacks: ${profile.attacks.filter(source => source.cost >= 0 && source.cost <= 1).map(source => `${source.card} (${source.cost} Energy)`).join('; ') || 'none'}. These still need actual Hand access and their own play conditions.`,
            `Owned Attack-play relics: ${attackRelics.map(source => source.relic).join(', ')}. A playable generated Attack contributes one actual Attack play to their counters; each hit of a multi-hit card shares that one play.`,
          ] : []),
        ];
        return result('covered-output-random-free-attack', attackRelics.length && !owned(id).length ? 'compare' : 'preserve', facts,
          attackRelics.length && !owned(id).length
            ? 'Compare a first free random Attack for the owned Attack-play relic with keeping access to existing output and protection. The result and whether it is playable are revealed in combat.'
            : 'Preserve access to existing output and protection over an additional random attack generator in this covered output pool.');
      }
      return review('random-free-attack-support', [`Owned Attack-play relics: ${attackRelics.map(source => source.relic).join(', ') || 'none'}; owned Exhaust payoff: ${exhaustPayoff ? 'present' : 'absent'}.`]);
    }
    if (id === 'Havoc') {
      const planners = cards.filter(source => payable(source)
        && (source.mechanics.hand_topdeck_after_draw > 0 || source.card.card === 'Headbutt'));
      const powers = cards.filter(source => payable(source) && source.mechanics.card_type === 'Power'
        && source.persistent_cost > 0);
      if (smallSkillWindow && !owned(id).length && row.type === 'Skill' && row.cost === 0
          && row.topDeckPlay?.exhaust_after_play === true && row.topDeckPlay.bypass_fixed_energy_cost === true
          && row.topDeckPlay.x_cost_uses_current_energy === true
          && planners.length && powers.length) return result('first-planned-free-topdeck-play', 'take',
        [`Owned topdeck planning: ${planners.map(labelCard).join(', ')}; owned payable Powers: ${powers.map(labelCard).join(', ')}. Owned Havoc: 0 copies.`],
        'Take a first Havoc+ for a planned free top-card play. Put a useful target on top before Havoc; deployment, target selection and Exhaust follow that card\'s actual play rules.');
      return review('planned-topdeck-play-coverage', [`Owned topdeck planning: ${planners.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'Armaments') {
      const wholeHand = owned(id).filter(source => payable(source)
        && source.mechanics.combat_upgrade_scope === 'whole_hand');
      if (earlyHandUpgrade.some(offer => offer.command === row.command)) return result('first-early-hand-upgrades', 'take',
        [`Owned upgradeable one-Energy basics with at least 3 additional damage or Block: ${copies(upgradeTargets.basics)} copies.`,
          ...upgradeTargets.access.map(target => target.fact),
          'Armaments upgrades other eligible cards sharing its actual Hand when played. Those upgrades last for this combat; subsequent draws may bring other targets for another play.'],
        'Take a first Armaments+ to improve the existing basic and access cards while gaining its own Block.');
      if (established && copies(wholeHand) >= 2 && row.combatUpgradeScope === 'whole_hand') {
        return result('covered-combat-upgrades', 'preserve',
          [`Owned payable whole-Hand Armaments: ${copies(wholeHand)} copies.`],
          'Preserve access to setup and output over a third whole-Hand combat-upgrade copy.');
      }
      return review('combat-upgrade-coverage', [`Owned payable whole-Hand Armaments: ${copies(wholeHand)} copies.`]);
    }
    if (id === 'Headbutt') {
      if (earlyRetrieval.some(offer => offer.command === row.command)) return result('first-early-output-retrieval', 'take',
        [...retrievalTargets.map(target => target.fact), 'Owned Headbutt: 0 copies. Owned payable on-play draw provides a subsequent access route.',
          'Play Headbutt after the useful target enters Discard, then draw the placed target and pay for its next play. A later turn can supply that draw and payment.'],
        'Take a first Headbutt to reuse the listed existing output.');
      const reusable = cards.filter(source => ['SpotWeakness', 'Rampage', 'Entrench'].includes(source.card.card)
        && source.mechanics.exhausts === false && source.persistent_cost >= 0
        && source.persistent_cost + row.cost <= profile.baseEnergy);
      if (smallSkillWindow && established && !owned(id).length && row.type === 'Attack'
          && row.cost === 1 && reusable.length && profile.onPlayDrawCoverage.copies >= 1
          && profile.onPlayDrawCoverage.copies <= 2) return result('first-key-discard-retrieval', 'take',
        [`Owned payable repeat-use targets: ${reusable.map(labelCard).join(', ')}. Owned Headbutt: 0 copies.`],
        'Take a first Headbutt for its printed Discard-to-top-of-Draw selection. A useful target must be in Discard; reach it with a later draw and pay for its next use.');
      return review('discard-retrieval-coverage', [`Owned repeat-use targets payable with Headbutt: ${reusable.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'TrueGrit') {
      const cleanup = cards.filter(source => ['Strike', 'Defend', 'AscendersBane'].includes(source.card.card));
      const existing = cards.filter(source => source.mechanics.exhausts_other_cards && payable(source));
      if (smallSkillWindow && !owned(id).length && copies(cleanup) >= 4 && copies(existing) <= 1
          && row.type === 'Skill' && row.cost === 1 && row.blockRequest >= 9
          && row.targetedHandExhaust && row.exhausts === false) return result('first-protective-cleanup', 'take',
        [`Owned basic cleanup pool: ${copies(cleanup)} copies. Owned payable other-card Exhaust: ${copies(existing)} copies.`],
        'Take a first targeted TrueGrit+ for protection and repeated weak-Hand-card cleanup. Burn a weak or obstructive card while keeping useful functions available.');
      return review('protective-cleanup-coverage', [`Owned basic cleanup pool: ${copies(cleanup)}; owned payable other-card Exhaust: ${copies(existing)} copies.`]);
    }
    if (id === 'ShrugItOff') {
      const count = copies(owned(id));
      if (smallSkillWindow && count <= 1 && row.type === 'Skill' && row.cost === 1
          && row.blockRequest >= 11 && row.onPlayDraw?.requested === 1
          && row.onPlayDraw.topdeck === 0 && row.onPlayDraw.handChange === 0
          && row.exhausts === false) return result('early-protective-replacement', 'take',
        [`Owned ShrugItOff: ${count} copies. Owned payable setup: ${setup.map(source => source.card).join(', ')}.`],
        'Take an upgraded first or second ShrugItOff for repeatable protection while drawing its replacement during setup and later turns.');
      const matching = owned(id).filter(source => payable(source) && source.mechanics.card_type === 'Skill'
        && source.persistent_cost <= row.cost && source.persistent_block >= row.blockRequest
        && source.mechanics.draw_cards >= row.onPlayDraw?.requested
        && (source.mechanics.hand_topdeck_after_draw ?? 0) === row.onPlayDraw?.topdeck
        && source.mechanics.exhausts === row.exhausts);
      if (smallSkillWindow && copies(matching) >= 2 && row.type === 'Skill' && row.cost === 1
          && row.blockRequest >= 11 && row.onPlayDraw?.requested === 1 && row.onPlayDraw.topdeck === 0
          && row.onPlayDraw.handChange === 0 && row.exhausts === false && firstReduction.length) {
        return result('first-reduction-over-repeated-replacement', 'preserve',
          [`Owned payable ShrugItOff at least matching this offer: ${copies(matching)} copies.`,
            `First payable persistent reduction offered: ${firstReduction.map(source => source.command).join(', ')}.`],
          'Prefer the first persistent enemy Strength reduction over another already-covered protective replacement in this reward. It reduces each affected later attack hit, subject to enemy debuff prevention and clears.');
      }
      return review('protective-draw-coverage', [`Owned ShrugItOff: ${count} copies.`]);
    }
    if (id === 'Warcry') {
      const count = copies(owned(id));
      if (smallSkillWindow && count === 0 && profile.onPlayDrawCoverage.copies >= 1
          && profile.onPlayDrawCoverage.copies <= 2 && row.type === 'Skill' && row.cost === 0
          && row.onPlayDraw?.requested === 2 && row.onPlayDraw.topdeck === 1
          && row.onPlayDraw.handChange === 0 && row.exhausts === true) return result('first-free-hand-planning', 'take',
        [`Owned Warcry: 0 copies. Owned payable setup: ${setup.map(source => source.card).join(', ')}.`,
          `Owned on-play multi-draw: ${profile.onPlayDrawCoverage.copies} copies.`],
        'Take a first Warcry+ to inspect two fresh cards and place one Hand card on top for a later draw. Its own Exhaust removes this copy from later ordinary reshuffles.');
      return review('hand-planning-coverage', [`Owned Warcry: ${count} copies; owned on-play multi-draw: ${profile.onPlayDrawCoverage.copies} copies.`]);
    }
    if (id === 'Disarm') {
      if ((act >= 2 || coveredEarlyOutput) && row.cost >= 0 && row.cost <= profile.baseEnergy && row.enemyStrengthReduction?.amount > 0
          && row.enemyStrengthReduction.restoration === 'no_scheduled_restoration' && !profile.persistentReduction.length) {
        return result('first-persistent-reduction', 'take', [
          'Owned persistent enemy Strength reduction: 0.',
          ...(act === 1 ? [`Owned payable transition attacks: ${copies(transitions)} with stronger focused output; a first Disarm supplies a separate protection function.`] : []),
          `${row.command}: ${row.cost} Energy; reduce one enemy's Strength by ${row.enemyStrengthReduction.amount} with no scheduled restoration, subject to debuff prevention and clearing. Each later Attack hit uses the remaining Strength; the card Exhausts after play.`,
        ],
          'Take the first payable Disarm for persistent reduction during setup and later attack turns.');
      }
      return review('persistent-reduction-coverage', [`Owned persistent reduction sources: ${profile.persistentReduction.length}.`]);
    }
    if (id === 'BloodForBlood') {
      const fuel = cards.filter(source => source.mechanics.end_of_turn_hp_loss_and_damage?.hp_loss_per_application > 0);
      if (coveredEarlyOutput && row.cost > profile.baseEnergy && row.costChangeOnHpLoss === -1
          && row.type === 'Attack' && row.exhausts === false && row.baseDamagePerHit === 18
          && row.attackDelivery?.targets === 'selected_enemy' && row.attackDelivery.hits?.count === 1
          && fuel.length && offers.some(offer => offer.card === 'Disarm'
            && offer.enemyStrengthReduction?.restoration === 'no_scheduled_restoration'
            && !profile.persistentReduction.length)) return result('delayed-discount-or-first-protection', 'preserve', [
        `BloodForBlood starts at ${row.cost} Energy against ${profile.baseEnergy} base Energy. Each positive HP-loss event lowers this copy's cost by 1 to a floor of 0 while it is in Hand, Draw or Discard.`,
        ...fuel.map(source => `${labelCard(source)} must deploy first; each actual player turn-end trigger requests ${source.mechanics.end_of_turn_hp_loss_and_damage.hp_loss_per_application} HP loss. Cost reduction follows actual positive loss, then the attack still needs access and payment.`),
        `Existing payable focused output is already supplied by ${retrievalTargets.map(target => labelCard(target.source)).join(', ')}. This reward also supplies first persistent enemy Strength reduction.`,
      ], 'Prefer the first protection function over another delayed-discount output card in this covered early attack pool. The owned HP-loss combination is real and retains its stated deployment and event timing.');
      return review('hp-loss-discount-coverage', [`Owned recurring HP-loss requests: ${fuel.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'Clash') {
      if (owned('AscendersBane').length && !clearsHand.length
          && !profile.skillExhaustPowers.length && !profile.exhaustDraw.length) return result('conditional-attack-hand', 'preserve',
        ['Owned AscendersBane; owned payable Attack-preserving whole-Hand non-Attack clearing routes: 0.',
          ...owned('FiendFire').map(source => `${labelCard(source)} clears the whole remaining Hand, including Clash itself. It supplies burst and cleanup; keeping Clash needs a different route.`)],
        'Preserve the draw slot while Clash requires clearing every non-Attack from Hand.');
      return review('conditional-attack-hand', [`Owned Attack-preserving whole-Hand clearing routes: ${clearsHand.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (['TwinStrike', 'SwordBoomerang', 'HeavyBlade'].includes(id)) {
      if (id === 'HeavyBlade' && coveredLaterOutput && !strengthSupport && row.upgrades === 0
          && row.type === 'Attack' && row.cost === 2 && row.exhausts === false && row.baseDamagePerHit === 14
          && row.attackDelivery?.amount?.kind === 'base' && row.attackDelivery.targets === 'selected_enemy'
          && row.attackDelivery.hits?.kind === 'fixed' && row.attackDelivery.hits.count === 1) {
        return result('covered-output-without-strength', 'preserve', [...outputCoverageFacts,
          'Owned reviewed Strength support: 0. HeavyBlade currently supplies one 14-base-damage hit for 2 Energy; its Strength multiplier becomes useful after an actual Strength source supplies a gain.',
        ], 'Preserve access to the supplied output and protection instead of another two-Energy attack whose added Strength payoff needs a new source.');
      }
      if (id === 'SwordBoomerang' && coveredEarlyOutput && !strengthSupport
          && row.cost === 1 && row.exhausts === false && row.baseDamagePerHit === 3
          && row.attackDelivery?.amount?.kind === 'base' && row.attackDelivery.hits?.count === 3
          && row.attackDelivery.targets === 'random_enemy_per_hit'
          && cards.some(source => source.persistent_cost === 1 && retrievalOutput(source, profile)?.total >= 9)) {
        return result('covered-unscaled-random-hits', 'preserve', [
          `Owned payable transition attacks: ${copies(transitions)}; one-Energy focused requests at least 9 already exist. Reviewed owned Strength support: 0. SwordBoomerang requests 3 x 3 base damage with independent random targets; its hits share one card play.`,
        ], 'Preserve the draw slot over an additional unscaled random-hit attack in this supplied output pool. Existing Strength support changes this comparison.');
      }
      if (established && multiHit.length && focused.length) return result('covered-strength-payoff', 'preserve',
        [`Owned enduring Strength: ${profile.strengthSources.map(row => row.card).join(', ')}.`,
          `Owned payable transition attacks: ${copies(transitions)}; focused attacks: ${copies(focused)}; multi-hit payoffs: ${copies(multiHit)}.`],
        'Preserve access to the existing Strength and attack plan over another plain Strength payoff.');
      return review('strength-payoff-coverage', [`Preparation Act ${act}; owned payable transition attacks: ${copies(transitions)}; multi-hit payoffs: ${copies(multiHit)}.`]);
    }
    if (id === 'Pummel') {
      const burst = transitions.filter(source => ['Pummel', 'HeavyBlade', 'FiendFire'].includes(source.card.card)
        || (source.mechanics.damage?.delivery?.targets === 'selected_enemy'
          && source.mechanics.damage.delivery.hits?.kind === 'fixed'
          && source.mechanics.damage.delivery.hits.count >= 4));
      if (established && profile.strengthSources.some(source => source.cost >= 0 && source.cost <= profile.baseEnergy)
          && !burst.length && row.cost >= 0 && row.cost <= 1 && row.exhausts === true
          && row.attackDelivery?.targets === 'selected_enemy' && row.attackDelivery.hits?.kind === 'fixed'
          && row.attackDelivery.hits.count >= 4) return result('first-focused-strength-burst', 'take',
        [`Owned payable enduring Strength: ${profile.strengthSources.filter(source => source.cost >= 0 && source.cost <= profile.baseEnergy).map(source => source.card).join(', ')}.`,
          `Owned payable focused high-Strength burst attacks: 0; offered Pummel costs ${row.cost}, hits ${row.attackDelivery.hits.count} times, then Exhausts.`],
        'Take a first cheap focused Strength burst. Use it after the owned Strength gain; the played copy leaves later draws through Exhaust.');
      return review('focused-strength-burst-coverage', [`Owned payable focused high-Strength burst attacks: ${burst.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'ThunderClap') {
      const vulnerable = cards.filter(source => payable(source) && source.mechanics.vulnerable > 0 && source.mechanics.exhausts === false);
      const area = cards.filter(source => payable(source) && source.mechanics.damage?.delivery?.kind === 'attack'
        && source.mechanics.damage.delivery.targets === 'each_enemy'
        && source.mechanics.damage.damage_per_hit >= row.baseDamagePerHit);
      if (established && vulnerable.length && area.length && !owned('Dropkick').length
          && !owned('SadisticNature').length && !(state.relics ?? []).some(relic => relic.relic === 'ChampionsBelt')) {
        return result('covered-area-vulnerable-transition', 'preserve',
          [`Owned payable reusable Vulnerable: ${vulnerable.map(labelCard).join(', ')}.`,
            `Owned payable area damage: ${area.map(labelCard).join(', ')}.`],
          'Preserve access to the established damage and setup plan over another small area-damage and Vulnerable transition card.');
      }
      return review('area-vulnerable-coverage', [`Owned payable reusable Vulnerable: ${vulnerable.map(labelCard).join(', ') || 'none'}; area damage: ${area.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'IronWave') {
      const blocks = cards.filter(source => payable(source) && source.persistent_cost <= row.cost
        && source.mechanics.base_block_delivery === 'once_on_play' && source.persistent_block >= row.blockRequest);
      if (established && row.cost >= 0 && row.cost <= 1 && row.blockRequest > 0 && copies(blocks) >= 3
          && blocks.some(source => source.persistent_block >= row.blockRequest * 2) && !owned('Juggernaut').length) {
        return result('covered-attack-block-transition', 'preserve',
          [`Owned payable transition attacks: ${copies(transitions)}; Block requests at least ${row.blockRequest} for at most ${row.cost} Energy: ${blocks.map(labelCard).join(', ')}.`],
          'Preserve access to the established damage and protection plan over another small combined attack and Block transition card.');
      }
      return review('attack-block-coverage', [`Owned same-or-lower-cost Block coverage: ${blocks.map(labelCard).join(', ') || 'none'}.`]);
    }
    if (id === 'PerfectedStrike') {
      const originals = owned(id).filter(payable);
      if (coveredLaterOutput && copies(originals) === 1 && row.upgrades === 0 && row.type === 'Attack'
          && row.cost === 2 && row.exhausts === false && row.strikeScaling?.base === 6
          && row.strikeScaling.perCard === 2 && row.tags?.includes('Strike') && profile.strikeTags
          && originals[0].mechanics.damage?.delivery?.amount?.per_card >= 3
          && retrievalOutput(originals[0], profile)?.total >= 6 + 2 * (profile.strikeTags.copies + 1)) {
        return result('covered-additional-strike-output', 'preserve', [...outputCoverageFacts,
          `Adding this Strike-tagged card changes the original pool ${profile.strikeTags.copies}->${profile.strikeTags.copies + 1}. Its initial request is 6 + 2 x ${profile.strikeTags.copies + 1} = ${6 + 2 * (profile.strikeTags.copies + 1)} before modifiers and pile changes.`,
          `${labelCard(originals[0])} also gains ${originals[0].mechanics.damage.delivery.amount.per_card} requested damage from that added tag while it remains counted. Exhausting counted cards lowers these requests.`,
        ], 'Preserve the draw slot over another weaker two-Energy Strike-count attack; its increment to the owned copy is included in this preference.');
      }
      if (established && owned(id).some(payable)) return result('covered-strike-transition', 'preserve',
        [`Owned payable PerfectedStrike: ${copies(owned(id).filter(payable))}; payable transition attacks: ${copies(transitions)}.`],
        'Preserve access to the established damage plan over another two-Energy Strike-count transition attack.');
      return review('strike-transition-coverage', [`Owned payable PerfectedStrike: ${copies(owned(id).filter(payable))}.`]);
    }
    if (id === 'SearingBlow') {
      if (coveredLaterOutput && !owned(id).length && row.upgrades === 0 && row.type === 'Attack'
          && row.cost === 2 && row.exhausts === false && row.baseDamagePerHit === 12
          && row.attackDelivery?.amount?.kind === 'base' && row.attackDelivery.targets === 'selected_enemy'
          && row.attackDelivery.hits?.kind === 'fixed' && row.attackDelivery.hits.count === 1
          && !(state.relics ?? []).some(source => source.relic === 'WarpedTongs')) {
        const upgrades = cards.filter(source => payable(source) && source.mechanics.combat_upgrade_scope === 'whole_hand'
          && source.mechanics.combat_upgrade_persistence === 'combat_only');
        return result('covered-output-before-repeat-upgrades', 'preserve', [...outputCoverageFacts,
          'SearingBlow currently requests 12 damage for 2 Energy. Its repeat-upgrade curve requires actual upgrade actions; permanent upgrades use preparation opportunities.',
          ...upgrades.map(source => `${labelCard(source)} can improve SearingBlow in the same actual Hand at its printed ${source.persistent_cost}-Energy cost; changes last this combat. Each further improvement needs another accessible upgrade use under its actual payment and lifecycle.`),
        ], 'Prefer existing output access over starting another upgrade-dependent attack. Owned combat upgrading is a real conditional benefit, with its own access and payment.');
      }
      return review('repeat-upgrade-output-coverage', [`Owned SearingBlow copies: ${copies(owned(id))}; payable transition attacks: ${copies(transitions)}.`]);
    }
    if (id === 'Flex') {
      const persistence = (state.relics ?? []).some(row => row.relic === 'OrangePellets');
      const artifactSupport = cards.some(source => ['Panacea', 'CoreSurge'].includes(source.card.card))
        || (state.relics ?? []).some(source => source.relic === 'ClockworkSouvenir')
        || (state.potions ?? []).some(source => source.potion?.potion === 'AncientPotion');
      const limitedHits = profile.attacks.length > 0 && cards.filter(source => source.mechanics.card_type === 'Attack').every(source => {
        const delivery = source.mechanics.damage?.delivery;
        return delivery?.kind === 'attack' && (delivery.hits?.kind === 'fixed' && delivery.hits.count === 1
          && ['base', 'base_plus_strike_cards', 'player_block'].includes(delivery.amount?.kind)
          || source.card.card === 'FiendFire' && delivery.amount?.kind === 'base'
          && delivery.hits?.kind === 'per_hand_card_at_resolution_before_exhaust');
      });
      if (coveredLaterOutput && limitedHits && profile.onPlayDrawCoverage.copies <= 2 && !profile.exhaustDraw.length
          && !strengthAmplifiers.length && !persistence && !artifactSupport && !owned('DualWield').length
          && row.type === 'Skill' && row.cost === 0 && row.exhausts === false
          && [2, 4].includes(row.temporaryStrength) && row.strengthLossAtTurnEnd === row.temporaryStrength) {
        const burst = owned('FiendFire').filter(payable);
        return result('covered-output-temporary-strength', 'preserve', [
          ...outputCoverageFacts,
          `Flex adds ${row.temporaryStrength} same-turn Strength for reached and paid attacks, with a separate ${row.strengthLossAtTurnEnd}-Strength loss at turn end. It occupies another permanent draw slot. Other owned attacks use fixed one-hit deliveries.`,
          ...burst.map(source => {
            const perHit = source.persistent_damage, boost = row.temporaryStrength;
            return `Same-Hand example with Flex, ${labelCard(source)}, and 3 other cards: Exhausting Flex as fuel requests 4 x ${perHit} = ${4 * perHit}; playing Flex first requests 3 x (${perHit} + ${boost}) = ${3 * (perHit + boost)}. Increment ${3 * boost - perHit} before other Strength and modifiers. Actual draws and prior plays change the fuel count.`;
          }),
        ], 'Preserve access to existing damage, protection and draw over an additional temporary Strength setup in this single-hit and Hand-consuming burst pool.');
      }
      if (early && copies(transitions) >= 4 && singleHitPool && !strengthAmplifiers.length
          && !persistence && !artifactSupport && !owned('DualWield').length
          && row.type === 'Skill' && row.cost === 0 && row.exhausts === false) {
        return result('single-hit-temporary-strength', 'preserve', [
          `Owned attacks use fixed single hits; transition attacks: ${copies(transitions)}. Flex spends one draw for temporary same-turn Strength shared by attacks actually reached and paid that turn. Owned LimitBreak, OrangePellets, Artifact-persistence support and DualWield: 0.`,
        ], 'Preserve the draw slot over an additional temporary Strength setup in this already supplied single-hit output pool.');
      }
      if (established && multiHit.length && !strengthAmplifiers.length && !persistence) return result('covered-strength-source', 'preserve',
        [`Owned enduring Strength: ${profile.strengthSources.map(row => row.card).join(', ')}; owned LimitBreak: 0; owned OrangePellets: 0.`],
        'Preserve access to the enduring Strength plan over an extra draw spent on temporary Strength.');
      return review('temporary-strength-combination', [`Owned Strength amplifiers: ${strengthAmplifiers.map(labelCard).join(', ') || 'none'}; OrangePellets: ${persistence ? 'owned' : 'absent'}.`]);
    }
    return review('uncovered-reward-card', []);
  });
  const unresolved = assessments.filter(row => row.disposition === 'review');
  const takes = assessments.filter(row => row.disposition === 'take').map(row => row.command);
  const comparisons = assessments.filter(row => row.disposition === 'compare').map(row => row.command);
  const skip = profile.candidates.find(row => row.kind === 'preserve').command;
  return { id: 'ordinary-reward-defaults', origin: 'maintainer_strategy_default',
    sources: ['cx', 'je', 'panacea', 'equation'],
    dependencies: [...(assessments.some(row => ['first-early-output-retrieval', 'first-early-hand-upgrades',
      'first-hand-exhaust-burst', 'first-skill-payment-engine', 'first-free-payment-for-owned-draw-output',
      'covered-output-copying-free-attack', 'low-hp-covered-payment'].includes(row.policy)) ? ['draw'] : []),
      ...(assessments.some(row => row.policy === 'first-low-cost-enduring-strength') ? ['strength'] : []),
      ...(comparisons.length ? ['attack-play'] : [])], assessments,
    commands: unresolved.length ? [] : takes.length ? takes : [...comparisons, skip],
    ...(comparisons.length && !takes.length ? {
      defaultCommands: [skip],
      question: 'Compare the supplied free Attack toward the owned Attack-play relic with keeping access to existing damage and protection. Use its supplied play conditions, uncertainty and lifecycle.',
    } : {}),
    conditions: [...new Set(assessments.flatMap(row => row.facts))],
    preference: unresolved.length ? 'Review the retained candidates against a supplied drafting policy.'
      : takes.length ? 'Use the additional function accepted by the checked conditions.'
        : comparisons.length ? 'Default to preserving existing access; compare the supplied free-Attack interaction with that preservation.'
          : 'Preserve the current deck under the checked coverage rules.',
    ...(unresolved.length ? { gap: { kind: 'strategy', reason: 'uncovered-reward-condition',
      message: 'This reward needs reviewed conditions for the retained candidates.',
      candidates: unresolved.map(({ command, card, policy, facts }) => ({ command, card, policy, facts })) } } : {}) };
}

// These variants have reviewed numeric preferences. Compare the complete
// native effect objects too, so a change in timing, selection or lifecycle
// becomes a policy gap instead of disappearing behind the numeric summary.
const cardVariantValues = {
  BattleTrance: { field: 'draw_cards', label: 'requested draws' },
  BurningPact: { field: 'draw_cards', label: 'requested draws' },
  DeepBreath: { field: 'draw_cards', label: 'requested draws' },
  DarkEmbrace: { field: 'exhaust_draw_power', label: 'draws per Exhaust after deployment' },
  DemonForm: { field: 'strength_gain', label: 'Strength per supplied turn-start trigger' },
  Inflame: { field: 'strength_gain', label: 'lasting Strength on play' },
  Disarm: { field: 'enemy_strength_down', label: 'persistent enemy Strength reduction' },
};

function cardVariantComparisonPolicy(observation, offers, normalize) {
  const id = offers[0]?.card, value = cardVariantValues[id];
  const native = new Map((observation.position.actions ?? []).map(row =>
    [normalize(row.semantic_action, observation), preparationCardAcquisition(observation, row)?.mechanics]));
  const variants = offers.map(row => {
    const mechanics = native.get(row.command);
    const otherEffects = mechanics ? { ...mechanics } : undefined;
    if (otherEffects && value) {
      for (const field of ['upgrades', 'cost', value.field]) delete otherEffects[field];
    }
    return { command: row.command, card: row.card, cost: mechanics?.cost,
      amount: value ? mechanics?.[value.field] : undefined, otherEffects };
  });
  const facts = variants.map(row => `${row.command}: ${row.cost} Energy`
    + (value && row.card === id && Number.isSafeInteger(row.amount) ? `, ${row.amount} ${value.label}` : '') + '.');
  const rule = { id: 'same-card-variant', origin: 'maintainer_strategy_default', sources: ['cx', 'equation'],
    dependencies: [], commands: [], conditions: facts };
  const review = () => ({ ...rule, preference: 'Supply a reviewed preference for this candidate set.',
    gap: { kind: 'strategy', reason: 'uncovered-card-variant-comparison',
      message: 'This candidate set needs reviewed payment, effect and variant conditions.',
      candidates: variants.map(({ command, card, cost, amount }) => ({ command, card, cost,
        ...(value && card === id && Number.isSafeInteger(amount) ? { amount, value: value.label } : {}) })) } });
  if (!value || variants.some(row => row.card !== id || !Number.isSafeInteger(row.cost) || row.cost < 0
      || !Number.isSafeInteger(row.amount) || row.amount <= 0
      || !isDeepStrictEqual(row.otherEffects, variants[0].otherEffects))) return review();
  if ((observation.position.observation.observation.relics ?? []).some(row => row.relic === 'SneckoEye')) return review();
  const preferred = variants.filter(row => variants.every(other => row.cost <= other.cost && row.amount >= other.amount));
  if (!preferred.length) return review();
  return { ...rule, commands: [preferred.map(row => row.command).sort()[0]],
    conditions: [...facts, `Other supplied native effects of ${id} match across these variants.`],
    preference: `Use the ${id} variant with at least as much ${value.label} and at most the printed Energy payment.` };
}

// Conditions are resolved by the host. These defaults are strategy preferences,
// separate from the native mechanism facts and from the player's final choice.
function accessComparisonPolicy(observation, cards, offers, profile, normalize) {
  if (new Set(offers.map(row => row.card)).size === 1) return cardVariantComparisonPolicy(observation, offers, normalize);
  const state = observation.position.observation.observation;
  const known = new Set(['BattleTrance', 'BurningPact', 'DeepBreath']);
  const facts = offers.map(row => `${row.command}: ${row.cost} Energy, ${row.onPlayDraw.requested} requested draws.`);
  const rule = (id, commands, conditions, preference) => ({ id, origin: 'maintainer_strategy_default',
    sources: ['cx', 'equation'], dependencies: [], commands, conditions, preference });
  const review = () => ({ ...rule('access-comparison-coverage', [], facts,
    'Supply a reviewed preference for this access set.'), gap: { kind: 'strategy',
    reason: 'uncovered-access-comparison', message: 'This access set needs a reviewed default and its applicability conditions.',
    candidates: offers.map(row => ({ command: row.command, card: row.card, cost: row.cost,
      draws: row.onPlayDraw.requested })) } });
  if (offers.some(row => !known.has(row.card))) return review();
  if ((state.relics ?? []).some(row => row.relic === 'SneckoEye')) return review();
  const variantFacts = [];
  const groups = [...new Set(offers.map(row => row.card))].map(id => offers.filter(row => row.card === id));
  const retained = [];
  for (const variants of groups) {
    if (variants.length === 1) { retained.push(variants[0]); continue; }
    const resolved = cardVariantComparisonPolicy(observation, variants, normalize);
    if (resolved.gap) return resolved;
    variantFacts.push(resolved.conditions.at(-1));
    retained.push(variants.find(row => row.command === resolved.commands[0]));
  }
  offers = retained;
  facts.push(...variantFacts);
  const pact = offers.find(row => row.card === 'BurningPact' && row.cost === 1 && row.onPlayDraw.exhaustsOther);
  const breath = offers.find(row => row.card === 'DeepBreath' && row.cost === 0);
  const cleanup = cards.filter(row => ['Strike', 'Defend', 'AscendersBane'].includes(row.card.card));
  const cleanupCopies = cleanup.reduce((n, row) => n + row.card.count, 0);
  if (offers.length === 2 && pact && breath && profile.baseEnergy >= 3 && cleanupCopies >= 4
      && pact.onPlayDraw.requested >= breath.onPlayDraw.requested) {
    return rule('access-with-basic-cleanup', [pact.command], [...facts,
      `Owned basic cleanup pool: ${cleanupCopies} copies (${cleanup.map(labelCard).join(', ')}). Base Energy: ${profile.baseEnergy}.`],
    'Default to BurningPact in this access pair: select a weak recurring Hand card for cleanup while reaching the owned functions.');
  }
  const payable = row => row.persistent_cost >= 0 && row.persistent_cost <= profile.baseEnergy;
  const payoffs = cards.filter(row => payable(row)
    && ((row.mechanics.card_type === 'Power' && (row.mechanics.exhaust_draw_power > 0 || row.card.card === 'FeelNoPain'))
      || row.mechanics.energy_gain_on_exhaust > 0));
  if (pact && profile.baseEnergy >= 3 && payoffs.length
      && (!breath || pact.onPlayDraw.requested >= breath.onPlayDraw.requested)) {
    return rule('access-with-exhaust-payoff', [pact.command], [...facts,
      `Owned payable Exhaust payoffs: ${payoffs.map(labelCard).join(', ')}.`],
    'Default to BurningPact for targeted Exhaust and access. Deploy the owned Powers before their triggers; on-Exhaust Energy requires selecting that source card.');
  }
  const trance = offers.find(row => row.card === 'BattleTrance' && row.cost === 0);
  if (trance && profile.baseEnergy >= 3 && !payoffs.length
      && !cards.some(row => ['BattleTrance', 'Corruption'].includes(row.card.card))
      && offers.every(row => row.onPlayDraw.requested <= trance.onPlayDraw.requested)) {
    return rule('first-free-large-draw', [trance.command], facts,
      'Default to the first BattleTrance for this access set. Use other needed draws before BattleTrance; its draw precedes NoDraw for the rest of that turn.');
  }
  return review();
}

function rewardFunctionComparisonPolicy(observation, offers, profile, normalize) {
  if (observation.position.observation.observation.run.act === 1 && offers.length > 1
      && offers.every(row => ['FiendFire', 'Corruption', 'DemonForm'].includes(row.card))
      && new Set(offers.map(row => row.card)).size === offers.length) {
    return { id: 'first-burst-payment-or-growth', origin: 'maintainer_strategy_default',
      sources: ['cx', 'panacea', 'jorbs'], dependencies: [], commands: offers.map(row => row.command), conditions: [],
      preference: 'Default to FiendFire for burst and basic-card cleanup with owned draw. Compare Corruption for Skill payment and DemonForm for protected delayed growth. These are maintainer drafting preferences.',
      question: 'Choose burst cleanup, Skill payment, or protected later Strength from the checked owned support and deployment costs.' };
  }
  if (observation.position.observation.observation.run.act === 1 && offers.length === 2
      && offers.some(row => row.card === 'Uppercut') && offers.some(row => row.card === 'FeelNoPain')) {
    return { id: 'first-weak-or-exhaust-block', origin: 'maintainer_strategy_default',
      sources: ['cx', 'panacea'], dependencies: [], commands: offers.map(row => row.command), conditions: [],
      preference: 'Default to Uppercut for the first recurring Weak source. FeelNoPain is the alternative when the listed owned Exhaust events provide the more useful protection after deployment; sparse single-card triggers give modest immediate Block.',
      question: 'Choose Uppercut for direct Weak coverage, or FeelNoPain for protection from the supplied owned Exhaust fuel. Compare the current Weak duration and payment with actual post-deployment trigger quantity and timing.' };
  }
  if (observation.position.observation.observation.run.act === 1 && offers.length === 2
      && offers.some(row => row.card === 'Armaments') && offers.some(row => row.card === 'Cleave')) {
    return { id: 'hand-upgrades-or-first-area-output', origin: 'maintainer_strategy_default',
      sources: ['cx', 'equation'], dependencies: [], commands: offers.map(row => row.command), conditions: [],
      preference: 'Both first functions meet the checked drafting conditions. Default to Armaments+ to improve the owned basic cards and listed access targets; Cleave adds the larger immediate hit to each enemy.',
      question: 'Choose one supplied function: use Armaments+ for the checked upgrades across combat hands, or Cleave when the first stronger standalone area hit is the more pressing need.' };
  }
  if (offers.length === 2 && offers.some(row => row.card === 'Havoc')
      && offers.some(row => row.card === 'TrueGrit')) {
    return { id: 'planned-free-play-or-protective-cleanup', origin: 'maintainer_strategy_default',
      sources: ['cx', 'equation'], dependencies: ['draw', 'retention'],
      commands: offers.map(row => row.command), conditions: [],
      preference: 'Both first functions fit the checked window. TrueGrit+ adds protection and selects weak Hand cards for cleanup. Havoc+ turns an arranged top card into a free play and applies its printed Exhaust rule.',
      question: 'Choose one: favor TrueGrit+ for protection with controlled cleanup; choose Havoc+ when arranging a useful free top-card play is the stronger need. Include the target\'s later reuse or Exhaust in that choice.' };
  }
  if (offers.length === 2 && offers.some(row => row.card === 'Headbutt')
      && offers.some(row => row.card === 'TrueGrit')) {
    return { id: 'discard-retrieval-or-protective-cleanup', origin: 'maintainer_strategy_default',
      sources: ['cx', 'equation'], dependencies: ['draw', 'exhaust', 'strength', 'retention'],
      commands: offers.map(row => row.command), conditions: [],
      preference: 'Both candidates add a useful first function under the checked conditions. Headbutt schedules another use of a useful Discard card; TrueGrit+ protects while removing weak recurring draws.',
      question: 'Choose one: favor TrueGrit+ when weak recurring cards obstruct setup; choose Headbutt when retrieving an owned repeat-use target is the stronger need. Use the actual selection, draw and payment conditions.' };
  }
  if (offers.length === 2 && offers.some(row => row.card === 'ShrugItOff')
      && offers.some(row => row.card === 'Warcry')) {
    return { id: 'protective-draw-or-hand-planning', origin: 'maintainer_strategy_default',
      sources: ['cx', 'equation'], dependencies: ['draw',
        ...(profile.strengthSources.length ? ['strength'] : []),
        ...(profile.blockSupport.retention.length ? ['retention'] : []),
        ...(profile.exhaustDraw.length ? ['exhaust-draw'] : [])],
      commands: offers.map(row => row.command), conditions: [],
      preference: 'Both supplied Skills fit the checked setup window. ShrugItOff adds recurring protection with a replacement draw; Warcry adds free Hand planning and leaves circulation after play.',
      question: 'Choose one: favor ShrugItOff for protection while reaching setup; choose Warcry when preserving Energy for the owned setup and arranging the next draw is the stronger need.' };
  }
  return cardVariantComparisonPolicy(observation, offers, normalize);
}

export function preparationCardPolicies(observation, profile, normalize = command => command) {
  const state = observation.position.observation.observation;
  const act = state.run?.pending_act_entry ?? state.run?.act;
  const cards = observation.deck_evidence.cards;
  const offers = profile.candidates.filter(row => row.kind === 'addition');
  const payable = row => row.cost >= 0 && row.cost <= profile.baseEnergy;
  const rules = [];
  const add = (id, sources, dependencies, conditions, commands, preference) => {
    if (commands.length) rules.push({ id, origin: 'maintainer_strategy_default', sources,
      dependencies, conditions, commands, preference });
  };
  const engines = [...profile.strengthSources, ...profile.blockSupport.retention, ...profile.exhaustDraw];
  const payment = [...profile.onPlayEnergy, ...profile.skillExhaustPowers];
  const weakCoverage = cards.filter(row => row.mechanics.weak > 0 && row.mechanics.exhausts === false
    && row.persistent_cost >= 0 && row.persistent_cost <= profile.baseEnergy);
  const existingAttacks = payableTransitions(cards, profile.baseEnergy);
  const enduringStrength = profile.strengthSources.filter(row => row.cost >= 0 && row.cost <= profile.baseEnergy);
  const accessibleEngines = engines.filter(row => row.cost >= 0 && row.cost <= profile.baseEnergy);
  if (act >= 2 && act <= 3 && profile.baseEnergy >= 3 && accessibleEngines.length
      && !cards.some(row => row.card.card === 'Offering')
      && Number.isSafeInteger(state.run?.current_hp) && Number.isSafeInteger(state.run?.max_hp)
      && state.run.max_hp > 0 && state.run.current_hp >= Math.ceil(state.run.max_hp * 0.75)
      && !(state.relics ?? []).some(row => ['SneckoEye', 'VelvetChoker'].includes(row.relic))) {
    const burstAccess = offers.filter(row => row.card === 'Offering' && row.type === 'Skill'
      && row.cost === 0 && row.exhausts === true && expandsHandAccess(row.onPlayDraw)
      && row.onPlayDraw.requested >= 3 && row.onPlayDraw.topdeck === 0
      && row.onPlayEnergy?.amount >= 2 && Number.isSafeInteger(row.selfHpLoss?.base_hp_loss)
      && row.selfHpLoss.base_hp_loss > 0 && row.selfHpLoss.base_hp_loss <= state.run.max_hp / 4);
    add('first-hp-funded-burst-access', ['cx', 'panacea'], [], [
      `Preparation Act ${act}; HP ${state.run.current_hp}/${state.run.max_hp}; base Energy ${profile.baseEnergy}. Owned Offering: 0.`,
      `Owned payable setup: ${accessibleEngines.map(row => row.card).join(', ')}.`,
      ...burstAccess.map(row => `${row.command}: 0 Energy; base HP loss ${row.selfHpLoss.base_hp_loss}; gain ${row.onPlayEnergy.amount} Energy; request ${row.onPlayDraw.requested} draws; Exhaust after play.`),
    ], burstAccess.map(row => row.command),
    'Default to a first Offering for immediate access and payment to deploy or use the owned functions. The checked HP reserve supports its base loss; choose when to play it using actual combat HP and modifiers. Its HP loss and once-per-combat lifecycle remain part of the tradeoff.');
  }
  const paidSingleWeak = weakCoverage.filter(row => row.persistent_cost > 0
    && row.mechanics.weak >= 2 && row.mechanics.play_target === 'one_enemy');
  const freeGroupWeak = cards.filter(row => row.persistent_cost === 0 && row.mechanics.weak > 0
    && row.mechanics.play_target === 'all_enemies');
  const paidSetup = engines.filter(row => row.cost >= 2 && row.cost <= profile.baseEnergy);
  if (act >= 2 && act <= 3 && profile.baseEnergy >= 3 && paidSingleWeak.length
      && paidSetup.length && !freeGroupWeak.length
      && !(state.relics ?? []).some(row => ['SneckoEye', 'VelvetChoker'].includes(row.relic))) {
    const freeWeak = offers.filter(row => row.card === 'Intimidate' && row.type === 'Skill'
      && row.cost === 0 && row.enemyWeak >= 2 && row.enemyWeakTarget === 'all_enemies' && row.exhausts === true);
    add('first-free-group-weak', ['cx', 'panacea'], [], [
      `Preparation Act ${act}; base Energy ${profile.baseEnergy}. Owned zero-Energy group Weak: 0.`,
      `Owned payable setup costing at least 2 Energy: ${paidSetup.map(row => row.card).join(', ')}.`,
      `Owned paid recurring single-target Weak: ${paidSingleWeak.map(labelCard).join(', ')}.`,
      ...freeWeak.map(row => `${row.command}: 0 Energy; ${row.enemyWeak} Weak turns to every enemy; Exhaust after play.`),
    ], freeWeak.map(row => row.command),
    'Default to a first free two-turn group Weak to protect paid setup while preserving Energy. It complements the owned paid single-target coverage, then leaves ordinary redraws through Exhaust. Enemy debuff prevention applies to each target; later Weak uses the owned recurring source.');
  }
  if (act >= 2 && act <= 3 && enduringStrength.length
      && existingAttacks.reduce((n, row) => n + row.card.count, 0) >= 3 && !weakCoverage.length
      && !(state.relics ?? []).some(row => row.relic === 'SneckoEye')) {
    const firstWeak = offers.filter(row => ['Uppercut', 'Clothesline'].includes(row.card)
      && row.type === 'Attack' && row.cost >= 0 && row.cost <= 2 && row.exhausts === false
      && row.enemyWeak >= 2 && row.cost + 1 <= profile.baseEnergy);
    add('first-reusable-multiturn-weak', ['cx', 'panacea'], [], [
      `Preparation Act ${act}; base Energy ${profile.baseEnergy}. Owned payable reusable Weak: 0.`,
      `Owned payable Strength: ${enduringStrength.map(row => row.card).join(', ')}; transition attacks: ${existingAttacks.reduce((n, row) => n + row.card.count, 0)}.`,
      ...firstWeak.map(row => `${row.command}: ${row.cost} Energy; ${row.enemyWeak} Weak turns; ordinary redraw permits another application.`),
    ], firstWeak.map(row => row.command),
    'Default to a first payable multi-turn Weak attack for setup and later protection. Its printed Weak applies after its own hit, reduces subsequent enemy Attack damage and follows enemy debuff prevention. Reaching and paying the card again refreshes coverage.');
  }
  const recoveredProtection = cards.filter(row => row.mechanics.exhausts === true
    && row.mechanics.intangible_gain > 0 && row.mechanics.intangible_damage_cap === 1
    && row.persistent_cost >= 0 && row.persistent_cost <= profile.baseEnergy);
  const recoveredHealing = cards.filter(row => row.card.card === 'Reaper' && row.mechanics.exhausts === true
    && row.persistent_cost >= 0 && row.persistent_cost <= profile.baseEnergy);
  if (act >= 2 && profile.strengthSources.some(row => row.cost >= 0 && row.cost <= profile.baseEnergy)
      && !cards.some(row => row.card.card === 'Exhume')
      && recoveredProtection.reduce((n, row) => n + row.card.count, 0) >= 2 && recoveredHealing.length
      && !(state.relics ?? []).some(row => ['SneckoEye', 'VelvetChoker'].includes(row.relic))) {
    add('recover-owned-protection-and-healing', ['cx', 'je'], [], [
      `Owned playable Exhaust targets: ${[...recoveredProtection, ...recoveredHealing].map(labelCard).join(', ')}. Owned Exhume: 0 copies.`,
    ], offers.filter(row => row.card === 'Exhume' && row.type === 'Skill' && row.cost === 0
      && row.exhausts === true && row.exhaustRecovery?.count === 1
      && row.exhaustRecovery.excluded_card === 'Exhume').map(row => row.command),
    'Default to a first zero-Energy Exhume to reuse the owned Intangible protection or Strength-supported Reaper. Select a target after it enters Exhaust, return it to Hand and pay for its next play.');
  }
  if (act >= 2 && profile.onPlayDrawCoverage.copies <= 1 && (engines.length || payment.length)
      && !offers.some(row => row.card === 'Offering' || row.selfHpLoss)) {
    // HP-funded access uses its reserve policy above; ordinary access cannot
    // bypass that check when Offering is the only draw offer.
    const accessOffers = offers.filter(row => payable(row) && expandsHandAccess(row.onPlayDraw));
    add('reach-owned-functions', ['cx', 'equation'], ['draw',
      ...(profile.strengthSources.length ? ['strength'] : []),
      ...(profile.blockSupport.retention.length ? ['retention'] : []),
      ...(payment.length ? ['payment'] : []), ...(profile.exhaustDraw.length ? ['exhaust-draw'] : [])], [
      `Preparation Act ${act}.`,
      `Owned on-play multi-draw copies: ${profile.onPlayDrawCoverage.copies} (${profile.onPlayDrawCoverage.owned}).`,
    ], accessOffers.map(row => row.command),
    'Default to the offered on-play multi-draw options to reach the owned functions.');
  }
  if (act >= 2 && profile.reusableOtherCardExhaustCopies > 0 && profile.onPlayDrawCoverage.copies === 0
      && profile.exhaustDraw.length === 0) {
    add('first-exhaust-draw', ['je', 'cx'], ['exhaust', ...(profile.onPlayDraw.length ? ['draw'] : [])], [
      'Owned Exhaust-draw Powers: 0. Owned on-play multi-draw copies: 0.',
    ], offers.filter(row => payable(row) && row.drawPerExhaustAfterDeployment > 0).map(row => row.command),
    'Default to a first Exhaust-draw Power for the owned reusable Exhaust route; pay for deployment before its draw triggers.');
  }
  const multiHit = cards.filter(row => row.mechanics.damage?.delivery?.kind === 'attack'
    && (row.mechanics.damage.delivery.hits?.count > 1 || row.mechanics.damage.delivery.hits?.kind === 'per_effective_x'));
  const ownsStrengthRelic = (state.relics ?? []).some(row => strengthRelics.has(row.relic));
  if (act >= 2 && !profile.strengthSources.length && !ownsStrengthRelic && multiHit.length) {
    const recurring = offers.filter(row => payable(row) && row.enduringStrength?.delivery === 'each_player_turn_start_post_draw');
    add('first-recurring-strength', ['cx', 'panacea'], ['multi-hit'], [
      'Owned native enduring Strength sources: 0.',
    ], recurring.map(row => row.command),
    'Default to a first payable recurring Strength source for the owned multi-hit attacks. Protect deployment and use attacks after the actual Strength gains.');
  }
  if (!rules.length && state.context.kind === 'card_reward'
      && profile.candidates.some(row => row.kind === 'preserve')) {
    rules.push(ordinaryRewardPolicy(state, cards, offers, profile));
  }
  if (rules.length) {
    const common = rules[0].commands.filter(command => rules.every(rule => rule.commands.includes(command)));
    if (common.length > 1 && !rules.some(rule => rule.question)) {
      const remaining = offers.filter(row => common.includes(row.command));
      rules.push(rules.some(rule => rule.id === 'reach-owned-functions')
        ? accessComparisonPolicy(observation, cards, remaining, profile, normalize)
        : rewardFunctionComparisonPolicy(observation, remaining, profile, normalize));
    }
  }
  const resource = profile.candidates.find(row => row.resource?.kind === 'singing_bowl');
  if (resource && rules.length && !rules.some(rule => rule.gap)) {
    const common = rules[0].commands.filter(command => rules.every(rule => rule.commands.includes(command)));
    // Preserve a real conflict for review. The HP alternative cannot resolve
    // incompatible card policies simply by appearing in their intersection.
    if (common.length) {
      const onlyPreserve = common.every(command => profile.candidates.find(row => row.command === command)?.kind === 'preserve');
      for (const rule of rules) {
        rule.defaultCommands = onlyPreserve ? [resource.command] : [...rule.commands];
        rule.commands = onlyPreserve ? [resource.command] : [...new Set([...rule.commands, resource.command])];
      }
      rules.push({ id: 'singing-bowl-reward-alternative', origin: 'maintainer_comparison_preference',
        sources: [], dependencies: [], conditions: [],
        commands: onlyPreserve ? [resource.command] : [...common, resource.command],
        preference: onlyPreserve ? 'Take the permanent HP gain when the checked card policies preserve the deck.'
          : 'Compare the supplied useful card function with permanent HP and keeping the current deck.',
        ...(!onlyPreserve ? { question: 'Is the supplied card function worth an additional permanent card here, compared with the displayed immediate HP gain? Use the checked strategy conditions and current support.' } : {}) });
    }
  }
  return rules;
}

const labelCard = row => `${row.card.count}x ${row.card.card}${row.card.upgrades > 0 ? '+' : ''}`;

export function renderCandidateExpertGuidance(candidate, { includeCommand = true, locale = 'en' } = {}) {
  return candidate?.preference
    ? strategyText(locale, 'offeredCardGuidance', { card: candidate.card, commandSuffix: includeCommand ? ` (${candidate.command})` : '', preference: candidate.preference, change: candidate.change }) : '';
}

export function renderExpertGuidance(guidance, { includeCandidates = true, locale = 'en' } = {}) {
  if (!guidance) return '';
  // Matching IDs stay in structured audit data. The player receives the actual
  // relationship once, rather than candidate tags that resemble a ranking.
  const lines = [strategyText(locale, 'guidanceHeading'),
    ...guidance.inventory.map(text => strategyText(locale, 'guidanceCurrentFacts', { text: text })),
    ...guidance.policies.map(row => strategyText(locale, 'guidanceCurrentSite', { preference: row.preference, change: row.change })),
    ...(guidance.construction?.priorities.length ? [strategyText(locale, 'guidanceOwnedFunctions'),
      ...guidance.construction.priorities.flatMap(row => [
        ...(row.ownedFacts.length ? [strategyText(locale, 'guidanceOwnedBasis', { facts: row.ownedFacts.join('; ') })] : []),
        strategyText(locale, 'guidanceDevelopmentRule', { preference: row.preference }),
      ])] : []),
    ...(guidance.families ?? []).map(row => `  ${row.text}`),
    ...(includeCandidates ? guidance.candidates.map(candidate => renderCandidateExpertGuidance(candidate, { locale })).filter(Boolean).map(text => `  ${text}`) : []),
    strategyText(locale, 'guidanceExpertBasis', { authors: Object.values(guidance.sources).map(row => row.author).join(', ') })];
  return lines.join('\n');
}
