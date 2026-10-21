use sts_oracle_runtime::ai::combat_search_v2::{
    compile_combat_search_witness_prior_v0, CombatSearchV2WitnessLine,
};
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::super::search_intervention::ReviewSearchIntervention;
use super::super::search_runner::run_config_search;
use super::feedback_comparison::compare_success_feedback;
use super::specs::QualityLaneSpec;
use super::types::{CombatSuccessFeedbackMetrics, CombatSuccessFeedbackRerun};

pub(super) struct CombatSuccessFeedbackSource {
    pub(super) spec: QualityLaneSpec,
    pub(super) baseline: CombatSuccessFeedbackMetrics,
    pub(super) witness: CombatSearchV2WitnessLine,
    pub(super) source_kind: &'static str,
}

pub(super) fn run_success_feedback_rerun(
    case: &CombatCase,
    source: CombatSuccessFeedbackSource,
    max_nodes: usize,
    wall_ms: u64,
    action_preview_limit: usize,
) -> Option<CombatSuccessFeedbackRerun> {
    let witness_prior =
        compile_combat_search_witness_prior_v0(&case.core.position, &source.witness);
    if witness_prior.prior.is_empty() {
        return None;
    }
    let prior_states = witness_prior.prior_states;
    let duplicate_prior_hints = witness_prior.duplicate_prior_hints;
    let config = ReviewSearchIntervention::default()
        .with_input_label(format!("success_feedback_rerun:{}", source.spec.label))
        .with_root_action_prior(witness_prior.prior)
        .apply(source.spec.config(max_nodes, wall_ms));
    let (rerun, _report) = run_config_search(
        "quality_success_feedback_rerun",
        case,
        config,
        action_preview_limit,
    );
    let comparison = compare_success_feedback(&source.baseline, &rerun);
    Some(CombatSuccessFeedbackRerun {
        schema: "combat_success_feedback_rerun_v0",
        contract: "best_complete_or_estimated_rollout_witness_compiled_to_exact_state_action_prior_then_rerun_with_same_lane_budget",
        source_kind: source.source_kind,
        source_lane: source.spec.label,
        witness_action_count: source.witness.actions.len(),
        prior_states,
        duplicate_prior_hints,
        baseline: source.baseline,
        rerun,
        comparison,
    })
}
