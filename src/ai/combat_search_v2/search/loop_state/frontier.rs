use std::time::Instant;

use super::super::super::frontier::{QueueEntry, SearchNode};
use super::super::super::PendingChoiceActionWork;
use super::super::root_round_scheduler::RootRoundDecision;
use super::SearchLoopState;

impl SearchLoopState {
    pub(in crate::ai::combat_search_v2::search) fn push_frontier(&mut self, node: SearchNode) {
        self.frontier.push_node(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn push_pending_choice_work(
        &mut self,
        node: SearchNode,
        work: PendingChoiceActionWork,
    ) {
        self.frontier.push_pending_choice_work(node, work);
    }

    pub(in crate::ai::combat_search_v2::search) fn pop_frontier(&mut self) -> Option<QueueEntry> {
        let started = Instant::now();
        let entry = self.pop_scheduled_frontier();
        self.performance.frontier_pop_calls = self.performance.frontier_pop_calls.saturating_add(1);
        self.performance.frontier_pop_elapsed_us = self
            .performance
            .frontier_pop_elapsed_us
            .saturating_add(started.elapsed().as_micros());
        entry
    }

    fn pop_scheduled_frontier(&mut self) -> Option<QueueEntry> {
        if !self.root_surface_fully_materialized() {
            return self
                .frontier
                .pop_unattributed()
                .or_else(|| self.frontier.pop());
        }
        if let Some(entry) = self.frontier.pop_unattributed() {
            return Some(entry);
        }
        let mut states = self.root_action_schedule_states();
        if !self.root_round_scheduler.started() {
            if states.len() <= 1 {
                return self.frontier.pop();
            }
            // Retain whatever result was reportable before sibling roots
            // receive their first meaningful comparison budget.
            self.completed_root_round_trajectories = Some(self.trajectories.clone());
            self.root_round_scheduler
                .activate(&states, "root_surface_materialized");
        }

        loop {
            match self.root_round_scheduler.decide(&states) {
                RootRoundDecision::PopRoot(id) => {
                    if let Some(entry) = self.frontier.pop_root_action(id) {
                        return Some(entry);
                    }
                }
                RootRoundDecision::PopBest => {
                    // Once a comparison round has produced exact wins, spend
                    // exploitation on the root with the best observed exact
                    // outcome. Node-level rollout estimates still order work
                    // inside that root; they no longer redirect an entire
                    // exploitation tranche to a root already known to lose far
                    // more HP.
                    if let Some(id) = self.best_exact_win_root_with_work(&states) {
                        if let Some(entry) = self.frontier.pop_root_action(id) {
                            return Some(entry);
                        }
                    }
                    if let Some(entry) = self.frontier.pop() {
                        return Some(entry);
                    }
                }
                RootRoundDecision::CompleteComparison { exhausted } => {
                    self.completed_root_round_trajectories = Some(self.trajectories.clone());
                    self.root_round_scheduler
                        .complete_comparison(&states, exhausted);
                    if exhausted {
                        return None;
                    }
                }
                RootRoundDecision::Exhausted => return None,
                RootRoundDecision::NoRootActions => return self.frontier.pop(),
            }
            states = self.root_action_schedule_states();
        }
    }
}
