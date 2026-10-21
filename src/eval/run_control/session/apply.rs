use crate::engine::run_loop::tick_run_active_with_observer;
use crate::state::core::{ClientInput, EngineState, RunResult};

use super::{CombatCompletionSource, RecentCombatEnemyHpV1, RunControlSession, RunProgressOutcome};
use crate::eval::run_control::auto_capture::render_auto_capture_result;
use crate::eval::run_control::outcome::combat_enemy_hp;
use crate::eval::run_control::render::render_run_control_state;
use crate::eval::run_control::trace_annotation::RunControlTraceAnnotationV1;
use crate::eval::run_control::transition_report::{
    action_result_changes_from_domain_events, action_result_from_transition,
    action_result_from_transition_with_extra_changes, render_action_result,
    transition_action_for_input, ActionResult, RunApplyStatus, RunVisibleSnapshot,
    TransitionAction,
};
use crate::eval::run_control::view_model::CandidateAction;
use crate::eval::run_control::{
    AtomicCombatSearchAttemptV2, AtomicCombatSearchOptionsV2, RunControlAutoStepOptions,
    RunControlHpLossLimit,
};
use crate::eval::run_control::{
    RunDecisionAction, RunDecisionBoundaryV1, RunDecisionSelectionSourceV1,
    RunDecisionTransactionV1, RunForcedTransitionKindV1, RunForcedTransitionV1,
};

const MAX_STABLE_ADVANCE_TICKS: usize = 2_000;

struct AppliedDecisionEffect {
    progress_message: String,
    result: ActionResult,
    trace_annotations: Vec<RunControlTraceAnnotationV1>,
}

impl AppliedDecisionEffect {
    fn project_progress_outcome(self) -> RunProgressOutcome {
        RunProgressOutcome::action(self.progress_message, self.result)
            .with_trace_annotations(self.trace_annotations)
    }
}

impl RunControlSession {
    pub fn apply_decision_action(
        &mut self,
        action: RunDecisionAction,
    ) -> Result<RunProgressOutcome, String> {
        match action {
            RunDecisionAction::Input(input) => self.apply_input(input),
            RunDecisionAction::SkipCardReward { reward_item_index } => {
                self.apply_branch_skip_card_reward(reward_item_index)
            }
            RunDecisionAction::SingingBowlCardReward { reward_item_index } => {
                self.apply_singing_bowl_card_reward(reward_item_index)
            }
        }
    }

    pub fn apply_candidate_id(&mut self, candidate_id: &str) -> Result<RunProgressOutcome, String> {
        let transaction = self.execute_candidate_transaction_with_source(
            candidate_id,
            RunDecisionSelectionSourceV1::ExplicitCandidate,
            Vec::new(),
        )?;
        Ok(transaction.project_progress_outcome(self))
    }

    pub fn execute_candidate_transaction(
        &mut self,
        candidate_id: &str,
    ) -> Result<RunDecisionTransactionV1, String> {
        self.execute_candidate_transaction_with_source(
            candidate_id,
            RunDecisionSelectionSourceV1::ExplicitCandidate,
            Vec::new(),
        )
    }

    /// Execute one visible candidate on an isolated, non-committable session.
    ///
    /// This is the owner-level read-only preview boundary. The returned
    /// session contains the in-memory successor, but every externally visible
    /// capability is disabled before execution. Callers may inspect it, but
    /// must execute the candidate again on the original session to commit it.
    pub fn preview_candidate_successor(&self, candidate_id: &str) -> Result<Self, String> {
        let mut preview = self.clone();
        preview.set_auto_capture_config(Default::default());
        preview.execute_candidate_transaction(candidate_id)?;
        Ok(preview)
    }

    pub fn execute_only_candidate_transaction(
        &mut self,
    ) -> Result<RunDecisionTransactionV1, String> {
        let surface = super::super::decision_surface::build_decision_surface(self);
        let [candidate] = surface.view.candidates.as_slice() else {
            return Err("exactly one visible candidate is required".to_string());
        };
        let candidate_id = candidate.id.clone();
        self.execute_candidate_transaction_with_source(
            &candidate_id,
            RunDecisionSelectionSourceV1::OnlyVisibleCandidate,
            Vec::new(),
        )
    }

    pub(in crate::eval::run_control) fn execute_routine_candidate_transaction(
        &mut self,
        candidate_id: &str,
    ) -> Result<RunDecisionTransactionV1, String> {
        self.execute_candidate_transaction_with_source(
            candidate_id,
            RunDecisionSelectionSourceV1::RoutinePolicy,
            Vec::new(),
        )
    }

    pub(in crate::eval::run_control) fn execute_route_candidate_transaction(
        &mut self,
        candidate_id: &str,
    ) -> Result<RunDecisionTransactionV1, String> {
        self.execute_candidate_transaction_with_source(
            candidate_id,
            RunDecisionSelectionSourceV1::RoutePolicy,
            Vec::new(),
        )
    }

    pub(in crate::eval::run_control) fn execute_reward_candidate_transaction(
        &mut self,
        candidate_id: &str,
        trace_annotation: RunControlTraceAnnotationV1,
    ) -> Result<RunDecisionTransactionV1, String> {
        self.execute_candidate_transaction_with_source(
            candidate_id,
            RunDecisionSelectionSourceV1::RewardPolicy,
            vec![trace_annotation],
        )
    }

    pub fn execute_owner_candidate_transaction(
        &mut self,
        candidate_id: &str,
        action: RunDecisionAction,
    ) -> Result<RunDecisionTransactionV1, String> {
        self.execute_bound_candidate_transaction_with_source(
            candidate_id,
            Some(action),
            RunDecisionSelectionSourceV1::OwnerPolicy,
            Vec::new(),
        )
    }

    pub fn apply_owner_candidate(
        &mut self,
        candidate_id: &str,
        action: RunDecisionAction,
    ) -> Result<RunProgressOutcome, String> {
        let transaction = self.execute_owner_candidate_transaction(candidate_id, action)?;
        Ok(transaction.project_progress_outcome(self))
    }

    pub fn apply_learning_candidate(
        &mut self,
        candidate_id: &str,
        action: RunDecisionAction,
    ) -> Result<RunProgressOutcome, String> {
        let transaction = self.execute_bound_candidate_transaction_with_source(
            candidate_id,
            Some(action),
            RunDecisionSelectionSourceV1::LearnedPolicy,
            Vec::new(),
        )?;
        Ok(transaction.project_progress_outcome(self))
    }

    pub fn execute_forced_transition(
        &mut self,
        kind: RunForcedTransitionKindV1,
    ) -> Result<RunForcedTransitionV1, String> {
        let mut trial = self.clone();
        let transition = trial.execute_forced_transition_inner(kind)?;
        *self = trial;
        Ok(transition)
    }

    pub fn apply_forced_transition(
        &mut self,
        kind: RunForcedTransitionKindV1,
    ) -> Result<RunProgressOutcome, String> {
        let transition = self.execute_forced_transition(kind)?;
        Ok(transition.project_progress_outcome(self))
    }

    fn execute_forced_transition_inner(
        &mut self,
        kind: RunForcedTransitionKindV1,
    ) -> Result<RunForcedTransitionV1, String> {
        match kind {
            RunForcedTransitionKindV1::EmptyCampfireExit => {
                if !matches!(self.engine_state, EngineState::Campfire) {
                    return Err("empty-campfire exit requires Campfire state".to_string());
                }
                if !crate::engine::campfire_handler::get_available_options(&self.run_state)
                    .is_empty()
                {
                    return Err(
                        "empty-campfire exit is not forced while campfire options exist"
                            .to_string(),
                    );
                }
                let before = RunDecisionBoundaryV1::capture(self);
                if !before.candidates.is_empty() {
                    return Err(
                        "empty-campfire exit unexpectedly exposed legal candidates".to_string()
                    );
                }
                let visible_before = RunVisibleSnapshot::capture(self);
                tick_run_active_with_observer(
                    &mut self.engine_state,
                    &mut self.run_state,
                    &mut self.active_combat,
                    None,
                );
                if matches!(self.engine_state, EngineState::Campfire) {
                    return Err("empty-campfire forced transition made no progress".to_string());
                }
                let visible_after = RunVisibleSnapshot::capture(self);
                let status = match self.engine_state {
                    EngineState::GameOver(RunResult::Victory) => RunApplyStatus::Victory,
                    EngineState::GameOver(RunResult::Defeat) => RunApplyStatus::Defeat,
                    _ => RunApplyStatus::Running,
                };
                let result = action_result_from_transition(
                    TransitionAction {
                        label: "Empty campfire auto-exit".to_string(),
                    },
                    &visible_before,
                    &visible_after,
                    status,
                );
                let after = RunDecisionBoundaryV1::capture(self);
                RunForcedTransitionV1::new(kind, before, result, after)
            }
        }
    }

    fn execute_candidate_transaction_with_source(
        &mut self,
        candidate_id: &str,
        source: RunDecisionSelectionSourceV1,
        additional_trace_annotations: Vec<RunControlTraceAnnotationV1>,
    ) -> Result<RunDecisionTransactionV1, String> {
        self.execute_bound_candidate_transaction_with_source(
            candidate_id,
            None,
            source,
            additional_trace_annotations,
        )
    }

    fn execute_bound_candidate_transaction_with_source(
        &mut self,
        candidate_id: &str,
        bound_action: Option<RunDecisionAction>,
        source: RunDecisionSelectionSourceV1,
        additional_trace_annotations: Vec<RunControlTraceAnnotationV1>,
    ) -> Result<RunDecisionTransactionV1, String> {
        let surface = super::super::decision_surface::build_decision_surface(self);
        let candidate = super::super::decision_surface::resolve_surface_candidate(
            &surface,
            &self.engine_state,
            candidate_id,
        )
        .ok_or_else(|| format!("no visible candidate '{candidate_id}'"))?;
        let canonical_candidate_id = candidate.id.clone();
        let action = match (&candidate.action, bound_action) {
            (CandidateAction::Execute(expected), None) => expected.clone(),
            (CandidateAction::Execute(expected), Some(actual)) if expected == &actual => actual,
            (CandidateAction::Execute(_), Some(_)) => {
                return Err(format!(
                    "bound action disagrees with visible candidate '{candidate_id}'"
                ));
            }
            (
                CandidateAction::Parameterized { .. },
                Some(actual @ RunDecisionAction::Input(ClientInput::SubmitSelection(_))),
            ) if matches!(
                candidate.key.as_ref(),
                Some(crate::eval::run_control::DecisionCandidateKey::SelectionSubmit { .. })
            ) =>
            {
                actual
            }
            (CandidateAction::Parameterized { .. }, Some(_)) => {
                return Err(format!(
                    "candidate '{candidate_id}' does not accept this bound action"
                ));
            }
            (CandidateAction::Parameterized { .. }, None)
            | (CandidateAction::Unavailable { .. }, _) => {
                return Err(format!(
                    "candidate '{candidate_id}' is not directly executable: {}",
                    candidate.action.summary()
                ));
            }
        };
        let candidate_label = candidate.label.clone();
        let before = RunDecisionBoundaryV1::capture(self);
        let mut effect = self.execute_decision_action_inner(action.clone(), candidate_label)?;
        effect
            .trace_annotations
            .extend(additional_trace_annotations);
        let after = RunDecisionBoundaryV1::capture(self);
        RunDecisionTransactionV1::new(
            before,
            source,
            canonical_candidate_id,
            action,
            effect.result,
            after,
            effect.trace_annotations,
        )
    }

    pub fn apply_only_candidate(&mut self) -> Result<RunProgressOutcome, String> {
        let transaction = self.execute_only_candidate_transaction()?;
        Ok(transaction.project_progress_outcome(self))
    }

    pub fn apply_progress_step(
        &mut self,
        options: RunControlAutoStepOptions,
    ) -> Result<RunProgressOutcome, String> {
        super::super::auto_step::apply_guarded_auto_step(self, options)
    }

    pub fn apply_atomic_combat_search_v2(
        &mut self,
        options: AtomicCombatSearchOptionsV2,
    ) -> Result<RunProgressOutcome, String> {
        super::super::atomic_combat_search::apply_atomic_combat_search_v2(self, options)
    }

    pub fn atomic_combat_search_attempt_v2(
        &self,
        options: AtomicCombatSearchOptionsV2,
    ) -> Result<AtomicCombatSearchAttemptV2, String> {
        super::super::atomic_combat_search_attempt::run_atomic_combat_search_attempt_v2(
            self, options,
        )
    }

    pub fn apply_atomic_combat_search_attempt_v2(
        &mut self,
        attempt: AtomicCombatSearchAttemptV2,
        max_hp_loss: RunControlHpLossLimit,
    ) -> Result<RunProgressOutcome, String> {
        super::super::atomic_combat_search_attempt::apply_atomic_combat_search_attempt_v2(
            self,
            attempt,
            Some(max_hp_loss),
        )
    }

    pub fn apply_route_plan(&mut self) -> Result<RunProgressOutcome, String> {
        super::super::route_policy::apply_route_plan(self)
    }

    fn execute_decision_action_inner(
        &mut self,
        action: RunDecisionAction,
        candidate_label: String,
    ) -> Result<AppliedDecisionEffect, String> {
        match action {
            RunDecisionAction::Input(input) => self.execute_input_inner(input, true),
            custom @ (RunDecisionAction::SkipCardReward { .. }
            | RunDecisionAction::SingingBowlCardReward { .. }) => {
                self.execute_custom_decision_atomically(custom, candidate_label)
            }
        }
    }

    fn execute_custom_decision_atomically(
        &mut self,
        action: RunDecisionAction,
        candidate_label: String,
    ) -> Result<AppliedDecisionEffect, String> {
        let mut trial = self.clone();
        let effect = match action {
            RunDecisionAction::SkipCardReward { reward_item_index } => {
                trial.execute_skip_card_reward_inner(reward_item_index, candidate_label)?
            }
            RunDecisionAction::SingingBowlCardReward { reward_item_index } => {
                trial.execute_singing_bowl_card_reward_inner(reward_item_index, candidate_label)?
            }
            RunDecisionAction::Input(_) => {
                return Err("ordinary input is not a custom decision action".to_string());
            }
        };
        *self = trial;
        Ok(effect)
    }

    fn execute_skip_card_reward_inner(
        &mut self,
        reward_index: usize,
        candidate_label: String,
    ) -> Result<AppliedDecisionEffect, String> {
        let reward = match &self.engine_state {
            EngineState::RewardScreen(reward) => reward,
            EngineState::RewardOverlay { reward_state, .. } => reward_state,
            _ => {
                return Err(
                    "skip-card-reward is only valid on a visible reward boundary".to_string(),
                );
            }
        };
        let pending_index = reward
            .pending_card_reward_index
            .ok_or_else(|| "skip-card-reward requires an opened card reward".to_string())?;
        if pending_index != reward_index {
            return Err(format!(
                "opened card reward item is {pending_index}, not {reward_index}"
            ));
        }
        if reward.pending_card_choice.is_none() {
            return Err("opened card reward is missing its card choices".to_string());
        }
        if !matches!(
            reward.items.get(reward_index),
            Some(crate::state::rewards::RewardItem::Card { .. })
        ) {
            return Err(format!("reward item {reward_index} is not a card reward"));
        }

        let before = RunVisibleSnapshot::capture(self);
        let closed = self.execute_input_inner(ClientInput::Cancel, false)?;
        self.finish_custom_decision_effect(candidate_label, before, closed.trace_annotations, None)
    }

    fn execute_singing_bowl_card_reward_inner(
        &mut self,
        reward_index: usize,
        candidate_label: String,
    ) -> Result<AppliedDecisionEffect, String> {
        super::super::reward_auto::ensure_singing_bowl_card_reward_action(self, reward_index)?;
        let before = RunVisibleSnapshot::capture(self);
        let opened = self.execute_input_inner(ClientInput::ClaimReward(reward_index), false)?;
        let Some(opened_cards) = super::super::reward_auto::active_pending_reward_cards(self)
        else {
            return Err(
                "Singing Bowl opened a reward item but no pending card choice appeared".to_string(),
            );
        };
        let consumed =
            self.execute_input_inner(ClientInput::SelectCard(opened_cards.len()), false)?;
        let mut trace_annotations = opened.trace_annotations;
        trace_annotations.extend(consumed.trace_annotations);
        self.finish_custom_decision_effect(candidate_label, before, trace_annotations, None)
    }

    fn finish_custom_decision_effect(
        &mut self,
        candidate_label: String,
        before: RunVisibleSnapshot,
        trace_annotations: Vec<RunControlTraceAnnotationV1>,
        report_prefix: Option<String>,
    ) -> Result<AppliedDecisionEffect, String> {
        self.decision_step = self.decision_step.saturating_add(1);
        let after = RunVisibleSnapshot::capture(self);
        let status = match self.engine_state {
            EngineState::GameOver(RunResult::Victory) => RunApplyStatus::Victory,
            EngineState::GameOver(RunResult::Defeat) => RunApplyStatus::Defeat,
            _ => RunApplyStatus::Running,
        };
        let result = action_result_from_transition(
            TransitionAction {
                label: candidate_label,
            },
            &before,
            &after,
            status,
        );
        let mut report = render_action_result(&result);
        if let Some(prefix) = report_prefix {
            report = format!("{prefix}\n{report}");
        }
        Ok(AppliedDecisionEffect {
            progress_message: format!("{report}\n{}", render_run_control_state(self)),
            result,
            trace_annotations,
        })
    }

    fn apply_branch_skip_card_reward(
        &mut self,
        reward_index: usize,
    ) -> Result<RunProgressOutcome, String> {
        self.execute_custom_decision_atomically(
            RunDecisionAction::SkipCardReward {
                reward_item_index: reward_index,
            },
            "Skip card reward".to_string(),
        )
        .map(AppliedDecisionEffect::project_progress_outcome)
    }

    fn apply_singing_bowl_card_reward(
        &mut self,
        reward_index: usize,
    ) -> Result<RunProgressOutcome, String> {
        self.execute_custom_decision_atomically(
            RunDecisionAction::SingingBowlCardReward {
                reward_item_index: reward_index,
            },
            "Singing Bowl | gain 2 max HP".to_string(),
        )
        .map(AppliedDecisionEffect::project_progress_outcome)
    }

    pub(in crate::eval::run_control) fn apply_input(
        &mut self,
        input: ClientInput,
    ) -> Result<RunProgressOutcome, String> {
        self.apply_input_inner(input)
    }

    pub(in crate::eval::run_control) fn apply_combat_resolution_input(
        &mut self,
        input: ClientInput,
    ) -> Result<RunProgressOutcome, String> {
        if self.active_combat.is_none() {
            return Err("combat resolution input requires an active combat".to_string());
        }
        let effect = self.execute_input_inner(input, false)?;
        Ok(effect.project_progress_outcome())
    }

    fn apply_input_inner(&mut self, input: ClientInput) -> Result<RunProgressOutcome, String> {
        let effect = self.execute_input_inner(input, true)?;
        Ok(effect.project_progress_outcome())
    }

    fn execute_input_inner(
        &mut self,
        input: ClientInput,
        advance_decision_step: bool,
    ) -> Result<AppliedDecisionEffect, String> {
        self.ensure_combat_started_if_needed()?;
        self.validate_input_for_current_state(&input)?;
        self.observe_shop_visit_before_input();
        let gold_before_input = self.run_state.gold;
        let before_snapshot = RunVisibleSnapshot::capture(self);
        let action_report = transition_action_for_input(self, &input);
        self.observe_active_combat_started();
        let potion_observation = self.combat_outcomes.observe_input_before(
            self.active_combat
                .as_ref()
                .map(|active| &active.combat_state),
            &input,
        );
        let mut tick = tick_run_active_with_observer(
            &mut self.engine_state,
            &mut self.run_state,
            &mut self.active_combat,
            Some(input),
        );
        let mut finished_combat = tick.finished_combat.take();
        let mut advance_ticks = 0usize;
        while tick.keep_running && matches!(self.engine_state, EngineState::CombatProcessing) {
            if advance_ticks >= MAX_STABLE_ADVANCE_TICKS {
                return Err(format!(
                    "run-control exceeded {MAX_STABLE_ADVANCE_TICKS} engine ticks while advancing to a stable boundary"
                ));
            }
            advance_ticks += 1;
            tick = tick_run_active_with_observer(
                &mut self.engine_state,
                &mut self.run_state,
                &mut self.active_combat,
                None,
            );
            if finished_combat.is_none() {
                finished_combat = tick.finished_combat.take();
            }
        }
        self.collapse_completed_event_room();
        self.observe_shop_visit_after_input(gold_before_input);
        let after_combat = finished_combat
            .as_ref()
            .map(|finished| &finished.combat_state)
            .or_else(|| {
                self.active_combat
                    .as_ref()
                    .map(|active| &active.combat_state)
            });
        self.combat_outcomes
            .observe_input_after(potion_observation, after_combat);
        let combat_observation_changes = if let Some(finished) = finished_combat.as_mut() {
            action_result_changes_from_domain_events(finished.combat_state.take_emitted_events())
        } else if let Some(active) = self.active_combat.as_mut() {
            action_result_changes_from_domain_events(active.combat_state.take_emitted_events())
        } else {
            Vec::new()
        };
        if let Some(finished) = finished_combat.as_ref() {
            let completion_source = self
                .current_combat_source
                .take()
                .unwrap_or(CombatCompletionSource::Manual);
            let case_id = format!(
                "seed-{}-combat-{}",
                self.run_state.seed, self.combat_sequence
            );
            let outcome = self.combat_outcomes.finish(case_id, finished);
            self.recent_combat_enemy_hp = Some(RecentCombatEnemyHpV1 {
                combat_sequence: self.combat_sequence,
                terminal_enemy_hp: combat_enemy_hp(&finished.combat_state),
            });
            self.recent_combat_attrition = Some(super::RecentCombatAttritionV1 {
                act: self.run_state.act_num,
                floor: self.run_state.floor_num,
                combat_sequence: self.combat_sequence,
                start_hp: outcome.start_hp,
                end_combat_hp: outcome.final_hp,
                raw_hp_loss: outcome.hp_loss,
                turns: outcome.turns,
                potions_used: outcome.potions_used,
            });
            self.last_completed_combat_sequence = Some(self.combat_sequence);
            self.last_completed_combat_source = Some(completion_source);
        }
        self.cleanup_inactive_combat();
        self.ensure_combat_started_if_needed()?;
        self.cleanup_inactive_combat();
        self.ensure_combat_started_if_needed()?;
        self.observe_active_combat_started();
        let auto_capture = super::super::auto_capture::maybe_auto_capture_combat_start(self)?;
        let mut trace_annotations = Vec::new();
        if advance_decision_step {
            self.decision_step = self.decision_step.saturating_add(1);
        }

        let status = if tick.keep_running {
            RunApplyStatus::Running
        } else {
            match self.engine_state {
                EngineState::GameOver(RunResult::Victory) => RunApplyStatus::Victory,
                EngineState::GameOver(RunResult::Defeat) => RunApplyStatus::Defeat,
                _ => RunApplyStatus::Stopped,
            }
        };
        let after_snapshot = RunVisibleSnapshot::capture(self);
        let action_result = action_result_from_transition_with_extra_changes(
            action_report,
            &before_snapshot,
            &after_snapshot,
            status,
            combat_observation_changes,
        );
        let report = render_action_result(&action_result);
        let report = if let Some(auto_capture) = auto_capture.as_ref() {
            format!("{report}\n{}", render_auto_capture_result(auto_capture))
        } else {
            report
        };
        if let Some(auto_capture) = auto_capture.as_ref() {
            trace_annotations.push(RunControlTraceAnnotationV1::AutoCombatCapture {
                case_id: auto_capture.case_id.clone(),
                capture_path: auto_capture.capture_path.display().to_string(),
                benchmark_manifest_path: auto_capture.benchmark_manifest.display().to_string(),
                label_role: "diagnostic_capture_not_human_baseline".to_string(),
            });
        }
        Ok(AppliedDecisionEffect {
            progress_message: format!("{report}\n{}", render_run_control_state(self)),
            result: action_result,
            trace_annotations,
        })
    }

    fn collapse_completed_event_room(&mut self) {
        if !matches!(self.engine_state, EngineState::EventRoom) {
            return;
        }
        let Some(event_state) = self.run_state.event_state.as_ref() else {
            return;
        };
        // Original Neow keeps an explicit completed dialogue after an
        // immediate reward. The player must activate its Leave button before
        // the map becomes the public decision boundary. Collapsing that page
        // here merges two original-game actions and prevents Java/Rust
        // successor parity.
        if event_state.id == crate::state::events::EventId::Neow && event_state.current_screen == 2
        {
            return;
        }
        if event_state.completed && !event_state.combat_pending {
            // Java potion permissions inspect the current room's WeMeetAgain
            // instance even after its Leave button opens the map. Retain that
            // completed room context until map travel clears it.
            if event_state.id != crate::state::events::EventId::WeMeetAgain {
                self.run_state.event_state = None;
            }
            self.engine_state = EngineState::MapNavigation;
        }
    }
}
