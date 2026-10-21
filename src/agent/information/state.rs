//! Complete public combat information for a deployed policy and value model.
//!
//! This projection is deliberately explicit. It must not serialize
//! `CombatState`, RNG pools, queued engine actions, or simulator-only runtime
//! bundles as a shortcut.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::ai::planner_core::PlannerPlayerClass;
use crate::content::cards::{evaluate_card_for_play, get_card_definition, CardId, CardType};
use crate::content::monsters::EnemyId;
use crate::content::potions::PotionId;
use crate::content::powers::PowerId;
use crate::content::relics::RelicState;
use crate::runtime::combat::{
    CardLineageId, CombatCard, CombatPhase, CombatState, EphemeralCounters, Intent, MonsterEntity,
    OrbEntity, OrbId, Power, PowerPayload, StanceId,
};

use crate::agent::information::combat::{
    combat_public_draw_evidence_v1, combat_public_hidden_reasons_v1, combat_public_intent_facts_v1,
    HiddenInformationReasonV1, InformationAccessV1, ObservationEvidenceKindV1,
};

pub const PUBLIC_COMBAT_STATE_SCHEMA_NAME: &str = "PublicCombatState";
pub const PUBLIC_COMBAT_STATE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatStateV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub information_access: InformationAccessV1,
    pub potions: Vec<Option<CombatLearningPotionV1>>,
    pub hidden_reasons: Vec<HiddenInformationReasonV1>,
    pub encounter: CombatLearningEncounterV1,
    pub turn: CombatLearningTurnV1,
    pub player: CombatLearningPlayerStateV1,
    pub cards: CombatLearningCardZonesV1,
    pub monsters: Vec<CombatLearningMonsterStateV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningEncounterV1 {
    pub is_boss_fight: bool,
    pub is_elite_fight: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningTurnV1 {
    pub turn_count: u32,
    pub phase: CombatPhase,
    /// Escape has been committed; the remaining EndTurn settles it rather
    /// than opening another tactical turn.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub player_escaping: bool,
    pub energy: u8,
    pub turn_start_draw_modifier: i32,
    pub counters: CombatLearningTurnCountersV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningTurnCountersV1 {
    pub cards_played_this_turn: u8,
    pub attacks_played_this_turn: u8,
    pub cards_discarded_this_turn: u16,
    pub card_ids_played_this_turn: Vec<CardId>,
    pub card_ids_played_this_combat: Vec<CardId>,
    pub orbs_channeled_this_turn: Vec<OrbId>,
    pub orbs_channeled_this_combat: Vec<OrbId>,
    pub mantra_gained_this_combat: i32,
    pub times_damaged_this_combat: u8,
    pub discovery_cost_for_turn: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningPlayerStateV1 {
    pub player_class: Option<PlannerPlayerClass>,
    pub ascension_level: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub facing_left: bool,
    pub gold: i32,
    pub gold_delta_this_combat: i32,
    pub energy_master: u8,
    pub max_orbs: u8,
    pub stance: StanceId,
    pub orbs: Vec<CombatLearningOrbV1>,
    pub relics: Vec<RelicState>,
    pub powers: Vec<CombatLearningPowerV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningPotionV1 {
    pub potion_id: PotionId,
    pub can_use: bool,
    pub can_discard: bool,
    pub requires_target: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningOrbV1 {
    pub orb: OrbId,
    pub base_passive_amount: i32,
    pub base_evoke_amount: i32,
    pub passive_amount: i32,
    pub evoke_amount: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningMonsterStateV1 {
    pub slot: u8,
    pub enemy: CombatLearningEnemyIdentityV1,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub escaped: bool,
    pub dying: bool,
    pub half_dead: bool,
    pub intent: CombatLearningIntentV1,
    pub executed_moves: CombatLearningMonsterMoveHistoryV1,
    pub public_counters: Vec<CombatLearningMonsterPublicCounterV1>,
    pub powers: Vec<CombatLearningPowerV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningMonsterMoveHistoryV1 {
    /// Move ids are encounter-local and are namespaced by the monster's
    /// `enemy` identity. Only moves that actually began execution appear here.
    pub evidence: ObservationEvidenceKindV1,
    pub move_ids: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatLearningMonsterPublicCounterV1 {
    HexaghostActiveOrbs { count: u8 },
    StolenGold { amount: i32 },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CombatLearningEnemyIdentityV1 {
    Known { enemy_id: EnemyId },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningIntentV1 {
    pub evidence: ObservationEvidenceKindV1,
    pub intent: Option<Intent>,
    pub preview_damage_per_hit: Option<i32>,
    pub hidden_reason: Option<HiddenInformationReasonV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningPowerV1 {
    pub power: PowerId,
    pub amount: i32,
    pub extra_data: i32,
    pub just_applied: bool,
    pub payload_card: Option<CombatLearningCardV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_creature: Option<CombatLearningCreatureV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CombatLearningCreatureV1 {
    Player,
    Monster { monster_index: usize },
    Unobserved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningCardZonesV1 {
    pub master_deck: CombatLearningCardCollectionV1,
    pub hand: CombatLearningCardCollectionV1,
    pub draw: CombatLearningCardCollectionV1,
    pub discard: CombatLearningCardCollectionV1,
    pub exhaust: CombatLearningCardCollectionV1,
    pub limbo: CombatLearningCardCollectionV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningCardCollectionV1 {
    pub evidence: ObservationEvidenceKindV1,
    pub cards: Vec<CombatLearningCardV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningCardV1 {
    pub card_id: CardId,
    pub upgrades: u8,
    pub misc_value: i32,
    pub base_damage_override: Option<i32>,
    pub base_block_override: Option<i32>,
    pub cost_modifier: i8,
    pub cost_for_turn: Option<u8>,
    pub effective_cost: i32,
    pub current_damage: i32,
    pub current_block: i32,
    pub current_magic_number: i32,
    pub damage_by_monster_order: Vec<i32>,
    pub exhaust_override: Option<bool>,
    pub retain_override: Option<bool>,
    pub free_to_play_once: bool,
    pub energy_on_use: i32,
}

/// Accepts the single maintained replay migration from the original public
/// combat schema to V2. V1 exposed the private lineage stored by Stasis and did
/// not bind the visibly held card to its monster. V2 replaces that lineage with
/// the typed public payload. No other observation difference is compatible.
pub(crate) fn public_combat_state_replay_compatible_v2(
    expected: &PublicCombatStateV1,
    observed: &PublicCombatStateV1,
) -> bool {
    if expected == observed {
        return true;
    }
    if expected.schema_name != PUBLIC_COMBAT_STATE_SCHEMA_NAME
        || observed.schema_name != PUBLIC_COMBAT_STATE_SCHEMA_NAME
        || expected.schema_version != 1
        || observed.schema_version != PUBLIC_COMBAT_STATE_SCHEMA_VERSION
    {
        return false;
    }

    let mut migrated_expected = expected.clone();
    let mut normalized_observed = observed.clone();
    migrated_expected.schema_version = PUBLIC_COMBAT_STATE_SCHEMA_VERSION;
    normalize_stasis_payload_for_replay(&mut migrated_expected);
    normalize_stasis_payload_for_replay(&mut normalized_observed);
    migrated_expected == normalized_observed
}

fn normalize_stasis_payload_for_replay(observation: &mut PublicCombatStateV1) {
    for power in observation
        .player
        .powers
        .iter_mut()
        .chain(
            observation
                .monsters
                .iter_mut()
                .flat_map(|monster| monster.powers.iter_mut()),
        )
        .filter(|power| power.power == PowerId::Stasis)
    {
        power.extra_data = 0;
        power.payload_card = None;
    }
}

impl Ord for CombatLearningCardV1 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.card_id
            .stable_ordinal()
            .cmp(&other.card_id.stable_ordinal())
            .then_with(|| self.upgrades.cmp(&other.upgrades))
            .then_with(|| self.misc_value.cmp(&other.misc_value))
            .then_with(|| self.base_damage_override.cmp(&other.base_damage_override))
            .then_with(|| self.base_block_override.cmp(&other.base_block_override))
            .then_with(|| self.cost_modifier.cmp(&other.cost_modifier))
            .then_with(|| self.cost_for_turn.cmp(&other.cost_for_turn))
            .then_with(|| self.effective_cost.cmp(&other.effective_cost))
            .then_with(|| self.current_damage.cmp(&other.current_damage))
            .then_with(|| self.current_block.cmp(&other.current_block))
            .then_with(|| self.current_magic_number.cmp(&other.current_magic_number))
            .then_with(|| {
                self.damage_by_monster_order
                    .cmp(&other.damage_by_monster_order)
            })
            .then_with(|| self.exhaust_override.cmp(&other.exhaust_override))
            .then_with(|| self.retain_override.cmp(&other.retain_override))
            .then_with(|| self.free_to_play_once.cmp(&other.free_to_play_once))
            .then_with(|| self.energy_on_use.cmp(&other.energy_on_use))
    }
}

impl PartialOrd for CombatLearningCardV1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn public_combat_state_v1(combat: &CombatState) -> PublicCombatStateV1 {
    let draw_evidence = combat_public_draw_evidence_v1(combat);
    let player = &combat.entities.player;
    let monsters = combat
        .entities
        .monsters
        .iter()
        .map(|monster| {
            let intent = combat_public_intent_facts_v1(combat, monster.id);
            CombatLearningMonsterStateV1 {
                slot: monster.slot,
                enemy: CombatLearningEnemyIdentityV1::Known {
                    enemy_id: monster.enemy_id,
                },
                hp: monster.current_hp,
                max_hp: monster.max_hp,
                block: monster.block,
                alive: monster.is_alive_for_action(),
                escaped: monster.is_escaped,
                dying: monster.is_dying,
                half_dead: monster.half_dead,
                intent: CombatLearningIntentV1 {
                    evidence: intent.evidence,
                    intent: intent.intent,
                    preview_damage_per_hit: intent.preview_damage_per_hit,
                    hidden_reason: intent.hidden_reason,
                },
                executed_moves: CombatLearningMonsterMoveHistoryV1 {
                    evidence: ObservationEvidenceKindV1::PublicOrderedCollection,
                    move_ids: combat
                        .monster_protocol_executed_move_history(monster.id)
                        .to_vec(),
                },
                public_counters: learning_monster_public_counters(monster),
                powers: learning_powers(combat, monster.id),
            }
        })
        .collect();

    PublicCombatStateV1 {
        schema_name: PUBLIC_COMBAT_STATE_SCHEMA_NAME.to_string(),
        schema_version: PUBLIC_COMBAT_STATE_SCHEMA_VERSION,
        information_access: InformationAccessV1::Public,
        potions: combat
            .entities
            .potions
            .iter()
            .map(|slot| {
                slot.as_ref().map(|potion| CombatLearningPotionV1 {
                    potion_id: potion.id,
                    can_use: potion.can_use,
                    can_discard: potion.can_discard,
                    requires_target: potion.requires_target,
                })
            })
            .collect(),
        hidden_reasons: combat_public_hidden_reasons_v1(combat),
        encounter: CombatLearningEncounterV1 {
            is_boss_fight: combat.meta.is_boss_fight,
            is_elite_fight: combat.meta.is_elite_fight,
        },
        turn: CombatLearningTurnV1 {
            turn_count: combat.turn.turn_count,
            phase: combat.turn.current_phase,
            player_escaping: combat.turn.counters.player_escaping,
            energy: combat.turn.energy,
            turn_start_draw_modifier: combat.turn.turn_start_draw_modifier,
            counters: learning_turn_counters(&combat.turn.counters),
        },
        player: CombatLearningPlayerStateV1 {
            player_class: learning_player_class(&combat.meta.player_class),
            ascension_level: combat.meta.ascension_level,
            hp: player.current_hp,
            max_hp: player.max_hp,
            block: player.block,
            facing_left: player.facing_left,
            gold: player.gold,
            gold_delta_this_combat: player.gold_delta_this_combat,
            energy_master: player.energy_master,
            max_orbs: player.max_orbs,
            stance: player.stance,
            orbs: player.orbs.iter().map(learning_orb).collect(),
            relics: player.relics.clone(),
            powers: learning_powers(combat, player.id),
        },
        cards: CombatLearningCardZonesV1 {
            master_deck: unordered_cards(combat, combat.meta.master_deck_snapshot.iter()),
            hand: ordered_cards(
                combat,
                combat.zones.hand.iter(),
                ObservationEvidenceKindV1::PublicOrderedCollection,
                true,
            ),
            draw: if draw_evidence == ObservationEvidenceKindV1::PublicOrderedCollection {
                ordered_cards(combat, combat.zones.draw_pile.iter(), draw_evidence, false)
            } else {
                unordered_cards_with_evidence(combat, combat.zones.draw_pile.iter(), draw_evidence)
            },
            discard: unordered_cards(combat, combat.zones.discard_pile.iter()),
            exhaust: unordered_cards(combat, combat.zones.exhaust_pile.iter()),
            limbo: ordered_cards(
                combat,
                combat.zones.limbo.iter().filter(|card| {
                    // Rust stores stolen cards here; the original game keeps
                    // them only on Stasis. They are exposed through that power's
                    // payload, not as cards whose play is currently resolving.
                    !combat
                        .entities
                        .monsters
                        .iter()
                        .filter(|m| !m.is_dead_or_escaped())
                        .any(|monster| {
                            combat
                                .entities
                                .power_db
                                .get(&monster.id)
                                .into_iter()
                                .flatten()
                                .any(|power| {
                                    power.power_type == PowerId::Stasis
                                        && crate::content::powers::core::stasis::held_card_lineage(
                                            power.extra_data,
                                        ) == Some(card.lineage)
                                })
                        })
                }),
                ObservationEvidenceKindV1::VisibleExact,
                true,
            ),
        },
        monsters,
    }
}

/// Resolves one exact master-deck card identity to its occurrence ordinal in
/// the public unordered master-deck projection. The lineage is consumed only at
/// this private projection boundary and is never returned to the actor.
pub fn public_master_deck_ordinal_for_lineage_v1(
    combat: &CombatState,
    card_lineage: CardLineageId,
) -> Option<usize> {
    let target = combat
        .meta
        .master_deck_snapshot
        .iter()
        .find(|card| card.lineage == card_lineage)
        .map(|card| learning_card(combat, card, false))?;
    let mut cards = combat
        .meta
        .master_deck_snapshot
        .iter()
        .map(|card| learning_card(combat, card, false))
        .collect::<Vec<_>>();
    cards.sort();
    // Mechanically identical occurrences have the same public consequence
    // when one is modified. Canonicalizing to the first equivalent public
    // ordinal prevents their private source order from entering the outcome.
    cards.iter().position(|card| card == &target)
}

fn learning_monster_public_counters(
    monster: &MonsterEntity,
) -> Vec<CombatLearningMonsterPublicCounterV1> {
    match monster.enemy_id {
        EnemyId::Hexaghost => {
            vec![CombatLearningMonsterPublicCounterV1::HexaghostActiveOrbs {
                count: monster.hexaghost.orb_active_count,
            }]
        }
        EnemyId::Looter | EnemyId::Mugger => {
            vec![CombatLearningMonsterPublicCounterV1::StolenGold {
                amount: monster.thief.stolen_gold,
            }]
        }
        _ => Vec::new(),
    }
}

fn learning_turn_counters(counters: &EphemeralCounters) -> CombatLearningTurnCountersV1 {
    CombatLearningTurnCountersV1 {
        cards_played_this_turn: counters.cards_played_this_turn,
        attacks_played_this_turn: counters.attacks_played_this_turn,
        cards_discarded_this_turn: counters.cards_discarded_this_turn,
        card_ids_played_this_turn: counters.card_ids_played_this_turn.to_vec(),
        card_ids_played_this_combat: counters.card_ids_played_this_combat.to_vec(),
        orbs_channeled_this_turn: counters.orbs_channeled_this_turn.clone(),
        orbs_channeled_this_combat: counters.orbs_channeled_this_combat.clone(),
        mantra_gained_this_combat: counters.mantra_gained_this_combat,
        times_damaged_this_combat: counters.times_damaged_this_combat,
        discovery_cost_for_turn: counters.discovery_cost_for_turn,
    }
}

fn learning_player_class(player_class: &str) -> Option<PlannerPlayerClass> {
    match player_class {
        "Ironclad" => Some(PlannerPlayerClass::Ironclad),
        "Silent" => Some(PlannerPlayerClass::Silent),
        "Defect" => Some(PlannerPlayerClass::Defect),
        "Watcher" => Some(PlannerPlayerClass::Watcher),
        _ => None,
    }
}

fn learning_orb(orb: &OrbEntity) -> CombatLearningOrbV1 {
    CombatLearningOrbV1 {
        orb: orb.id,
        base_passive_amount: orb.base_passive_amount,
        base_evoke_amount: orb.base_evoke_amount,
        passive_amount: orb.passive_amount,
        evoke_amount: orb.evoke_amount,
    }
}

fn learning_powers(combat: &CombatState, entity_id: usize) -> Vec<CombatLearningPowerV1> {
    combat
        .entities
        .power_db
        .get(&entity_id)
        .into_iter()
        .flatten()
        .map(|power| learning_power(combat, power))
        .collect()
}

fn learning_power(combat: &CombatState, power: &Power) -> CombatLearningPowerV1 {
    let (extra_data, payload_card) = match &power.payload {
        PowerPayload::Card(card) => (power.extra_data, Some(learning_card(combat, card, false))),
        PowerPayload::None if power.power_type == PowerId::Stasis => {
            let card = crate::content::powers::core::stasis::held_card_lineage(power.extra_data)
                .and_then(|lineage| {
                    combat
                        .zones
                        .limbo
                        .iter()
                        .find(|card| card.lineage == lineage)
                })
                .map(|card| learning_card(combat, card, false));
            (0, card)
        }
        PowerPayload::None | PowerPayload::SourceEntity(_) => (power.extra_data, None),
    };
    CombatLearningPowerV1 {
        power: power.power_type,
        amount: power.amount,
        extra_data,
        just_applied: power.just_applied,
        payload_card,
        source_creature: match power.payload {
            PowerPayload::SourceEntity(0) => Some(CombatLearningCreatureV1::Player),
            PowerPayload::SourceEntity(source) => Some(
                combat
                    .entities
                    .monsters
                    .iter()
                    .position(|monster| monster.id == source)
                    .map(|monster_index| CombatLearningCreatureV1::Monster { monster_index })
                    .unwrap_or(CombatLearningCreatureV1::Unobserved),
            ),
            _ => None,
        },
    }
}

fn ordered_cards<'a>(
    combat: &CombatState,
    cards: impl Iterator<Item = &'a CombatCard>,
    evidence: ObservationEvidenceKindV1,
    include_damage_by_monster_order: bool,
) -> CombatLearningCardCollectionV1 {
    CombatLearningCardCollectionV1 {
        evidence,
        cards: cards
            .map(|card| learning_card(combat, card, include_damage_by_monster_order))
            .collect(),
    }
}

fn unordered_cards<'a>(
    combat: &CombatState,
    cards: impl Iterator<Item = &'a CombatCard>,
) -> CombatLearningCardCollectionV1 {
    unordered_cards_with_evidence(
        combat,
        cards,
        ObservationEvidenceKindV1::PublicUnorderedCollection,
    )
}

fn unordered_cards_with_evidence<'a>(
    combat: &CombatState,
    cards: impl Iterator<Item = &'a CombatCard>,
    evidence: ObservationEvidenceKindV1,
) -> CombatLearningCardCollectionV1 {
    let mut cards = cards
        .map(|card| learning_card(combat, card, false))
        .collect::<Vec<_>>();
    cards.sort();
    CombatLearningCardCollectionV1 { evidence, cards }
}

pub(crate) fn learning_card(
    combat: &CombatState,
    card: &CombatCard,
    include_damage_by_monster_order: bool,
) -> CombatLearningCardV1 {
    // `CombatCard` keeps Java-style rendering caches, but production combat
    // execution does not guarantee that applyPowers() populated them before a
    // learning observation is requested. Project from a fresh clone through
    // the same mechanism pipeline used by actual card play instead of exposing
    // stale cache values or mutating the live card.
    let evaluated = evaluate_card_for_play(card, combat, None);
    let definition = get_card_definition(card.id);
    let damage_by_monster_order = if !include_damage_by_monster_order
        || definition.card_type != CardType::Attack
    {
        Vec::new()
    } else if definition.is_multi_damage {
        evaluated.multi_damage.iter().copied().collect()
    } else {
        combat
            .entities
            .monsters
            .iter()
            .map(|monster| evaluate_card_for_play(card, combat, Some(monster.id)).base_damage_mut)
            .collect()
    };

    CombatLearningCardV1 {
        card_id: card.id,
        upgrades: card.upgrades,
        misc_value: card.misc_value,
        base_damage_override: card.base_damage_override,
        base_block_override: card.base_block_override,
        cost_modifier: card.cost_modifier,
        cost_for_turn: card.cost_for_turn,
        effective_cost: card.cost_for_turn_java(),
        current_damage: evaluated.base_damage_mut,
        // Java-style applyPowers updates the numeric block cache even for a
        // card whose base Block field is not delivered on play (for example,
        // Shockwave under Dexterity). Only reviewed base-Block delivery rules
        // may turn that cache into a public Block claim. An absent rule is
        // still unknown, not proof that the card has no other Block effect.
        current_block: if crate::content::cards::mechanics::card_base_block_delivery_v1(card.id)
            .is_some()
        {
            evaluated.base_block_mut
        } else {
            0
        },
        current_magic_number: evaluated.base_magic_num_mut,
        damage_by_monster_order,
        exhaust_override: card.exhaust_override,
        retain_override: card.retain_override,
        free_to_play_once: card.free_to_play_once,
        energy_on_use: card.energy_on_use,
    }
}

#[cfg(test)]
mod tests;
