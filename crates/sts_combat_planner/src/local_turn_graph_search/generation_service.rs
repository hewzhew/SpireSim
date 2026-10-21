use super::*;
use crate::generator::diagnostics::TurnOptionGeneratorTiming;

impl LocalTurnGraphWitnessSession {
    pub(super) fn widen(
        &mut self,
        node_id: usize,
        path: &[(usize, usize)],
        boundary_service_view: LocalServiceView,
        generation_view: LocalServiceView,
        requested_work: usize,
        deadline: Option<Instant>,
        stepper: &dyn CombatStepper,
    ) -> bool {
        let proposal_progress = self.seed_public_plan_proposals(node_id, deadline, stepper);
        let remaining_work = self
            .granted_generation_work
            .saturating_sub(self.used.generation_work);
        let remaining_steps = self
            .granted_engine_steps
            .saturating_sub(self.used.engine_steps);
        let generator_work = self.nodes[node_id].generator.counters().generation_work;
        // `select_work` owns the semantic distinction between a fresh anchor
        // batch and a fresh guide/proposal grounding batch. Do not apply a
        // second initial-expansion floor here: doing so makes the reported
        // backed quantum ineffective whenever it is below the anchor quantum.
        let requested_work = if node_id == 0 && generator_work == 0 {
            self.config.root_initial_expansion_work
        } else {
            requested_work
        };
        let work = requested_work.max(1).min(remaining_work);
        if work == 0 || remaining_steps == 0 {
            return proposal_progress;
        }
        let generation_started = Instant::now();
        let (
            before,
            after,
            before_diagnostics,
            after_diagnostics,
            before_timing,
            after_timing,
            options,
            new_gaps,
        ) = {
            let node = &mut self.nodes[node_id];
            node.generator.prefer_lane(match generation_view {
                LocalServiceView::Anchor
                | LocalServiceView::ProposalRoot
                | LocalServiceView::ProposalContinuation => {
                    TurnOptionGeneratorPreferredLane::Anchor
                }
                LocalServiceView::Guide(lane) => TurnOptionGeneratorPreferredLane::Guide(lane),
            });
            let before = node.generator.counters();
            let before_diagnostics = node.generator.diagnostics();
            let before_timing = node.generator.timing();
            let quantum =
                CombatPlanningQuantum {
                    additional_generation_work: work,
                    additional_engine_steps: remaining_steps.min(work.saturating_mul(
                        self.config.generator.max_engine_steps_per_transition.max(1),
                    )),
                    deadline,
                };
            match boundary_service_view {
                LocalServiceView::ProposalRoot => {
                    node.generator.advance_with_root_proposal(stepper, quantum);
                }
                LocalServiceView::ProposalContinuation => {
                    node.generator
                        .advance_with_continuation_proposal(stepper, quantum);
                }
                LocalServiceView::Anchor | LocalServiceView::Guide(_) => {
                    node.generator.advance(stepper, quantum);
                }
            }
            let after = node.generator.counters();
            for lane in node.generator.retained_guide_lanes() {
                let view = LocalServiceView::Guide(lane);
                if !node.generation_service_views.contains(&view) {
                    node.generation_service_views.push(view);
                }
            }
            let after_diagnostics = node.generator.diagnostics();
            let after_timing = node.generator.timing();
            let options = node.generator.take_completed_options();
            let gaps = node.generator.gaps()[node.synced_gaps..].to_vec();
            node.synced_gaps = node.generator.gaps().len();
            if node.generator.is_finished() {
                node.generator.retire_finished_search_storage();
            }
            (
                before,
                after,
                before_diagnostics,
                after_diagnostics,
                before_timing,
                after_timing,
                options,
                gaps,
            )
        };
        self.performance_timing.generation_elapsed_ns = self
            .performance_timing
            .generation_elapsed_ns
            .saturating_add(elapsed_nanos_u64(generation_started));

        let used_work = after.generation_work.saturating_sub(before.generation_work);
        let used_steps = after.engine_steps.saturating_sub(before.engine_steps);
        if used_work == 0 && used_steps == 0 && options.is_empty() {
            return proposal_progress;
        }
        self.shared_agenda
            .charge_generation_work(boundary_service_view, used_work);
        match boundary_service_view {
            LocalServiceView::Anchor => {
                self.nodes[node_id].boundary_anchor_services = self.nodes[node_id]
                    .boundary_anchor_services
                    .saturating_add(1);
            }
            LocalServiceView::ProposalRoot => {
                self.nodes[node_id].boundary_proposal_root_services = self.nodes[node_id]
                    .boundary_proposal_root_services
                    .saturating_add(1);
            }
            LocalServiceView::ProposalContinuation => {
                self.nodes[node_id].boundary_proposal_continuation_services = self.nodes[node_id]
                    .boundary_proposal_continuation_services
                    .saturating_add(1);
            }
            LocalServiceView::Guide(_) => {
                self.nodes[node_id].boundary_guide_services = self.nodes[node_id]
                    .boundary_guide_services
                    .saturating_add(1);
            }
        }
        match generation_view {
            LocalServiceView::Anchor
            | LocalServiceView::ProposalRoot
            | LocalServiceView::ProposalContinuation => {
                self.nodes[node_id].generation_anchor_services = self.nodes[node_id]
                    .generation_anchor_services
                    .saturating_add(1);
            }
            LocalServiceView::Guide(_) => {
                self.nodes[node_id].generation_guide_services = self.nodes[node_id]
                    .generation_guide_services
                    .saturating_add(1);
            }
        }
        self.used.generation_work = self.used.generation_work.saturating_add(used_work);
        self.used.engine_steps = self.used.engine_steps.saturating_add(used_steps);
        self.used.applied_action_transitions = self.used.applied_action_transitions.saturating_add(
            after_diagnostics
                .applied_action_transitions
                .saturating_sub(before_diagnostics.applied_action_transitions),
        );
        self.used.plan_prefix_attempts = self.used.plan_prefix_attempts.saturating_add(
            after_diagnostics
                .plan_prefix_attempts
                .saturating_sub(before_diagnostics.plan_prefix_attempts),
        );
        self.used.plan_prefix_completed = self.used.plan_prefix_completed.saturating_add(
            after_diagnostics
                .plan_prefix_completed
                .saturating_sub(before_diagnostics.plan_prefix_completed),
        );
        self.used.plan_prefix_rejections = self.used.plan_prefix_rejections.saturating_add(
            after_diagnostics
                .plan_prefix_rejections
                .saturating_sub(before_diagnostics.plan_prefix_rejections),
        );
        self.used.unique_successor_states = self.used.unique_successor_states.saturating_add(
            after_diagnostics
                .unique_successor_states
                .saturating_sub(before_diagnostics.unique_successor_states),
        );
        self.used.duplicate_exact_successors = self.used.duplicate_exact_successors.saturating_add(
            after_diagnostics
                .duplicate_exact_successors
                .saturating_sub(before_diagnostics.duplicate_exact_successors),
        );
        self.performance_timing
            .accumulate(generator_timing_delta(before_timing, after_timing));
        self.generation_gaps.extend(new_gaps);

        let admission_started = Instant::now();
        for option in options {
            let from_plan_prefix = option.source() == CompleteTurnOptionSource::EncounterPlanPrefix;
            let root_option_started = Instant::now();
            if node_id == 0 {
                self.record_root_option(&option);
            }
            self.nodes[node_id].generated_options =
                self.nodes[node_id].generated_options.saturating_add(1);
            self.used.completed_turn_options = self.used.completed_turn_options.saturating_add(1);
            self.performance_timing.admission_root_option_elapsed_ns = self
                .performance_timing
                .admission_root_option_elapsed_ns
                .saturating_add(elapsed_nanos_u64(root_option_started));
            match option.boundary() {
                CompleteTurnOptionBoundary::TerminalWin => {
                    self.used.terminal_win_options =
                        self.used.terminal_win_options.saturating_add(1);
                    if !self.trajectory_progress_satisfies(option.trajectory_progress()) {
                        self.used.trajectory_query_rejected_terminal_wins = self
                            .used
                            .trajectory_query_rejected_terminal_wins
                            .saturating_add(1);
                        continue;
                    }
                    let witness_filter_started = Instant::now();
                    let (mut actions, prefix_negative_log_policy) = self.path_actions(path);
                    actions.extend_from_slice(option.actions());
                    let negative_log_policy =
                        prefix_negative_log_policy + option.negative_log_policy();
                    let candidate_is_dominated = !terminal_candidate_could_improve_witness_frontier(
                        &self.original_root,
                        &self.witness_frontier,
                        option.exact_successor(),
                        &actions,
                        negative_log_policy,
                        self.config.max_potions_used,
                    );
                    self.performance_timing.admission_witness_filter_elapsed_ns = self
                        .performance_timing
                        .admission_witness_filter_elapsed_ns
                        .saturating_add(elapsed_nanos_u64(witness_filter_started));
                    if candidate_is_dominated {
                        self.used.witness_replay_dominated_skips =
                            self.used.witness_replay_dominated_skips.saturating_add(1);
                        continue;
                    }
                    self.used.witness_replay_attempts =
                        self.used.witness_replay_attempts.saturating_add(1);
                    let witness_replay_started = Instant::now();
                    let admission = match replay_witness(
                        &self.original_root,
                        &actions,
                        negative_log_policy,
                        OracleCombatWitnessDiscoverySource::PlannerSearch,
                        stepper,
                    ) {
                        Ok(witness) => {
                            match self.verify_witness_trajectory_query(&witness, stepper) {
                                Ok(()) => self.remember_witness(witness),
                                Err(error) => {
                                    self.replay_failure = Some(error);
                                    WitnessAdmission::default()
                                }
                            }
                        }
                        Err(error) => {
                            self.replay_failure = Some(error);
                            WitnessAdmission::default()
                        }
                    };
                    if admission.selected_changed {
                        self.used.witness_replay_improvements =
                            self.used.witness_replay_improvements.saturating_add(1);
                    }
                    if admission.frontier_changed {
                        self.used.witness_frontier_changes =
                            self.used.witness_frontier_changes.saturating_add(1);
                    }
                    self.performance_timing.admission_witness_replay_elapsed_ns = self
                        .performance_timing
                        .admission_witness_replay_elapsed_ns
                        .saturating_add(elapsed_nanos_u64(witness_replay_started));
                    if self.witness_satisfies() {
                        self.performance_timing.admission_elapsed_ns = self
                            .performance_timing
                            .admission_elapsed_ns
                            .saturating_add(elapsed_nanos_u64(admission_started));
                        return true;
                    }
                }
                CompleteTurnOptionBoundary::TerminalLoss => {
                    self.used.terminal_losses = self.used.terminal_losses.saturating_add(1);
                }
                CompleteTurnOptionBoundary::Escape => {}
                CompleteTurnOptionBoundary::NextPlayerTurn => {
                    if let Some(successor) = self.accept_successor(node_id, path, option) {
                        if from_plan_prefix
                            && self
                                .shared_agenda
                                .publish_proposal_continuation(successor, &self.nodes[successor])
                        {
                            self.used.plan_prefix_continuation_enqueues = self
                                .used
                                .plan_prefix_continuation_enqueues
                                .saturating_add(1);
                        }
                    }
                }
            }
        }
        let refresh_started = Instant::now();
        self.refresh_exhaustion(node_id);
        if self.nodes[node_id].generator.is_finished() || self.nodes[node_id].exhausted {
            self.shared_agenda
                .remove_guide_entries(node_id, &self.nodes[node_id]);
        }
        self.performance_timing.admission_refresh_elapsed_ns = self
            .performance_timing
            .admission_refresh_elapsed_ns
            .saturating_add(elapsed_nanos_u64(refresh_started));
        self.performance_timing.admission_elapsed_ns = self
            .performance_timing
            .admission_elapsed_ns
            .saturating_add(elapsed_nanos_u64(admission_started));
        true
    }

    fn refresh_exhaustion(&mut self, node_id: usize) {
        if self.nodes[node_id].exhausted || !self.nodes[node_id].generator.is_finished() {
            return;
        }
        let all_children_exhausted = self.nodes[node_id]
            .children
            .iter()
            .all(|edge| self.nodes[edge.successor].exhausted);
        if all_children_exhausted {
            self.nodes[node_id].exhausted = true;
            self.used.exhausted_nodes = self.used.exhausted_nodes.saturating_add(1);
        }
    }
}

fn generator_timing_delta(
    before: TurnOptionGeneratorTiming,
    after: TurnOptionGeneratorTiming,
) -> LocalTurnGraphPerformanceTiming {
    let TurnOptionGeneratorTiming {
        atomic_expand_elapsed_ns: before_atomic_expand_elapsed_ns,
        transition_simulation_elapsed_ns: before_transition_simulation_elapsed_ns,
        transition_identity_elapsed_ns: before_transition_identity_elapsed_ns,
        transition_key_build_elapsed_ns: before_transition_key_build_elapsed_ns,
        transition_key_index_elapsed_ns: before_transition_key_index_elapsed_ns,
        transition_admission_elapsed_ns: before_transition_admission_elapsed_ns,
        transition_trace_elapsed_ns: before_transition_trace_elapsed_ns,
        transition_seen_elapsed_ns: before_transition_seen_elapsed_ns,
        transition_publish_elapsed_ns: before_transition_publish_elapsed_ns,
        transition_publish_trace_node_elapsed_ns: before_transition_publish_trace_node_elapsed_ns,
        transition_publish_boundary_elapsed_ns: before_transition_publish_boundary_elapsed_ns,
        transition_publish_complete_elapsed_ns: before_transition_publish_complete_elapsed_ns,
        transition_publish_push_elapsed_ns: before_transition_publish_push_elapsed_ns,
        transition_publish_guide_elapsed_ns: before_transition_publish_guide_elapsed_ns,
        transition_publish_retain_elapsed_ns: before_transition_publish_retain_elapsed_ns,
        transition_publish_agenda_elapsed_ns: before_transition_publish_agenda_elapsed_ns,
    } = before;
    let TurnOptionGeneratorTiming {
        atomic_expand_elapsed_ns,
        transition_simulation_elapsed_ns,
        transition_identity_elapsed_ns,
        transition_key_build_elapsed_ns,
        transition_key_index_elapsed_ns,
        transition_admission_elapsed_ns,
        transition_trace_elapsed_ns,
        transition_seen_elapsed_ns,
        transition_publish_elapsed_ns,
        transition_publish_trace_node_elapsed_ns,
        transition_publish_boundary_elapsed_ns,
        transition_publish_complete_elapsed_ns,
        transition_publish_push_elapsed_ns,
        transition_publish_guide_elapsed_ns,
        transition_publish_retain_elapsed_ns,
        transition_publish_agenda_elapsed_ns,
    } = after;

    LocalTurnGraphPerformanceTiming {
        atomic_expand_elapsed_ns: atomic_expand_elapsed_ns
            .saturating_sub(before_atomic_expand_elapsed_ns),
        transition_simulation_elapsed_ns: transition_simulation_elapsed_ns
            .saturating_sub(before_transition_simulation_elapsed_ns),
        transition_identity_elapsed_ns: transition_identity_elapsed_ns
            .saturating_sub(before_transition_identity_elapsed_ns),
        transition_key_build_elapsed_ns: transition_key_build_elapsed_ns
            .saturating_sub(before_transition_key_build_elapsed_ns),
        transition_key_index_elapsed_ns: transition_key_index_elapsed_ns
            .saturating_sub(before_transition_key_index_elapsed_ns),
        transition_admission_elapsed_ns: transition_admission_elapsed_ns
            .saturating_sub(before_transition_admission_elapsed_ns),
        transition_trace_elapsed_ns: transition_trace_elapsed_ns
            .saturating_sub(before_transition_trace_elapsed_ns),
        transition_seen_elapsed_ns: transition_seen_elapsed_ns
            .saturating_sub(before_transition_seen_elapsed_ns),
        transition_publish_elapsed_ns: transition_publish_elapsed_ns
            .saturating_sub(before_transition_publish_elapsed_ns),
        transition_publish_trace_node_elapsed_ns: transition_publish_trace_node_elapsed_ns
            .saturating_sub(before_transition_publish_trace_node_elapsed_ns),
        transition_publish_boundary_elapsed_ns: transition_publish_boundary_elapsed_ns
            .saturating_sub(before_transition_publish_boundary_elapsed_ns),
        transition_publish_complete_elapsed_ns: transition_publish_complete_elapsed_ns
            .saturating_sub(before_transition_publish_complete_elapsed_ns),
        transition_publish_push_elapsed_ns: transition_publish_push_elapsed_ns
            .saturating_sub(before_transition_publish_push_elapsed_ns),
        transition_publish_guide_elapsed_ns: transition_publish_guide_elapsed_ns
            .saturating_sub(before_transition_publish_guide_elapsed_ns),
        transition_publish_retain_elapsed_ns: transition_publish_retain_elapsed_ns
            .saturating_sub(before_transition_publish_retain_elapsed_ns),
        transition_publish_agenda_elapsed_ns: transition_publish_agenda_elapsed_ns
            .saturating_sub(before_transition_publish_agenda_elapsed_ns),
        ..LocalTurnGraphPerformanceTiming::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generator_timing(value: u64) -> TurnOptionGeneratorTiming {
        TurnOptionGeneratorTiming {
            atomic_expand_elapsed_ns: value,
            transition_simulation_elapsed_ns: value,
            transition_identity_elapsed_ns: value,
            transition_key_build_elapsed_ns: value,
            transition_key_index_elapsed_ns: value,
            transition_admission_elapsed_ns: value,
            transition_trace_elapsed_ns: value,
            transition_seen_elapsed_ns: value,
            transition_publish_elapsed_ns: value,
            transition_publish_trace_node_elapsed_ns: value,
            transition_publish_boundary_elapsed_ns: value,
            transition_publish_complete_elapsed_ns: value,
            transition_publish_push_elapsed_ns: value,
            transition_publish_guide_elapsed_ns: value,
            transition_publish_retain_elapsed_ns: value,
            transition_publish_agenda_elapsed_ns: value,
        }
    }

    #[test]
    fn generator_timing_delta_maps_every_generator_field_only() {
        let delta = generator_timing_delta(generator_timing(5), generator_timing(8));
        assert_eq!(
            delta,
            LocalTurnGraphPerformanceTiming {
                atomic_expand_elapsed_ns: 3,
                transition_simulation_elapsed_ns: 3,
                transition_identity_elapsed_ns: 3,
                transition_key_build_elapsed_ns: 3,
                transition_key_index_elapsed_ns: 3,
                transition_admission_elapsed_ns: 3,
                transition_trace_elapsed_ns: 3,
                transition_seen_elapsed_ns: 3,
                transition_publish_elapsed_ns: 3,
                transition_publish_trace_node_elapsed_ns: 3,
                transition_publish_boundary_elapsed_ns: 3,
                transition_publish_complete_elapsed_ns: 3,
                transition_publish_push_elapsed_ns: 3,
                transition_publish_guide_elapsed_ns: 3,
                transition_publish_retain_elapsed_ns: 3,
                transition_publish_agenda_elapsed_ns: 3,
                ..LocalTurnGraphPerformanceTiming::default()
            }
        );
    }
}
