//! Experimental successor-state best-first search. Estimates order work only.
//! Rust owns transitions, query-relative terminal facts, coverage and replay.
mod data;
mod immediate;
mod lookahead;
#[cfg(test)]
mod tests;
pub use data::{route_neighborhood, DistanceData, DistanceRow, NodeOutcomeEvidence};
pub use immediate::ImmediateCheck;
use immediate::ImmediateProbe;
pub use lookahead::ShortCheck;
use lookahead::ShortLookahead;

use crate::entities::EntityPosition;
use crate::path_search::{admission::AdmittedChoices, linear::encode_entities, PathContext};
use crate::{CombatSearchSpace, CombatSearchState, ResourceRequirement, SearchSpace, Transition};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Instant;
use sts_combat_outcome::combat_witness_materialization::{
    materialize_replay_checked_combat_outcome_v1,
    project_observed_combat_outcome_with_expenditures_v1, CombatInputExpendituresV1,
    CombatWitnessMaterializedOutcomeV1,
};
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use sts_core::state::core::ClientInput;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

pub struct DistanceBudget {
    pub work: usize,
    pub max_nodes: usize,
    /// Search grammar depth, independent of the prediction range H.
    pub max_depth: usize,
    pub step: CombatStepLimits,
}

/// Exploration order only; neither variant ranks materialized resources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceOrdering {
    #[default]
    TotalInputs,
    RemainingInputs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DistanceStop {
    NeedsEvaluation,
    WorkBudget,
    Deadline,
    NodeLimit,
    DepthLimit,
    EngineStepLimit,
    NoOpenWork,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DistanceStats {
    pub prepared: usize,
    pub transitions: usize,
    pub real_inputs: usize,
    pub virtual_choices: usize,
    pub engine_steps: usize,
    pub replay_attempts: usize,
    pub replayed_inputs: usize,
    pub evaluated_states: usize,
    pub evaluation_batches: usize,
    pub verified_wins: usize,
    pub first_verified_work: Option<usize>,
    pub rejected_originals: usize,
    pub peak_frontier: usize,
    pub short_checks_started: usize,
    pub short_checks_completed: usize,
    pub short_transitions: usize,
    pub lower_bound_updates: usize,
    pub forced_prepared: usize,
    /// Subset of transitions, including interrupted attempts.
    pub forced_transitions: usize,
    pub immediate_checks_started: usize,
    pub immediate_checks_completed: usize,
    /// Subset of transitions; virtual selection and interrupted attempts count.
    pub immediate_transitions: usize,
    /// Subset of immediate_transitions past the first input, at unique choices.
    #[serde(skip_serializing_if = "is_zero")]
    pub probe_forced_transitions: usize,
    /// Prepared sources whose exact distance made a model estimate unnecessary.
    #[serde(skip_serializing_if = "is_zero")]
    pub exact_estimate_skips: usize,
}
fn is_zero(value: &usize) -> bool {
    *value == 0
}
impl DistanceStats {
    pub fn work(&self) -> usize {
        self.prepared + self.transitions + self.replay_attempts + self.evaluated_states
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DistancePlan {
    pub inputs: Vec<ClientInput>,
    pub successor: CombatPosition,
    pub outcome: CombatWitnessMaterializedOutcomeV1,
    pub found_at_work: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct EvaluationToken(pub u64, pub u64);

pub struct EvaluationBatch {
    pub token: EvaluationToken,
    pub nodes: Vec<usize>,
    pub positions: Vec<EntityPosition>,
}

struct Node {
    state: CombatSearchState,
    parent: Option<usize>,
    input: Option<ClientInput>,
    branch: Option<crate::CombatBranch>,
    depth: usize,
    g: usize,
    expenditures: CombatInputExpendituresV1,
    spent: u64,
    // None is unresolved. Some(false) is query-relative terminal failure.
    terminal: Option<bool>,
    choices: Option<AdmittedChoices>,
    features: Option<EntityPosition>,
    cursor: usize,
    children: Vec<usize>,
    witness_upper: Option<usize>,
    estimate: Option<f64>,
    lower: u32,
    /// All admitted edges can be known before their boundary states are opened.
    released: bool,
    probe_boundary: Option<Vec<usize>>,
    release_cursor: usize,
}

#[derive(Clone)]
struct Entry {
    node: usize,
    f: f64,
    tie_g: usize,
}
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.tie_g.cmp(&self.tie_g))
            .then_with(|| other.node.cmp(&self.node))
    }
}

struct Replay {
    node: usize,
    current: CombatPosition,
    inputs: Vec<ClientInput>,
    cursor: usize,
}

pub struct DistanceSearch {
    root: CombatPosition,
    goal: ResourceRequirement,
    horizon: u32,
    model_revision: String,
    ordering: DistanceOrdering,
    session: u64,
    generation: u64,
    issued: Option<EvaluationToken>,
    batch_size: usize,
    nodes: Vec<Node>,
    frontier: BinaryHeap<Entry>,
    deferred: BTreeMap<usize, Vec<Entry>>,
    active: Option<usize>,
    preparing: Option<usize>,
    pending: Vec<usize>,
    replay: Option<Replay>,
    blocked_engine: Option<usize>,
    failure: Option<String>,
    stats: DistanceStats,
    plans: Vec<DistancePlan>,
    lookahead_inputs: usize,
    lookahead: Option<ShortLookahead>,
    short_checks: Vec<ShortCheck>,
    advance_forced: bool,
    /// Shallowest-first order only serves immediately admitted single choices;
    /// depth-blocked sources remain here until the caller raises its limit.
    forced: BTreeSet<(usize, usize)>,
    probe_immediate: bool,
    probe_forced_boundaries: bool,
    estimate_probe_boundaries: bool,
    /// Opt-in: propagate factual distance lower bounds and let them cap the
    /// queue priority even without short checks or immediate probes. Exact
    /// sources skip the model estimate entirely.
    factual_bounds: bool,
    immediate: Option<ImmediateProbe>,
    immediate_checks: Vec<ImmediateCheck>,
}

impl DistanceSearch {
    pub fn new(
        root: CombatPosition,
        goal: ResourceRequirement,
        horizon: u32,
        model_revision: String,
        batch_size: usize,
    ) -> Result<Self, String> {
        goal.validate(&root)?;
        if horizon == 0
            || horizon == u32::MAX
            || model_revision.trim().is_empty()
            || batch_size == 0
        {
            return Err("positive finite range/batch and frozen model revision required".into());
        }
        let state = CombatSearchState::new(root.clone())?;
        if CombatSearchSpace::default().terminal(&state).is_some() {
            return Err("distance search requires an unresolved full combat root".into());
        }
        let node = Node {
            state,
            parent: None,
            input: None,
            branch: None,
            depth: 0,
            g: 0,
            expenditures: Default::default(),
            spent: 0,
            terminal: None,
            choices: None,
            features: None,
            cursor: 0,
            children: vec![],
            witness_upper: None,
            estimate: None,
            lower: 0,
            released: false,
            probe_boundary: None,
            release_cursor: 0,
        };
        let session = NEXT_SESSION
            .fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |n| {
                n.checked_add(1)
            })
            .map_err(|_| "distance session identity exhausted")?;
        Ok(Self {
            root,
            goal,
            horizon,
            model_revision,
            ordering: DistanceOrdering::default(),
            session,
            generation: 0,
            issued: None,
            batch_size,
            nodes: vec![node],
            frontier: BinaryHeap::new(),
            deferred: BTreeMap::new(),
            active: None,
            preparing: Some(0),
            pending: vec![],
            replay: None,
            blocked_engine: None,
            failure: None,
            stats: Default::default(),
            plans: vec![],
            lookahead_inputs: 0,
            lookahead: None,
            short_checks: vec![],
            advance_forced: false,
            forced: BTreeSet::new(),
            probe_immediate: false,
            probe_forced_boundaries: false,
            estimate_probe_boundaries: false,
            factual_bounds: false,
            immediate: None,
            immediate_checks: vec![],
        })
    }
    pub fn stats(&self) -> &DistanceStats {
        &self.stats
    }
    pub fn plans(&self) -> &[DistancePlan] {
        &self.plans
    }
    pub fn retained_nodes(&self) -> usize {
        self.nodes.len()
    }
    pub fn model_revision(&self) -> &str {
        &self.model_revision
    }

    pub fn with_ordering(mut self, ordering: DistanceOrdering) -> Result<Self, String> {
        if self.stats.work() != 0 {
            return Err("configure distance ordering before search starts".into());
        }
        self.ordering = ordering;
        Ok(self)
    }

    pub fn ordering(&self) -> DistanceOrdering {
        self.ordering
    }

    /// Propagate complete-successor distance lower bounds and cap queue
    /// priority by them without enabling short checks or immediate probes.
    /// Sources whose factual bounds already pin an exact distance skip the
    /// model estimate. This only changes ordering and estimation requests;
    /// it does not merge states, alter input distances, budgets, or replays.
    pub fn with_factual_bounds(mut self, enabled: bool) -> Result<Self, String> {
        if self.stats.work() != 0 {
            return Err("configure factual bounds before search starts".into());
        }
        self.factual_bounds = enabled;
        Ok(self)
    }

    /// Single admitted choices use real transitions without a model queue stop.
    /// This changes allocation of work, not state identity or input distance.
    pub fn with_forced_advance(mut self, enabled: bool) -> Result<Self, String> {
        if self.stats.work() != 0 {
            return Err("configure forced advancement before search starts".into());
        }
        if !enabled && self.probe_forced_boundaries {
            return Err("forced probe boundaries require forced advancement".into());
        }
        self.advance_forced = enabled;
        Ok(self)
    }

    pub fn forced_pending(&self) -> usize {
        self.forced.len()
    }

    fn forced_source(&self, max_depth: usize) -> Option<usize> {
        self.forced
            .iter()
            .take_while(|(depth, _)| *depth < max_depth)
            .find_map(|(_, node)| {
                // A short check cannot silently grow past its real-input range.
                // Boundary sources resume after that check has been recorded.
                self.lookahead
                    .as_ref()
                    .is_none_or(|check| self.nodes[*node].g < check.stop_g)
                    .then_some(*node)
            })
    }

    /// Locks this exact batch until accepted. Re-reading does not change it.
    pub fn evaluations(&mut self) -> Result<EvaluationBatch, String> {
        if self.pending.is_empty() {
            return Err("no pending distance evaluations".into());
        }
        let token = match self.issued {
            Some(token) => token,
            None => {
                self.generation = self
                    .generation
                    .checked_add(1)
                    .ok_or("evaluation token overflow")?;
                let token = EvaluationToken(self.session, self.generation);
                self.issued = Some(token);
                token
            }
        };
        Ok(EvaluationBatch {
            token,
            nodes: self.pending.clone(),
            positions: self
                .pending
                .iter()
                .map(|i| self.nodes[*i].features.as_ref().unwrap().clone())
                .collect(),
        })
    }

    /// True when propagated lower bounds meet a verified witness upper bound.
    fn exact_distance(&self, node: usize) -> bool {
        match self.nodes[node].witness_upper {
            Some(upper) => u64::from(self.required_lower(node)) == upper as u64,
            None => false,
        }
    }

    fn enqueue_with_estimate(&mut self, node: usize, estimate: f64) -> Result<(), String> {
        let f = self.priority(node, estimate);
        if !f.is_finite() {
            return Err("distance priority overflow".to_owned());
        }
        self.frontier.push(Entry {
            node,
            f,
            tie_g: match self.ordering {
                DistanceOrdering::TotalInputs => 0,
                DistanceOrdering::RemainingInputs => self.nodes[node].g,
            },
        });
        self.stats.peak_frontier = self.stats.peak_frontier.max(self.frontier.len());
        Ok(())
    }

    pub fn accept_estimates(
        &mut self,
        token: EvaluationToken,
        revision: &str,
        values: &[f64],
    ) -> Result<(), String> {
        if self.issued != Some(token)
            || revision != self.model_revision
            || values.len() != self.pending.len()
            || values
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0 || *v > f64::from(self.horizon + 1))
        {
            return Err("stale, foreign, changing or invalid distance estimates".into());
        }
        // Validate every entry before changing the frontier.
        let entries = self
            .pending
            .iter()
            .zip(values)
            .map(|(node, h)| {
                let f = self.priority(*node, *h);
                if !f.is_finite() {
                    return Err("distance priority overflow".to_owned());
                }
                Ok(Entry {
                    node: *node,
                    f,
                    tie_g: match self.ordering {
                        DistanceOrdering::TotalInputs => 0,
                        DistanceOrdering::RemainingInputs => self.nodes[*node].g,
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        for (node, value) in self.pending.iter().zip(values) {
            self.nodes[*node].estimate = Some(*value);
        }
        self.stats.evaluated_states += entries.len();
        self.stats.evaluation_batches += 1;
        self.frontier.extend(entries);
        self.stats.peak_frontier = self.stats.peak_frontier.max(self.frontier.len());
        self.pending.clear();
        self.issued = None;
        Ok(())
    }

    pub fn advance(&mut self, budget: DistanceBudget) -> Result<DistanceStop, String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if budget.step.max_engine_steps == 0 {
            return Err("positive engine limit required".into());
        }
        let result = self.advance_inner(&budget);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn advance_inner(&mut self, b: &DistanceBudget) -> Result<DistanceStop, String> {
        if self.issued.is_some() {
            return Ok(DistanceStop::NeedsEvaluation);
        }
        if self
            .blocked_engine
            .is_some_and(|old| b.step.max_engine_steps <= old)
        {
            return Ok(DistanceStop::EngineStepLimit);
        }
        self.blocked_engine = None;
        while self
            .deferred
            .first_key_value()
            .is_some_and(|(d, _)| *d < b.max_depth)
        {
            let (_, entries) = self.deferred.pop_first().unwrap();
            self.frontier.extend(entries);
        }
        let start = self.stats.work();
        loop {
            if b.step.deadline.is_some_and(|d| Instant::now() >= d) {
                return Ok(DistanceStop::Deadline);
            }
            if self.stats.work() - start >= b.work {
                return Ok(DistanceStop::WorkBudget);
            }
            if self.nodes.len() > b.max_nodes {
                return Ok(DistanceStop::NodeLimit);
            }
            if self.replay.is_some() {
                if let Some(stop) = self.replay_step(b.step)? {
                    return Ok(stop);
                }
                continue;
            }
            if let Some(index) = self.preparing.take() {
                if self.nodes[index].choices.is_none() {
                    let node = &self.nodes[index];
                    let choices = AdmittedChoices::new(&self.root, &node.state, &self.goal)?;
                    if choices.len() == 0 {
                        return Err("unresolved distance state has no admitted choices".into());
                    }
                    self.stats.prepared += 1;
                    self.stats.rejected_originals += choices.rejected();
                    let features = encode_entities(
                        &node.state,
                        PathContext {
                            root: &self.root,
                            goal: &self.goal,
                            spent_originals: node.spent,
                        },
                        &choices.branches(),
                    )?;
                    self.nodes[index].features = Some(features);
                    self.nodes[index].choices = Some(choices);
                }
                // A probe may already have classified a boundary. Reopening it
                // reuses that exact source and charges no second preparation.
                let forced =
                    self.advance_forced && self.nodes[index].choices.as_ref().unwrap().len() == 1;
                if self.immediate.is_some() {
                    // Assembly and opt-in forced tails belong to this probe;
                    // neither requests an intermediate model estimate.
                } else if forced {
                    self.forced.insert((self.nodes[index].depth, index));
                    self.stats.forced_prepared += 1;
                } else if self.probe_immediate {
                    self.start_immediate(index)?;
                } else if self.factual_bounds && self.exact_distance(index) {
                    // The graph already pins this source's distance; asking
                    // the model again would only risk overriding a fact.
                    let exact = self.nodes[index].witness_upper.unwrap() as f64;
                    self.nodes[index].estimate = Some(exact);
                    self.stats.exact_estimate_skips += 1;
                    self.enqueue_with_estimate(index, exact)?;
                } else {
                    self.pending.push(index);
                }
                continue;
            }
            if self.immediate.is_some() {
                if let Some(stop) = self.immediate_step(b)? {
                    return Ok(stop);
                }
                continue;
            }
            if let Some(index) = self.forced_source(b.max_depth) {
                if self.nodes.len() >= b.max_nodes {
                    return Ok(DistanceStop::NodeLimit);
                }
                self.stats.forced_transitions += 1;
                if let Some(stop) = self.transition(index, b.step)? {
                    return Ok(stop);
                }
                self.forced.remove(&(self.nodes[index].depth, index));
                self.nodes[index].released = true;
                self.propagate_lower(index);
                continue;
            }
            if !self.pending.is_empty()
                && (self.active.is_none() || self.pending.len() >= self.batch_size)
            {
                return Ok(DistanceStop::NeedsEvaluation);
            }
            if let Some(index) = self.active {
                if let Some(boundary) = &self.nodes[index].probe_boundary {
                    if let Some(child) = boundary.get(self.nodes[index].release_cursor).copied() {
                        self.nodes[index].release_cursor += 1;
                        self.preparing = Some(child);
                    } else {
                        self.nodes[index].released = true;
                        self.active = None;
                    }
                    continue;
                }
                if self.nodes[index].depth >= b.max_depth {
                    return Ok(DistanceStop::DepthLimit);
                }
                if self.nodes[index].cursor == self.nodes[index].choices.as_ref().unwrap().len() {
                    self.propagate_lower(index);
                    self.nodes[index].released = true;
                    self.active = None;
                    continue;
                }
                if self.nodes.len() >= b.max_nodes {
                    return Ok(DistanceStop::NodeLimit);
                }
                if let Some(stop) = self.transition(index, b.step)? {
                    return Ok(stop);
                }
                continue;
            }
            if let Some(index) = self.next_short_source() {
                self.active = Some(index);
                continue;
            }
            if self.forced_source(b.max_depth).is_some() {
                // Completing a short check may release its boundary sources.
                continue;
            }
            match self.frontier_entry() {
                Some(entry) if self.nodes[entry.node].depth >= b.max_depth => {
                    self.deferred
                        .entry(self.nodes[entry.node].depth)
                        .or_default()
                        .push(entry);
                }
                Some(entry) => {
                    if self.estimate_probe_boundaries {
                        // Admission only estimated this prepared boundary. Its
                        // outgoing check starts when it wins queue selection.
                        self.start_immediate(entry.node)?;
                    } else {
                        self.start_short_check(entry.node)?;
                        self.active = Some(entry.node);
                    }
                }
                None => {
                    return Ok(if self.deferred.is_empty() && self.forced.is_empty() {
                        DistanceStop::NoOpenWork
                    } else {
                        DistanceStop::DepthLimit
                    })
                }
            }
        }
    }

    fn transition(
        &mut self,
        index: usize,
        limits: CombatStepLimits,
    ) -> Result<Option<DistanceStop>, String> {
        let parent = &self.nodes[index];
        let choices = parent.choices.as_ref().unwrap();
        let input = choices.input(parent.cursor)?;
        self.stats.transitions += 1;
        if self.lookahead.is_some() {
            self.stats.short_transitions += 1;
        }
        if self.immediate.is_some() {
            self.stats.immediate_transitions += 1;
            if self.probe_forced_boundaries
                && self.nodes[index].g >= self.immediate.as_ref().unwrap().stop_g
            {
                self.stats.probe_forced_transitions += 1;
            }
        }
        let (state, steps) = match choices.transition(parent.cursor, limits)? {
            Transition::Settled {
                state,
                engine_steps,
            } => (state, engine_steps),
            Transition::Interrupted {
                engine_steps,
                cause,
            } => {
                self.stats.engine_steps += engine_steps;
                return Ok(Some(self.interrupted(cause, limits)));
            }
        };
        self.stats.engine_steps += steps;
        let mut expenditures = parent.expenditures;
        if let Some(input) = &input {
            expenditures.record(input);
            self.stats.real_inputs += 1;
        } else {
            self.stats.virtual_choices += 1;
        }
        let spent = self
            .goal
            .spent_originals(&self.root, expenditures, state.position());
        let mut terminal = CombatSearchSpace::default().terminal(&state);
        if spent & self.goal.preserve_starting_potion_slots != 0 {
            terminal = Some(false);
            self.stats.rejected_originals += 1;
        } else if terminal == Some(true) {
            terminal = Some(self.goal.accepts(
                &project_observed_combat_outcome_with_expenditures_v1(
                    &self.root,
                    expenditures,
                    state.position(),
                ),
            ));
        }
        let node = Node {
            state,
            parent: Some(index),
            branch: Some(choices.branch(parent.cursor).clone()),
            g: parent
                .g
                .checked_add(usize::from(input.is_some()))
                .ok_or("input distance overflow")?,
            input,
            depth: parent.depth.checked_add(1).ok_or("depth overflow")?,
            expenditures,
            spent,
            terminal,
            choices: None,
            features: None,
            cursor: 0,
            children: vec![],
            witness_upper: None,
            estimate: None,
            lower: match terminal {
                Some(false) => self.horizon + 1,
                Some(true) => 0,
                None if self.lookahead.is_some() || self.immediate.is_some() => 1,
                None => 0,
            },
            released: terminal.is_some(),
            probe_boundary: None,
            release_cursor: 0,
        };
        let child = self.nodes.len();
        self.nodes.push(node);
        self.nodes[index].children.push(child);
        self.nodes[index].cursor += 1;
        self.retain_short_child(child);
        if terminal == Some(true) {
            self.replay = Some(Replay {
                node: child,
                current: self.root.clone(),
                inputs: self.inputs(child),
                cursor: 0,
            });
        } else if terminal.is_none() {
            if self.immediate.is_some() {
                self.retain_immediate_child(child);
            } else {
                self.preparing = Some(child);
            }
        }
        Ok(None)
    }

    fn interrupted(
        &mut self,
        cause: crate::TransitionInterruption,
        limits: CombatStepLimits,
    ) -> DistanceStop {
        match cause {
            crate::TransitionInterruption::Deadline => DistanceStop::Deadline,
            crate::TransitionInterruption::EngineStepLimit => {
                self.blocked_engine = Some(limits.max_engine_steps);
                DistanceStop::EngineStepLimit
            }
        }
    }

    fn inputs(&self, mut node: usize) -> Vec<ClientInput> {
        let mut inputs = Vec::new();
        while let Some(parent) = self.nodes[node].parent {
            if let Some(input) = &self.nodes[node].input {
                inputs.push(input.clone());
            }
            node = parent;
        }
        inputs.reverse();
        inputs
    }

    fn replay_step(&mut self, limits: CombatStepLimits) -> Result<Option<DistanceStop>, String> {
        let replay = self.replay.as_mut().unwrap();
        let input = &replay.inputs[replay.cursor];
        if !EngineCombatStepper.is_legal_action(&replay.current, input) {
            return Err("illegal distance replay".into());
        }
        self.stats.replay_attempts += 1;
        let step = EngineCombatStepper.apply_to_stable(&replay.current, input.clone(), limits);
        self.stats.engine_steps += step.engine_steps;
        if step.timed_out || step.truncated {
            return Ok(Some(self.interrupted(
                if step.timed_out {
                    crate::TransitionInterruption::Deadline
                } else {
                    crate::TransitionInterruption::EngineStepLimit
                },
                limits,
            )));
        }
        self.stats.replayed_inputs += 1;
        replay.current = step.position;
        replay.cursor += 1;
        if replay.cursor != replay.inputs.len() {
            return Ok(None);
        }
        let replay = self.replay.take().unwrap();
        if replay.current != *self.nodes[replay.node].state.position() {
            return Err("distance replay successor mismatch".into());
        }
        let outcome = materialize_replay_checked_combat_outcome_v1(
            &self.root,
            &replay.inputs,
            &replay.current,
        )?;
        if !self.goal.accepts(&outcome) {
            return Err("distance replay violates resource query".into());
        }
        let mut node = replay.node;
        let final_g = self.nodes[node].g;
        loop {
            let remaining = final_g - self.nodes[node].g;
            self.nodes[node].witness_upper = Some(
                self.nodes[node]
                    .witness_upper
                    .map_or(remaining, |k| k.min(remaining)),
            );
            match self.nodes[node].parent {
                Some(parent) => node = parent,
                None => break,
            }
        }
        self.stats.verified_wins += 1;
        let work = self.stats.work();
        self.stats.first_verified_work.get_or_insert(work);
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
        self.plans.push(DistancePlan {
            inputs: replay.inputs,
            successor: replay.current,
            outcome,
            found_at_work: work,
        });
        Ok(None)
    }
}
