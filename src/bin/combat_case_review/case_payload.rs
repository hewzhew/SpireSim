use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::key_card_lifecycle::key_card_lifecycle;
use super::strategic_feedback::combat_strategic_feedback;

#[path = "case_payload/derived.rs"]
mod derived;
#[path = "case_payload/types.rs"]
mod types;

use derived::derived_payload_from_case;
pub(super) use types::{CombatCaseReview, CombatCaseReviewArtifacts};

pub(super) fn assemble_combat_case_review(
    case_path: String,
    case: CombatCase,
    artifacts: CombatCaseReviewArtifacts,
) -> CombatCaseReview {
    let CombatCaseReviewArtifacts {
        ladder,
        classification,
        review_focus,
        review_focus_replay,
        review_focus_prior_rerun,
        line_lab,
        quality_lanes,
        counterfactual_hp_probe,
        combat_deficit_evidence,
        static_boss_matchup_audit_v0,
        boss_matchup_acquisition_pressure_v0,
        awakened_one_failure_evidence,
        awakened_one_path_audit_v0,
        awakened_opening_probe,
        power_setup_counterfactual,
        boss_pressure_lens,
        frozen_panel_lanes,
        champ_phase_audit,
        adjudication_probe,
    } = artifacts;
    let derived = derived_payload_from_case(&case);
    let combat_strategic_feedback = combat_strategic_feedback(
        &case,
        &derived.static_strategic_deficit,
        &classification,
        review_focus.as_ref(),
        &ladder,
    );
    let key_card_lifecycle = key_card_lifecycle(&case.core.position, review_focus.as_ref());
    CombatCaseReview {
        schema: "combat_case_review",
        case_path,
        static_strategic_deficit: derived.static_strategic_deficit,
        deck: derived.deck,
        relics: derived.relics,
        potions: derived.potions,
        path_tail: derived.path_tail,
        saved_atomic_combat_search: case.failed_atomic_combat_search.clone(),
        adjudication_probe,
        source: case.core.source,
        gap: case.core.gap,
        run: case.core.run,
        combat: case.core.combat,
        ladder,
        classification,
        review_focus,
        review_focus_replay,
        review_focus_prior_rerun,
        line_lab,
        quality_lanes,
        counterfactual_hp_probe,
        combat_deficit_evidence,
        combat_strategic_feedback,
        static_boss_matchup_audit_v0,
        boss_matchup_acquisition_pressure_v0,
        awakened_one_failure_evidence,
        awakened_one_path_audit_v0,
        awakened_opening_probe,
        power_setup_counterfactual,
        boss_pressure_lens,
        frozen_panel_lanes,
        champ_phase_audit,
        key_card_lifecycle,
    }
}
