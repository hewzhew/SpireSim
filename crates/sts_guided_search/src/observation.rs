//! Explicit gameplay projection for fixed-oracle experiments. This is an
//! observation for proposals, not a complete state identity or a NoSL view.

mod monster;
pub(crate) use monster::decision as monster_decision;
pub use monster::{MonsterDecisionObservation, MonsterFact, MonsterFactId};
pub mod consequences;

use serde::{Deserialize, Serialize};
use sts_core::content::cards::{evaluate_card_for_play, get_card_definition, CardId, CardType};
use sts_core::content::identity::gameplay_display_label_v1 as name;
use sts_core::content::monsters::EnemyId;
use sts_core::content::potions::PotionId;
use sts_core::content::powers::{uses_sentinel_amount, PowerId};
use sts_core::content::relics::RelicId;
use sts_core::runtime::combat::{
    CardLineageId, CombatCard, CombatPhase, CombatState, OrbEntity, OrbId, PowerPayload, StanceId,
};
use sts_core::sim::combat::{CombatStepper, EngineCombatStepper};
use sts_core::sim::combat_action_surface::{
    CombatIndexedChoiceCandidateV2, CombatIndexedChoiceReasonV2, CombatSelectionReasonV2,
};
use sts_core::sim::combat_projection::{
    project_monster_move_preview_in_combat, MonsterMovePreview,
};
use sts_core::state::core::{ClientInput, PileType};

use crate::selection::SelectionDomain;
use crate::{CombatBranch, CombatSearchState, SelectionMember};

fn zero<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CombatObservation {
    pub player_class: String,
    pub ascension: u8,
    pub turn: u32,
    pub phase: CombatPhase,
    pub hp: [i32; 2],
    #[serde(skip_serializing_if = "zero")]
    pub block: i32,
    pub energy: u8,
    pub energy_per_turn: u8,
    pub gold: i32,
    pub facing_left: bool,
    pub potion_uses: usize,
    pub stance: StanceId,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub orbs: Vec<OrbEntity>,
    pub max_orbs: u8,
    pub powers: Vec<PowerObservation>,
    pub relics: Vec<RelicObservation>,
    pub potions: Vec<Option<PotionObservation>>,
    pub monsters: Vec<MonsterObservation>,
    pub hand: Vec<CardObservation>,
    /// Every pile retains simulator order. In particular, index 0 draws next.
    pub draw_top_first: Vec<CardObservation>,
    pub discard: Vec<CardObservation>,
    pub exhaust: Vec<CardObservation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub limbo: Vec<CardObservation>,
    pub counters: TurnCounters,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub selections: Vec<SelectionObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_choice_reason: Option<CombatIndexedChoiceReasonV2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assembling: Option<SelectionAssembly>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TurnCounters {
    pub played: u8,
    pub attacks: u8,
    pub discarded: u16,
    pub draw_modifier_next_turn: i32,
    pub damaged_this_combat: u8,
    pub cards_played_this_turn: Vec<CardId>,
    pub cards_played_this_combat: Vec<CardId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_cost_for_turn: Option<u8>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub orbs_channeled_this_turn: Vec<OrbId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub orbs_channeled_this_combat: Vec<OrbId>,
    #[serde(skip_serializing_if = "zero")]
    pub mantra_gained_this_combat: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CardCost {
    Energy(i32),
    Special(&'static str),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CardObservation {
    pub card: CardId,
    pub name: String,
    #[serde(skip_serializing_if = "zero")]
    pub upgrades: u8,
    /// Displayed cost-for-turn. Free-play flags and powers may waive payment;
    /// actual energy expenditure remains owned by simulator execution.
    pub cost: CardCost,
    /// Cost after the turn-only override resets, when different from `cost`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_cost: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_damage_override: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_block_override: Option<i32>,
    #[serde(skip_serializing_if = "zero")]
    pub free_once: bool,
    #[serde(skip_serializing_if = "zero")]
    pub misc: i32,
    /// Current card numbers, not a simulated outcome. Hooks can change results.
    #[serde(skip_serializing_if = "zero")]
    pub damage: i32,
    #[serde(skip_serializing_if = "zero")]
    pub block: i32,
    #[serde(skip_serializing_if = "zero")]
    pub magic: i32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub damage_by_target: Vec<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exhaust_override: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_override: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MonsterObservation {
    pub enemy: EnemyId,
    pub name: String,
    pub logical_position: i32,
    pub hp: [i32; 2],
    #[serde(skip_serializing_if = "zero")]
    pub block: i32,
    pub alive: bool,
    #[serde(skip_serializing_if = "zero")]
    pub half_dead: bool,
    #[serde(skip_serializing_if = "zero")]
    pub escaped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<MonsterMovePreview>,
    /// Current engine plan, before intervening cards, powers and target hooks.
    /// This is an Oracle semantic projection, not a guaranteed future outcome.
    pub intent_steps: Vec<sts_core::runtime::monster_move::MoveStep>,
    pub powers: Vec<PowerObservation>,
    /// None means this enemy's hidden controller fields have not been admitted.
    pub decision: Option<MonsterDecisionObservation>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PowerObservation {
    pub power: PowerId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i32>,
    #[serde(skip_serializing_if = "zero")]
    pub extra: i32,
    #[serde(skip_serializing_if = "zero")]
    pub just_applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<Box<CardObservation>>,
    /// Stasis refers to the actual card in the projected zones, so it cannot
    /// be mistaken for a second copy of the same card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_card: Option<CardAddress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Creature>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "index", rename_all = "snake_case")]
pub enum Creature {
    Player,
    Monster(usize),
    Unobserved,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RelicObservation {
    pub relic: RelicId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counter: Option<i32>,
    #[serde(skip_serializing_if = "zero")]
    pub used_up: bool,
    #[serde(skip_serializing_if = "zero")]
    pub amount: i32,
    /// Simulator-owned binding, never the compact lineage number in `amount`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_card: Option<BoundCardObservation>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BoundCardObservation {
    pub card: Box<CardObservation>,
    /// A played power may no longer be in any live pile. Its master-deck
    /// binding remains a description, not a second playable card.
    pub location: Option<CardAddress>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PotionObservation {
    pub potion: PotionId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "zone", content = "index", rename_all = "snake_case")]
pub enum CardAddress {
    Hand(usize),
    Draw(usize),
    Discard(usize),
    Exhaust(usize),
    Limbo(usize),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SelectionObservation {
    pub reason: CombatSelectionReasonV2,
    pub source_pile: Option<PileType>,
    pub min: usize,
    pub max: usize,
    /// Ordered, distinct members; never a list of combinations.
    pub members: Vec<CardAddress>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SelectionAssembly {
    pub family: usize,
    pub selected: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelChoice {
    Play {
        hand: usize,
        target: Option<usize>,
    },
    UsePotion {
        slot: usize,
        target: Option<usize>,
    },
    DiscardPotion {
        slot: usize,
    },
    EndTurn {},
    Choose {
        candidate: CombatIndexedChoiceCandidateV2,
        name: String,
    },
    BeginSelection {
        family: usize,
    },
    SelectMember {
        member: usize,
    },
    FinishSelection {},
    Cancel {},
    Proceed {},
}

fn card(combat: &CombatState, card: &CombatCard, targets: bool) -> CardObservation {
    let evaluated = evaluate_card_for_play(card, combat, None);
    let definition = get_card_definition(card.id);
    let damage_by_target = if !targets || definition.card_type != CardType::Attack {
        Vec::new()
    } else if definition.is_multi_damage {
        evaluated.multi_damage.to_vec()
    } else {
        combat
            .entities
            .monsters
            .iter()
            .map(|monster| evaluate_card_for_play(card, combat, Some(monster.id)).base_damage_mut)
            .collect()
    };
    CardObservation {
        card: card.id,
        name: card_name(card.id, card.upgrades),
        upgrades: card.upgrades,
        cost: match card.cost_for_turn_java() {
            -1 => CardCost::Special("X"),
            -2 => CardCost::Special("unplayable"),
            n => CardCost::Energy(n),
        },
        free_once: card.free_to_play_once,
        combat_cost: (card.combat_cost_without_turn_override_java() != card.cost_for_turn_java())
            .then(|| card.combat_cost_without_turn_override_java()),
        base_damage_override: card.base_damage_override,
        base_block_override: card.base_block_override,
        misc: card.misc_value,
        damage: evaluated.base_damage_mut,
        block: evaluated.base_block_mut,
        magic: evaluated.base_magic_num_mut,
        damage_by_target,
        exhaust_override: card.exhaust_override,
        retain_override: card.retain_override,
    }
}

fn card_name(id: CardId, upgrades: u8) -> String {
    match upgrades {
        0 => name(id).to_owned(),
        n if id == CardId::SearingBlow || n > 1 => format!("{}+{n}", name(id)),
        _ => format!("{}+", name(id)),
    }
}

fn creature(combat: &CombatState, id: usize) -> Creature {
    if id == combat.entities.player.id {
        Creature::Player
    } else {
        combat
            .entities
            .monsters
            .iter()
            .position(|m| m.id == id)
            .map(Creature::Monster)
            .unwrap_or(Creature::Unobserved)
    }
}

fn powers(combat: &CombatState, id: usize) -> Result<Vec<PowerObservation>, String> {
    combat
        .entities
        .power_db
        .get(&id)
        .into_iter()
        .flatten()
        .map(|power| {
            let extra = if power.power_type == PowerId::Stasis {
                0
            } else {
                power.extra_data
            };
            let mut held_card = None;
            let payload = match &power.payload {
                PowerPayload::Card(c) => Some(Box::new(card(combat, c, false))),
                PowerPayload::None if power.power_type == PowerId::Stasis => {
                    let lineage = sts_core::content::powers::core::stasis::held_card_lineage(
                        power.extra_data,
                    )
                    .ok_or("missing Stasis card identity")?;
                    let held = address(combat, lineage)?;
                    if !matches!(held, CardAddress::Limbo(_)) {
                        return Err("Stasis card is outside its holding zone".into());
                    }
                    held_card = Some(held);
                    None
                }
                _ => None,
            };
            Ok(PowerObservation {
                power: power.power_type,
                name: name(power.power_type).to_owned(),
                amount: (!(uses_sentinel_amount(power.power_type) && power.amount < 0))
                    .then_some(power.amount),
                extra,
                just_applied: power.just_applied,
                card: payload,
                held_card,
                source: match power.payload {
                    PowerPayload::SourceEntity(id) => Some(creature(combat, id)),
                    _ => None,
                },
            })
        })
        .collect()
}

fn card_locations(combat: &CombatState, lineage: CardLineageId) -> Vec<CardAddress> {
    let mut found = Vec::new();
    for (cards, address) in [
        (
            combat.zones.hand.iter().collect::<Vec<_>>(),
            CardAddress::Hand as fn(usize) -> CardAddress,
        ),
        (combat.zones.draw_pile.iter().collect(), CardAddress::Draw),
        (
            combat.zones.discard_pile.iter().collect(),
            CardAddress::Discard,
        ),
        (
            combat.zones.exhaust_pile.iter().collect(),
            CardAddress::Exhaust,
        ),
        (combat.zones.limbo.iter().collect(), CardAddress::Limbo),
    ] {
        found.extend(
            cards
                .into_iter()
                .enumerate()
                .filter(|(_, c)| c.lineage == lineage)
                .map(|(i, _)| address(i)),
        );
    }
    found
}

fn address(combat: &CombatState, lineage: CardLineageId) -> Result<CardAddress, String> {
    let mut found = card_locations(combat, lineage);
    if found.len() != 1 {
        return Err("selection card has missing or ambiguous source".into());
    }
    Ok(found.remove(0))
}

fn relic(
    combat: &CombatState,
    r: &sts_core::content::relics::RelicState,
) -> Result<RelicObservation, String> {
    let bottle = matches!(
        r.id,
        RelicId::BottledFlame | RelicId::BottledLightning | RelicId::BottledTornado
    );
    let bound_card = if bottle {
        sts_core::content::relics::persistent_state::persistent_relic_bound_card_lineage_v1(r)?
            .map(|lineage| {
                let mut cards = combat
                    .meta
                    .master_deck_snapshot
                    .iter()
                    .filter(|c| c.lineage == lineage);
                let bound = cards
                    .next()
                    .ok_or("bottled target missing from master deck")?;
                if cards.next().is_some() {
                    return Err("ambiguous bottled master-deck target".to_owned());
                }
                let mut locations = card_locations(combat, lineage);
                if locations.len() > 1 {
                    return Err("ambiguous bottled live-card target".to_owned());
                }
                Ok(BoundCardObservation {
                    card: Box::new(card(combat, bound, false)),
                    location: locations.pop(),
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(RelicObservation {
        relic: r.id,
        name: name(r.id).to_owned(),
        counter: (r.counter != -1).then_some(r.counter),
        used_up: r.used_up,
        amount: if bottle { 0 } else { r.amount },
        bound_card,
    })
}

pub fn combat_observation(state: &CombatSearchState) -> Result<CombatObservation, String> {
    let combat = &state.position().combat;
    let surface = EngineCombatStepper.legal_action_surface(state.position());
    let mut selections = Vec::new();
    for (family, raw) in surface.selection_families.iter().enumerate() {
        let domain = SelectionDomain::new(raw)?;
        if state.selection_family() == Some(family)
            && state.selection_members() != Some(domain.members.as_slice())
        {
            return Err("selection source changed".into());
        }
        let members = domain
            .members
            .iter()
            .map(|member| match member {
                SelectionMember::Card(lineage) => address(combat, *lineage),
                SelectionMember::Scry { index, lineage } => {
                    if combat.zones.draw_pile.get(*index).map(|c| c.lineage) != Some(*lineage) {
                        return Err("scry member source changed".into());
                    }
                    Ok(CardAddress::Draw(*index))
                }
            })
            .collect::<Result<_, _>>()?;
        selections.push(SelectionObservation {
            reason: raw.reason.clone(),
            source_pile: raw.source_pile,
            min: domain.min,
            max: domain.max,
            members,
        });
    }
    let assembling = match state.selected_members() {
        Some(selected) => Some(SelectionAssembly {
            family: state
                .selection_family()
                .filter(|family| *family < selections.len())
                .ok_or("selection family missing")?,
            selected: selected.to_vec(),
        }),
        None => None,
    };
    let p = &combat.entities.player;
    let c = &combat.turn.counters;
    Ok(CombatObservation {
        player_class: combat.meta.player_class.clone(),
        ascension: combat.meta.ascension_level,
        turn: combat.turn.turn_count.saturating_add(1),
        phase: combat.turn.current_phase,
        hp: [p.current_hp, p.max_hp],
        block: p.block,
        energy: combat.turn.energy,
        energy_per_turn: p.energy_master,
        gold: p.gold,
        facing_left: p.facing_left,
        potion_uses: state.potion_uses(),
        stance: p.stance,
        orbs: p.orbs.clone(),
        max_orbs: p.max_orbs,
        powers: powers(combat, p.id)?,
        relics: p
            .relics
            .iter()
            .map(|r| relic(combat, r))
            .collect::<Result<_, _>>()?,
        potions: combat
            .entities
            .potions
            .iter()
            .map(|p| {
                p.as_ref().map(|p| PotionObservation {
                    potion: p.id,
                    name: name(p.id).to_owned(),
                })
            })
            .collect(),
        monsters: combat
            .entities
            .monsters
            .iter()
            .map(|m| {
                Ok(MonsterObservation {
                    enemy: m.enemy_id,
                    name: name(m.enemy_id).to_owned(),
                    logical_position: m.logical_position,
                    hp: [m.current_hp, m.max_hp],
                    block: m.block,
                    alive: m.is_alive_for_action(),
                    half_dead: m.half_dead,
                    escaped: m.is_escaped,
                    intent: m
                        .is_alive_for_action()
                        .then(|| project_monster_move_preview_in_combat(combat, m)),
                    powers: powers(combat, m.id)?,
                    intent_steps: if m.is_alive_for_action() {
                        sts_core::content::monsters::resolve_monster_turn_plan(combat, m)
                            .steps
                            .into_iter()
                            .collect()
                    } else {
                        Vec::new()
                    },
                    decision: monster::decision_in_group(m, &combat.entities.monsters),
                })
            })
            .collect::<Result<_, String>>()?,
        hand: combat
            .zones
            .hand
            .iter()
            .map(|c| card(combat, c, true))
            .collect(),
        draw_top_first: combat
            .zones
            .draw_pile
            .iter()
            .map(|c| card(combat, c, false))
            .collect(),
        discard: combat
            .zones
            .discard_pile
            .iter()
            .map(|c| card(combat, c, false))
            .collect(),
        exhaust: combat
            .zones
            .exhaust_pile
            .iter()
            .map(|c| card(combat, c, false))
            .collect(),
        limbo: combat
            .zones
            .limbo
            .iter()
            .map(|c| card(combat, c, false))
            .collect(),
        counters: TurnCounters {
            played: c.cards_played_this_turn,
            attacks: c.attacks_played_this_turn,
            discarded: c.cards_discarded_this_turn,
            draw_modifier_next_turn: combat.turn.turn_start_draw_modifier,
            damaged_this_combat: c.times_damaged_this_combat,
            cards_played_this_turn: c.card_ids_played_this_turn.clone(),
            cards_played_this_combat: c.card_ids_played_this_combat.clone(),
            discovery_cost_for_turn: c.discovery_cost_for_turn,
            orbs_channeled_this_turn: c.orbs_channeled_this_turn.clone(),
            orbs_channeled_this_combat: c.orbs_channeled_this_combat.clone(),
            mantra_gained_this_combat: c.mantra_gained_this_combat,
        },
        selections,
        indexed_choice_reason: surface.indexed_choice.map(|c| c.reason),
        assembling,
    })
}

pub fn model_choices(
    state: &CombatSearchState,
    actions: &[&CombatBranch],
) -> Result<Vec<ModelChoice>, String> {
    let combat = &state.position().combat;
    let surface = EngineCombatStepper.legal_action_surface(state.position());
    let target = |id: &Option<usize>| -> Result<Option<usize>, String> {
        id.map(|id| {
            combat
                .entities
                .monsters
                .iter()
                .position(|m| m.id == id)
                .ok_or_else(|| "missing target identity".to_owned())
        })
        .transpose()
    };
    actions
        .iter()
        .map(|action| {
            Ok(match action {
                CombatBranch::Input(input) => match input {
                    ClientInput::PlayCard {
                        card_index,
                        target: id,
                    } => ModelChoice::Play {
                        hand: *card_index,
                        target: target(id)?,
                    },
                    ClientInput::UsePotion {
                        potion_index,
                        target: id,
                    } => ModelChoice::UsePotion {
                        slot: *potion_index,
                        target: target(id)?,
                    },
                    ClientInput::DiscardPotion(slot) => ModelChoice::DiscardPotion { slot: *slot },
                    ClientInput::EndTurn => ModelChoice::EndTurn {},
                    ClientInput::Cancel => ModelChoice::Cancel {},
                    ClientInput::Proceed => ModelChoice::Proceed {},
                    ClientInput::SubmitDiscoverChoice(index) => {
                        let candidate = *surface
                            .indexed_choice
                            .as_ref()
                            .and_then(|s| s.candidates.get(*index))
                            .ok_or("missing indexed choice")?;
                        let name = match candidate {
                            CombatIndexedChoiceCandidateV2::Card { card_id, upgrades } => {
                                card_name(card_id, upgrades)
                            }
                            CombatIndexedChoiceCandidateV2::Stance { stance } => {
                                name(stance).to_owned()
                            }
                        };
                        ModelChoice::Choose { candidate, name }
                    }
                    _ => return Err("unprojected combat input".into()),
                },
                CombatBranch::BeginSelection(family) => {
                    ModelChoice::BeginSelection { family: *family }
                }
                CombatBranch::SelectMember(member) => ModelChoice::SelectMember { member: *member },
                CombatBranch::FinishSelection => ModelChoice::FinishSelection {},
            })
        })
        .collect()
}
