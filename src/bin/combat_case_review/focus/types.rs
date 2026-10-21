use serde::Serialize;
use sts_oracle_runtime::sim::combat::CombatTerminal;

use super::super::search_types::{SearchDiagnosticProgressFacts, SearchReview};

#[derive(Serialize)]
pub(crate) struct CombatReviewFocus {
    pub(crate) selected_review: &'static str,
    pub(crate) reason: &'static str,
    pub(crate) progress: SearchDiagnosticProgressFacts,
}

#[derive(Serialize)]
pub(crate) struct CombatReviewFocusPriorRerun {
    pub(crate) selected_review: &'static str,
    pub(crate) witness_replayed_actions: usize,
    pub(crate) witness_action_count: Option<usize>,
    pub(crate) witness_terminal: CombatTerminal,
    pub(crate) prior_states: usize,
    pub(crate) duplicate_prior_hints: usize,
    pub(crate) rerun: SearchReview,
}
