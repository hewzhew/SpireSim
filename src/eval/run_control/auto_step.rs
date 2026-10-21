use crate::content::identity::gameplay_display_label_v1;
use crate::eval::event_boundary_classifier_v1::classify_event_option_boundary_v1;
use crate::state::core::{ClientInput, EngineState, RunResult};

use super::combat_line_adjudication::{CombatLineAdjudicationV1, CombatLineRejectionReasonV1};
use super::progress_options::{
    AtomicCombatSearchOptionsV2, RunControlAutoStepOptions, RunControlRouteAutomationMode,
};
use super::session::{
    AtomicCombatSearchRejectionV2, RunControlAutoAppliedKindV1, RunControlAutoAppliedStepV1,
    RunControlDecisionParentSnapshotV1, RunControlSession, RunProgressOutcome,
};
use super::trace_annotation::RunControlTraceAnnotationV1;
use super::transition_report::{
    action_result_from_transition, render_action_result, RunApplyStatus, RunVisibleSnapshot,
    TransitionAction,
};
use super::view_model::{build_run_control_view_model, DecisionCandidate, RunControlViewModel};
use super::{RunControlAutoStopKind, RunControlAutoStopV1, RunProgressStepV1};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoAdvanceClass {
    Routine,
    Forced,
    Strategic,
    Unsafe,
}

struct AutoAdvanceCandidate<'a> {
    candidate: &'a DecisionCandidate,
    class: AutoAdvanceClass,
    reason: &'static str,
}

struct AutoAppliedLog {
    steps: Vec<RunControlAutoAppliedStepV1>,
    progress_steps: Vec<RunProgressStepV1>,
}

impl AutoAppliedLog {
    fn new() -> Self {
        Self {
            steps: Vec::new(),
            progress_steps: Vec::new(),
        }
    }

    fn push_step(
        &mut self,
        kind: RunControlAutoAppliedKindV1,
        label: impl Into<String>,
        action_result: Option<super::transition_report::ActionResult>,
    ) {
        let label = label.into();
        self.steps.push(RunControlAutoAppliedStepV1 {
            kind,
            label,
            action_result,
        });
    }

    fn push_outcome(
        &mut self,
        kind: RunControlAutoAppliedKindV1,
        label: impl Into<String>,
        outcome: &RunProgressOutcome,
    ) {
        self.progress_steps
            .extend(outcome.progress_steps.iter().cloned());
        self.steps.push(RunControlAutoAppliedStepV1 {
            kind,
            label: label.into(),
            action_result: outcome.action_result.clone(),
        });
    }

    fn extend(&mut self, labels: Vec<String>) {
        for label in labels {
            self.push_step(RunControlAutoAppliedKindV1::AutoCapture, label, None);
        }
    }

    fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    fn len(&self) -> usize {
        self.steps.len()
    }
}

pub(super) fn apply_guarded_auto_step(
    session: &mut RunControlSession,
    options: RunControlAutoStepOptions,
) -> Result<RunProgressOutcome, String> {
    let before = RunVisibleSnapshot::capture(session);
    let mut applied = AutoAppliedLog::new();
    let mut trace_annotations = Vec::new();
    let mut decision_parent_snapshots = Vec::new();

    if let Some(outcome) = super::reward_auto::apply_reward_policy_step(session)? {
        applied.push_outcome(
            RunControlAutoAppliedKindV1::RewardPolicyCandidate,
            "reward policy candidate",
            &outcome,
        );
        trace_annotations.extend(outcome.trace_annotations);
        return finish_applied_progress_step(
            session,
            &before,
            applied,
            trace_annotations,
            decision_parent_snapshots,
        );
    }

    if let Some(outcome) = super::reward_auto::apply_reward_potion_space_step(session)? {
        applied.push_outcome(
            RunControlAutoAppliedKindV1::RewardPolicyCandidate,
            "reward potion-space candidate",
            &outcome,
        );
        trace_annotations.extend(outcome.trace_annotations);
        return finish_applied_progress_step(
            session,
            &before,
            applied,
            trace_annotations,
            decision_parent_snapshots,
        );
    }

    if session.current_active_combat_position().is_ok() {
        if high_stakes_auto_search_requires_hp_loss_gate(session, &options.search) {
            return finish_auto_step(
                    session,
                    &before,
                    applied,
                    trace_annotations,
                    decision_parent_snapshots,
                    RunControlAutoStopKind::HpLossGateRequired,
                    "high-stakes combat auto-search requires an hp-loss gate",
                    Some(
                        "Use `n max_hp_loss=N` or `nr max_hp_loss=N` for this combat, or `sd max_hp_loss=N` to set a session default. Use `n max_hp_loss=off` only when you deliberately want to accept any winning search line."
                            .to_string(),
                    ),
                );
        }

        let mut no_potion_rejection = None;
        let mut no_potion_rejection_kind = None;
        let mut no_potion_adjudication = None;
        if let Some(no_potion_options) = auto_no_potion_first_options(session, &options.search) {
            let outcome = super::atomic_combat_search::apply_atomic_combat_search_v2(
                session,
                no_potion_options,
            )?;
            if let Some(result) = outcome.action_result.as_ref() {
                applied.push_outcome(
                    RunControlAutoAppliedKindV1::CombatSearch,
                    format!("combat search(no potion): {}", result.chosen_label),
                    &outcome,
                );
                let auto_capture_summaries = auto_capture_summaries(&outcome.trace_annotations);
                decision_parent_snapshots.extend(outcome.decision_parent_snapshots);
                trace_annotations.extend(outcome.trace_annotations);
                applied.extend(auto_capture_summaries);
                return finish_applied_progress_step(
                    session,
                    &before,
                    applied,
                    trace_annotations,
                    decision_parent_snapshots,
                );
            }
            decision_parent_snapshots.extend(outcome.decision_parent_snapshots);
            trace_annotations.extend(outcome.trace_annotations);
            no_potion_rejection_kind = outcome.atomic_combat_search_rejection;
            no_potion_adjudication = outcome.execution_adjudication.clone();
            no_potion_rejection = Some(trim_search_rejection(&outcome.message));
        }

        let outcome = super::atomic_combat_search::apply_atomic_combat_search_v2(
            session,
            auto_search_options(session, options.search.clone()),
        )?;
        if let Some(result) = outcome.action_result.as_ref() {
            let label = if no_potion_rejection.is_some() {
                format!("combat search(semantic fallback): {}", result.chosen_label)
            } else {
                format!("combat search: {}", result.chosen_label)
            };
            applied.push_outcome(RunControlAutoAppliedKindV1::CombatSearch, label, &outcome);
            let auto_capture_summaries = auto_capture_summaries(&outcome.trace_annotations);
            decision_parent_snapshots.extend(outcome.decision_parent_snapshots);
            trace_annotations.extend(outcome.trace_annotations);
            applied.extend(auto_capture_summaries);
            return finish_applied_progress_step(
                session,
                &before,
                applied,
                trace_annotations,
                decision_parent_snapshots,
            );
        }
        let fallback_rejection = trim_search_rejection(&outcome.message);
        let fallback_rejection_kind = outcome.atomic_combat_search_rejection;
        let fallback_adjudication = outcome.execution_adjudication.clone();
        decision_parent_snapshots.extend(outcome.decision_parent_snapshots);
        trace_annotations.extend(outcome.trace_annotations);
        if let Some(rescue_options) = auto_potion_rescue_options(session, &options.search) {
            let rescue = super::atomic_combat_search::apply_atomic_combat_search_v2(
                session,
                rescue_options,
            )?;
            if let Some(result) = rescue.action_result.as_ref() {
                applied.push_outcome(
                    RunControlAutoAppliedKindV1::CombatSearch,
                    format!("combat search(potion rescue): {}", result.chosen_label),
                    &rescue,
                );
                let auto_capture_summaries = auto_capture_summaries(&rescue.trace_annotations);
                decision_parent_snapshots.extend(rescue.decision_parent_snapshots);
                trace_annotations.extend(rescue.trace_annotations);
                applied.extend(auto_capture_summaries);
                return finish_applied_progress_step(
                    session,
                    &before,
                    applied,
                    trace_annotations,
                    decision_parent_snapshots,
                );
            }
            decision_parent_snapshots.extend(rescue.decision_parent_snapshots);
            trace_annotations.extend(rescue.trace_annotations);
            let rescue_rejection_kind = rescue.atomic_combat_search_rejection;
            let rescue_adjudication = rescue.execution_adjudication.clone();
            return finish_auto_step(
                session,
                &before,
                applied,
                trace_annotations,
                decision_parent_snapshots,
                RunControlAutoStopKind::CombatSearchNoCompleteWin,
                combat_search_stop_reason(
                    &[
                        no_potion_rejection_kind,
                        fallback_rejection_kind,
                        rescue_rejection_kind,
                    ],
                    &[
                        no_potion_adjudication,
                        fallback_adjudication,
                        rescue_adjudication,
                    ],
                ),
                Some(combine_three_search_rejections(
                    no_potion_rejection,
                    fallback_rejection,
                    trim_search_rejection(&rescue.message),
                )),
            );
        }
        return finish_auto_step(
            session,
            &before,
            applied,
            trace_annotations,
            decision_parent_snapshots,
            RunControlAutoStopKind::CombatSearchNoCompleteWin,
            combat_search_stop_reason(
                &[no_potion_rejection_kind, fallback_rejection_kind],
                &[no_potion_adjudication, fallback_adjudication],
            ),
            Some(combine_search_rejections(
                no_potion_rejection,
                fallback_rejection,
            )),
        );
    }

    if let Some((outcome, summary)) = apply_map_overlay_back_without_route_candidates(session)? {
        let auto_capture_summaries = auto_capture_summaries(&outcome.trace_annotations);
        applied.push_outcome(
            RunControlAutoAppliedKindV1::RewardOverlay,
            summary,
            &outcome,
        );
        decision_parent_snapshots.extend(outcome.decision_parent_snapshots);
        trace_annotations.extend(outcome.trace_annotations);
        applied.extend(auto_capture_summaries);
        return finish_applied_progress_step(
            session,
            &before,
            applied,
            trace_annotations,
            decision_parent_snapshots,
        );
    }

    if session.engine_state.is_map_surface()
        && options.route == RunControlRouteAutomationMode::Policy
    {
        let route_result = super::route_policy::apply_route_policy_with_summary(session);
        match route_result {
            Ok(applied_route) => {
                if applied_route.outcome.action_result.is_some() {
                    let auto_capture_summaries =
                        auto_capture_summaries(&applied_route.outcome.trace_annotations);
                    applied.push_outcome(
                        RunControlAutoAppliedKindV1::RoutePolicy,
                        applied_route.auto_step_summary,
                        &applied_route.outcome,
                    );
                    decision_parent_snapshots
                        .extend(applied_route.outcome.decision_parent_snapshots);
                    trace_annotations.extend(applied_route.outcome.trace_annotations);
                    applied.extend(auto_capture_summaries);
                    return finish_applied_progress_step(
                        session,
                        &before,
                        applied,
                        trace_annotations,
                        decision_parent_snapshots,
                    );
                }
                decision_parent_snapshots.extend(applied_route.outcome.decision_parent_snapshots);
                trace_annotations.extend(applied_route.outcome.trace_annotations);
                return finish_auto_step(
                    session,
                    &before,
                    applied,
                    trace_annotations,
                    decision_parent_snapshots,
                    RunControlAutoStopKind::RoutePolicyNoMutation,
                    "route policy did not modify state",
                    Some(applied_route.outcome.message),
                );
            }
            Err(err) => {
                return finish_auto_step(
                    session,
                    &before,
                    applied,
                    trace_annotations,
                    decision_parent_snapshots,
                    RunControlAutoStopKind::RoutePolicyDeclined,
                    "exact route policy declined automatic map selection",
                    Some(err),
                );
            }
        }
    }

    if let Some((outcome, summary)) = apply_pending_shop_reward_overlay(session)? {
        let auto_capture_summaries = auto_capture_summaries(&outcome.trace_annotations);
        applied.push_outcome(
            RunControlAutoAppliedKindV1::RewardOverlay,
            summary,
            &outcome,
        );
        decision_parent_snapshots.extend(outcome.decision_parent_snapshots);
        trace_annotations.extend(outcome.trace_annotations);
        applied.extend(auto_capture_summaries);
        return finish_applied_progress_step(
            session,
            &before,
            applied,
            trace_annotations,
            decision_parent_snapshots,
        );
    }

    let view = build_run_control_view_model(session);
    if let Some(auto_candidate) = auto_advance_candidate(session, &view) {
        if auto_candidate.candidate.action.executable_input().is_none() {
            return finish_auto_step(
                session,
                &before,
                applied,
                trace_annotations,
                decision_parent_snapshots,
                RunControlAutoStopKind::AutoCandidateNotExecutable,
                "auto-selected candidate is not executable",
                None,
            );
        }
        let transaction =
            session.execute_routine_candidate_transaction(&auto_candidate.candidate.id)?;
        let outcome = transaction.project_progress_outcome(session);
        let label = outcome
            .action_result
            .as_ref()
            .map(|result| result.chosen_label.clone())
            .unwrap_or_else(|| auto_candidate.candidate.label.clone());
        let auto_capture_summaries = auto_capture_summaries(&outcome.trace_annotations);
        applied.push_outcome(
            RunControlAutoAppliedKindV1::RoutineCandidate,
            format!(
                "{}: {label} ({})",
                auto_class_label(auto_candidate.class),
                auto_candidate.reason
            ),
            &outcome,
        );
        decision_parent_snapshots.extend(outcome.decision_parent_snapshots);
        trace_annotations.extend(outcome.trace_annotations);
        applied.extend(auto_capture_summaries);
        return finish_applied_progress_step(
            session,
            &before,
            applied,
            trace_annotations,
            decision_parent_snapshots,
        );
    }

    finish_auto_step(
        session,
        &before,
        applied,
        trace_annotations,
        decision_parent_snapshots,
        RunControlAutoStopKind::HumanBoundary,
        human_stop_reason(session),
        None,
    )
}

fn finish_applied_progress_step(
    session: &RunControlSession,
    before: &RunVisibleSnapshot,
    applied: AutoAppliedLog,
    trace_annotations: Vec<RunControlTraceAnnotationV1>,
    decision_parent_snapshots: Vec<RunControlDecisionParentSnapshotV1>,
) -> Result<RunProgressOutcome, String> {
    finish_progress_outcome(
        session,
        before,
        applied,
        trace_annotations,
        decision_parent_snapshots,
        None,
        "one atomic progress step applied",
        None,
    )
}

fn apply_pending_shop_reward_overlay(
    session: &mut RunControlSession,
) -> Result<Option<(RunProgressOutcome, String)>, String> {
    let EngineState::Shop(shop) = &session.engine_state else {
        return Ok(None);
    };
    if shop.pending_reward_overlay.is_none() {
        return Ok(None);
    }

    let view = build_run_control_view_model(session);
    let Some(candidate) = view.candidates.iter().find(|candidate| {
        candidate.action.executable_input() == Some(ClientInput::OpenRewardOverlay)
    }) else {
        return Ok(None);
    };
    let label = candidate.label.clone();
    let outcome = session.apply_input(ClientInput::OpenRewardOverlay)?;
    Ok(Some((
        outcome,
        format!("routine: {label} (pending shop reward overlay)"),
    )))
}

fn auto_capture_summaries(annotations: &[RunControlTraceAnnotationV1]) -> Vec<String> {
    annotations
        .iter()
        .filter_map(|annotation| match annotation {
            RunControlTraceAnnotationV1::AutoCombatCapture {
                case_id,
                capture_path,
                ..
            } => Some(format!("auto capture: {case_id} -> {capture_path}")),
            RunControlTraceAnnotationV1::CardRewardOwnerDecision { .. }
            | RunControlTraceAnnotationV1::NonCombatPolicyDecision { .. }
            | RunControlTraceAnnotationV1::NonCombatHumanBoundary { .. }
            | RunControlTraceAnnotationV1::PlannerBehaviorDecision { .. }
            | RunControlTraceAnnotationV1::CombatAutomationTrajectory { .. }
            | RunControlTraceAnnotationV1::CombatSearchPerformance { .. }
            | RunControlTraceAnnotationV1::AcceptedCombatLine { .. } => None,
        })
        .collect()
}

fn auto_search_options(
    session: &RunControlSession,
    mut options: AtomicCombatSearchOptionsV2,
) -> AtomicCombatSearchOptionsV2 {
    let plan = super::combat_auto_policy::combat_auto_search_plan(session, &options);
    if should_apply_auto_default_wall_ms(&options) {
        options.wall_ms = plan.default_wall_ms;
    }
    if options.potion_policy.is_none() && session.search_potion_policy.is_none() {
        options.potion_policy = plan.primary_potion_policy;
    }
    if options.max_potions_used.is_none() && session.search_max_potions_used.is_none() {
        options.max_potions_used = plan.primary_max_potions_used;
    }
    options
}

fn auto_no_potion_first_options(
    session: &RunControlSession,
    options: &AtomicCombatSearchOptionsV2,
) -> Option<AtomicCombatSearchOptionsV2> {
    let plan = super::combat_auto_policy::combat_auto_search_plan(session, options);
    if !plan.no_potion_first {
        return None;
    }

    let mut no_potion = options.clone();
    if should_apply_auto_default_wall_ms(&no_potion) {
        no_potion.wall_ms = plan.default_wall_ms;
    }
    no_potion.potion_policy = Some(crate::ai::combat_search_v2::CombatSearchV2PotionPolicy::Never);
    no_potion.max_potions_used = Some(0);
    Some(no_potion)
}

fn auto_potion_rescue_options(
    session: &RunControlSession,
    options: &AtomicCombatSearchOptionsV2,
) -> Option<AtomicCombatSearchOptionsV2> {
    let plan = super::combat_auto_policy::combat_auto_search_plan(session, options);
    let potion_policy = plan.potion_rescue_policy?;

    let mut rescue = options.clone();
    if should_apply_auto_default_wall_ms(&rescue) {
        rescue.wall_ms = plan.default_wall_ms;
    }
    rescue.potion_policy = Some(potion_policy);
    rescue.max_potions_used = plan.potion_rescue_max_potions_used;
    Some(rescue)
}

fn should_apply_auto_default_wall_ms(options: &AtomicCombatSearchOptionsV2) -> bool {
    options.wall_ms.is_none() && options.profile.is_none()
}

fn high_stakes_auto_search_requires_hp_loss_gate(
    session: &RunControlSession,
    options: &AtomicCombatSearchOptionsV2,
) -> bool {
    super::combat_auto_policy::combat_auto_search_plan(session, options)
        .requires_explicit_hp_loss_gate
}

fn apply_map_overlay_back_without_route_candidates(
    session: &mut RunControlSession,
) -> Result<Option<(RunProgressOutcome, String)>, String> {
    if !matches!(session.engine_state, EngineState::MapOverlay { .. }) {
        return Ok(None);
    }

    let view = build_run_control_view_model(session);
    let has_route_candidate = view.candidates.iter().any(|candidate| {
        matches!(
            candidate.action.executable_input(),
            Some(ClientInput::SelectMapNode(_))
        )
    });
    if has_route_candidate {
        return Ok(None);
    }

    let Some(label) = view
        .candidates
        .iter()
        .find(|candidate| candidate.action.executable_input() == Some(ClientInput::Cancel))
        .map(|candidate| candidate.label.clone())
    else {
        return Ok(None);
    };

    let outcome = session.apply_input(ClientInput::Cancel)?;
    let label = outcome
        .action_result
        .as_ref()
        .map(|result| result.chosen_label.clone())
        .unwrap_or(label);
    Ok(Some((
        outcome,
        format!("routine: {label} (map preview has no route action)"),
    )))
}

fn auto_advance_candidate<'a>(
    session: &RunControlSession,
    view: &'a RunControlViewModel,
) -> Option<AutoAdvanceCandidate<'a>> {
    if let EngineState::RewardScreen(reward) = &session.engine_state {
        if reward.pending_card_choice.is_none() && reward.items.is_empty() && reward.skippable {
            return view
                .candidates
                .iter()
                .find(|candidate| candidate.action.executable_input() == Some(ClientInput::Proceed))
                .map(|candidate| AutoAdvanceCandidate {
                    candidate,
                    class: AutoAdvanceClass::Routine,
                    reason: "empty reward screen",
                });
        }
    }
    if let EngineState::RewardOverlay { reward_state, .. } = &session.engine_state {
        if reward_state.pending_card_choice.is_none()
            && reward_state.items.is_empty()
            && reward_state.skippable
        {
            return view
                .candidates
                .iter()
                .find(|candidate| candidate.action.executable_input() == Some(ClientInput::Cancel))
                .map(|candidate| AutoAdvanceCandidate {
                    candidate,
                    class: AutoAdvanceClass::Routine,
                    reason: "empty overlay reward screen",
                });
        }
    }
    if view.candidates.len() == 1
        && view.candidates[0].note.as_deref() == Some("routine")
        && view.candidates[0].action.executable_input().is_some()
    {
        return Some(AutoAdvanceCandidate {
            candidate: &view.candidates[0],
            class: AutoAdvanceClass::Routine,
            reason: "single routine action",
        });
    }

    let executable = view
        .candidates
        .iter()
        .filter(|candidate| candidate.action.executable_input().is_some())
        .collect::<Vec<_>>();
    if executable.len() == 1 {
        let candidate = executable[0];
        let class = classify_single_executable_candidate(session, candidate);
        if matches!(class, AutoAdvanceClass::Routine | AutoAdvanceClass::Forced) {
            return Some(AutoAdvanceCandidate {
                candidate,
                class,
                reason: single_candidate_reason(session, candidate, class),
            });
        }
    }

    None
}

fn classify_single_executable_candidate(
    session: &RunControlSession,
    candidate: &DecisionCandidate,
) -> AutoAdvanceClass {
    if candidate.action.executable_input().is_none() {
        return AutoAdvanceClass::Unsafe;
    }
    match &session.engine_state {
        EngineState::TreasureRoom(_)
            if candidate.action.executable_input() == Some(ClientInput::OpenChest) =>
        {
            AutoAdvanceClass::Routine
        }
        EngineState::Shop(_) if candidate.id == "leave" => AutoAdvanceClass::Routine,
        EngineState::RewardScreen(reward)
            if reward.pending_card_choice.is_none()
                && reward.items.is_empty()
                && candidate.action.executable_input() == Some(ClientInput::Proceed) =>
        {
            AutoAdvanceClass::Routine
        }
        EngineState::RewardOverlay { reward_state, .. }
            if reward_state.pending_card_choice.is_none()
                && reward_state.items.is_empty()
                && candidate.action.executable_input() == Some(ClientInput::Cancel) =>
        {
            AutoAdvanceClass::Routine
        }
        EngineState::EventRoom
            if event_single_candidate_auto_reason(session, candidate).is_some() =>
        {
            AutoAdvanceClass::Forced
        }
        EngineState::RunPendingChoice(choice)
            if choice.min_choices == 1
                && choice.max_choices == 1
                && !matches!(
                    choice.source,
                    crate::state::selection::DomainEventSource::Event(
                        crate::state::events::EventId::LivingWall
                    )
                ) =>
        {
            AutoAdvanceClass::Forced
        }
        EngineState::GameOver(_) => AutoAdvanceClass::Unsafe,
        _ => AutoAdvanceClass::Strategic,
    }
}

fn event_single_candidate_auto_reason(
    session: &RunControlSession,
    candidate: &DecisionCandidate,
) -> Option<&'static str> {
    if session.run_state.event_state.as_ref().is_some_and(|event| {
        event.id == crate::state::events::EventId::Neow && event.current_screen > 0
    }) {
        return None;
    }
    if let Ok(index) = candidate.id.parse::<usize>() {
        let options = crate::engine::event_handler::get_event_options(&session.run_state);
        if let Some(option) = options.get(index) {
            return classify_event_option_boundary_v1(option).single_auto_advance_reason();
        }
    }
    let Some(resolution) = candidate.resolution.as_ref() else {
        return (candidate.note.as_deref() == Some("routine"))
            .then_some("routine event transition");
    };
    (resolution.known_effects.is_empty()
        && resolution.unresolved_effects.is_empty()
        && matches!(
            resolution.followup,
            Some(
                super::view_model::FollowupBoundary::EventScreenAdvance
                    | super::view_model::FollowupBoundary::EventComplete
            )
        ))
    .then_some("routine event transition")
}

fn single_candidate_reason(
    session: &RunControlSession,
    candidate: &DecisionCandidate,
    class: AutoAdvanceClass,
) -> &'static str {
    match (&session.engine_state, class, candidate.id.as_str()) {
        (EngineState::TreasureRoom(_), AutoAdvanceClass::Routine, _) => "single chest action",
        (EngineState::Shop(_), AutoAdvanceClass::Routine, "leave") => "only shop exit remains",
        (EngineState::EventRoom, AutoAdvanceClass::Forced, _) => {
            event_single_candidate_auto_reason(session, candidate)
                .unwrap_or("single forced event transition")
        }
        (EngineState::RunPendingChoice(_), AutoAdvanceClass::Forced, _) => {
            "single forced run choice"
        }
        _ => "single safe action",
    }
}

fn auto_class_label(class: AutoAdvanceClass) -> &'static str {
    match class {
        AutoAdvanceClass::Routine => "routine",
        AutoAdvanceClass::Forced => "forced",
        AutoAdvanceClass::Strategic => "strategic",
        AutoAdvanceClass::Unsafe => "unsafe",
    }
}

fn human_stop_reason(session: &RunControlSession) -> String {
    match &session.engine_state {
        EngineState::EventRoom => {
            let is_neow_bonus = session.run_state.event_state.as_ref().is_some_and(|event| {
                event.id == crate::state::events::EventId::Neow && event.current_screen > 0
            });
            if is_neow_bonus {
                "Neow bonus requires human choice".to_string()
            } else {
                "event option requires human choice".to_string()
            }
        }
        EngineState::MapNavigation => "map route requires human choice".to_string(),
        EngineState::MapOverlay { .. } => "map preview requires route choice or cancel".to_string(),
        EngineState::RewardScreen(reward) if reward.pending_card_choice.is_some() => {
            "card reward requires human choice".to_string()
        }
        EngineState::RewardOverlay { reward_state, .. }
            if reward_state.pending_card_choice.is_some() =>
        {
            "card reward requires human choice".to_string()
        }
        EngineState::RewardScreen(reward) if reward_has_card_item(reward) => {
            "card reward requires human choice".to_string()
        }
        EngineState::RewardOverlay { reward_state, .. } if reward_has_card_item(reward_state) => {
            "card reward requires human choice".to_string()
        }
        EngineState::RewardScreen(reward)
            if reward_has_relic_item(reward) && reward_has_sapphire_key_item(reward) =>
        {
            "relic reward or Sapphire Key requires human choice".to_string()
        }
        EngineState::RewardOverlay { reward_state, .. }
            if reward_has_relic_item(reward_state)
                && reward_has_sapphire_key_item(reward_state) =>
        {
            "relic reward or Sapphire Key requires human choice".to_string()
        }
        EngineState::RewardScreen(reward) if reward_has_relic_item(reward) => {
            "relic reward requires human choice".to_string()
        }
        EngineState::RewardOverlay { reward_state, .. } if reward_has_relic_item(reward_state) => {
            "relic reward requires human choice".to_string()
        }
        EngineState::RewardScreen(reward) if !reward.items.is_empty() => {
            "remaining reward requires human choice".to_string()
        }
        EngineState::RewardOverlay { reward_state, .. } if !reward_state.items.is_empty() => {
            "remaining overlay reward requires human choice".to_string()
        }
        EngineState::RewardScreen(_) => {
            "reward screen cannot be advanced automatically".to_string()
        }
        EngineState::RewardOverlay { .. } => {
            "overlay reward screen cannot be advanced automatically".to_string()
        }
        EngineState::TreasureRoom(_) => {
            "treasure room is not at an executable routine boundary".to_string()
        }
        EngineState::Campfire => "campfire action requires human choice".to_string(),
        EngineState::Shop(_) => "shop action requires human choice".to_string(),
        EngineState::RunPendingChoice(_) => "card selection requires human choice".to_string(),
        EngineState::BossRelicSelect(_) => "boss relic choice requires human choice".to_string(),
        EngineState::CombatStart(_) => {
            "combat start is not yet a stable player boundary".to_string()
        }
        EngineState::CombatProcessing => "combat is still processing".to_string(),
        EngineState::CombatPlayerTurn | EngineState::PendingChoice(_) => {
            "combat boundary requires search or human action".to_string()
        }
        EngineState::GameOver(_) => "run is over".to_string(),
        EngineState::DoubleBossComplete => "second boss continuation awaits Proceed".to_owned(),
        EngineState::Act3VictoryComplete => "Act 3 victory continuation awaits Proceed".to_owned(),
        EngineState::TrueVictoryComplete => "Heart victory continuation awaits Proceed".to_owned(),
    }
}

fn reward_has_card_item(reward: &crate::state::rewards::RewardState) -> bool {
    reward
        .items
        .iter()
        .any(|item| matches!(item, crate::state::rewards::RewardItem::Card { .. }))
}

fn reward_has_relic_item(reward: &crate::state::rewards::RewardState) -> bool {
    reward
        .items
        .iter()
        .any(|item| matches!(item, crate::state::rewards::RewardItem::Relic { .. }))
}

fn reward_has_sapphire_key_item(reward: &crate::state::rewards::RewardState) -> bool {
    reward
        .items
        .iter()
        .any(|item| matches!(item, crate::state::rewards::RewardItem::SapphireKey))
}

fn finish_auto_step(
    session: &RunControlSession,
    before: &RunVisibleSnapshot,
    applied: AutoAppliedLog,
    trace_annotations: Vec<RunControlTraceAnnotationV1>,
    decision_parent_snapshots: Vec<RunControlDecisionParentSnapshotV1>,
    stop_kind: RunControlAutoStopKind,
    reason: impl Into<String>,
    detail: Option<String>,
) -> Result<RunProgressOutcome, String> {
    finish_progress_outcome(
        session,
        before,
        applied,
        trace_annotations,
        decision_parent_snapshots,
        Some(stop_kind),
        reason,
        detail,
    )
}

fn finish_progress_outcome(
    session: &RunControlSession,
    before: &RunVisibleSnapshot,
    applied: AutoAppliedLog,
    mut trace_annotations: Vec<RunControlTraceAnnotationV1>,
    decision_parent_snapshots: Vec<RunControlDecisionParentSnapshotV1>,
    stop_kind: Option<RunControlAutoStopKind>,
    reason: impl Into<String>,
    detail: Option<String>,
) -> Result<RunProgressOutcome, String> {
    let reason = reason.into();
    let applied_operations = applied.len();
    let view = build_run_control_view_model(session);
    let mut lines = vec![if stop_kind.is_some() {
        format!("Stopped at boundary: {}", view.header.title)
    } else {
        format!("Applied one atomic progress step: {}", view.header.title)
    }];
    lines.push("Applied:".to_string());
    if applied.is_empty() {
        lines.push("  none".to_string());
    } else {
        for step in &applied.steps {
            lines.push(format!("  - {}", step.label));
        }
    }
    lines.push(format!(
        "{}: {reason}",
        if stop_kind.is_some() {
            "Reason"
        } else {
            "Result"
        }
    ));
    lines.push(super::next_hint::run_control_next_hint(session).to_string());
    if let Some(detail) = detail.filter(|detail| !detail.trim().is_empty()) {
        lines.push("Detail:".to_string());
        lines.extend(detail.lines().map(|line| format!("  {line}")));
    }
    if stop_kind.is_some() {
        if let Some(annotation) =
            super::noncombat_boundary::noncombat_human_boundary_annotation(session, &reason)?
        {
            trace_annotations.push(annotation);
        }
    }

    if applied.is_empty() {
        lines.push(super::render::render_run_control_state(session));
        let outcome = RunProgressOutcome::message(lines.join("\n"))
            .with_auto_applied_steps(applied.steps)
            .with_trace_annotations(trace_annotations)
            .with_decision_parent_snapshots(decision_parent_snapshots)
            .with_progress_steps(applied.progress_steps);
        return Ok(match stop_kind {
            Some(kind) => {
                outcome.with_progress_step(RunProgressStepV1::Stop(RunControlAutoStopV1 {
                    kind,
                    reason,
                    applied_operations,
                }))
            }
            None => outcome,
        });
    }

    let after = RunVisibleSnapshot::capture(session);
    let action_result = action_result_from_transition(
        TransitionAction {
            label: format!(
                "advance-to-human-boundary applied {} operation(s)",
                applied.len()
            ),
        },
        before,
        &after,
        current_run_apply_status(session),
    );
    lines.push(render_action_result(&action_result));
    lines.push(super::render::render_run_control_state(session));
    let outcome = RunProgressOutcome::action(lines.join("\n"), action_result)
        .with_auto_applied_steps(applied.steps)
        .with_trace_annotations(trace_annotations)
        .with_decision_parent_snapshots(decision_parent_snapshots)
        .with_progress_steps(applied.progress_steps);
    Ok(match stop_kind {
        Some(kind) => outcome.with_progress_step(RunProgressStepV1::Stop(RunControlAutoStopV1 {
            kind,
            reason,
            applied_operations,
        })),
        None => outcome,
    })
}

fn combat_search_stop_reason(
    rejections: &[Option<AtomicCombatSearchRejectionV2>],
    adjudications: &[Option<CombatLineAdjudicationV1>],
) -> String {
    if let Some(CombatLineAdjudicationV1::Rejected {
        reason: CombatLineRejectionReasonV1::NewCurse { cards },
        ..
    }) = adjudications.iter().rev().flatten().next()
    {
        let gained_curses = cards
            .iter()
            .map(|card| format!("{}#{}", gameplay_display_label_v1(card.id), card.lineage))
            .collect::<Vec<_>>()
            .join(",");
        return format!(
            "combat search rejected line under clean-only policy: gained_curses=[{gained_curses}]"
        );
    }
    let has = |kind| {
        rejections
            .iter()
            .flatten()
            .any(|rejection| *rejection == kind)
    };
    if has(AtomicCombatSearchRejectionV2::DirtyWinningCandidateRejected) {
        "combat search rejected dirty winning line".to_string()
    } else if has(AtomicCombatSearchRejectionV2::HpLossLimitExceeded) {
        "combat search win exceeded hp-loss limit".to_string()
    } else if has(AtomicCombatSearchRejectionV2::InvalidCardIdentity) {
        "combat search rejected invalid card identity".to_string()
    } else {
        "combat search did not find an executable complete win".to_string()
    }
}

fn current_run_apply_status(session: &RunControlSession) -> RunApplyStatus {
    match session.engine_state {
        EngineState::GameOver(RunResult::Victory) => RunApplyStatus::Victory,
        EngineState::GameOver(RunResult::Defeat) => RunApplyStatus::Defeat,
        _ => RunApplyStatus::Running,
    }
}

fn trim_search_rejection(message: &str) -> String {
    message
        .lines()
        .take_while(|line| !line.starts_with("===="))
        .take(12)
        .collect::<Vec<_>>()
        .join("\n")
}

fn combine_search_rejections(no_potion: Option<String>, fallback: String) -> String {
    match no_potion {
        Some(no_potion) => format!(
            "No-potion attempt:\n{}\n\nFallback attempt:\n{}",
            indent_block(&no_potion),
            indent_block(&fallback)
        ),
        None => fallback,
    }
}

fn combine_three_search_rejections(
    no_potion: Option<String>,
    fallback: String,
    potion_rescue: String,
) -> String {
    let mut sections = Vec::new();
    if let Some(no_potion) = no_potion {
        sections.push(format!("No-potion attempt:\n{}", indent_block(&no_potion)));
    }
    sections.push(format!("Fallback attempt:\n{}", indent_block(&fallback)));
    sections.push(format!(
        "Potion rescue attempt:\n{}",
        indent_block(&potion_rescue)
    ));
    sections.join("\n\n")
}

fn indent_block(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        apply_guarded_auto_step, auto_no_potion_first_options, auto_potion_rescue_options,
        auto_search_options, combat_search_stop_reason,
        high_stakes_auto_search_requires_hp_loss_gate,
    };
    use crate::ai::combat_search_v2::{
        CombatSearchAcceptancePluginId, CombatSearchArtifactPluginId, CombatSearchAttemptPolicy,
        CombatSearchBudgetSpec, CombatSearchEngineProfile, CombatSearchPluginStack,
        CombatSearchProfile, CombatSearchV2PotionPolicy, CombatSearchV2TurnPlanPolicy,
    };
    use crate::content::potions::{Potion, PotionId};
    use crate::eval::run_control::{
        AtomicCombatSearchOptionsV2, CombatLineAdjudicationV1, CombatLineObservedOutcomeV1,
        CombatLineRejectionReasonV1, RunActionCardSnapshotV1, RunControlAutoStepOptions,
        RunControlConfig, RunControlHpLossLimit, RunControlRouteAutomationMode, RunControlSession,
    };
    use crate::sim::combat::CombatTerminal;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;

    #[test]
    fn owner_audit_auto_step_stops_at_card_reward_human_boundary() {
        let mut session = test_session_at_pending_card_reward(vec![
            crate::content::cards::CardId::SearingBlow,
            crate::content::cards::CardId::HeavyBlade,
            crate::content::cards::CardId::Clothesline,
        ]);

        let outcome = apply_guarded_auto_step(
            &mut session,
            RunControlAutoStepOptions {
                route: RunControlRouteAutomationMode::Policy,
                ..RunControlAutoStepOptions::default()
            },
        )
        .expect("owner audit auto step should stop cleanly at card reward");

        assert_eq!(
            outcome.auto_stop().map(|stop| stop.kind),
            Some(crate::eval::run_control::RunControlAutoStopKind::HumanBoundary)
        );
        assert!(outcome.action_result.is_none());
        assert!(matches!(
            session.engine_state,
            EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. }
        ));
    }

    #[test]
    fn auto_search_options_only_add_interactive_time_budget_without_strategy_overrides() {
        let session = RunControlSession::new(RunControlConfig {
            search_wall_ms: Some(30_000),
            ..RunControlConfig::default()
        });

        let options = auto_search_options(&session, AtomicCombatSearchOptionsV2::default());
        assert_eq!(options.wall_ms, None);
        assert_eq!(options.turn_plan_policy, None);

        let options = auto_search_options(
            &session,
            AtomicCombatSearchOptionsV2 {
                wall_ms: Some(500),
                turn_plan_policy: Some(CombatSearchV2TurnPlanPolicy::DiagnosticOnly),
                ..AtomicCombatSearchOptionsV2::default()
            },
        );
        assert_eq!(options.wall_ms, Some(500));
        assert_eq!(
            options.turn_plan_policy,
            Some(CombatSearchV2TurnPlanPolicy::DiagnosticOnly)
        );

        let session = RunControlSession::new(RunControlConfig::default());
        let options = auto_search_options(&session, AtomicCombatSearchOptionsV2::default());
        assert_eq!(options.wall_ms, Some(5_000));

        let profile_options = auto_search_options(
            &session,
            AtomicCombatSearchOptionsV2 {
                profile: Some(CombatSearchProfile {
                    label: "test_profile_budget",
                    engine: CombatSearchEngineProfile {
                        budget: CombatSearchBudgetSpec {
                            max_nodes: 10_000,
                            wall_ms: 100,
                        },
                        plugins: CombatSearchPluginStack::default(),
                    },
                    policy: CombatSearchAttemptPolicy {
                        acceptance: CombatSearchAcceptancePluginId::AcceptedLineOnly,
                        artifacts: CombatSearchArtifactPluginId::PortfolioAttempt,
                    },
                }),
                ..AtomicCombatSearchOptionsV2::default()
            },
        );
        assert_eq!(profile_options.wall_ms, None);
    }

    fn test_session_at_pending_card_reward(
        card_ids: Vec<crate::content::cards::CardId>,
    ) -> RunControlSession {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        let cards = card_ids
            .into_iter()
            .map(|card_id| crate::state::rewards::RewardCard::new(card_id, 0))
            .collect::<Vec<_>>();
        let mut reward = crate::state::rewards::RewardState::new();
        reward.items = vec![crate::state::rewards::RewardItem::Card {
            cards: cards.clone(),
        }];
        reward.pending_card_choice = Some(cards);
        reward.pending_card_reward_index = Some(0);
        session.engine_state = EngineState::RewardScreen(reward);
        session
    }

    #[test]
    fn auto_search_options_enables_semantic_potions_for_boss_auto_combat() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.meta.is_boss_fight = true;
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomBoss,
            }),
        ));

        let options = auto_search_options(&session, AtomicCombatSearchOptionsV2::default());

        assert_eq!(
            options.potion_policy,
            Some(CombatSearchV2PotionPolicy::SemanticBudgeted)
        );
        assert_eq!(options.max_potions_used, Some(2));
    }

    #[test]
    fn auto_search_options_enables_single_semantic_potion_for_elite_auto_combat() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.meta.is_elite_fight = true;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomElite,
            }),
        ));

        let options = auto_search_options(&session, AtomicCombatSearchOptionsV2::default());

        assert_eq!(
            options.potion_policy,
            Some(CombatSearchV2PotionPolicy::SemanticBudgeted)
        );
        assert_eq!(options.max_potions_used, Some(1));
    }

    #[test]
    fn auto_search_options_keeps_potions_disabled_for_ordinary_auto_combat() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            crate::test_support::blank_test_combat(),
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));

        let options = auto_search_options(&session, AtomicCombatSearchOptionsV2::default());

        assert_eq!(options.potion_policy, None);
        assert_eq!(options.max_potions_used, None);
    }

    #[test]
    fn auto_search_options_keeps_user_potion_overrides_for_high_stakes_auto_combat() {
        let mut session = RunControlSession::new(RunControlConfig {
            search_potion_policy: Some(CombatSearchV2PotionPolicy::Never),
            ..RunControlConfig::default()
        });
        let mut combat = crate::test_support::blank_test_combat();
        combat.meta.is_elite_fight = true;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomElite,
            }),
        ));

        let options = auto_search_options(&session, AtomicCombatSearchOptionsV2::default());
        assert_eq!(options.potion_policy, None);

        let options = auto_search_options(
            &session,
            AtomicCombatSearchOptionsV2 {
                potion_policy: Some(CombatSearchV2PotionPolicy::All),
                max_potions_used: Some(1),
                ..AtomicCombatSearchOptionsV2::default()
            },
        );
        assert_eq!(options.potion_policy, Some(CombatSearchV2PotionPolicy::All));
        assert_eq!(options.max_potions_used, Some(1));
    }

    #[test]
    fn auto_no_potion_first_uses_hp_loss_limit() {
        let mut session = RunControlSession::new(RunControlConfig {
            search_max_hp_loss: Some(12),
            ..RunControlConfig::default()
        });
        let mut combat = crate::test_support::blank_test_combat();
        combat.meta.is_boss_fight = true;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomBoss,
            }),
        ));

        let probe = auto_no_potion_first_options(&session, &AtomicCombatSearchOptionsV2::default())
            .expect("hp-loss-limited boss auto combat should try no-potion first");

        assert_eq!(probe.wall_ms, Some(5_000));
        assert_eq!(probe.potion_policy, Some(CombatSearchV2PotionPolicy::Never));
        assert_eq!(probe.max_potions_used, Some(0));

        let mut no_limit = RunControlSession::new(RunControlConfig::default());
        let mut no_limit_combat = crate::test_support::blank_test_combat();
        no_limit_combat.meta.is_boss_fight = true;
        no_limit.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            no_limit_combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomBoss,
            }),
        ));
        assert_eq!(
            auto_no_potion_first_options(&no_limit, &AtomicCombatSearchOptionsV2::default()),
            None
        );
    }

    #[test]
    fn high_stakes_auto_search_requires_explicit_hp_loss_gate() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.meta.is_boss_fight = true;
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomBoss,
            }),
        ));

        assert!(high_stakes_auto_search_requires_hp_loss_gate(
            &session,
            &AtomicCombatSearchOptionsV2::default()
        ));
        assert!(!high_stakes_auto_search_requires_hp_loss_gate(
            &session,
            &AtomicCombatSearchOptionsV2 {
                max_hp_loss: Some(RunControlHpLossLimit::Limit(20)),
                ..AtomicCombatSearchOptionsV2::default()
            }
        ));
        assert!(!high_stakes_auto_search_requires_hp_loss_gate(
            &session,
            &AtomicCombatSearchOptionsV2 {
                max_hp_loss: Some(RunControlHpLossLimit::Unlimited),
                ..AtomicCombatSearchOptionsV2::default()
            }
        ));

        let outcome = apply_guarded_auto_step(&mut session, Default::default())
            .expect("guarded auto-step should reject without mutating");

        assert!(outcome.action_result.is_none());
        assert!(outcome
            .message
            .contains("Reason: high-stakes combat auto-search requires an hp-loss gate"));
        assert!(outcome.message.contains("n max_hp_loss=N"));
        assert!(matches!(
            session.engine_state,
            EngineState::CombatPlayerTurn
        ));
        assert!(session.active_combat.is_some());
    }

    #[test]
    fn ordinary_auto_search_does_not_require_hp_loss_gate() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            crate::test_support::blank_test_combat(),
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));

        assert!(!high_stakes_auto_search_requires_hp_loss_gate(
            &session,
            &AtomicCombatSearchOptionsV2::default()
        ));
    }

    #[test]
    fn auto_potion_rescue_uses_one_potion_only_when_hp_loss_gate_is_set() {
        let mut session = RunControlSession::new(RunControlConfig {
            search_max_hp_loss: Some(8),
            ..RunControlConfig::default()
        });
        session.engine_state = EngineState::CombatPlayerTurn;
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 42))];
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));

        let rescue = auto_potion_rescue_options(&session, &AtomicCombatSearchOptionsV2::default())
            .expect("hp-loss-limited ordinary combat with potion should allow rescue attempt");

        assert_eq!(rescue.wall_ms, Some(5_000));
        assert_eq!(rescue.potion_policy, Some(CombatSearchV2PotionPolicy::All));
        assert_eq!(rescue.max_potions_used, Some(1));
    }

    #[test]
    fn auto_potion_rescue_respects_explicit_potion_policy_and_missing_gate() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 42))];
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));

        assert_eq!(
            auto_potion_rescue_options(&session, &AtomicCombatSearchOptionsV2::default()),
            None
        );

        let blocked_by_explicit_policy = AtomicCombatSearchOptionsV2 {
            max_hp_loss: Some(RunControlHpLossLimit::Limit(8)),
            potion_policy: Some(CombatSearchV2PotionPolicy::Never),
            ..AtomicCombatSearchOptionsV2::default()
        };
        assert_eq!(
            auto_potion_rescue_options(&session, &blocked_by_explicit_policy),
            None
        );
    }

    #[test]
    fn combat_search_stop_reason_preserves_new_curse_detail() {
        let adjudication = CombatLineAdjudicationV1::Rejected {
            policy: CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse,
            reason: CombatLineRejectionReasonV1::NewCurse {
                cards: vec![RunActionCardSnapshotV1 {
                    id: crate::content::cards::CardId::Parasite,
                    lineage: crate::test_support::scenario_lineage(9001),
                    upgrades: 0,
                }],
            },
            observed_outcome: CombatLineObservedOutcomeV1 {
                terminal: CombatTerminal::Win,
                final_hp: 44,
                hp_loss: 0,
                potions_used: 0,
                action_count: 32,
                gold_delta: 0,
                ritual_dagger_growth: 0,
                gained_curses: vec![RunActionCardSnapshotV1 {
                    id: crate::content::cards::CardId::Parasite,
                    lineage: crate::test_support::scenario_lineage(9001),
                    upgrades: 0,
                }],
            },
        };

        assert_eq!(
            combat_search_stop_reason(
                &[Some(
                    super::AtomicCombatSearchRejectionV2::DirtyWinningCandidateRejected,
                )],
                &[Some(adjudication)],
            ),
            "combat search rejected line under clean-only policy: gained_curses=[Parasite#constructed_scenario:9001]"
        );
    }
}
