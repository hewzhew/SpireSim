use super::super::*;

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct SearchDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2::diagnostics) states_queried: u64,
    pub(in crate::ai::combat_search_v2::diagnostics) states_with_legal_actions: u64,
    pub(in crate::ai::combat_search_v2::diagnostics) legal_actions_total: u64,
    pub(in crate::ai::combat_search_v2::diagnostics) legal_actions_max: usize,
    pub(in crate::ai::combat_search_v2::diagnostics) expansion: ActionExpansionDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) target_fanout:
        TargetFanoutDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) equivalence:
        ActionEquivalenceDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) ordering: ActionOrderingDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) turn_branching:
        TurnBranchingDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) pending_choice:
        PendingChoiceDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) turn_prefix: TurnPrefixDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) turn_sequence:
        TurnSequenceDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) turn_plan: TurnPlanDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) card_identity:
        CardIdentityDiagnosticsCollector,
    pub(in crate::ai::combat_search_v2::diagnostics) turn_local_dominance:
        TurnLocalDominanceDiagnosticsCollector,
}

impl SearchDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe_legal_actions(
        &mut self,
        expansion: &ActionExpansionSummary,
    ) {
        let action_count = expansion.action_count;
        self.states_queried = self.states_queried.saturating_add(1);
        if action_count > 0 {
            self.states_with_legal_actions = self.states_with_legal_actions.saturating_add(1);
        }
        self.legal_actions_total = self.legal_actions_total.saturating_add(action_count as u64);
        self.legal_actions_max = self.legal_actions_max.max(action_count);
        self.expansion.observe(expansion);
    }

    pub(in crate::ai::combat_search_v2) fn observe_action_ordering(
        &mut self,
        ordering: &ActionOrderingSummary,
    ) {
        self.ordering.observe(ordering);
    }

    pub(in crate::ai::combat_search_v2) fn observe_target_fanout(
        &mut self,
        target_fanout: &TargetFanoutSummary,
    ) {
        self.target_fanout.observe(target_fanout);
    }

    pub(in crate::ai::combat_search_v2) fn observe_action_equivalence(
        &mut self,
        equivalence: &ActionEquivalenceSummary,
    ) {
        self.equivalence.observe(equivalence);
    }

    pub(in crate::ai::combat_search_v2) fn observe_turn_branching(
        &mut self,
        observation: &TurnBranchingStateObservation,
    ) {
        self.turn_branching.observe(observation);
    }

    pub(in crate::ai::combat_search_v2) fn observe_pending_choice(
        &mut self,
        profile: Option<&PendingChoiceProfile>,
    ) {
        self.pending_choice.observe(profile);
    }

    pub(in crate::ai::combat_search_v2) fn observe_pending_choice_ordering(
        &mut self,
        profile: Option<&PendingChoiceProfile>,
        ordering: &ActionOrderingSummary,
    ) {
        self.pending_choice.observe_ordering(profile, ordering);
    }

    pub(in crate::ai::combat_search_v2) fn observe_pending_choice_child_transition(
        &mut self,
        profile: Option<&PendingChoiceProfile>,
        truncated: bool,
        child_engine: &EngineState,
    ) {
        self.pending_choice
            .observe_child_transition(profile, truncated, child_engine);
    }

    pub(in crate::ai::combat_search_v2) fn observe_turn_prefix(
        &mut self,
        summary: &TurnPrefixSummary,
    ) {
        self.turn_prefix.observe(summary);
    }

    pub(in crate::ai::combat_search_v2) fn observe_turn_sequence(
        &mut self,
        summary: &TurnSequenceSummary,
        node: &SearchNode,
    ) {
        self.turn_sequence.observe_with_node(summary, node);
    }

    pub(in crate::ai::combat_search_v2) fn observe_root_turn_plan(
        &mut self,
        root: &SearchNode,
        stepper: &impl CombatStepper,
    ) {
        self.turn_plan.observe_root(root, stepper);
    }

    pub(in crate::ai::combat_search_v2) fn observe_turn_plan_frontier_seeded_plans(
        &mut self,
        plans: &[super::super::turn_planner::TurnPlanV1],
    ) {
        self.turn_plan.observe_frontier_seeded_plans(plans);
    }

    pub(in crate::ai::combat_search_v2) fn observe_turn_plan_prior_scored_plans(
        &mut self,
        plans: usize,
    ) {
        self.turn_plan.observe_prior_scored_plans(plans);
    }

    pub(in crate::ai::combat_search_v2) fn observe_card_identity(
        &mut self,
        summary: &CardIdentitySummary,
    ) {
        self.card_identity.observe(summary);
    }

    pub(in crate::ai::combat_search_v2) fn observe_turn_local_dominance(
        &mut self,
        observation: &TurnLocalDominanceStateObservation,
    ) {
        self.turn_local_dominance.observe(observation);
    }

    pub(in crate::ai::combat_search_v2) fn run_discard_order_exact_shadow_audit(
        &mut self,
        stepper: &impl CombatStepper,
        config: &CombatSearchV2Config,
        plugins: &CombatSearchPluginStack,
    ) {
        self.turn_sequence
            .run_discard_order_exact_shadow_audit(stepper, config, plugins);
    }
}
