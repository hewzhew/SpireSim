//! Healing resources shared by retention and tactical context. The optional
//! audit profile exposes effect dependencies without executing or ranking a
//! hypothetical future. It is not collected for ordinary search nodes.
use super::super::Node;
use serde::Serialize;
use std::collections::HashMap;
use sts_core::content::cards::{
    can_play_card, effective_target, exhausts_when_played, get_card_definition, resolve_card_play,
    CardId, CardTag, CardTarget, CardType,
};
use sts_core::content::powers::{store, PowerId};
use sts_core::content::relics::{hooks::on_calculate_heal, RelicId};
use sts_core::runtime::action::{Action, AddTo};
use sts_core::runtime::combat::{CardLineageId, CombatCard, CombatState, Power, PowerPayload};
use sts_core::state::core::EngineState;

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryCredit {
    pub conditional_reserve: i64,
    /// Passive revival inventory evaluated through the lethal-trigger owner.
    /// Never added to today's physical margin or capacity-capped healing hint.
    pub revival_reserve: i64,
    pub visible_attack: i64,
    /// Unsettled, phase-specific block and self-damage requests from current
    /// powers/hand. Shared with the game's callbacks, without future draws.
    pub turn_end: sts_core::sim::combat_projection::PlayerTurnEndRequests,
    /// Current enemy attack plus blockable end-turn self-damage, after current
    /// and pre-card power block. Unblockable turn-end HP loss is separate.
    pub uncovered_attack: i64,
    /// Best reviewed present-hand response, not a survival certificate.
    pub conditional_protection: i64,
    /// Signed current-HP margin after the selected protection/funding requests,
    /// before borrowing any remaining healing. Not a simulated survival bound.
    pub requested_hp_after_response: i64,
    /// This response horizon's capacity-capped hint. The full conditional
    /// reserve above remains available as a distinct future resource.
    pub conditional_health_hint: i64,
    /// Signed response margin plus remaining one-use healing and revivals, before the
    /// current-capacity cap. Conditional endurance across later damage, not HP.
    pub conditional_endurance_hint: i64,
    /// The same selected response, before exchanging lost recovery for current
    /// protection. Its net credit is not a physical survival margin.
    pub protection_response: Option<ProtectionResponse>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtectionSource {
    HandBlock {
        card: CardLineageId,
        funding: Option<CardLineageId>,
    },
    HandBlockSet {
        cards: Vec<CardLineageId>,
        energy_payment: usize,
    },
    RemoveAttacker {
        target: sts_core::EntityId,
    },
    RemoveAttackerAndBlock {
        target: sts_core::EntityId,
        attacks: Vec<(CardLineageId, CardId)>,
        block: Box<ProtectionSource>,
    },
}

/// A source-bound effect request, not a simulated response or proof that it can
/// be combined with another one. Physical protection and reserve depletion are
/// separate axes. Conditional endurance across later damage is used for ranking;
/// current-capacity health and the signed physical margin are separate hints.
#[derive(Clone, Debug, Serialize)]
pub struct ProtectionResponse {
    pub source: ProtectionSource,
    pub gross_protection: i64,
    pub recovery_reserve_lost: i64,
    /// Requested HP cost from funding, pre-effect triggers and after-use
    /// damage not absorbed before final Exhaust. Mitigation/revival are not
    /// borrowed, and later Block cannot retroactively absorb this cost.
    pub play_hp_cost: i64,
}

impl ProtectionResponse {
    pub(in crate::solver) fn credit(&self) -> i64 {
        (self.gross_protection - self.recovery_reserve_lost - self.play_hp_cost).max(0)
    }
}

fn is_healing(card: &CombatCard) -> bool {
    get_card_definition(card.id)
        .tags
        .contains(&CardTag::Healing)
}

pub(in crate::solver) type HealingInventory = HashMap<(CardLineageId, CardId, u8, i32), usize>;

pub(in crate::solver) fn healing_inventory(c: &CombatState) -> HealingInventory {
    let mut result = HealingInventory::new();
    for card in c
        .zones
        .hand
        .iter()
        .chain(&c.zones.draw_pile)
        .chain(&c.zones.discard_pile)
    {
        if is_healing(card) {
            *result
                .entry((card.lineage, card.id, card.upgrades, card.misc_value))
                .or_default() += 1;
        }
    }
    result
}

pub(super) fn reserve_from<'a>(
    c: &CombatState,
    cards: impl Iterator<Item = &'a CombatCard>,
) -> i64 {
    cards
        .filter(|card| is_healing(card))
        .map(|card| request(c, card))
        .sum::<i64>()
        .min(i64::from(c.entities.player.max_hp.max(0)))
}

/// Existing retention arithmetic: gross healing requests with reviewed packet
/// caps. This omits execution hooks, access, payment, repeated plays and future
/// damage. Even a nonzero request is not a certificate of actual HP gain.
fn request(c: &CombatState, card: &CombatCard) -> i64 {
    let player = &c.entities.player;
    if player.current_hp <= 0 {
        return 0;
    }
    let Some(target) = c
        .entities
        .monsters
        .iter()
        .find(|m| m.current_hp > 0 && !m.is_dying && !m.is_escaped)
        .map(|m| m.id)
    else {
        return 0;
    };
    let mut healing = 0;
    for effect in resolve_card_play(card.id, c, card, Some(target)) {
        match effect.action {
            Action::Heal { target, amount } if target == player.id => {
                healing += on_calculate_heal(c, amount).max(0);
            }
            Action::VampireDamageAllEnemies { damages, .. } => {
                let hp_loss: i32 = c
                    .entities
                    .monsters
                    .iter()
                    .zip(&damages)
                    .filter(|(m, _)| !m.is_dying && !m.is_escaped && m.current_hp > 0)
                    .map(|(m, &damage)| {
                        let mut damage = (damage - m.block.max(0)).max(0);
                        if damage > 0 && damage < 5 && player.has_relic(RelicId::Boot) {
                            damage = 5;
                        }
                        if c.get_power(m.id, PowerId::Buffer) > 0 {
                            damage = 0;
                        }
                        if store::has_power(c, m.id, PowerId::Invincible) {
                            damage = damage.min(c.get_power(m.id, PowerId::Invincible).max(0));
                        }
                        damage.max(0).min(m.current_hp)
                    })
                    .sum();
                healing += on_calculate_heal(c, hp_loss).max(0);
            }
            _ => {}
        }
    }
    i64::from(healing)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryZone {
    Hand,
    Draw,
    Discard,
    Exhaust,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryCardSource {
    pub lineage: CardLineageId,
    pub card: CardId,
    pub upgrades: u8,
    pub zone: RecoveryZone,
    pub index: usize,
    pub card_type: CardType,
    pub displayed_cost: i8,
    /// Payment request only; displayed-cost locks can still forbid a free play.
    pub energy_payment: i32,
    pub playable_now: bool,
    /// Requested exhaust, before prevention such as Strange Spoon. Power cards
    /// also leave circulation, but are distinguished by card_type.
    pub exhaust_requested: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoverySource {
    pub source: RecoveryCardSource,
    pub requested_healing: i64,
    /// Current missing-HP cap, not an actual successor or future health value.
    pub current_hp_capacity: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryGrowthSource {
    pub source: RecoveryCardSource,
    pub strength_added: i32,
    /// Flex's distinct end-turn loss request. Artifact and other hooks are not
    /// executed here, so this must not be treated as persistent net Strength.
    pub lose_strength_added: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryDependency {
    pub growth: usize,
    pub healing: usize,
    pub strength_after_request: i32,
    /// Same state with only the declared Strength request changed. All other
    /// fields, enemy HP and time stay fixed; no successor is fabricated.
    pub requested_after_strength: i64,
    pub requested_change: i64,
    pub current_hp_capacity_after_strength: i64,
    pub joint_energy_payment: i32,
    pub energy_shortfall: i32,
    pub both_in_hand: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryProfile {
    pub credit: RecoveryCredit,
    pub potion_budget_remaining: Option<usize>,
    /// Triggerable in engine order within the current resource contract.
    pub revivals: Vec<sts_core::engine::action_handlers::RevivalRequest>,
    pub strength: i32,
    pub lose_strength: i32,
    pub healing: Vec<RecoverySource>,
    pub exhausted_healing: Vec<RecoveryCardSource>,
    pub growth: Vec<RecoveryGrowthSource>,
    pub dependencies: Vec<RecoveryDependency>,
}

fn source(node: &Node, card: &CombatCard, zone: RecoveryZone, index: usize) -> RecoveryCardSource {
    let c = &node.state.position().combat;
    let kind = get_card_definition(card.id).card_type;
    let free_attack =
        kind == CardType::Attack && c.get_power(c.entities.player.id, PowerId::FreeAttackPower) > 0;
    let corruption =
        kind == CardType::Skill && store::has_power(c, c.entities.player.id, PowerId::Corruption);
    RecoveryCardSource {
        lineage: card.lineage,
        card: card.id,
        upgrades: card.upgrades,
        zone,
        index,
        card_type: kind,
        displayed_cost: card.get_cost(),
        energy_payment: if card.free_to_play_once || free_attack || corruption {
            0
        } else if card.get_cost() == -1 {
            i32::from(c.turn.energy)
        } else {
            i32::from(card.get_cost().max(0))
        },
        playable_now: zone == RecoveryZone::Hand
            && matches!(node.state.position().engine, EngineState::CombatPlayerTurn)
            && can_play_card(card, c).is_ok(),
        exhaust_requested: card
            .exhaust_override
            .unwrap_or_else(|| exhausts_when_played(card))
            || corruption,
    }
}

/// Narrow effect grammar, keyed by declared actions rather than card names.
/// Mixed damage/draw/removal effects need their own relation; they are not
/// stripped away to pretend that a preparation changes only Strength.
fn growth_request(c: &CombatState, card: &CombatCard) -> Option<(i32, i32)> {
    if !matches!(
        effective_target(card),
        CardTarget::SelfTarget | CardTarget::None
    ) {
        return None;
    }
    let mut strength: i32 = 0;
    let mut loss: i32 = 0;
    for effect in resolve_card_play(card.id, c, card, None) {
        if effect.insertion_mode != AddTo::Bottom {
            return None;
        }
        match effect.action {
            Action::ApplyPower {
                target,
                power_id: PowerId::Strength,
                amount,
                ..
            } if target == c.entities.player.id => strength = strength.checked_add(amount)?,
            Action::ApplyPower {
                target,
                power_id: PowerId::LoseStrength,
                amount,
                ..
            } if target == c.entities.player.id => loss = loss.checked_add(amount)?,
            Action::LimitBreak => {
                strength =
                    strength.checked_add(c.get_power(c.entities.player.id, PowerId::Strength))?
            }
            _ => return None,
        }
    }
    (strength > 0).then_some((strength, loss))
}

pub(in crate::solver) fn profile(node: &Node, credit: RecoveryCredit) -> RecoveryProfile {
    let c = &node.state.position().combat;
    let mut profile = RecoveryProfile {
        credit,
        potion_budget_remaining: node.remaining_potion_budget(),
        revivals: super::permitted_revivals(node).collect(),
        strength: c.get_power(c.entities.player.id, PowerId::Strength),
        lose_strength: c.get_power(c.entities.player.id, PowerId::LoseStrength),
        healing: Vec::new(),
        exhausted_healing: Vec::new(),
        growth: Vec::new(),
        dependencies: Vec::new(),
    };
    let mut healing_cards = Vec::new();
    for (zone, index, card) in c
        .zones
        .hand
        .iter()
        .enumerate()
        .map(|(i, card)| (RecoveryZone::Hand, i, card))
        .chain(
            c.zones
                .draw_pile
                .iter()
                .enumerate()
                .map(|(i, card)| (RecoveryZone::Draw, i, card)),
        )
        .chain(
            c.zones
                .discard_pile
                .iter()
                .enumerate()
                .map(|(i, card)| (RecoveryZone::Discard, i, card)),
        )
    {
        if is_healing(card) {
            let requested_healing = request(c, card);
            profile.healing.push(RecoverySource {
                source: source(node, card, zone, index),
                requested_healing,
                current_hp_capacity: requested_healing.min(i64::from(
                    (c.entities.player.max_hp - c.entities.player.current_hp).max(0),
                )),
            });
            healing_cards.push(card);
        }
        if let Some((strength_added, lose_strength_added)) = growth_request(c, card) {
            profile.growth.push(RecoveryGrowthSource {
                source: source(node, card, zone, index),
                strength_added,
                lose_strength_added,
            });
        }
    }
    for (index, card) in c
        .zones
        .exhaust_pile
        .iter()
        .enumerate()
        .filter(|(_, card)| is_healing(card))
    {
        profile
            .exhausted_healing
            .push(source(node, card, RecoveryZone::Exhaust, index));
    }
    for (growth, prep) in profile.growth.iter().enumerate() {
        let Some(strength) = c
            .get_power(c.entities.player.id, PowerId::Strength)
            .checked_add(prep.strength_added)
        else {
            continue;
        };
        // The engine's Strength stack is capped at the game's +/-999 limit.
        let strength = strength.clamp(-999, 999);
        let mut conditioned = c.clone();
        let powers = store::ensure_powers_for_mut(&mut conditioned, c.entities.player.id);
        if let Some(p) = powers
            .iter_mut()
            .find(|p| p.power_type == PowerId::Strength)
        {
            p.amount = strength;
        } else {
            powers.push(Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: strength,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            });
        }
        for (healing, (resource, card)) in profile.healing.iter().zip(&healing_cards).enumerate() {
            let requested_after_strength = request(&conditioned, card);
            let payment = prep.source.energy_payment + resource.source.energy_payment;
            profile.dependencies.push(RecoveryDependency {
                growth,
                healing,
                strength_after_request: strength,
                requested_after_strength,
                requested_change: requested_after_strength - resource.requested_healing,
                current_hp_capacity_after_strength: requested_after_strength.min(i64::from(
                    (c.entities.player.max_hp - c.entities.player.current_hp).max(0),
                )),
                joint_energy_payment: payment,
                energy_shortfall: (payment - i32::from(c.turn.energy)).max(0),
                both_in_hand: prep.source.zone == RecoveryZone::Hand
                    && resource.source.zone == RecoveryZone::Hand,
            });
        }
    }
    profile
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CombatSearchState;
    use sts_core::content::monsters::EnemyId;
    use sts_core::runtime::combat::Power;
    use sts_core::sim::combat::{
        CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
    };
    use sts_core::state::core::ClientInput;

    #[test]
    fn dependencies_match_replayed_growth_and_keep_payment_and_temporary_loss_separate() {
        let profile =
            |node: &Node| super::profile(node, super::super::evaluate(node, None).recovery);
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 10;
        c.entities.player.max_hp = 75;
        c.turn.energy = 3;
        let mut enemy = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        enemy.current_hp = 200;
        enemy.max_hp = 200;
        c.entities.monsters = vec![enemy];
        store::set_powers_for(
            &mut c,
            0,
            vec![Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: 8,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut limit = CombatCard::for_scenario(CardId::LimitBreak, 200);
        limit.upgrades = 1;
        c.zones.hand = vec![
            limit,
            CombatCard::for_scenario(CardId::Reaper, 201),
            CombatCard::for_scenario(CardId::BandageUp, 202),
            CombatCard::for_scenario(CardId::Flex, 203),
            CombatCard::for_scenario(CardId::Strike, 204),
        ];
        let node = |combat| {
            Node::root(
                CombatSearchState::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
                    .unwrap(),
            )
        };
        let original = node(c.clone());
        let p = profile(&original);
        assert_eq!(
            p.growth.len(),
            2,
            "targeted attacks are not self-only preparations"
        );
        assert_eq!(
            original.state.position().combat,
            c,
            "relation extraction must not execute an action"
        );
        assert_eq!(p.credit.conditional_reserve, 16);
        assert_eq!(p.healing[0].requested_healing, 12);
        assert!(p.healing[0].source.exhaust_requested);
        assert!(
            !p.growth[0].source.exhaust_requested,
            "upgraded Limit Break remains reusable"
        );
        let dependency = &p.dependencies[0];
        assert_eq!(dependency.requested_after_strength, 20);
        assert_eq!(dependency.joint_energy_payment, 3);
        assert_eq!(dependency.energy_shortfall, 0);
        assert!(dependency.both_in_hand);
        assert_eq!(
            p.dependencies[1].requested_change, 0,
            "flat healing does not depend on Strength"
        );
        assert_eq!(p.growth[1].strength_added, 2);
        assert_eq!(
            p.growth[1].lose_strength_added, 2,
            "Flex is not persistent net growth"
        );

        let play = |position: &CombatPosition, card_index| {
            let step = EngineCombatStepper.apply_to_stable(
                position,
                ClientInput::PlayCard {
                    card_index,
                    target: None,
                },
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            step.position
        };
        let prepared = play(original.state.position(), 0);
        let healed = play(&prepared, 0);
        assert_eq!(
            healed.combat.entities.player.current_hp - 10,
            dependency.requested_after_strength as i32
        );
        assert_eq!(healed.combat.turn.energy, 0);
        let after = profile(&node(healed.combat));
        assert!(after
            .healing
            .iter()
            .all(|h| h.source.card != CardId::Reaper));
        assert_eq!(after.exhausted_healing[0].card, CardId::Reaper);

        c.turn.energy = 2;
        let p = profile(&node(c.clone()));
        assert!(p.growth[0].source.playable_now && p.healing[0].source.playable_now);
        assert_eq!(
            p.dependencies[0].energy_shortfall, 1,
            "individual playability does not fund the sequence"
        );
        c.zones.discard_pile.push(c.zones.hand.remove(0));
        let p = profile(&node(c));
        let limit = p
            .growth
            .iter()
            .position(|g| g.source.card == CardId::LimitBreak)
            .unwrap();
        assert_eq!(p.growth[limit].source.zone, RecoveryZone::Discard);
        assert!(!p.growth[limit].source.playable_now);
        assert!(
            !p.dependencies
                .iter()
                .find(|d| d.growth == limit && d.healing == 0)
                .unwrap()
                .both_in_hand
        );

        let mut capped = original.state.position().combat.clone();
        capped.entities.monsters[0].current_hp = 3000;
        capped.entities.monsters[0].max_hp = 3000;
        store::ensure_powers_for_mut(&mut capped, 0)
            .iter_mut()
            .find(|p| p.power_type == PowerId::Strength)
            .unwrap()
            .amount = 900;
        let capped = node(capped);
        let p = profile(&capped);
        let prepared = play(capped.state.position(), 0);
        assert_eq!(
            p.dependencies[0].strength_after_request,
            prepared.combat.get_power(0, PowerId::Strength)
        );
        assert_eq!(p.dependencies[0].strength_after_request, 999);
        assert_eq!(p.dependencies[0].requested_after_strength, 1003);
        assert_eq!(
            p.dependencies[0].current_hp_capacity_after_strength, p.healing[0].current_hp_capacity,
            "more Strength need not yield more realizable healing"
        );
    }
}
