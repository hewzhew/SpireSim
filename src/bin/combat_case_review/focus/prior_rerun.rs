use sts_oracle_runtime::ai::combat_search_v2::{
    compile_combat_search_witness_prior_v0, CombatSearchV2PotionPolicy, CombatSearchV2WitnessReplay,
};
use sts_oracle_runtime::eval::combat_case::CombatCase;
use sts_oracle_runtime::sim::combat::CombatTerminal;

use super::super::options::ReviewOptions;
use super::super::search_intervention::ReviewSearchIntervention;
use super::super::search_runner::{review_search_profile, run_config_search};
use super::types::{CombatReviewFocus, CombatReviewFocusPriorRerun};
use super::witness::focus_witness_line;

pub(crate) fn witness_prior_rerun(
    options: &ReviewOptions,
    case: &CombatCase,
    focus: &CombatReviewFocus,
    replay: &CombatSearchV2WitnessReplay,
) -> Option<CombatReviewFocusPriorRerun> {
    if focus.progress.source != "rollout_frontier"
        || !matches!(replay.terminal, CombatTerminal::Win)
    {
        return None;
    }
    let witness_prior =
        compile_combat_search_witness_prior_v0(&case.core.position, &focus_witness_line(focus));
    if witness_prior.prior.is_empty() {
        return None;
    }
    let prior_states = witness_prior.prior_states;
    let duplicate_prior_hints = witness_prior.duplicate_prior_hints;
    let profile = review_search_profile(
        "focus_witness_prior_rerun",
        options.fast_nodes,
        options.fast_ms,
        options,
    )
    .with_potion_policy(CombatSearchV2PotionPolicy::Never)
    .with_max_potions_used(0);
    let config = ReviewSearchIntervention::default()
        .with_root_action_prior(witness_prior.prior)
        .apply_to_profile(profile);
    let (rerun, _) = run_config_search(
        "focus_witness_prior_rerun",
        case,
        config,
        options.action_preview_limit,
    );
    Some(CombatReviewFocusPriorRerun {
        selected_review: focus.selected_review,
        witness_replayed_actions: replay.replayed_actions,
        witness_action_count: focus.progress.action_count,
        witness_terminal: replay.terminal,
        prior_states,
        duplicate_prior_hints,
        rerun,
    })
}
