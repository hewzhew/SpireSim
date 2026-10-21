use std::collections::BTreeMap;

use super::super::frontier::RootLineageId;

pub(super) const ROOT_SCHEDULING_POLICY: &str =
    "eager_periodic_root_comparison_exact_win_exploitation_v8";

const COMPARISON_EXPANSIONS_PER_ACTION: u64 = 64;
const EXPLOITATION_MULTIPLIER: u64 = 4;

#[derive(Clone, Copy, Debug)]
pub(super) struct RootActionScheduleState {
    pub(super) id: RootLineageId,
    pub(super) expanded: u64,
    pub(super) has_work: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootSchedulingPhase {
    Comparison,
    Exploitation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RootRoundDecision {
    PopRoot(RootLineageId),
    PopBest,
    CompleteComparison { exhausted: bool },
    Exhausted,
    NoRootActions,
}

pub(super) struct RootRoundScheduler {
    initialized: bool,
    finished: bool,
    phase: RootSchedulingPhase,
    round_index: u32,
    completed_rounds: u32,
    comparison_expansions_per_action: u64,
    exploitation_multiplier: u64,
    round_start_expanded: BTreeMap<RootLineageId, u64>,
    exploitation_start_expanded: u64,
    exploitation_budget: u64,
    activation_reason: &'static str,
}

impl Default for RootRoundScheduler {
    fn default() -> Self {
        Self::with_policy(COMPARISON_EXPANSIONS_PER_ACTION, EXPLOITATION_MULTIPLIER)
    }
}

impl RootRoundScheduler {
    fn with_policy(comparison_expansions_per_action: u64, exploitation_multiplier: u64) -> Self {
        Self {
            initialized: false,
            finished: false,
            phase: RootSchedulingPhase::Comparison,
            round_index: 0,
            completed_rounds: 0,
            comparison_expansions_per_action: comparison_expansions_per_action.max(1),
            exploitation_multiplier: exploitation_multiplier.max(1),
            round_start_expanded: BTreeMap::new(),
            exploitation_start_expanded: 0,
            exploitation_budget: 0,
            activation_reason: "inactive",
        }
    }

    pub(super) fn activate(&mut self, states: &[RootActionScheduleState], reason: &'static str) {
        if self.initialized {
            return;
        }
        self.activation_reason = reason;
        self.start_comparison(states);
    }

    pub(super) fn decide(&mut self, states: &[RootActionScheduleState]) -> RootRoundDecision {
        if states.is_empty() {
            return RootRoundDecision::NoRootActions;
        }
        if !self.initialized {
            return RootRoundDecision::NoRootActions;
        }
        if self.finished {
            return RootRoundDecision::Exhausted;
        }

        match self.phase {
            RootSchedulingPhase::Comparison => self.comparison_decision(states),
            RootSchedulingPhase::Exploitation => {
                if states.iter().all(|state| !state.has_work) {
                    self.finished = true;
                    return RootRoundDecision::Exhausted;
                }
                let spent = total_expanded(states).saturating_sub(self.exploitation_start_expanded);
                if spent < self.exploitation_budget {
                    RootRoundDecision::PopBest
                } else {
                    self.start_comparison(states);
                    self.comparison_decision(states)
                }
            }
        }
    }

    fn comparison_decision(&mut self, states: &[RootActionScheduleState]) -> RootRoundDecision {
        for state in states {
            self.round_start_expanded
                .entry(state.id)
                .or_insert(state.expanded);
        }

        let candidate = states
            .iter()
            .filter(|state| state.has_work)
            .filter_map(|state| {
                let spent = self.comparison_spent(*state);
                (spent < self.comparison_expansions_per_action).then_some((spent, state.id))
            })
            .min_by_key(|(spent, id)| (*spent, *id));

        if let Some((_, id)) = candidate {
            RootRoundDecision::PopRoot(id)
        } else {
            RootRoundDecision::CompleteComparison {
                exhausted: states.iter().all(|state| !state.has_work),
            }
        }
    }

    pub(super) fn complete_comparison(
        &mut self,
        states: &[RootActionScheduleState],
        exhausted: bool,
    ) {
        self.completed_rounds = self.completed_rounds.saturating_add(1);
        self.round_index = self.round_index.saturating_add(1);
        if exhausted {
            self.finished = true;
            return;
        }

        let live_roots = states.iter().filter(|state| state.has_work).count() as u64;
        self.phase = RootSchedulingPhase::Exploitation;
        self.exploitation_start_expanded = total_expanded(states);
        self.exploitation_budget = self
            .comparison_expansions_per_action
            .saturating_mul(live_roots.max(1))
            .saturating_mul(self.exploitation_multiplier);
    }

    pub(super) fn current_comparison_complete(&self, states: &[RootActionScheduleState]) -> bool {
        self.initialized
            && !self.finished
            && self.phase == RootSchedulingPhase::Comparison
            && !states.is_empty()
            && states
                .iter()
                .filter(|state| state.has_work)
                .all(|state| self.comparison_spent(*state) >= self.comparison_expansions_per_action)
    }

    fn comparison_spent(&self, state: RootActionScheduleState) -> u64 {
        let start = self
            .round_start_expanded
            .get(&state.id)
            .copied()
            .unwrap_or(state.expanded);
        state.expanded.saturating_sub(start)
    }

    fn start_comparison(&mut self, states: &[RootActionScheduleState]) {
        self.initialized = true;
        self.finished = false;
        self.phase = RootSchedulingPhase::Comparison;
        self.round_start_expanded = states
            .iter()
            .map(|state| (state.id, state.expanded))
            .collect();
    }

    pub(super) fn started(&self) -> bool {
        self.initialized
    }

    pub(super) fn phase_name(&self) -> &'static str {
        if !self.initialized {
            return "inactive";
        }
        match self.phase {
            RootSchedulingPhase::Comparison => "comparison",
            RootSchedulingPhase::Exploitation => "exploitation",
        }
    }

    pub(super) fn activation_reason(&self) -> &'static str {
        self.activation_reason
    }

    pub(super) fn round_index(&self) -> u32 {
        self.round_index
    }

    pub(super) fn completed_rounds(&self) -> u32 {
        self.completed_rounds
    }

    pub(super) fn comparison_expansions_per_action(&self) -> u64 {
        self.comparison_expansions_per_action
    }
}

fn total_expanded(states: &[RootActionScheduleState]) -> u64 {
    states
        .iter()
        .fold(0u64, |total, state| total.saturating_add(state.expanded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_funds_every_live_root_before_exploitation() {
        let mut scheduler = RootRoundScheduler::with_policy(2, 2);
        let mut expanded = [0u64, 0u64];
        scheduler.activate(&states(expanded), "test");

        for expected in [
            RootLineageId(0),
            RootLineageId(1),
            RootLineageId(0),
            RootLineageId(1),
        ] {
            let states = states(expanded);
            assert_eq!(
                scheduler.decide(&states),
                RootRoundDecision::PopRoot(expected)
            );
            expanded[expected.0 as usize] += 1;
        }

        let states = states(expanded);
        assert_eq!(
            scheduler.decide(&states),
            RootRoundDecision::CompleteComparison { exhausted: false }
        );
        assert!(scheduler.current_comparison_complete(&states));

        scheduler.complete_comparison(&states, false);
        assert_eq!(scheduler.completed_rounds(), 1);
        assert_eq!(scheduler.decide(&states), RootRoundDecision::PopBest);
    }

    fn states(expanded: [u64; 2]) -> [RootActionScheduleState; 2] {
        [
            RootActionScheduleState {
                id: RootLineageId(0),
                expanded: expanded[0],
                has_work: true,
            },
            RootActionScheduleState {
                id: RootLineageId(1),
                expanded: expanded[1],
                has_work: true,
            },
        ]
    }
}
