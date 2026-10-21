use super::super::action_effects::CardPlayEffectDiagnostics;
use super::super::phase_action_ordering::PhaseActionOrderingHint;
use super::constants::ROLE_END_TURN;
use super::role::ActionOrderingRole;
use sts_combat_knowledge::CombatTacticalActionRankV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct ActionOrderingPriority {
    pub(in crate::ai::combat_search_v2) role: ActionOrderingRole,
    pub(in crate::ai::combat_search_v2) recoverable_resource_urgency: i32,
    pub(in crate::ai::combat_search_v2) role_rank: i32,
    pub(in crate::ai::combat_search_v2) lethal_external_payoff: i32,
    pub(in crate::ai::combat_search_v2) potion_tactical_rank: i32,
    pub(in crate::ai::combat_search_v2) mitigation: i32,
    pub(in crate::ai::combat_search_v2) action_supply: i32,
    pub(in crate::ai::combat_search_v2) reactive_risk: i32,
    pub(in crate::ai::combat_search_v2) targets_timed_threat: i32,
    pub(in crate::ai::combat_search_v2) timed_threat_urgency: i32,
    pub(in crate::ai::combat_search_v2) timed_threat_raw_damage: i32,
    pub(in crate::ai::combat_search_v2) target_progress: i32,
    pub(in crate::ai::combat_search_v2) block: i32,
    pub(in crate::ai::combat_search_v2) damage: i32,
    pub(in crate::ai::combat_search_v2) cheaper_cost: i32,
    pub(in crate::ai::combat_search_v2) phase_setup: i32,
    pub(in crate::ai::combat_search_v2) phase_survival: i32,
    pub(in crate::ai::combat_search_v2) phase_transition_safety: i32,
    pub(in crate::ai::combat_search_v2) resource_timing: i32,
    pub(in crate::ai::combat_search_v2) pending_choice_primary: i32,
    pub(in crate::ai::combat_search_v2) pending_choice_secondary: i32,
    pub(in crate::ai::combat_search_v2) pending_choice_selected_count: i32,
    /// Signed base-two multiplier applied after ordinal rank is converted to
    /// a positive policy weight.  It is deliberately not part of `Ord`:
    /// categorical ordering and semantic concentration remain auditable.
    pub(in crate::ai::combat_search_v2) policy_log2_bias: i32,
    pub(in crate::ai::combat_search_v2) phase_hint: PhaseActionOrderingHint,
    pub(in crate::ai::combat_search_v2) effects: CardPlayEffectDiagnostics,
}

impl ActionOrderingPriority {
    pub(super) fn neutral(role: ActionOrderingRole) -> Self {
        Self {
            role,
            recoverable_resource_urgency: 0,
            role_rank: ROLE_END_TURN,
            lethal_external_payoff: 0,
            potion_tactical_rank: 0,
            mitigation: 0,
            action_supply: 0,
            reactive_risk: 0,
            targets_timed_threat: 0,
            timed_threat_urgency: 0,
            timed_threat_raw_damage: 0,
            target_progress: 0,
            block: 0,
            damage: 0,
            cheaper_cost: 0,
            phase_setup: 0,
            phase_survival: 0,
            phase_transition_safety: 0,
            resource_timing: 0,
            pending_choice_primary: 0,
            pending_choice_secondary: 0,
            pending_choice_selected_count: 0,
            policy_log2_bias: 0,
            phase_hint: PhaseActionOrderingHint::default(),
            effects: CardPlayEffectDiagnostics::default(),
        }
    }

    pub(in crate::ai::combat_search_v2) fn tactical_rank_v1(self) -> CombatTacticalActionRankV1 {
        CombatTacticalActionRankV1 {
            recoverable_resource_urgency: self.recoverable_resource_urgency,
            role_rank: self.role_rank,
            lethal_external_payoff: self.lethal_external_payoff,
            potion_tactical_rank: self.potion_tactical_rank,
            mitigation: self.mitigation,
            action_supply: self.action_supply,
            reactive_risk: self.reactive_risk,
            targets_timed_threat: self.targets_timed_threat,
            timed_threat_urgency: self.timed_threat_urgency,
            timed_threat_raw_damage: self.timed_threat_raw_damage,
            target_progress: self.target_progress,
            block: self.block,
            damage: self.damage,
            cheaper_cost: self.cheaper_cost,
            phase_setup: self.phase_setup,
            phase_survival: self.phase_survival,
            phase_transition_safety: self.phase_transition_safety,
            resource_timing: self.resource_timing,
            pending_choice_primary: self.pending_choice_primary,
            pending_choice_secondary: self.pending_choice_secondary,
            pending_choice_selected_count: self.pending_choice_selected_count,
            policy_log2_bias: self.policy_log2_bias,
        }
    }
}

impl Ord for ActionOrderingPriority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tactical_rank_v1().cmp(&other.tactical_rank_v1())
    }
}

impl PartialOrd for ActionOrderingPriority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_prefers_more_block_after_earlier_axes_tie() {
        let low = ActionOrderingPriority::neutral(ActionOrderingRole::Neutral);
        let mut high = low;
        high.block = 1;

        assert!(high > low);
    }

    #[test]
    fn ordering_prefers_more_damage_after_earlier_axes_tie() {
        let low = ActionOrderingPriority::neutral(ActionOrderingRole::Neutral);
        let mut high = low;
        high.damage = 1;

        assert!(high > low);
    }
}
