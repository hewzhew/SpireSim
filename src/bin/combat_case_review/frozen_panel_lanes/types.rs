use serde::Serialize;
use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchActionPriorPluginId, CombatSearchProfile, CombatSearchV2ChildRolloutPolicy,
    CombatSearchV2PotionPolicy, CombatSearchV2RolloutPolicy, CombatSearchV2TurnPlanPolicy,
};

use super::super::focus::CombatReviewFocus;
use super::super::key_card_lifecycle::KeyCardLifecycleReport;
use super::super::search_types::SearchReview;

#[derive(Serialize)]
pub(crate) struct FrozenPanelLaneReview {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) lanes: Vec<FrozenPanelLaneResult>,
}

#[derive(Serialize)]
pub(super) struct FrozenPanelLaneResult {
    pub(super) lane: &'static str,
    pub(super) search_config_summary: FrozenPanelLaneConfigSummary,
    pub(super) review: SearchReview,
    pub(super) focus: Option<CombatReviewFocus>,
    pub(super) key_card_lifecycle: Option<KeyCardLifecycleReport>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct FrozenPanelLaneConfigSummary {
    pub(super) max_nodes: usize,
    pub(super) wall_ms: u64,
    pub(super) turn_plan_policy: &'static str,
    pub(super) potion_policy: &'static str,
    pub(super) max_potions_used: u32,
    pub(super) rollout_policy: &'static str,
    pub(super) child_rollout_policy: &'static str,
    pub(super) setup_bias_policy: &'static str,
    pub(super) phase_guard_policy: &'static str,
}

impl FrozenPanelLaneConfigSummary {
    pub(super) fn from_profile(profile: CombatSearchProfile) -> Self {
        Self {
            max_nodes: profile.engine.budget.max_nodes,
            wall_ms: profile.engine.budget.wall_ms,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::from(profile.engine.plugins.turn_plan)
                .label(),
            potion_policy: potion_policy_label(profile.engine.plugins.potion.policy),
            max_potions_used: profile
                .engine
                .plugins
                .potion
                .max_potions_used
                .unwrap_or_default(),
            rollout_policy: CombatSearchV2RolloutPolicy::from(profile.engine.plugins.rollout)
                .label(),
            child_rollout_policy: CombatSearchV2ChildRolloutPolicy::from(
                profile.engine.plugins.child_rollout,
            )
            .label(),
            setup_bias_policy: profile.engine.plugins.action_prior.label(),
            phase_guard_policy: profile.engine.plugins.phase_guard.label(),
        }
    }
}

fn potion_policy_label(policy: CombatSearchV2PotionPolicy) -> &'static str {
    match policy {
        CombatSearchV2PotionPolicy::Never => "never",
        CombatSearchV2PotionPolicy::All => "all",
        CombatSearchV2PotionPolicy::SemanticBudgeted => "semantic_budgeted",
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrozenPanelLaneSpec {
    pub(crate) lane: &'static str,
    pub(crate) action_prior_plugin: CombatSearchActionPriorPluginId,
}

#[cfg(test)]
mod tests {
    use sts_oracle_runtime::ai::combat_search_v2::{
        CombatSearchAcceptancePluginId, CombatSearchArtifactPluginId, CombatSearchAttemptPolicy,
        CombatSearchBudgetSpec, CombatSearchChildRolloutPluginId, CombatSearchEngineProfile,
        CombatSearchPhaseGuardPluginId, CombatSearchPluginStack, CombatSearchPotionPlugin,
        CombatSearchProfile, CombatSearchRolloutPluginId, CombatSearchV2PotionPolicy,
    };

    use super::FrozenPanelLaneConfigSummary;

    #[test]
    fn config_summary_is_derived_from_the_profile_that_will_run() {
        let profile = CombatSearchProfile {
            label: "panel_lane",
            engine: CombatSearchEngineProfile {
                budget: CombatSearchBudgetSpec {
                    max_nodes: 12,
                    wall_ms: 34,
                },
                plugins: CombatSearchPluginStack {
                    rollout: CombatSearchRolloutPluginId::Disabled,
                    child_rollout: CombatSearchChildRolloutPluginId::Immediate,
                    potion: CombatSearchPotionPlugin {
                        policy: CombatSearchV2PotionPolicy::All,
                        max_potions_used: Some(3),
                        allowed_potion_slots: None,
                        allow_potion_discard: None,
                    },
                    phase_guard: CombatSearchPhaseGuardPluginId::TimeEaterClockHint,
                    ..CombatSearchPluginStack::default()
                },
            },
            policy: CombatSearchAttemptPolicy {
                acceptance: CombatSearchAcceptancePluginId::AcceptedLineOnly,
                artifacts: CombatSearchArtifactPluginId::None,
            },
        };

        let summary = FrozenPanelLaneConfigSummary::from_profile(profile);

        assert_eq!(summary.max_nodes, 12);
        assert_eq!(summary.wall_ms, 34);
        assert_eq!(summary.rollout_policy, "disabled");
        assert_eq!(summary.child_rollout_policy, "immediate");
        assert_eq!(summary.potion_policy, "all");
        assert_eq!(summary.max_potions_used, 3);
        assert_eq!(summary.phase_guard_policy, "time_eater_clock_hint");
    }
}
