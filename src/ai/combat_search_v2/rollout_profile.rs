use super::types::CombatSearchV2RolloutPerformanceReport;

#[derive(Clone, Debug, Default)]
pub(super) struct RolloutPerformanceCounters {
    pub(super) cache_lookup_elapsed_us: u128,
    pub(super) policy_dispatch_elapsed_us: u128,
    pub(super) no_potion_iterations: u64,
    pub(super) no_potion_phase_profile_elapsed_us: u128,
    pub(super) no_potion_legal_actions_elapsed_us: u128,
    pub(super) no_potion_choose_action_elapsed_us: u128,
    pub(super) no_potion_choose_ordering_elapsed_us: u128,
    pub(super) no_potion_probe_elapsed_us: u128,
    pub(super) no_potion_probe_score_calls: u64,
    pub(super) no_potion_probe_actions_evaluated: u64,
    pub(super) no_potion_probe_step_reuses: u64,
    pub(super) no_potion_probe_engine_step_elapsed_us: u128,
    pub(super) no_potion_probe_phase_profile_elapsed_us: u128,
    pub(super) no_potion_probe_action_facts_elapsed_us: u128,
    pub(super) no_potion_engine_step_elapsed_us: u128,
    pub(super) no_potion_child_build_elapsed_us: u128,
}

impl RolloutPerformanceCounters {
    pub(super) fn to_report(&self) -> CombatSearchV2RolloutPerformanceReport {
        CombatSearchV2RolloutPerformanceReport {
            cache_lookup_us: self.cache_lookup_elapsed_us,
            policy_dispatch_us: self.policy_dispatch_elapsed_us,
            no_potion_iterations: self.no_potion_iterations,
            no_potion_phase_profile_us: self.no_potion_phase_profile_elapsed_us,
            no_potion_legal_actions_us: self.no_potion_legal_actions_elapsed_us,
            no_potion_choose_action_us: self.no_potion_choose_action_elapsed_us,
            no_potion_choose_ordering_us: self.no_potion_choose_ordering_elapsed_us,
            no_potion_probe_us: self.no_potion_probe_elapsed_us,
            no_potion_probe_score_calls: self.no_potion_probe_score_calls,
            no_potion_probe_actions_evaluated: self.no_potion_probe_actions_evaluated,
            no_potion_probe_step_reuses: self.no_potion_probe_step_reuses,
            no_potion_probe_engine_step_us: self.no_potion_probe_engine_step_elapsed_us,
            no_potion_probe_phase_profile_us: self.no_potion_probe_phase_profile_elapsed_us,
            no_potion_probe_action_facts_us: self.no_potion_probe_action_facts_elapsed_us,
            no_potion_engine_step_us: self.no_potion_engine_step_elapsed_us,
            no_potion_child_build_us: self.no_potion_child_build_elapsed_us,
        }
    }
}
