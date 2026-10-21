//! Experimental fixed-goal, prior-guided whole-combat path search.
//! One resident frontier; no hand-written tactical scores or suffix solvers.
//! The prior and goal are frozen. Work exhaustion never labels a state unsolvable.
pub(crate) mod admission;
pub mod inspection;
pub mod linear;
mod prior;
pub mod queue_data;
pub mod routes;
#[cfg(test)]
mod tests;
pub use prior::{choice_groups, log_softmax, GroupedNeutralPrior, PathContext, PathPrior};

use crate::{
    CombatBranch, CombatSearchSpace, CombatSearchState, ResourceRequirement, SearchSpace,
    Transition, TransitionInterruption,
};
use admission::AdmittedChoices;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};
use std::time::Instant;
use sts_combat_outcome::combat_witness_materialization::{
    materialize_replay_checked_combat_outcome_v1,
    project_observed_combat_outcome_with_expenditures_v1, CombatInputExpendituresV1,
    CombatWitnessMaterializedOutcomeV1,
};
use sts_core::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_core::state::core::ClientInput;

/// Per-call work; storage/depth limits are absolute resident limits. These are
/// object counts, not a byte limit. Raising them resumes the retained frontier.
pub struct PathBudget {
    pub work: usize,
    pub max_nodes: usize,
    pub max_pending_edges: usize,
    pub max_depth: usize,
    pub max_engine_steps_per_input: usize,
    pub deadline: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PathStop {
    WorkBudget,
    Deadline,
    NodeLimit,
    EdgeLimit,
    DepthLimit,
    EngineStepLimit,
    NoOpenWork,
}

/// Frozen exploration order; neither variant estimates terminal resource value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathOrder {
    #[default]
    PathProbability,
    /// Sum log(p_best / p_action), preferring deeper prefixes on equal cost.
    /// Depth-blocked edges wait while other branches continue. No C/P bound.
    RelativePrior,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathStats {
    pub prepared_states: usize,
    pub prior_evaluations: usize,
    pub transition_attempts: usize,
    pub settled_inputs: usize,
    pub virtual_choices: usize,
    pub replay_attempts: usize,
    pub replayed_inputs: usize,
    pub engine_steps: usize,
    pub explicit_constraint_rejections: usize,
    pub successor_constraint_rejections: usize,
    pub observed_wins: usize,
    pub goal_misses: usize,
    pub verified_wins: usize,
    /// First replay-verified qualifying win, even if a later plan replaces it.
    pub first_verified_win_work: Option<usize>,
    pub losses_or_escapes: usize,
    pub peak_pending_edges: usize,
    pub deepest_search_depth: usize,
    pub deepest_combat_turn: u32,
}
impl PathStats {
    /// Includes guidance and unsuccessful/interrupted transition attempts.
    /// Not interchangeable with the earlier solver's transition-only work.
    pub fn work(&self) -> usize {
        self.prepared_states + self.transition_attempts + self.replay_attempts
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PathStep {
    pub branch: CombatBranch,
    pub input: Option<ClientInput>,
    pub log_probability: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PathPlan {
    pub steps: Vec<PathStep>,
    pub inputs: Vec<ClientInput>,
    pub outcome: CombatWitnessMaterializedOutcomeV1,
    pub successor: CombatPosition,
    pub found_at_work: usize,
}

struct Node {
    state: CombatSearchState,
    parent: Option<usize>,
    step: Option<PathStep>,
    depth: usize,
    log_probability: f64,
    order_cost: f64,
    expenditures: CombatInputExpendituresV1,
    spent_originals: u64,
    expansion: Option<Expansion>,
}
struct Expansion {
    choices: AdmittedChoices,
    logs: Vec<f64>,
    best_log: f64,
    cursor: usize,
}

#[derive(Clone)]
struct Edge {
    parent: usize,
    choice: usize,
    log_probability: f64,
    priority: f64,
    tie_depth: usize,
    sequence: u64,
}
impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        self.sequence == other.sequence
    }
}
impl Eq for Edge {}
impl PartialOrd for Edge {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Edge {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .total_cmp(&self.priority)
            .then_with(|| self.tie_depth.cmp(&other.tie_depth))
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}
struct Replay {
    node: usize,
    inputs: Vec<ClientInput>,
    current: CombatPosition,
    index: usize,
}

/// State and guidance are owned by this instance. No mutable access to either
/// is exposed, and no model update or goal change occurs during advance.
pub struct PathSearch<P: PathPrior> {
    root: CombatPosition,
    goal: ResourceRequirement,
    prior: P,
    prior_revision: String,
    order: PathOrder,
    nodes: Vec<Node>,
    frontier: BinaryHeap<Edge>,
    depth_deferred: BTreeMap<usize, Vec<Edge>>,
    depth_deferred_count: usize,
    expanding: Option<usize>,
    replay: Option<Replay>,
    next_sequence: u64,
    blocked_engine_limit: Option<usize>,
    failure: Option<String>,
    stats: PathStats,
    plans: Vec<PathPlan>,
}

impl<P: PathPrior> PathSearch<P> {
    pub fn new(root: CombatPosition, goal: ResourceRequirement, prior: P) -> Result<Self, String> {
        Self::with_order(root, goal, prior, PathOrder::PathProbability)
    }

    pub fn with_order(
        root: CombatPosition,
        goal: ResourceRequirement,
        prior: P,
        order: PathOrder,
    ) -> Result<Self, String> {
        goal.validate(&root)?;
        let state = CombatSearchState::new(root.clone())?;
        if EngineCombatStepper.terminal(&root) != CombatTerminal::Unresolved
            || root.combat.runtime.combat_smoked
        {
            return Err("path search requires an unresolved combat root".into());
        }
        let prior_revision = prior.revision().to_owned();
        if prior_revision.trim().is_empty() {
            return Err("path prior revision is required".into());
        }
        let node = Node {
            state,
            parent: None,
            step: None,
            depth: 0,
            log_probability: 0.0,
            order_cost: 0.0,
            expenditures: Default::default(),
            spent_originals: 0,
            expansion: None,
        };
        let deepest_combat_turn = root.combat.turn.turn_count;
        Ok(Self {
            root,
            goal,
            prior,
            prior_revision,
            order,
            nodes: vec![node],
            frontier: BinaryHeap::new(),
            depth_deferred: BTreeMap::new(),
            depth_deferred_count: 0,
            expanding: Some(0),
            replay: None,
            next_sequence: 0,
            blocked_engine_limit: None,
            failure: None,
            stats: PathStats {
                deepest_combat_turn,
                ..Default::default()
            },
            plans: vec![],
        })
    }

    pub fn stats(&self) -> &PathStats {
        &self.stats
    }
    pub fn plans(&self) -> &[PathPlan] {
        &self.plans
    }
    pub fn retained_nodes(&self) -> usize {
        self.nodes.len()
    }
    pub fn pending_edges(&self) -> usize {
        self.frontier.len() + self.depth_deferred_count
    }
    pub fn depth_deferred_edges(&self) -> usize {
        self.depth_deferred_count
    }
    pub fn order(&self) -> PathOrder {
        self.order
    }
    pub fn prior_revision(&self) -> &str {
        &self.prior_revision
    }

    pub fn advance(&mut self, budget: PathBudget) -> Result<PathStop, String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if budget.max_engine_steps_per_input == 0 {
            return Err("positive engine-step limit required".into());
        }
        let result = self.advance_inner(&budget);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn advance_inner(&mut self, budget: &PathBudget) -> Result<PathStop, String> {
        if self.prior.revision() != self.prior_revision {
            return Err("path prior revision changed".into());
        }
        if self
            .blocked_engine_limit
            .is_some_and(|old| budget.max_engine_steps_per_input <= old)
        {
            return Ok(PathStop::EngineStepLimit);
        }
        self.blocked_engine_limit = None;
        while self
            .depth_deferred
            .first_key_value()
            .is_some_and(|(depth, _)| *depth < budget.max_depth)
        {
            let (_, edges) = self.depth_deferred.pop_first().unwrap();
            self.depth_deferred_count -= edges.len();
            self.frontier.extend(edges);
        }
        let initial_work = self.stats.work();
        loop {
            if self.nodes.len() > budget.max_nodes {
                return Ok(PathStop::NodeLimit);
            }
            if self.pending_edges() > budget.max_pending_edges {
                return Ok(PathStop::EdgeLimit);
            }
            if budget.deadline.is_some_and(|d| Instant::now() >= d) {
                return Ok(PathStop::Deadline);
            }
            if self.stats.work() - initial_work >= budget.work {
                return Ok(PathStop::WorkBudget);
            }
            if self.replay.is_some() {
                if let Some(stop) = self.replay_step(budget)? {
                    return Ok(stop);
                }
                continue;
            }
            if let Some(node) = self.expanding {
                if self.nodes[node].expansion.is_none() {
                    self.prepare(node)?;
                    continue;
                }
                loop {
                    let expansion = self.nodes[node].expansion.as_ref().unwrap();
                    if expansion.cursor == expansion.choices.len() {
                        break;
                    }
                    if self.pending_edges() >= budget.max_pending_edges {
                        return Ok(PathStop::EdgeLimit);
                    }
                    if budget.deadline.is_some_and(|d| Instant::now() >= d) {
                        return Ok(PathStop::Deadline);
                    }
                    let i = expansion.cursor;
                    let log_probability = expansion.logs[i];
                    let choice = i;
                    let depth = self.nodes[node]
                        .depth
                        .checked_add(1)
                        .ok_or("path depth overflow")?;
                    let log_path = self.nodes[node].log_probability + log_probability;
                    let (priority, tie_depth) = match self.order {
                        PathOrder::PathProbability => ((depth as f64).ln() - log_path, 0),
                        PathOrder::RelativePrior => (
                            self.nodes[node].order_cost + (expansion.best_log - log_probability),
                            depth,
                        ),
                    };
                    if !priority.is_finite() {
                        return Err("path priority exceeds numeric limits".into());
                    }
                    self.frontier.push(Edge {
                        parent: node,
                        choice,
                        log_probability,
                        priority,
                        tie_depth,
                        sequence: self.next_sequence,
                    });
                    self.next_sequence = self
                        .next_sequence
                        .checked_add(1)
                        .ok_or("path sequence overflow")?;
                    self.nodes[node].expansion.as_mut().unwrap().cursor += 1;
                    self.stats.peak_pending_edges =
                        self.stats.peak_pending_edges.max(self.pending_edges());
                }
                self.expanding = None;
                continue;
            }
            let Some(edge) = self.frontier.peek() else {
                return Ok(if self.depth_deferred_count > 0 {
                    PathStop::DepthLimit
                } else {
                    PathStop::NoOpenWork
                });
            };
            if self.nodes.len() >= budget.max_nodes {
                return Ok(PathStop::NodeLimit);
            }
            if self.nodes[edge.parent].depth >= budget.max_depth {
                if self.order == PathOrder::PathProbability {
                    return Ok(PathStop::DepthLimit);
                }
                let edge = self.frontier.pop().unwrap();
                self.depth_deferred
                    .entry(self.nodes[edge.parent].depth)
                    .or_default()
                    .push(edge);
                self.depth_deferred_count += 1;
                continue;
            }
            let edge = self.frontier.pop().unwrap();
            if let Some(stop) = self.transition(edge, budget)? {
                return Ok(stop);
            }
        }
    }

    fn prepare(&mut self, index: usize) -> Result<(), String> {
        let node = &self.nodes[index];
        let choices = AdmittedChoices::new(&self.root, &node.state, &self.goal)?;
        self.stats.explicit_constraint_rejections += choices.rejected();
        self.stats.prepared_states += 1;
        let branches = choices.branches();
        let logs = if branches.is_empty() {
            Vec::new()
        } else {
            self.stats.prior_evaluations += 1;
            let logs = self.prior.log_probabilities(
                &node.state,
                PathContext {
                    root: &self.root,
                    goal: &self.goal,
                    spent_originals: node.spent_originals,
                },
                &branches,
            )?;
            prior::validate(&logs, branches.len())?;
            logs
        };
        self.nodes[index].expansion = Some(Expansion {
            choices,
            best_log: logs.iter().copied().reduce(f64::max).unwrap_or(0.0),
            logs,
            cursor: 0,
        });
        Ok(())
    }

    fn limits(b: &PathBudget) -> CombatStepLimits {
        CombatStepLimits {
            max_engine_steps: b.max_engine_steps_per_input,
            deadline: b.deadline,
        }
    }

    fn interrupted(&mut self, cause: TransitionInterruption, budget: &PathBudget) -> PathStop {
        match cause {
            TransitionInterruption::Deadline => PathStop::Deadline,
            TransitionInterruption::EngineStepLimit => {
                self.blocked_engine_limit = Some(budget.max_engine_steps_per_input);
                PathStop::EngineStepLimit
            }
        }
    }

    fn transition(&mut self, edge: Edge, budget: &PathBudget) -> Result<Option<PathStop>, String> {
        let parent = &self.nodes[edge.parent];
        let choices = &parent.expansion.as_ref().unwrap().choices;
        let input = choices.input(edge.choice)?;
        let branch = choices.branch(edge.choice).clone();
        self.stats.transition_attempts += 1;
        let (state, engine_steps) = match choices.transition(edge.choice, Self::limits(budget))? {
            Transition::Interrupted {
                engine_steps,
                cause,
            } => {
                self.stats.engine_steps += engine_steps;
                self.frontier.push(edge);
                return Ok(Some(self.interrupted(cause, budget)));
            }
            Transition::Settled {
                state,
                engine_steps,
            } => (state, engine_steps),
        };
        self.stats.engine_steps += engine_steps;
        self.stats.deepest_search_depth = self.stats.deepest_search_depth.max(parent.depth + 1);
        self.stats.deepest_combat_turn = self
            .stats
            .deepest_combat_turn
            .max(state.position().combat.turn.turn_count);
        let mut expenditures = parent.expenditures;
        if let Some(input) = &input {
            expenditures.record(input);
            self.stats.settled_inputs += 1;
        } else {
            self.stats.virtual_choices += 1;
        }
        let spent_originals = self
            .goal
            .spent_originals(&self.root, expenditures, state.position());
        if spent_originals & self.goal.preserve_starting_potion_slots != 0 {
            self.stats.successor_constraint_rejections += 1;
            return Ok(None);
        }
        let node = Node {
            state,
            parent: Some(edge.parent),
            step: Some(PathStep {
                branch,
                input,
                log_probability: edge.log_probability,
            }),
            depth: parent.depth + 1,
            log_probability: parent.log_probability + edge.log_probability,
            order_cost: edge.priority,
            expenditures,
            spent_originals,
            expansion: None,
        };
        let terminal = CombatSearchSpace::default().terminal(&node.state);
        if terminal == Some(false) {
            self.stats.losses_or_escapes += 1;
            return Ok(None);
        }
        let index = self.nodes.len();
        self.nodes.push(node);
        if terminal == Some(true) {
            self.stats.observed_wins += 1;
            let node = &self.nodes[index];
            let outcome = project_observed_combat_outcome_with_expenditures_v1(
                &self.root,
                node.expenditures,
                node.state.position(),
            );
            if !self.goal.accepts(&outcome) {
                self.stats.goal_misses += 1;
                return Ok(None);
            }
            if self
                .plans
                .iter()
                .any(|p| p.outcome.strictly_dominates(&outcome))
            {
                return Ok(None);
            }
            let inputs = self
                .steps(index)
                .into_iter()
                .filter_map(|s| s.input)
                .collect();
            self.replay = Some(Replay {
                node: index,
                inputs,
                current: self.root.clone(),
                index: 0,
            });
        } else {
            self.expanding = Some(index);
        }
        Ok(None)
    }

    fn steps(&self, mut node: usize) -> Vec<PathStep> {
        let mut steps = Vec::new();
        while let Some(parent) = self.nodes[node].parent {
            steps.push(self.nodes[node].step.clone().unwrap());
            node = parent;
        }
        steps.reverse();
        steps
    }

    fn replay_step(&mut self, budget: &PathBudget) -> Result<Option<PathStop>, String> {
        let replay = self.replay.as_mut().unwrap();
        let input = &replay.inputs[replay.index];
        if !EngineCombatStepper.is_legal_action(&replay.current, input) {
            return Err("path witness replay input is illegal".into());
        }
        self.stats.replay_attempts += 1;
        let step = EngineCombatStepper.apply_to_stable(
            &replay.current,
            input.clone(),
            Self::limits(budget),
        );
        self.stats.engine_steps += step.engine_steps;
        if step.timed_out || step.truncated {
            return Ok(Some(self.interrupted(
                if step.timed_out {
                    TransitionInterruption::Deadline
                } else {
                    TransitionInterruption::EngineStepLimit
                },
                budget,
            )));
        }
        self.stats.replayed_inputs += 1;
        replay.current = step.position;
        replay.index += 1;
        if replay.index == replay.inputs.len() {
            let replay = self.replay.take().unwrap();
            if replay.current != *self.nodes[replay.node].state.position() {
                return Err("path witness replay differs from searched successor".into());
            }
            let outcome = materialize_replay_checked_combat_outcome_v1(
                &self.root,
                &replay.inputs,
                &replay.current,
            )?;
            if !self.goal.accepts(&outcome) {
                return Err("path replay violates the fixed resource goal".into());
            }
            self.stats.verified_wins += 1;
            let work = self.stats.work();
            self.stats.first_verified_win_work.get_or_insert(work);
            if self.plans.iter().any(|p| {
                p.outcome.strictly_dominates(&outcome)
                    || (p.outcome.has_same_materialized_resources_v1(&outcome)
                        && p.inputs.len() <= replay.inputs.len())
            }) {
                return Ok(None);
            }
            self.plans.retain(|p| {
                !outcome.strictly_dominates(&p.outcome)
                    && !outcome.has_same_materialized_resources_v1(&p.outcome)
            });
            self.plans.push(PathPlan {
                steps: self.steps(replay.node),
                inputs: replay.inputs,
                outcome,
                successor: replay.current,
                found_at_work: self.stats.work(),
            });
        }
        Ok(None)
    }
}
