use super::decision_surface::build_decision_surface;
use super::route_policy_prior::{
    exact_route_policy_decision_v1, RoutePolicyActionV1, RoutePolicyBandV1,
};
use super::session::{
    RunControlDecisionParentSnapshotV1, RunControlSession, RunControlSessionCheckpointV1,
    RunProgressOutcome,
};
use super::RunPolicyCandidateV1;

pub(in crate::eval::run_control) struct RoutePlanApplied {
    pub outcome: RunProgressOutcome,
    pub auto_step_summary: String,
}

pub(in crate::eval::run_control) fn apply_route_plan(
    session: &mut RunControlSession,
) -> Result<RunProgressOutcome, String> {
    Ok(apply_route_policy_with_summary(session)?.outcome)
}

pub(in crate::eval::run_control) fn apply_route_policy_with_summary(
    session: &mut RunControlSession,
) -> Result<RoutePlanApplied, String> {
    if !session.engine_state.is_map_surface() {
        return Err("exact route policy is only valid on a map boundary".to_string());
    }

    let surface = build_decision_surface(session);
    let legal = surface
        .view
        .candidates
        .iter()
        .filter_map(|candidate| {
            candidate
                .action
                .executable_action_ref()
                .map(|action| RunPolicyCandidateV1 {
                    candidate_id: &candidate.id,
                    label: &candidate.label,
                    action,
                })
        })
        .collect::<Vec<_>>();
    let decision = exact_route_policy_decision_v1(session, &legal)?;
    let selected = decision
        .evidence
        .first()
        .ok_or_else(|| "exact route policy returned no ranked action".to_string())?;
    let candidate = surface
        .view
        .candidates
        .iter()
        .find(|candidate| candidate.id == selected.candidate_id)
        .ok_or_else(|| {
            format!(
                "exact route policy selected missing candidate '{}'",
                selected.candidate_id
            )
        })?;
    let auto_step_summary = render_route_policy_summary(selected.band, &selected.action);
    let parent_snapshot = RunControlDecisionParentSnapshotV1 {
        source: "exact_route_policy".to_string(),
        command: candidate.action.summary(),
        snapshot: RunControlSessionCheckpointV1::from_session(session),
    };
    let transaction = session.execute_route_candidate_transaction(&selected.candidate_id)?;
    let outcome = transaction.project_progress_outcome(session);

    Ok(RoutePlanApplied {
        auto_step_summary: auto_step_summary.clone(),
        outcome: RunProgressOutcome {
            message: format!("{auto_step_summary}\n{}", outcome.message),
            ..outcome
        }
        .with_decision_parent_snapshots(vec![parent_snapshot]),
    })
}

fn render_route_policy_summary(band: RoutePolicyBandV1, action: &RoutePolicyActionV1) -> String {
    match action {
        RoutePolicyActionV1::Select {
            x,
            y,
            room_type,
            uses_wing_boots,
            path,
            ..
        } => format!(
            "exact route policy: ({x},{y}) {room_type:?} band={band:?} paths={} coverage={:?} damage-before-recovery={}-{}{}",
            path.observed_path_count,
            path.coverage,
            path.min_damage_rooms_before_recovery,
            path.max_damage_rooms_before_recovery,
            if *uses_wing_boots {
                " WingBoots"
            } else {
                ""
            }
        ),
        RoutePolicyActionV1::CancelToPendingRewards => {
            "exact route policy: return to pending rewards".to_string()
        }
    }
}
