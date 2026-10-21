use super::accepted_combat_line_evidence::AcceptedCombatLineEvidenceV1;
use super::atomic_combat_search::run_atomic_combat_search_work_plan_v2;
use super::atomic_combat_search_rejection::{
    build_atomic_combat_search_rejection_outcome, CombatSearchRejectionOutcome,
};
use super::atomic_combat_search_setup::{
    effective_hp_loss_limit, prepare_atomic_combat_search_v2,
    search_report_has_invalid_card_identity, PreparedAtomicCombatSearchV2,
};
use super::combat_line_adjudication::{CombatLineAcceptancePolicy, CombatLineAdjudicationV1};
use super::combat_line_executor::apply_selected_combat_candidate_line;
use super::combat_line_selector::{
    select_accepted_search_combat_line, select_accepted_search_combat_line_with_hp_loss_at_most,
    CombatLineSelection, SelectedCombatLine,
};
use super::combat_line_trace::{
    attach_execution_adjudication, combat_candidate_line_summary,
    combat_line_performance_trace_annotation, combat_search_line_summary,
    combat_search_performance_trace_annotation,
};
use super::combat_no_win_fallback::{
    try_apply_no_win_fallback, try_apply_turn_segment_after_rejection,
};
use super::progress_options::{AtomicCombatSearchOptionsV2, RunControlHpLossLimit};
use super::session::{AtomicCombatSearchRejectionV2, RunControlSession, RunProgressOutcome};
use super::trace_annotation::{CombatAutomationTrajectorySource, RunControlTraceAnnotationV1};

pub struct AtomicCombatSearchAttemptV2 {
    prepared: PreparedAtomicCombatSearchV2,
    report: crate::ai::combat_search_v2::CombatSearchV2Report,
    review: ReviewedCombatSearchAttemptV1,
}

enum ReviewedCombatSearchAttemptV1 {
    InvalidCardIdentity,
    NoCompleteWinningCandidate,
    DirtyWinningCandidateRejected {
        adjudication: CombatLineAdjudicationV1,
        detail: String,
    },
    Selected(SelectedCombatLine),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtomicVerifiedCombatCandidateV2 {
    pub final_hp: i32,
    pub hp_loss: i32,
    pub potions_used: u32,
    pub turns: u32,
    pub action_count: usize,
}

impl AtomicCombatSearchAttemptV2 {
    pub fn verified_win(&self) -> Option<AtomicVerifiedCombatCandidateV2> {
        let ReviewedCombatSearchAttemptV1::Selected(selected) = &self.review else {
            return None;
        };
        Some(verified_candidate_facts(selected))
    }

    /// Re-selects the best exact clean witness that reaches the owner's HP
    /// quality target. The raw report's ordinary best win is not sufficient:
    /// its outcome ordering may intentionally prefer persistent payoff.
    pub fn select_verified_win_with_hp_loss_at_most(
        &mut self,
        session: &RunControlSession,
        max_hp_loss: u32,
    ) -> Result<Option<AtomicVerifiedCombatCandidateV2>, String> {
        ensure_attempt_parent_unchanged(session, &self.prepared)?;
        let acceptance_policy =
            CombatLineAcceptancePolicy::from_plugin(self.prepared.effective_profile.acceptance);
        let selected = select_accepted_search_combat_line_with_hp_loss_at_most(
            session,
            &self.prepared.start,
            &self.prepared.config,
            &self.report,
            acceptance_policy,
            max_hp_loss,
        )?;
        let Some(selected) = selected else {
            return Ok(None);
        };
        let facts = verified_candidate_facts(&selected);
        self.review = ReviewedCombatSearchAttemptV1::Selected(selected);
        Ok(Some(facts))
    }

    pub fn trace_annotation(
        &self,
        session: &RunControlSession,
        source: impl Into<String>,
    ) -> RunControlTraceAnnotationV1 {
        let source = source.into();
        match &self.review {
            ReviewedCombatSearchAttemptV1::Selected(selected) => {
                combat_line_performance_trace_annotation(
                    source,
                    session,
                    &self.prepared.start,
                    &self.report,
                    &selected.line,
                    None,
                )
            }
            ReviewedCombatSearchAttemptV1::InvalidCardIdentity
            | ReviewedCombatSearchAttemptV1::NoCompleteWinningCandidate
            | ReviewedCombatSearchAttemptV1::DirtyWinningCandidateRejected { .. } => {
                combat_search_performance_trace_annotation(
                    source,
                    session,
                    &self.prepared.start,
                    &self.report,
                )
            }
        }
    }
}

pub(super) fn run_atomic_combat_search_attempt_v2(
    session: &RunControlSession,
    options: AtomicCombatSearchOptionsV2,
) -> Result<AtomicCombatSearchAttemptV2, String> {
    let prepared = prepare_atomic_combat_search_v2(session, options)?;
    let report = run_atomic_combat_search_work_plan_v2(
        &prepared.start,
        prepared.config.clone(),
        &prepared.options.work_quanta,
    );
    reviewed_atomic_combat_search_attempt_v2(session, prepared, report)
}

pub(super) fn apply_prepared_atomic_combat_search_report_v2(
    session: &mut RunControlSession,
    prepared: PreparedAtomicCombatSearchV2,
    report: crate::ai::combat_search_v2::CombatSearchV2Report,
) -> Result<RunProgressOutcome, String> {
    let attempt = reviewed_atomic_combat_search_attempt_v2(session, prepared, report)?;
    apply_atomic_combat_search_attempt_v2(session, attempt, None)
}

pub(super) fn apply_atomic_combat_search_attempt_v2(
    session: &mut RunControlSession,
    attempt: AtomicCombatSearchAttemptV2,
    max_hp_loss_override: Option<RunControlHpLossLimit>,
) -> Result<RunProgressOutcome, String> {
    ensure_attempt_parent_unchanged(session, &attempt.prepared)?;
    let AtomicCombatSearchAttemptV2 {
        prepared,
        report,
        review,
    } = attempt;
    let effective_profile = prepared.effective_profile;
    let options = prepared.options;
    let start = prepared.start;
    let config = prepared.config;
    let max_hp_loss = match max_hp_loss_override {
        Some(RunControlHpLossLimit::Limit(limit)) => Some(limit),
        Some(RunControlHpLossLimit::Unlimited) => None,
        None => effective_hp_loss_limit(session, &options),
    };
    let selected = match review {
        ReviewedCombatSearchAttemptV1::InvalidCardIdentity => {
            return Ok(build_atomic_combat_search_rejection_outcome(
                session,
                &start,
                &report,
                CombatSearchRejectionOutcome {
                    result: "invalid_card_identity",
                    detail: None,
                    rejection: AtomicCombatSearchRejectionV2::InvalidCardIdentity,
                    trace_source: "search_combat_rejected",
                    execution_adjudication: None,
                },
            ));
        }
        ReviewedCombatSearchAttemptV1::NoCompleteWinningCandidate => {
            if options.enable_legacy_no_win_rescue {
                if let Some(outcome) = try_apply_no_win_fallback(
                    session,
                    &start,
                    &config,
                    &options,
                    &report,
                    max_hp_loss,
                )? {
                    return Ok(outcome);
                }
            } else {
                if options.segment_mode.is_some() {
                    if let Some(outcome) = try_apply_turn_segment_after_rejection(
                        session,
                        &start,
                        &config,
                        &options,
                        &report,
                        "no_complete_winning_candidate",
                    )? {
                        return Ok(outcome);
                    }
                }
                if options.allow_smoke_bomb_survival_fallback {
                    if let Some(outcome) =
                        super::combat_no_win_fallback::try_apply_smoke_bomb_survival_fallback_after_rejection(
                            session,
                            "no_complete_winning_candidate",
                        )?
                    {
                        return Ok(outcome);
                    }
                }
            }
            return Ok(build_atomic_combat_search_rejection_outcome(
                session,
                &start,
                &report,
                CombatSearchRejectionOutcome {
                    result: "no_complete_winning_candidate",
                    detail: None,
                    rejection: AtomicCombatSearchRejectionV2::NoCompleteWinningCandidate,
                    trace_source: "search_combat_rejected",
                    execution_adjudication: None,
                },
            ));
        }
        ReviewedCombatSearchAttemptV1::DirtyWinningCandidateRejected {
            adjudication,
            detail,
        } => {
            return Ok(build_atomic_combat_search_rejection_outcome(
                session,
                &start,
                &report,
                CombatSearchRejectionOutcome {
                    result: "dirty_winning_candidate_rejected",
                    detail: Some(detail),
                    rejection: AtomicCombatSearchRejectionV2::DirtyWinningCandidateRejected,
                    trace_source: "search_combat_rejected_dirty_win",
                    execution_adjudication: Some(adjudication),
                },
            ));
        }
        ReviewedCombatSearchAttemptV1::Selected(selected) => selected,
    };

    if let Some(max_hp_loss) = max_hp_loss {
        if selected.line.hp_loss > max_hp_loss as i32 {
            if let Some(outcome) = try_apply_turn_segment_after_rejection(
                session,
                &start,
                &config,
                &options,
                &report,
                "complete_winning_candidate_exceeds_hp_loss_limit",
            )? {
                return Ok(outcome);
            }
            return Ok(build_atomic_combat_search_rejection_outcome(
                session,
                &start,
                &report,
                CombatSearchRejectionOutcome {
                    result: "complete_winning_candidate_exceeds_hp_loss_limit",
                    detail: Some(format!(
                        "candidate_hp_loss={} max_hp_loss={max_hp_loss}",
                        selected.line.hp_loss
                    )),
                    rejection: AtomicCombatSearchRejectionV2::HpLossLimitExceeded,
                    trace_source: "search_combat_rejected",
                    execution_adjudication: None,
                },
            ));
        }
    }

    let mut summary = format!(
        "search-combat applied {} actions profile={}",
        selected.line.actions.len(),
        effective_profile.profile_id
    );
    if let Some(repair_summary) = selected.summary.as_ref() {
        summary.push_str(&format!(" {repair_summary}"));
    }
    let trajectory = report
        .best_win_trajectory
        .as_ref()
        .expect("a reviewed selected line must come from a winning report");
    let accepted_line_evidence = AcceptedCombatLineEvidenceV1::new(
        combat_search_line_summary(trajectory),
        combat_candidate_line_summary(&selected.line),
        selected.summary.clone(),
    );
    let selected_adjudication = selected.adjudication;
    let mut outcome = apply_selected_combat_candidate_line(
        session,
        &start,
        &config,
        &report,
        selected.line,
        CombatAutomationTrajectorySource::AtomicExactV2,
        summary,
        None,
    )?
    .with_execution_adjudication(selected_adjudication.clone());
    outcome
        .trace_annotations
        .push(accepted_line_evidence.into_annotation());
    attach_execution_adjudication(&mut outcome.trace_annotations, &selected_adjudication);
    Ok(outcome)
}

fn reviewed_atomic_combat_search_attempt_v2(
    session: &RunControlSession,
    prepared: PreparedAtomicCombatSearchV2,
    report: crate::ai::combat_search_v2::CombatSearchV2Report,
) -> Result<AtomicCombatSearchAttemptV2, String> {
    let review = if search_report_has_invalid_card_identity(&report) {
        ReviewedCombatSearchAttemptV1::InvalidCardIdentity
    } else if let Some(trajectory) = report.best_win_trajectory.as_ref() {
        let acceptance_policy =
            CombatLineAcceptancePolicy::from_plugin(prepared.effective_profile.acceptance);
        match select_accepted_search_combat_line(
            session,
            &prepared.start,
            &prepared.config,
            &report,
            trajectory,
            acceptance_policy,
        ) {
            CombatLineSelection::Selected(selected) => {
                ReviewedCombatSearchAttemptV1::Selected(selected)
            }
            CombatLineSelection::Rejected {
                adjudication,
                detail,
            } => ReviewedCombatSearchAttemptV1::DirtyWinningCandidateRejected {
                adjudication,
                detail,
            },
            CombatLineSelection::ReplayFailed { adjudication } => {
                let CombatLineAdjudicationV1::ReplayFailed { error, .. } = adjudication else {
                    unreachable!("replay-failed selection must carry replay-failed adjudication")
                };
                return Err(format!("combat line replay failed: {error}"));
            }
        }
    } else {
        ReviewedCombatSearchAttemptV1::NoCompleteWinningCandidate
    };
    Ok(AtomicCombatSearchAttemptV2 {
        prepared,
        report,
        review,
    })
}

fn verified_candidate_facts(selected: &SelectedCombatLine) -> AtomicVerifiedCombatCandidateV2 {
    AtomicVerifiedCombatCandidateV2 {
        final_hp: selected.line.final_hp,
        hp_loss: selected.line.hp_loss,
        potions_used: selected.line.potions_used,
        turns: selected.line.turns,
        action_count: selected.line.actions.len(),
    }
}

fn ensure_attempt_parent_unchanged(
    session: &RunControlSession,
    prepared: &PreparedAtomicCombatSearchV2,
) -> Result<(), String> {
    if session.current_active_combat_position()? != prepared.start {
        return Err(
            "combat search attempt parent changed before its result was committed".to_string(),
        );
    }
    Ok(())
}
