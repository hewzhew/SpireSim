use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchProfile, CombatSearchV2Config, CombatSearchV2RootActionPrior,
};

#[derive(Default)]
pub(crate) struct ReviewSearchIntervention {
    input_label: Option<String>,
    root_action_prior: Option<CombatSearchV2RootActionPrior>,
}

impl ReviewSearchIntervention {
    pub(crate) fn with_input_label(mut self, label: impl Into<String>) -> Self {
        self.input_label = Some(label.into());
        self
    }

    pub(crate) fn with_root_action_prior(mut self, prior: CombatSearchV2RootActionPrior) -> Self {
        self.root_action_prior = Some(prior);
        self
    }

    pub(crate) fn apply(self, mut config: CombatSearchV2Config) -> CombatSearchV2Config {
        if self.input_label.is_some() {
            config.input_label = self.input_label;
        }
        if self.root_action_prior.is_some() {
            config.root_action_prior = self.root_action_prior;
        }
        config
    }

    pub(crate) fn apply_to_profile(self, profile: CombatSearchProfile) -> CombatSearchV2Config {
        self.apply(profile.to_config())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use sts_oracle_runtime::ai::combat_search_v2::{
        CombatSearchAcceptancePluginId, CombatSearchArtifactPluginId, CombatSearchAttemptPolicy,
        CombatSearchBudgetSpec, CombatSearchEngineProfile, CombatSearchPluginStack,
        CombatSearchProfile, CombatSearchV2Config, CombatSearchV2RolloutPolicy,
        CombatSearchV2RootActionPrior,
    };

    use super::ReviewSearchIntervention;

    #[test]
    fn intervention_only_patches_experiment_fields() {
        let mut action_scores = HashMap::new();
        action_scores.insert("play/DemonForm".to_string(), 10.0);
        let mut scores_by_state = HashMap::new();
        scores_by_state.insert("state-a".to_string(), action_scores);
        let prior = CombatSearchV2RootActionPrior::from_scores(scores_by_state);

        let config = ReviewSearchIntervention::default()
            .with_input_label("probe-a")
            .with_root_action_prior(prior)
            .apply(CombatSearchV2Config {
                max_nodes: 123,
                wall_time: Some(Duration::from_millis(456)),
                rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
                ..CombatSearchV2Config::default()
            });

        assert_eq!(config.max_nodes, 123);
        assert_eq!(config.wall_time, Some(Duration::from_millis(456)));
        assert_eq!(config.rollout_policy, CombatSearchV2RolloutPolicy::Disabled);
        assert_eq!(config.input_label.as_deref(), Some("probe-a"));
        assert_eq!(
            config
                .root_action_prior
                .as_ref()
                .and_then(|prior| prior.score("state-a", "play/DemonForm")),
            Some(10.0)
        );
    }

    #[test]
    fn intervention_can_materialize_profile_without_erasing_profile_budget() {
        let profile = CombatSearchProfile {
            label: "profile-a",
            engine: CombatSearchEngineProfile {
                budget: CombatSearchBudgetSpec {
                    max_nodes: 17,
                    wall_ms: 23,
                },
                plugins: CombatSearchPluginStack::default(),
            },
            policy: CombatSearchAttemptPolicy {
                acceptance: CombatSearchAcceptancePluginId::AcceptedLineOnly,
                artifacts: CombatSearchArtifactPluginId::None,
            },
        };

        let config = ReviewSearchIntervention::default()
            .with_input_label("intervened")
            .apply_to_profile(profile);

        assert_eq!(config.max_nodes, 17);
        assert_eq!(config.wall_time, Some(Duration::from_millis(23)));
        assert_eq!(config.input_label.as_deref(), Some("intervened"));
    }
}
