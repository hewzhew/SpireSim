use super::super::*;
use super::collector::SearchDiagnosticsCollector;
use super::ratio::{resource_vector_count, rounded_ratio};
use super::FRONTIER_SAMPLE_LIMIT;

pub(in crate::ai::combat_search_v2) struct SearchDiagnosticsFinish<'a> {
    pub(in crate::ai::combat_search_v2) exact_transpositions:
        &'a HashMap<CombatExactStateKey, Vec<ResourceVector>>,
    pub(in crate::ai::combat_search_v2) dominance:
        &'a HashMap<CombatDominanceKey, Vec<ResourceVector>>,
    pub(in crate::ai::combat_search_v2) frontier_work_items: usize,
    pub(in crate::ai::combat_search_v2) frontier_sample_count: usize,
    pub(in crate::ai::combat_search_v2) stats: &'a CombatSearchV2Stats,
    pub(in crate::ai::combat_search_v2) coverage_status: SearchCoverageStatus,
    pub(in crate::ai::combat_search_v2) unresolved_leaf_count: u64,
    pub(in crate::ai::combat_search_v2) max_actions_cut_count: u64,
    pub(in crate::ai::combat_search_v2) engine_step_limit_count: u64,
    pub(in crate::ai::combat_search_v2) potion_budget_cut_count: u64,
}

impl SearchDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn finish(
        &self,
        input: SearchDiagnosticsFinish<'_>,
    ) -> CombatSearchV2DiagnosticsReport {
        let tables = CombatSearchV2DiagnosticsTables {
            exact_keys: input.exact_transpositions.len(),
            exact_resource_vectors: resource_vector_count(input.exact_transpositions),
            dominance_buckets: input.dominance.len(),
            dominance_resource_vectors: resource_vector_count(input.dominance),
        };
        let branching = CombatSearchV2DiagnosticsBranching {
            states_queried: self.states_queried,
            states_with_legal_actions: self.states_with_legal_actions,
            legal_actions_total: self.legal_actions_total,
            legal_actions_avg: rounded_ratio(self.legal_actions_total, self.states_queried),
            legal_actions_max: self.legal_actions_max,
            nodes_generated_per_expanded: rounded_ratio(
                input.stats.nodes_generated,
                input.stats.nodes_expanded,
            ),
        };
        let pruning = CombatSearchV2DiagnosticsPruning {
            transposition_prunes: input.stats.transposition_prunes,
            dominance_prunes: input.stats.dominance_prunes,
            turn_local_dominance_prunes: input.stats.turn_local_dominance_prunes,
            terminal_wins: input.stats.terminal_wins,
            terminal_losses: input.stats.terminal_losses,
            unresolved_leaf_count: input.unresolved_leaf_count,
            max_actions_cut_count: input.max_actions_cut_count,
            engine_step_limit_count: input.engine_step_limit_count,
            potion_budget_cut_count: input.potion_budget_cut_count,
        };
        let frontier = CombatSearchV2DiagnosticsFrontier {
            remaining_work_items: input.frontier_work_items,
            sample_limit: FRONTIER_SAMPLE_LIMIT,
            sampled_states: input.frontier_sample_count,
        };
        let expansion = self.expansion.finish();
        let target_fanout = self.target_fanout.finish();
        let equivalence = self.equivalence.finish();
        let ordering = self.ordering.finish();
        let turn_branching = self.turn_branching.finish();
        let pending_choice = self.pending_choice.finish();
        let turn_prefix = self.turn_prefix.finish();
        let turn_sequence = self.turn_sequence.finish();
        let turn_plan = self.turn_plan.finish();
        let card_identity = self.card_identity.finish();
        let turn_local_dominance = self.turn_local_dominance.finish();
        let diagnosis = diagnosis_tags(
            input.coverage_status,
            input.stats,
            &branching,
            &expansion,
            &target_fanout,
            &equivalence,
            &ordering,
            &turn_branching,
            &pending_choice,
            &turn_prefix,
            &turn_sequence,
            &turn_plan,
            &card_identity,
            &turn_local_dominance,
            &pruning,
            frontier.remaining_work_items,
        );

        CombatSearchV2DiagnosticsReport {
            schema_version: 15,
            mode: "summary",
            tables,
            branching,
            expansion,
            target_fanout,
            equivalence,
            ordering,
            turn_branching,
            pending_choice,
            turn_prefix,
            turn_sequence,
            turn_plan,
            card_identity,
            turn_local_dominance,
            pruning,
            frontier,
            diagnosis,
        }
    }
}
