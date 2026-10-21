use crate::content::cards::CardType;

use crate::ai::combat_search_v2::turn_planner::types::core::TurnPlanV1;

use super::bands::{
    TurnPlanBlockBandV1, TurnPlanCoverageKeyV1, TurnPlanDamageBandV1, TurnPlanDebuffClassV1,
    TurnPlanResourceClassV1, TurnPlanRiskBandV1, TurnPlanSetupClassV1,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct TurnPlanCoverageSignatureV1 {
    pub(in crate::ai::combat_search_v2) action_count: usize,
    pub(in crate::ai::combat_search_v2) cards_played: usize,
    pub(in crate::ai::combat_search_v2) attacks_played: usize,
    pub(in crate::ai::combat_search_v2) skills_played: usize,
    pub(in crate::ai::combat_search_v2) powers_played: usize,
    pub(in crate::ai::combat_search_v2) potions_used: usize,
    pub(in crate::ai::combat_search_v2) damage_done: i32,
    pub(in crate::ai::combat_search_v2) block_gained_proxy: i32,
    pub(in crate::ai::combat_search_v2) enemy_vulnerable_added: i32,
    pub(in crate::ai::combat_search_v2) enemy_weak_added: i32,
    pub(in crate::ai::combat_search_v2) enemy_strength_down_added: i32,
    pub(in crate::ai::combat_search_v2) player_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) player_temporary_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) energy_spent_proxy: i32,
    pub(in crate::ai::combat_search_v2) hand_delta: i32,
    pub(in crate::ai::combat_search_v2) draw_delta: i32,
    pub(in crate::ai::combat_search_v2) discard_delta: i32,
    pub(in crate::ai::combat_search_v2) exhaust_delta: i32,
    pub(in crate::ai::combat_search_v2) queued_cards_delta: i32,
    pub(in crate::ai::combat_search_v2) player_hp_lost: i32,
    pub(in crate::ai::combat_search_v2) reactive_player_hp_loss: i32,
    pub(in crate::ai::combat_search_v2) reactive_forced_turn_end_actions: usize,
    pub(in crate::ai::combat_search_v2) pending_choice_steps: usize,
}

impl TurnPlanCoverageSignatureV1 {
    pub(in crate::ai::combat_search_v2) fn from_plan(plan: &TurnPlanV1) -> Self {
        let mut signature = Self {
            action_count: plan.actions.len(),
            ..Self::default()
        };
        for action in &plan.actions {
            match action.input {
                crate::state::core::ClientInput::PlayCard { .. } => {
                    signature.cards_played = signature.cards_played.saturating_add(1);
                }
                crate::state::core::ClientInput::UsePotion { .. } => {
                    signature.potions_used = signature.potions_used.saturating_add(1);
                }
                _ => {}
            }
        }
        for facts in &plan.action_facts {
            if let Some(card) = facts.card.as_ref() {
                match card.card_type {
                    CardType::Attack => {
                        signature.attacks_played = signature.attacks_played.saturating_add(1);
                    }
                    CardType::Skill => {
                        signature.skills_played = signature.skills_played.saturating_add(1);
                    }
                    CardType::Power => {
                        signature.powers_played = signature.powers_played.saturating_add(1);
                    }
                    CardType::Status | CardType::Curse => {}
                }
            }
            let exact = &facts.exact_one_step_delta;
            signature.damage_done = signature
                .damage_done
                .saturating_add((-exact.total_enemy_hp_delta).max(0));
            signature.block_gained_proxy = signature
                .block_gained_proxy
                .saturating_add(exact.player_block_delta.max(0));
            signature.energy_spent_proxy = signature
                .energy_spent_proxy
                .saturating_add((-exact.energy_delta).max(0));
            signature.hand_delta = signature.hand_delta.saturating_add(exact.hand_delta);
            signature.draw_delta = signature.draw_delta.saturating_add(exact.draw_delta);
            signature.discard_delta = signature.discard_delta.saturating_add(exact.discard_delta);
            signature.exhaust_delta = signature.exhaust_delta.saturating_add(exact.exhaust_delta);
            signature.queued_cards_delta = signature
                .queued_cards_delta
                .saturating_add(exact.queued_cards_delta);
            signature.player_hp_lost = signature
                .player_hp_lost
                .saturating_add((-exact.player_hp_delta).max(0));
            if exact.pending_choice_present {
                signature.pending_choice_steps = signature.pending_choice_steps.saturating_add(1);
            }

            let mechanics = &facts.mechanics;
            signature.enemy_vulnerable_added = signature
                .enemy_vulnerable_added
                .saturating_add(mechanics.derived.enemy_vulnerable);
            signature.enemy_weak_added = signature
                .enemy_weak_added
                .saturating_add(mechanics.derived.enemy_weak);
            signature.enemy_strength_down_added = signature
                .enemy_strength_down_added
                .saturating_add(mechanics.direct.persistent_enemy_strength_down)
                .saturating_add(mechanics.direct.temporary_enemy_strength_down);
            let declared_durable_strength = mechanics
                .direct
                .player_strength_gain
                .saturating_sub(mechanics.direct.player_temporary_strength_gain);
            // EndTurn includes the entire enemy phase and the next turn start.
            // Its exact Strength delta can therefore contain relic thresholds
            // (for example Red Skull) or recurring powers.  Those are real
            // state changes, but they are not evidence that this action line
            // itself supplied a Strength-gain mechanic.
            let exact_durable_strength = if facts.action_kind == "end_turn" {
                0
            } else {
                exact
                    .player_strength_delta
                    .saturating_sub(mechanics.direct.player_temporary_strength_gain)
            };
            signature.player_strength_gain = signature
                .player_strength_gain
                .saturating_add(declared_durable_strength.max(exact_durable_strength).max(0));
            signature.player_temporary_strength_gain = signature
                .player_temporary_strength_gain
                .saturating_add(mechanics.direct.player_temporary_strength_gain);
            signature.reactive_player_hp_loss = signature
                .reactive_player_hp_loss
                .saturating_add(mechanics.reactive.player_hp_loss);
            if mechanics.reactive.forced_turn_end {
                signature.reactive_forced_turn_end_actions =
                    signature.reactive_forced_turn_end_actions.saturating_add(1);
            }
        }
        signature
    }

    pub(in crate::ai::combat_search_v2) fn coverage_key(self) -> TurnPlanCoverageKeyV1 {
        TurnPlanCoverageKeyV1 {
            damage: damage_band(self.damage_done),
            block: block_band(self.block_gained_proxy),
            debuff: debuff_class(self),
            setup: setup_class(self),
            resource: resource_class(self),
            risk: risk_band(self),
        }
    }
}

fn damage_band(value: i32) -> TurnPlanDamageBandV1 {
    match value {
        i32::MIN..=0 => TurnPlanDamageBandV1::None,
        1..=7 => TurnPlanDamageBandV1::Low,
        8..=17 => TurnPlanDamageBandV1::Medium,
        _ => TurnPlanDamageBandV1::High,
    }
}

fn block_band(value: i32) -> TurnPlanBlockBandV1 {
    match value {
        i32::MIN..=0 => TurnPlanBlockBandV1::None,
        1..=5 => TurnPlanBlockBandV1::Low,
        6..=12 => TurnPlanBlockBandV1::Medium,
        _ => TurnPlanBlockBandV1::High,
    }
}

fn debuff_class(signature: TurnPlanCoverageSignatureV1) -> TurnPlanDebuffClassV1 {
    let weak = signature.enemy_weak_added > 0;
    let vulnerable = signature.enemy_vulnerable_added > 0;
    let strength_down = signature.enemy_strength_down_added > 0;
    match (weak, vulnerable, strength_down) {
        (false, false, false) => TurnPlanDebuffClassV1::None,
        (true, false, false) => TurnPlanDebuffClassV1::Weak,
        (false, true, false) => TurnPlanDebuffClassV1::Vulnerable,
        (false, false, true) => TurnPlanDebuffClassV1::StrengthDown,
        _ => TurnPlanDebuffClassV1::Mixed,
    }
}

fn setup_class(signature: TurnPlanCoverageSignatureV1) -> TurnPlanSetupClassV1 {
    let power = signature.powers_played > 0;
    let strength =
        signature.player_strength_gain > 0 || signature.player_temporary_strength_gain > 0;
    let access = signature.hand_delta > 0 || signature.draw_delta < 0;
    let shape = signature.exhaust_delta != 0 || signature.queued_cards_delta != 0;
    if [power, strength, access, shape]
        .into_iter()
        .filter(|present| *present)
        .count()
        > 1
    {
        return TurnPlanSetupClassV1::Mixed;
    }
    if power {
        TurnPlanSetupClassV1::Power
    } else if strength {
        TurnPlanSetupClassV1::PlayerStrength
    } else if access {
        TurnPlanSetupClassV1::AccessGain
    } else if shape {
        TurnPlanSetupClassV1::ExhaustOrQueueChange
    } else {
        TurnPlanSetupClassV1::None
    }
}

fn resource_class(signature: TurnPlanCoverageSignatureV1) -> TurnPlanResourceClassV1 {
    if signature.potions_used > 0 {
        return TurnPlanResourceClassV1::UsesPotion;
    }
    if signature.hand_delta > 0 || signature.draw_delta < 0 {
        return TurnPlanResourceClassV1::GainsAccess;
    }
    if signature.energy_spent_proxy > 0 {
        return TurnPlanResourceClassV1::SpendsEnergy;
    }
    TurnPlanResourceClassV1::Neutral
}

fn risk_band(signature: TurnPlanCoverageSignatureV1) -> TurnPlanRiskBandV1 {
    if signature.reactive_forced_turn_end_actions > 0 || signature.reactive_player_hp_loss > 0 {
        return TurnPlanRiskBandV1::ForcedTurnEndOrReactiveLoss;
    }
    match signature.player_hp_lost {
        i32::MIN..=0 => TurnPlanRiskBandV1::NoHpLoss,
        1..=6 => TurnPlanRiskBandV1::LowHpLoss,
        _ => TurnPlanRiskBandV1::HighHpLoss,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_hand_gain_or_draw_consumption_is_access_setup() {
        let hand_gain = TurnPlanCoverageSignatureV1 {
            hand_delta: 1,
            ..TurnPlanCoverageSignatureV1::default()
        };
        let draw_consumption = TurnPlanCoverageSignatureV1 {
            draw_delta: -1,
            ..TurnPlanCoverageSignatureV1::default()
        };

        assert_eq!(setup_class(hand_gain), TurnPlanSetupClassV1::AccessGain);
        assert_eq!(
            setup_class(draw_consumption),
            TurnPlanSetupClassV1::AccessGain
        );
    }
}
