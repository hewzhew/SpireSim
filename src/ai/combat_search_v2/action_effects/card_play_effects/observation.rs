use std::collections::{BTreeMap, BTreeSet};

use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};

#[derive(Default)]
pub(super) struct CardPlayEffectAccumulator {
    pub(super) direct: DirectCardPlayEffectAccumulator,
    pub(super) reactive: super::super::ReactiveCardPlayEffectFacts,
    pub(super) retaliation_projection_state: Option<CombatState>,
}

#[derive(Default)]
pub(super) struct DirectCardPlayEffectAccumulator {
    pub(super) player_block: i32,
    pub(super) artifact_consumed_by_target: BTreeMap<usize, i32>,
    pub(super) enemy_strength_down_by_target: BTreeMap<usize, i32>,
    pub(super) enemy_strength_gain_by_target: BTreeMap<usize, i32>,
    pub(super) enemy_weak_by_target: BTreeMap<usize, i32>,
    pub(super) shackled_targets: BTreeSet<usize>,
    pub(super) player_strength_gain: i32,
    pub(super) player_lose_strength: i32,
    pub(super) player_recurring_turn_energy_delta: i32,
    pub(super) player_attack_replay_charges: i32,
    pub(super) player_intangible_gain: i32,
    pub(super) player_hp_loss: i32,
    pub(super) player_energy_gain: i32,
    pub(super) declared_draw_cards: i32,
    pub(super) conditional_draw_cards: i32,
    pub(super) enemy_weak: i32,
    pub(super) enemy_vulnerable: i32,
    pub(super) player_vulnerable: i32,
}

pub(super) fn observe_card_play_effects(
    combat: &CombatState,
    card: &CombatCard,
    actions: impl IntoIterator<Item = Action>,
) -> CardPlayEffectAccumulator {
    let mut accumulator = CardPlayEffectAccumulator::default();
    for action in actions {
        super::attack_retaliation_observation::observe_attack_retaliation_action(
            combat,
            &mut accumulator,
            &action,
        );
        observe_direct_action(combat, &mut accumulator.direct, action);
    }
    super::reactive_observation::observe_card_play_reactive_actions(combat, card, &mut accumulator);
    accumulator
}

pub(super) fn observe_direct_action(
    combat: &CombatState,
    direct: &mut DirectCardPlayEffectAccumulator,
    action: Action,
) {
    match action {
        Action::GainBlock { target: 0, amount } => {
            direct.player_block = direct.player_block.saturating_add(amount.max(0));
        }
        Action::ApplyPower {
            target,
            power_id,
            amount,
            ..
        }
        | Action::ApplyPowerDetailed {
            target,
            power_id,
            amount,
            ..
        }
        | Action::ApplyPowerWithPayload {
            target,
            power_id,
            amount,
            ..
        } => observe_apply_power(combat, direct, target, power_id, amount),
        Action::SpotWeakness { target, amount } => {
            // Describe the deferred request at this input, not an unconditional
            // card grant or a forecast of intervening on-play reactions.
            if let Some(request) =
                crate::engine::action_handlers::powers::spot_weakness_strength_request(
                    target, amount, combat,
                )
            {
                observe_direct_action(combat, direct, request);
            }
        }
        Action::DrawCards(amount) | Action::DrawCardsWithHistory { amount, .. } => {
            direct.declared_draw_cards = direct.declared_draw_cards.saturating_add(amount as i32);
        }
        Action::GainEnergy { amount } => {
            direct.player_energy_gain = direct.player_energy_gain.saturating_add(amount.max(0));
        }
        Action::LoseHp {
            target: 0, amount, ..
        }
        | Action::PoisonLoseHp {
            target: 0, amount, ..
        } => {
            direct.player_hp_loss = direct.player_hp_loss.saturating_add(amount.max(0));
        }
        Action::DoubleEnergy => {
            direct.player_energy_gain = direct
                .player_energy_gain
                .saturating_add(i32::from(combat.turn.energy));
        }
        Action::ExpertiseDraw { target_hand_size } => {
            let hand_after_play = combat.zones.hand.len().saturating_sub(1) as i32;
            direct.conditional_draw_cards = direct
                .conditional_draw_cards
                .saturating_add(target_hand_size.saturating_sub(hand_after_play).max(0));
        }
        Action::InnerPeace { draw_amount } | Action::Sanctity { draw_amount } => {
            direct.conditional_draw_cards = direct
                .conditional_draw_cards
                .saturating_add(draw_amount as i32);
        }
        Action::CalculatedGamble { draw_extra } => {
            let hand_after_play = combat.zones.hand.len().saturating_sub(1) as i32;
            direct.conditional_draw_cards = direct
                .conditional_draw_cards
                .saturating_add(hand_after_play.saturating_add(i32::from(draw_extra)).max(0));
        }
        Action::DrawForUniqueOrbTypes {
            amount_per_orb_type,
        } => {
            direct.conditional_draw_cards = direct
                .conditional_draw_cards
                .saturating_add(amount_per_orb_type as i32);
        }
        _ => {}
    }
}

fn observe_apply_power(
    combat: &CombatState,
    direct: &mut DirectCardPlayEffectAccumulator,
    target: usize,
    power_id: PowerId,
    amount: i32,
) {
    if crate::content::powers::is_debuff_application(power_id, amount) {
        let available_artifact =
            crate::content::powers::store::power_amount(combat, target, PowerId::Artifact).max(0);
        let consumed = direct
            .artifact_consumed_by_target
            .entry(target)
            .or_default();
        if *consumed < available_artifact {
            *consumed = consumed.saturating_add(1);
            return;
        }
    }
    match power_id {
        PowerId::Strength if target == 0 && amount > 0 => {
            direct.player_strength_gain = direct.player_strength_gain.saturating_add(amount);
        }
        PowerId::LoseStrength if target == 0 && amount > 0 => {
            direct.player_lose_strength = direct.player_lose_strength.saturating_add(amount);
        }
        PowerId::Berserk | PowerId::DevaForm if target == 0 && amount != 0 => {
            direct.player_recurring_turn_energy_delta = direct
                .player_recurring_turn_energy_delta
                .saturating_add(amount);
        }
        PowerId::EnergyDownPower if target == 0 && amount != 0 => {
            direct.player_recurring_turn_energy_delta = direct
                .player_recurring_turn_energy_delta
                .saturating_sub(amount);
        }
        PowerId::DoubleTap if target == 0 && amount > 0 => {
            direct.player_attack_replay_charges =
                direct.player_attack_replay_charges.saturating_add(amount);
        }
        PowerId::IntangiblePlayer if target == 0 && amount > 0 => {
            direct.player_intangible_gain = direct.player_intangible_gain.saturating_add(amount);
        }
        PowerId::Strength if target != 0 && amount < 0 => {
            *direct
                .enemy_strength_down_by_target
                .entry(target)
                .or_default() += -amount;
        }
        PowerId::Strength if target != 0 && amount > 0 => {
            *direct
                .enemy_strength_gain_by_target
                .entry(target)
                .or_default() += amount;
        }
        PowerId::Shackled if amount > 0 => {
            direct.shackled_targets.insert(target);
        }
        PowerId::Weak if target != 0 && amount > 0 => {
            direct.enemy_weak = direct.enemy_weak.saturating_add(amount);
            *direct.enemy_weak_by_target.entry(target).or_default() += amount;
        }
        PowerId::Vulnerable if target == 0 && amount > 0 => {
            direct.player_vulnerable = direct.player_vulnerable.saturating_add(amount);
        }
        PowerId::Vulnerable if target != 0 && amount > 0 => {
            direct.enemy_vulnerable = direct.enemy_vulnerable.saturating_add(amount);
        }
        _ => {}
    }
}
