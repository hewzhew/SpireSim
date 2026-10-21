use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchActionPriorPluginId, CombatSearchProfile, CombatSearchRolloutPluginId,
};

use super::super::options::ReviewOptions;
use super::super::search_runner::review_all_potions_profile;
use super::types::FrozenPanelLaneSpec;

impl FrozenPanelLaneSpec {
    pub(super) fn profile(
        self,
        options: &ReviewOptions,
        rollout_plugin: CombatSearchRolloutPluginId,
    ) -> CombatSearchProfile {
        review_all_potions_profile(self.lane, options.slow_nodes, options.slow_ms, options)
            .with_action_prior_plugin(self.action_prior_plugin)
            .with_rollout_plugin(rollout_plugin)
    }
}

pub(crate) fn frozen_panel_lane_specs() -> [FrozenPanelLaneSpec; 2] {
    [
        FrozenPanelLaneSpec {
            lane: "baseline",
            action_prior_plugin: CombatSearchActionPriorPluginId::Default,
        },
        FrozenPanelLaneSpec {
            lane: "key_setup_bias",
            action_prior_plugin: CombatSearchActionPriorPluginId::KeyCardOnline,
        },
    ]
}

#[cfg(test)]
mod tests {
    use sts_oracle_runtime::ai::combat_search_v2::CombatSearchActionPriorPluginId;

    use super::frozen_panel_lane_specs;

    #[test]
    fn frozen_panel_lane_specs_only_differ_by_setup_bias() {
        let specs = frozen_panel_lane_specs();

        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].lane, "baseline");
        assert_eq!(
            specs[0].action_prior_plugin,
            CombatSearchActionPriorPluginId::Default
        );
        assert_eq!(specs[1].lane, "key_setup_bias");
        assert_eq!(
            specs[1].action_prior_plugin,
            CombatSearchActionPriorPluginId::KeyCardOnline
        );
    }
}
