//! Adapter from the retained tactical implementation to the neutral policy
//! contract.
//!
//! The implementation remains temporarily co-located with AtomicExactV2 while
//! search consumers depend only on `sts_combat_tactics`. This adapter is
//! deliberately free of planner, runtime, and terminal-result authority.

use std::sync::Arc;

use sts_combat_tactics::{
    CombatActionPolicy, CombatPolicyChoice, CombatStateGuide, SharedCombatActionPolicy,
    COMBAT_HORIZON_GUIDE_LANE_V1, COMBAT_PROGRESS_GUIDE_LANE_V1, COMBAT_SETUP_GUIDE_LANE_V1,
    COMBAT_SURVIVAL_GUIDE_LANE_V1, COMBAT_TURN_DEPTH_GUIDE_LANE_V1,
};
use sts_core::sim::combat::CombatPosition;
use sts_core::sim::combat_action_surface::CombatSelectionActionFamilyV2;
use sts_core::state::core::ClientInput;

use crate::ai::combat_search_v2::oracle_action_policy;

#[derive(Clone, Copy, Debug, Default)]
pub struct RetainedCombatTacticalPolicyV1;

pub fn retained_combat_tactical_policy_v1() -> SharedCombatActionPolicy {
    Arc::new(RetainedCombatTacticalPolicyV1)
}

impl CombatActionPolicy for RetainedCombatTacticalPolicyV1 {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        let atomic_inputs = choices
            .iter()
            .filter_map(|choice| match choice {
                CombatPolicyChoice::Atomic(input) => Some(*input),
                CombatPolicyChoice::StructuredSelection(_) => None,
            })
            .collect::<Vec<_>>();
        let mut atomic_weights =
            oracle_action_policy::oracle_legal_atomic_action_policy_weights_for_refs(
                position,
                &atomic_inputs,
            )
            .into_iter();
        choices
            .iter()
            .map(|choice| match choice {
                CombatPolicyChoice::Atomic(_) => atomic_weights.next().unwrap_or(1.0),
                CombatPolicyChoice::StructuredSelection(_) => 1.0,
            })
            .collect()
    }

    fn structured_selection_member_weights(
        &self,
        position: &CombatPosition,
        _family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        oracle_action_policy::oracle_atomic_action_policy_weights(position, members)
    }

    fn state_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        let guides = oracle_action_policy::oracle_combat_guide_bundle_v1(position);
        vec![
            CombatStateGuide::new(COMBAT_PROGRESS_GUIDE_LANE_V1, guides.progress),
            CombatStateGuide::new(COMBAT_SURVIVAL_GUIDE_LANE_V1, guides.survival),
            CombatStateGuide::new(COMBAT_HORIZON_GUIDE_LANE_V1, guides.horizon),
            CombatStateGuide::new(COMBAT_SETUP_GUIDE_LANE_V1, guides.setup),
        ]
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        let guides = oracle_action_policy::oracle_combat_guide_bundle_v1(position);
        vec![
            CombatStateGuide::new(COMBAT_PROGRESS_GUIDE_LANE_V1, guides.progress),
            CombatStateGuide::new(COMBAT_SURVIVAL_GUIDE_LANE_V1, guides.survival),
            CombatStateGuide::new(COMBAT_TURN_DEPTH_GUIDE_LANE_V1, guides.turn_generation),
            CombatStateGuide::new(COMBAT_SETUP_GUIDE_LANE_V1, guides.setup),
        ]
    }
}

#[cfg(test)]
mod tests;
