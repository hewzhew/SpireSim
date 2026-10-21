use sts_oracle_runtime::ai::boss_matchup::boss_matchup_acquisition_pressure_v0;
use sts_oracle_runtime::ai::combat_search_v2::{
    derive_combat_deficit_evidence, replay_combat_search_witness_line_v0,
};
use sts_oracle_runtime::eval::combat_case::CombatCase;

#[path = "review_pipeline/ladder.rs"]
mod ladder;

use super::adjudication_probe::{
    run_adjudication_probe, run_candidate_censuses, run_persistent_burden_cutpoint_probes,
};
use super::awakened_one_evidence::{
    awakened_one_failure_evidence, awakened_one_path_audit_v0, static_boss_matchup_audit_v0,
};
use super::awakened_opening_probe::run_awakened_opening_probe;
use super::boss_pressure_lens::boss_pressure_lens;
use super::case_payload::{
    assemble_combat_case_review, CombatCaseReview, CombatCaseReviewArtifacts,
};
use super::champ_phase::champ_phase_audit;
use super::classification::classify_gap_review;
use super::counterfactual_hp::run_counterfactual_hp_probe;
use super::focus::{focus_witness_line, review_focus, witness_prior_rerun};
use super::frozen_panel_lanes::run_frozen_panel_lanes;
use super::line_lab::run_line_lab;
use super::options::ReviewOptions;
use super::power_setup_counterfactual::run_power_setup_counterfactual;
use super::quality_lanes::run_quality_lanes;
use ladder::{run_review_ladder, ReviewLadderRun};

pub(super) fn build_review(
    case_path: String,
    options: ReviewOptions,
    case: CombatCase,
) -> CombatCaseReview {
    let ReviewLadderRun {
        reviews: mut ladder,
        line_lab_parent,
        adjudication_runs,
    } = run_review_ladder(&options, &case);
    let review_focus = review_focus(&ladder);
    let adjudication_probe = run_adjudication_probe(
        options.adjudicate,
        &adjudication_runs,
        review_focus.as_ref().map(|focus| focus.selected_review),
        Some(&case),
    );
    if let Some(censuses) =
        run_candidate_censuses(options.adjudicate, &adjudication_runs, Some(&case))
    {
        for census in censuses {
            let attached = ladder
                .iter_mut()
                .any(|review| review.attach_candidate_adjudication_census(census.clone()));
            debug_assert!(attached, "candidate census must match one ladder row");
        }
    }
    if let Some(probes) =
        run_persistent_burden_cutpoint_probes(options.adjudicate, &adjudication_runs, Some(&case))
    {
        for probe in probes {
            let attached = ladder
                .iter_mut()
                .any(|review| review.attach_persistent_burden_cutpoint_probe(probe.clone()));
            debug_assert!(
                attached,
                "persistent burden probe must match one ladder row"
            );
        }
    }
    let classification = classify_gap_review(
        case.failed_atomic_combat_search.as_ref(),
        &ladder,
        review_focus.as_ref(),
    );
    let review_focus_replay = if options.replay_focus {
        review_focus.as_ref().map(|focus| {
            replay_combat_search_witness_line_v0(&case.core.position, &focus_witness_line(focus))
        })
    } else {
        None
    };
    let review_focus_prior_rerun = review_focus
        .as_ref()
        .zip(review_focus_replay.as_ref())
        .and_then(|(focus, replay)| witness_prior_rerun(&options, &case, focus, replay));
    let line_lab = run_line_lab(&options, &case, line_lab_parent.as_ref());
    let combat_deficit_evidence = line_lab.as_ref().map(derive_combat_deficit_evidence);
    let boss_pressure_lens = boss_pressure_lens(&case, &ladder, line_lab.as_ref());
    let frozen_panel_lanes = run_frozen_panel_lanes(&options, &case);
    let quality_lanes = if options.quality_lanes {
        Some(run_quality_lanes(&options, &case))
    } else {
        None
    };
    let counterfactual_hp_probe = if options.counterfactual_hp_probe {
        Some(run_counterfactual_hp_probe(&options, &case))
    } else {
        None
    };
    let static_boss_matchup_audit_v0 = static_boss_matchup_audit_v0(&case);
    let boss_matchup_acquisition_pressure_v0 =
        boss_matchup_acquisition_pressure_v0(&case.core.position.combat);
    let awakened_one_path_audit_v0 = awakened_one_path_audit_v0(&case);
    let awakened_one_failure_evidence =
        awakened_one_failure_evidence(&case, counterfactual_hp_probe.as_ref());
    let awakened_opening_probe = run_awakened_opening_probe(&options, &case);
    let power_setup_counterfactual = if options.power_setup_counterfactual {
        Some(run_power_setup_counterfactual(&options, &case))
    } else {
        None
    };
    let champ_phase_audit = review_focus
        .as_ref()
        .and_then(|focus| champ_phase_audit(&case.core.position, focus));
    assemble_combat_case_review(
        case_path,
        case,
        CombatCaseReviewArtifacts {
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
        },
    )
}
