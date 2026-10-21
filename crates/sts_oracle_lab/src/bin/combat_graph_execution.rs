//! Resolved execution controls for the local turn-graph laboratory command.
//!
//! This module owns how orthogonal CLI controls compose into one executable
//! search profile. It deliberately does not load cases, advance search, write
//! artifacts, or render reports.

use serde::Serialize;
use sts_combat_planner::{
    combat_plan_selection_timing_policy_v1, combat_plan_state_guide_policy_v1, CombatDecisionRoot,
    LocalTurnGraphWitnessConfig, LocalTurnGraphWitnessSession, SharedCombatActionPolicy,
};

use super::combat_policy_controls::{
    anchor_only_policy, omit_guide_lane_policy, root_turn_anchor_only_policy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LocalGraphGuideService {
    AnchorOnly,
    RootTurnAnchorThenGuides,
    AnchorAndGuides,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct LocalGraphExecutionProfile {
    guide_service: LocalGraphGuideService,
    typed_plan_state_guide: bool,
    plan_selection_timing: bool,
    omitted_guide_lane: Option<u32>,
}

impl LocalGraphExecutionProfile {
    pub(super) fn from_controls(
        anchor_only: bool,
        root_turn_anchor_only: bool,
        typed_plan_state_guide: bool,
        plan_selection_timing: bool,
        omitted_guide_lane: Option<u32>,
    ) -> Result<Self, String> {
        if anchor_only && root_turn_anchor_only {
            return Err("anchor-only conflicts with root-turn-anchor-only".to_owned());
        }
        if anchor_only && typed_plan_state_guide {
            return Err("typed-plan-guide conflicts with anchor-only".to_owned());
        }
        if omitted_guide_lane == Some(0) {
            return Err("omit-guide-lane requires a positive typed lane id".to_owned());
        }
        if anchor_only && omitted_guide_lane.is_some() {
            return Err("omit-guide-lane conflicts with anchor-only".to_owned());
        }
        Ok(Self {
            guide_service: if anchor_only {
                LocalGraphGuideService::AnchorOnly
            } else if root_turn_anchor_only {
                LocalGraphGuideService::RootTurnAnchorThenGuides
            } else {
                LocalGraphGuideService::AnchorAndGuides
            },
            typed_plan_state_guide,
            plan_selection_timing,
            omitted_guide_lane,
        })
    }

    /// Historical summary retained in the V1 report alongside the complete
    /// typed profile. It describes graph service, not every policy overlay.
    pub(super) fn scheduler_label(self) -> &'static str {
        match self.guide_service {
            LocalGraphGuideService::AnchorOnly => "anchor_only",
            LocalGraphGuideService::RootTurnAnchorThenGuides => "root_turn_anchor_then_guides",
            LocalGraphGuideService::AnchorAndGuides => "anchor_and_guides",
        }
    }

    pub(super) fn prepare_session(
        self,
        root: CombatDecisionRoot,
        root_player_turn: u32,
        config: LocalTurnGraphWitnessConfig,
        base_policy: SharedCombatActionPolicy,
    ) -> LocalTurnGraphWitnessSession {
        let policy = self.decorate_policy(root_player_turn, base_policy);
        LocalTurnGraphWitnessSession::with_policy(root, config, policy)
    }

    fn decorate_policy(
        self,
        root_player_turn: u32,
        base_policy: SharedCombatActionPolicy,
    ) -> SharedCombatActionPolicy {
        // Policy overlays are established before guide suppression so an
        // anchor-only service promise remains authoritative for every guide,
        // including plan-owned lanes.
        let policy = if self.typed_plan_state_guide {
            combat_plan_state_guide_policy_v1(base_policy)
        } else {
            base_policy
        };
        let policy = if self.plan_selection_timing {
            combat_plan_selection_timing_policy_v1(policy)
        } else {
            policy
        };
        let policy = if let Some(lane) = self.omitted_guide_lane {
            omit_guide_lane_policy(sts_combat_planner::CombatGuideLaneId::new(lane), policy)
        } else {
            policy
        };
        match self.guide_service {
            LocalGraphGuideService::AnchorOnly => anchor_only_policy(policy),
            LocalGraphGuideService::RootTurnAnchorThenGuides => {
                root_turn_anchor_only_policy(root_player_turn, policy)
            }
            LocalGraphGuideService::AnchorAndGuides => policy,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use sts_combat_planner::{
        CombatActionPolicy, CombatGuideLaneId, CombatPolicyChoice, CombatStateGuide,
    };
    use sts_oracle_runtime::sim::combat::CombatPosition;
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::test_support::blank_test_combat;

    struct OneGuidePolicy;

    impl CombatActionPolicy for OneGuidePolicy {
        fn weights(
            &self,
            _position: &CombatPosition,
            choices: &[CombatPolicyChoice<'_>],
        ) -> Vec<f64> {
            vec![1.0; choices.len()]
        }

        fn state_guides(&self, _position: &CombatPosition) -> Vec<CombatStateGuide> {
            vec![CombatStateGuide::new(CombatGuideLaneId::new(7), vec![1])]
        }
    }

    #[test]
    fn execution_profile_reports_every_orthogonal_control() {
        let profile = LocalGraphExecutionProfile::from_controls(false, false, true, true, Some(3))
            .expect("valid profile");
        let value = serde_json::to_value(profile).expect("serialize profile");

        assert_eq!(value["guide_service"], "anchor_and_guides");
        assert_eq!(value["typed_plan_state_guide"], true);
        assert_eq!(value["plan_selection_timing"], true);
        assert_eq!(value["omitted_guide_lane"], 3);
        assert_eq!(profile.scheduler_label(), "anchor_and_guides");
    }

    #[test]
    fn invalid_combinations_are_rejected_below_clap() {
        assert!(LocalGraphExecutionProfile::from_controls(true, true, false, false, None).is_err());
        assert!(LocalGraphExecutionProfile::from_controls(true, false, true, false, None).is_err());
        assert!(
            LocalGraphExecutionProfile::from_controls(false, false, false, false, Some(0)).is_err()
        );
    }

    #[test]
    fn root_turn_anchor_suppression_wraps_every_inner_guide() {
        let profile = LocalGraphExecutionProfile::from_controls(false, true, true, false, None)
            .expect("valid root-turn profile");
        let mut position = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
        let root_turn = position.combat.turn.turn_count;
        let policy = profile.decorate_policy(root_turn, Arc::new(OneGuidePolicy));

        assert!(policy.state_guides(&position).is_empty());
        position.combat.turn.turn_count += 1;
        assert!(!policy.state_guides(&position).is_empty());
    }

    #[test]
    fn omitted_guide_lane_filters_only_that_typed_lane() {
        let profile =
            LocalGraphExecutionProfile::from_controls(false, false, false, false, Some(7))
                .expect("valid omitted guide profile");
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
        let policy = profile.decorate_policy(0, Arc::new(OneGuidePolicy));

        assert!(policy.state_guides(&position).is_empty());
        assert_eq!(policy.weights(&position, &[]), Vec::<f64>::new());
    }
}
