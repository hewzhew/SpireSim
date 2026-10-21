use sts_oracle_runtime::ai::strategy::deck_strategic_deficit::DeckStrategicDeficit;
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::classification::CombatGapReviewClassification;
use super::focus::CombatReviewFocus;
use super::search_types::SearchReview;

#[path = "strategic_feedback/observations.rs"]
mod observations;
#[path = "strategic_feedback/signal_context.rs"]
mod signal_context;
#[path = "strategic_feedback/signals.rs"]
mod signals;
#[path = "strategic_feedback/site.rs"]
mod site;
#[path = "strategic_feedback/types.rs"]
mod types;

pub(super) use types::CombatStrategicFeedbackReport;

use observations::feedback_observations;
use signals::strategic_signals;
use site::combat_site;

pub(super) fn combat_strategic_feedback(
    case: &CombatCase,
    static_deficit: &DeckStrategicDeficit,
    classification: &CombatGapReviewClassification,
    focus: Option<&CombatReviewFocus>,
    ladder: &[SearchReview],
) -> Option<CombatStrategicFeedbackReport> {
    if !should_emit_strategic_feedback(classification.kind, !ladder.is_empty()) {
        return None;
    }

    let site = combat_site(&case.core.combat.enemies);
    let progress = focus.map(|focus| &focus.progress);
    Some(CombatStrategicFeedbackReport {
        schema: "combat_strategic_feedback_v0",
        site,
        signals: strategic_signals(case, static_deficit, classification, progress, site, ladder),
        observations: feedback_observations(case, static_deficit, classification, progress),
    })
}

fn should_emit_strategic_feedback(classification_kind: &str, has_ladder: bool) -> bool {
    has_ladder && classification_kind != "SavedCompleteWinRejectedByPolicy"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_policy_rejection_does_not_emit_deck_failure_feedback() {
        assert!(!should_emit_strategic_feedback(
            "SavedCompleteWinRejectedByPolicy",
            true,
        ));
        assert!(should_emit_strategic_feedback(
            "StillNoWinAfterReview",
            true,
        ));
        assert!(!should_emit_strategic_feedback(
            "StillNoWinAfterReview",
            false,
        ));
    }
}
