//! Explicit laboratory policy wrappers.

use std::path::Path;
use std::sync::Arc;

use sts_combat_planner::{
    CombatActionPolicy, CombatGuideLaneId, CombatPolicyChoice, CombatStateGuide,
    SharedCombatActionPolicy,
};
use sts_oracle_runtime::eval::combat_action_imitation::{
    combat_action_imitation_policy_v1, CombatActionImitationArtifactV1,
};
use sts_oracle_runtime::sim::combat::CombatPosition;
use sts_oracle_runtime::sim::combat_action_surface::CombatSelectionActionFamilyV2;
use sts_oracle_runtime::state::core::ClientInput;

struct AnchorOnlyPolicy {
    base: SharedCombatActionPolicy,
}

struct RootTurnAnchorOnlyPolicy {
    root_player_turn: u32,
    base: SharedCombatActionPolicy,
}

struct OmitGuideLanePolicy {
    lane: CombatGuideLaneId,
    base: SharedCombatActionPolicy,
}

pub(super) fn load_action_imitation_policy(
    path: &Path,
    base: SharedCombatActionPolicy,
) -> Result<SharedCombatActionPolicy, String> {
    let artifact = CombatActionImitationArtifactV1::load(path)?;
    combat_action_imitation_policy_v1(base, artifact)
}

impl CombatActionPolicy for AnchorOnlyPolicy {
    fn weights(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
        choices: &[CombatPolicyChoice<'_>],
    ) -> Vec<f64> {
        self.base.weights(position, choices)
    }

    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> sts_combat_planner::CombatSelectionPreferences {
        self.base.structured_selection_preferences(position, family)
    }

    fn structured_selection_member_weights(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
        family: &sts_oracle_runtime::sim::combat_action_surface::CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        self.base
            .structured_selection_member_weights(position, family, members)
    }

    fn state_guides(
        &self,
        _position: &sts_oracle_runtime::sim::combat::CombatPosition,
    ) -> Vec<CombatStateGuide> {
        Vec::new()
    }

    fn turn_generation_guides(
        &self,
        _position: &sts_oracle_runtime::sim::combat::CombatPosition,
    ) -> Vec<CombatStateGuide> {
        Vec::new()
    }
}

pub(super) fn anchor_only_policy(base: SharedCombatActionPolicy) -> SharedCombatActionPolicy {
    Arc::new(AnchorOnlyPolicy { base })
}

impl CombatActionPolicy for RootTurnAnchorOnlyPolicy {
    fn weights(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
        choices: &[CombatPolicyChoice<'_>],
    ) -> Vec<f64> {
        self.base.weights(position, choices)
    }

    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> sts_combat_planner::CombatSelectionPreferences {
        self.base.structured_selection_preferences(position, family)
    }

    fn structured_selection_member_weights(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
        family: &sts_oracle_runtime::sim::combat_action_surface::CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        self.base
            .structured_selection_member_weights(position, family, members)
    }

    fn state_guides(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
    ) -> Vec<CombatStateGuide> {
        if position.combat.turn.turn_count == self.root_player_turn {
            Vec::new()
        } else {
            self.base.state_guides(position)
        }
    }

    fn turn_generation_guides(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
    ) -> Vec<CombatStateGuide> {
        if position.combat.turn.turn_count == self.root_player_turn {
            Vec::new()
        } else {
            self.base.turn_generation_guides(position)
        }
    }
}

pub(super) fn root_turn_anchor_only_policy(
    root_player_turn: u32,
    base: SharedCombatActionPolicy,
) -> SharedCombatActionPolicy {
    Arc::new(RootTurnAnchorOnlyPolicy {
        root_player_turn,
        base,
    })
}

impl CombatActionPolicy for OmitGuideLanePolicy {
    fn weights(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
        choices: &[CombatPolicyChoice<'_>],
    ) -> Vec<f64> {
        self.base.weights(position, choices)
    }

    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> sts_combat_planner::CombatSelectionPreferences {
        self.base.structured_selection_preferences(position, family)
    }

    fn structured_selection_member_weights(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
        family: &sts_oracle_runtime::sim::combat_action_surface::CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        self.base
            .structured_selection_member_weights(position, family, members)
    }

    fn state_guides(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
    ) -> Vec<CombatStateGuide> {
        let mut guides = self.base.state_guides(position);
        guides.retain(|guide| guide.lane != self.lane);
        guides
    }

    fn turn_generation_guides(
        &self,
        position: &sts_oracle_runtime::sim::combat::CombatPosition,
    ) -> Vec<CombatStateGuide> {
        let mut guides = self.base.turn_generation_guides(position);
        guides.retain(|guide| guide.lane != self.lane);
        guides
    }
}

pub(super) fn omit_guide_lane_policy(
    lane: CombatGuideLaneId,
    base: SharedCombatActionPolicy,
) -> SharedCombatActionPolicy {
    Arc::new(OmitGuideLanePolicy { lane, base })
}
