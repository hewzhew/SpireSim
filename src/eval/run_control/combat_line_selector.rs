use crate::ai::combat_search_v2::{
    CombatSearchV2Config, CombatSearchV2Report, CombatSearchV2TrajectoryReport,
};
use crate::sim::combat::CombatPosition;

use super::combat_candidate_line::CombatCandidateLine;
use super::combat_line_adjudication::{
    CombatLineAcceptancePolicy, CombatLineAdjudicationV1, CombatLineCleanlinessV1,
};
use super::combat_line_outcome::{
    evaluate_combat_candidate_line_outcome, find_accepted_alternative_in_report,
    find_accepted_alternative_in_report_matching, prefer_quality_gated_accepted_outcome,
    render_combat_line_outcome_detail,
};
use super::session::RunControlSession;

pub(super) struct SelectedCombatLine {
    pub(super) line: CombatCandidateLine,
    pub(super) summary: Option<String>,
    pub(super) adjudication: CombatLineAdjudicationV1,
}

pub(super) enum CombatLineSelection {
    Selected(SelectedCombatLine),
    Rejected {
        adjudication: CombatLineAdjudicationV1,
        detail: String,
    },
    ReplayFailed {
        adjudication: CombatLineAdjudicationV1,
    },
}

pub(super) fn select_accepted_search_combat_line(
    session: &RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    report: &CombatSearchV2Report,
    trajectory: &CombatSearchV2TrajectoryReport,
    policy: CombatLineAcceptancePolicy,
) -> CombatLineSelection {
    let selected_line = match CombatCandidateLine::try_from_search_trajectory(trajectory) {
        Ok(line) => line,
        Err(error) => return replay_failed(policy, error),
    };
    let mut summary = None;

    let selected_eval =
        match evaluate_combat_candidate_line_outcome(session, start, config, selected_line.clone())
        {
            Ok(evaluation) => evaluation,
            Err(error) => return replay_failed(policy, error),
        };
    let selected_adjudication = policy.adjudicate(selected_eval.outcome.clone());
    if matches!(
        &selected_adjudication,
        CombatLineAdjudicationV1::Accepted { .. }
    ) {
        return CombatLineSelection::Selected(SelectedCombatLine {
            line: selected_eval.line,
            summary,
            adjudication: selected_adjudication,
        });
    }
    if !policy.requires_clean_line() {
        return CombatLineSelection::Rejected {
            detail: render_combat_line_outcome_detail(&selected_eval.outcome),
            adjudication: selected_adjudication,
        };
    }

    let same_report_alternative =
        match find_accepted_alternative_in_report(session, start, config, report, policy) {
            Ok(alternative) => alternative,
            Err(error) => return replay_failed(policy, error),
        };
    if let Some(alternative) = same_report_alternative {
        let adjudication = policy.adjudicate(alternative.outcome.clone());
        debug_assert!(matches!(
            &adjudication,
            CombatLineAdjudicationV1::Accepted {
                cleanliness: CombatLineCleanlinessV1::Clean,
                ..
            }
        ));
        append_selection_summary(
            &mut summary,
            format!(
                "same_report_clean_alternative replaced dirty_win gained_curses={} original_final_hp={} clean_final_hp={}",
                selected_eval.outcome.gained_curse_count(),
                selected_eval.outcome.final_hp,
                alternative.outcome.final_hp
            ),
        );
        return CombatLineSelection::Selected(SelectedCombatLine {
            line: alternative.line,
            summary,
            adjudication,
        });
    }

    CombatLineSelection::Rejected {
        detail: render_combat_line_outcome_detail(&selected_eval.outcome),
        adjudication: selected_adjudication,
    }
}

pub(super) fn select_accepted_search_combat_line_with_hp_loss_at_most(
    session: &RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    report: &CombatSearchV2Report,
    policy: CombatLineAcceptancePolicy,
    max_hp_loss: u32,
) -> Result<Option<SelectedCombatLine>, String> {
    let Some(evaluation) = find_accepted_alternative_in_report_matching(
        session,
        start,
        config,
        report,
        policy,
        |trajectory| trajectory.hp_loss.max(0) as u32 <= max_hp_loss,
        |outcome| outcome.hp_loss.max(0) as u32 <= max_hp_loss,
        prefer_quality_gated_accepted_outcome,
    )?
    else {
        return Ok(None);
    };
    let adjudication = policy.adjudicate(evaluation.outcome);
    debug_assert!(matches!(
        &adjudication,
        CombatLineAdjudicationV1::Accepted {
            cleanliness: CombatLineCleanlinessV1::Clean,
            ..
        }
    ));
    Ok(Some(SelectedCombatLine {
        line: evaluation.line,
        summary: Some(format!(
            "same_report_quality_candidate hp_loss_at_most={max_hp_loss}"
        )),
        adjudication,
    }))
}

fn replay_failed(policy: CombatLineAcceptancePolicy, error: String) -> CombatLineSelection {
    CombatLineSelection::ReplayFailed {
        adjudication: CombatLineAdjudicationV1::ReplayFailed {
            policy: policy.plugin(),
            error,
        },
    }
}

fn append_selection_summary(summary: &mut Option<String>, item: String) {
    match summary {
        Some(summary) => {
            summary.push(' ');
            summary.push_str(&item);
        }
        None => *summary = Some(item),
    }
}
