//! Receding finite-budget PUCT. Predictions replace unfinished continuations;
//! Completed simulations expose their priors, visits and backed-up values.
//! This actor does not wait for a whole-combat winning witness before acting.
use crate::combat::PreparedCombatChoices;
use crate::entities::{encode_choices, EntityPosition};
use crate::local::{ConstructedCombat, NumericBatch};
use crate::{CombatSearchSpace, CombatSearchState, SearchSpace, Transition};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use sts_core::sim::combat::CombatStepLimits;
use sts_core::state::core::ClientInput;
mod rollout;
#[cfg(test)]
mod tests;

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);
const PUCT_EXPLORATION: f64 = 1.5;

fn limits() -> CombatStepLimits {
    CombatStepLimits {
        max_engine_steps: 1024,
        deadline: None,
    }
}

/// Local combat utility: loss/escape=0; victory=(1+terminal HP/max HP)/2.
/// Potion allowance is an explicit task constraint, not an implicit HP price.
pub const OBJECTIVE: &str = "combat_win_plus_retained_hp_v1";
fn outcome_value(space: &CombatSearchSpace, state: &CombatSearchState) -> Option<f64> {
    space.terminal(state).map(|win| {
        let player = &state.position().combat.entities.player;
        if win {
            0.5 + 0.5 * (f64::from(player.current_hp) / f64::from(player.max_hp)).clamp(0., 1.)
        } else {
            0.
        }
    })
}

#[derive(Default)]
struct Edge {
    prior: f64,
    visits: u32,
    sum: f64,
    terminal_visits: u32,
    child: Option<usize>,
}
struct Node {
    state: CombatSearchState,
    choices: PreparedCombatChoices,
    edges: Vec<Edge>,
    prediction: Option<f64>,
    terminal: Option<f64>,
    visits: u32,
}
impl Node {
    fn new(space: &CombatSearchSpace, state: CombatSearchState) -> Result<Self, String> {
        let choices = space.prepare_choices(&state)?;
        let edges = choices.branches().iter().map(|_| Edge::default()).collect();
        let terminal = outcome_value(space, &state);
        if terminal.is_none() && choices.branches().is_empty() {
            return Err("nonterminal policy search has no legal branch".into());
        }
        Ok(Self {
            state,
            choices,
            edges,
            prediction: None,
            terminal,
            visits: 0,
        })
    }
    fn encode(&self, space: &CombatSearchSpace) -> Result<EntityPosition, String> {
        encode_choices(
            &self.state,
            &self.choices.branches().iter().collect::<Vec<_>>(),
            space.max_potion_uses(),
        )
    }
}

struct Tree {
    space: CombatSearchSpace,
    nodes: Vec<Node>,
    pending: Option<(usize, Vec<(usize, usize)>)>,
    completed: usize,
    transitions: usize,
}
impl Tree {
    fn new(space: CombatSearchSpace, state: CombatSearchState) -> Result<Self, String> {
        let root = Node::new(&space, state)?;
        Ok(Self {
            space,
            nodes: vec![root],
            pending: None,
            completed: 0,
            transitions: 0,
        })
    }
    fn backup(&mut self, leaf: usize, path: &[(usize, usize)], value: f64) {
        let terminal = self.nodes[leaf].terminal.is_some();
        self.nodes[leaf].visits += 1;
        for &(node, edge) in path {
            self.nodes[node].visits += 1;
            self.nodes[node].edges[edge].visits += 1;
            self.nodes[node].edges[edge].sum += value;
            self.nodes[node].edges[edge].terminal_visits += u32::from(terminal);
        }
        self.completed += 1;
    }
    fn pump(&mut self, simulations: usize) -> Result<(), String> {
        while self.pending.is_none()
            && self.completed < simulations
            && self.nodes[0].terminal.is_none()
        {
            let mut index = 0;
            let mut path = Vec::new();
            loop {
                let node = &self.nodes[index];
                if let Some(value) = node.terminal {
                    self.backup(index, &path, value);
                    break;
                }
                let Some(prediction) = node.prediction else {
                    self.pending = Some((index, path));
                    break;
                };
                let scale = (f64::from(node.visits) + 1.).sqrt();
                let selected = node
                    .edges
                    .iter()
                    .enumerate()
                    .max_by(|(a, x), (b, y)| {
                        let score = |e: &Edge| {
                            let q = if e.visits == 0 {
                                prediction
                            } else {
                                e.sum / f64::from(e.visits)
                            };
                            q + PUCT_EXPLORATION * e.prior * scale / (1. + f64::from(e.visits))
                        };
                        score(x).total_cmp(&score(y)).then_with(|| b.cmp(a))
                    })
                    .unwrap()
                    .0;
                let next = if let Some(child) = node.edges[selected].child {
                    child
                } else {
                    let state = match node.choices.transition(selected, limits())? {
                        Transition::Settled { state, .. } => state,
                        Transition::Interrupted { cause, .. } => {
                            return Err(format!("policy transition interrupted: {cause:?}"))
                        }
                    };
                    let child = self.nodes.len();
                    self.nodes.push(Node::new(&self.space, state)?);
                    self.nodes[index].edges[selected].child = Some(child);
                    self.transitions += 1;
                    child
                };
                path.push((index, selected));
                index = next;
            }
        }
        Ok(())
    }
    fn validate(&self, weights: &[f64], value: f64) -> Result<(), String> {
        let (index, _) = self
            .pending
            .as_ref()
            .ok_or("no pending policy evaluation")?;
        let sum: f64 = weights.iter().sum();
        if weights.len() != self.nodes[*index].edges.len()
            || weights.iter().any(|w| !w.is_finite() || *w < 0.)
            || !sum.is_finite()
            || sum <= 0.
            || !value.is_finite()
            || !(0.0..=1.0).contains(&value)
        {
            return Err("invalid policy/value prediction".into());
        }
        Ok(())
    }
    fn evaluate(&mut self, weights: &[f64], value: f64) {
        let (index, path) = self.pending.take().unwrap();
        let sum: f64 = weights.iter().sum();
        for (edge, weight) in self.nodes[index].edges.iter_mut().zip(weights) {
            edge.prior = *weight / sum;
        }
        self.nodes[index].prediction = Some(value);
        // The root prediction is an initialization, not an action visit.
        if !path.is_empty() {
            self.backup(index, &path, value);
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Outcome {
    pub slot: usize,
    pub kind: &'static str,
    pub value: Option<f64>,
    pub hp: i32,
    pub max_hp: i32,
    pub turns: u32,
    pub decisions: usize,
    pub potion_uses: usize,
    pub potions: Vec<Option<sts_core::content::potions::PotionId>>,
    pub search_transitions: usize,
}
struct Episode {
    tree: Tree,
    inputs: Vec<ClientInput>,
    decisions: usize,
    transitions: usize,
}

pub struct PolicySearchBatch {
    episodes: Vec<Episode>,
    specifications: Vec<ConstructedCombat>,
    simulations: usize,
    max_decisions: usize,
    revision: u64,
    source_revision: Option<u64>,
    leaf_sources: Vec<rollout::LeafSource>,
}
pub struct PolicyDecisions {
    pub frame: NumericBatch<EntityPosition>,
    pub visits: Vec<Vec<u32>>,
    pub priors: Vec<Vec<f64>>,
    /// Means of backed-up estimates/results, absent for unvisited actions.
    pub qvalues: Vec<Vec<Option<f64>>>,
    pub root_values: Vec<f64>,
    pub regularization: Vec<f64>,
    pub terminal_visits: Vec<Vec<u32>>,
    /// Exact outcomes only when the immediate successor is terminal.
    pub terminal_values: Vec<Vec<Option<f64>>>,
}
impl PolicySearchBatch {
    pub fn new(
        specifications: &[ConstructedCombat],
        simulations: usize,
        max_decisions: usize,
    ) -> Result<Self, String> {
        if specifications.is_empty()
            || specifications.len() > 128
            || !(1..=4096).contains(&simulations)
            || !(1..=1024).contains(&max_decisions)
        {
            return Err("invalid policy actor budget".into());
        }
        let episodes = specifications
            .iter()
            .map(|spec| {
                let space = spec.max_potion_uses.map_or_else(
                    CombatSearchSpace::default,
                    CombatSearchSpace::with_potion_limit,
                );
                Ok(Episode {
                    tree: Tree::new(space, CombatSearchState::new(spec.build()?)?)?,
                    inputs: vec![],
                    decisions: 0,
                    transitions: 0,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            episodes,
            specifications: specifications.to_vec(),
            simulations,
            max_decisions,
            revision: NEXT_REVISION.fetch_add(1, Ordering::Relaxed),
            source_revision: None,
            leaf_sources: vec![],
        })
    }
    fn active(&self, ep: &Episode) -> bool {
        ep.tree.nodes[0].terminal.is_none() && ep.decisions < self.max_decisions
    }
    pub fn evaluations(&mut self) -> Result<NumericBatch<EntityPosition>, String> {
        let mut frame = NumericBatch {
            revision: self.revision,
            slots: vec![],
            positions: vec![],
        };
        for (slot, ep) in self.episodes.iter_mut().enumerate() {
            if ep.decisions >= self.max_decisions {
                continue;
            }
            ep.tree.pump(self.simulations)?;
            if let Some((index, _)) = &ep.tree.pending {
                frame.slots.push(slot);
                frame
                    .positions
                    .push(ep.tree.nodes[*index].encode(&ep.tree.space)?);
            }
        }
        Ok(frame)
    }
    pub fn evaluate(
        &mut self,
        revision: u64,
        weights: &[Vec<f64>],
        values: &[f64],
    ) -> Result<(), String> {
        let slots: Vec<_> = self
            .episodes
            .iter()
            .enumerate()
            .filter(|(_, e)| e.tree.pending.is_some())
            .map(|(i, _)| i)
            .collect();
        if revision != self.revision
            || slots.len() != weights.len()
            || slots.len() != values.len()
            || slots.is_empty()
        {
            return Err("stale or malformed policy evaluation batch".into());
        }
        for ((slot, w), v) in slots.iter().zip(weights).zip(values) {
            self.episodes[*slot].tree.validate(w, *v)?;
        }
        for ((slot, w), v) in slots.iter().zip(weights).zip(values) {
            self.episodes[*slot].tree.evaluate(w, *v);
        }
        self.revision = NEXT_REVISION.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    pub fn decisions(&self) -> Result<PolicyDecisions, String> {
        let mut frame = NumericBatch {
            revision: self.revision,
            slots: vec![],
            positions: vec![],
        };
        let mut visits = vec![];
        let mut priors = vec![];
        let mut qvalues = vec![];
        let mut root_values = vec![];
        let mut regularization = vec![];
        let mut terminal_visits = vec![];
        let mut terminal_values = vec![];
        for (slot, ep) in self
            .episodes
            .iter()
            .enumerate()
            .filter(|(_, ep)| self.active(ep))
        {
            if ep.tree.pending.is_some() || ep.tree.completed < self.simulations {
                return Err("policy search still needs evaluation".into());
            }
            frame.slots.push(slot);
            frame
                .positions
                .push(ep.tree.nodes[0].encode(&ep.tree.space)?);
            visits.push(ep.tree.nodes[0].edges.iter().map(|e| e.visits).collect());
            let root = &ep.tree.nodes[0];
            priors.push(root.edges.iter().map(|e| e.prior).collect());
            qvalues.push(
                root.edges
                    .iter()
                    .map(|e| (e.visits > 0).then(|| e.sum / f64::from(e.visits)))
                    .collect(),
            );
            root_values.push(root.prediction.unwrap());
            // With smoothed visits (n_a + 1)/(N + A), PUCT's bonus is
            // lambda * prior / smoothed_visits. Export that same lambda.
            regularization.push(
                PUCT_EXPLORATION * (f64::from(root.visits) + 1.).sqrt()
                    / (f64::from(root.visits) + root.edges.len() as f64),
            );
            terminal_visits.push(root.edges.iter().map(|e| e.terminal_visits).collect());
            terminal_values.push(
                root.edges
                    .iter()
                    .map(|e| e.child.and_then(|child| ep.tree.nodes[child].terminal))
                    .collect(),
            );
        }
        Ok(PolicyDecisions {
            frame,
            visits,
            priors,
            qvalues,
            root_values,
            regularization,
            terminal_visits,
            terminal_values,
        })
    }
    pub fn advance(&mut self, revision: u64, ordinals: &[usize]) -> Result<(), String> {
        let slots: Vec<_> = self
            .episodes
            .iter()
            .enumerate()
            .filter(|(_, ep)| self.active(ep))
            .map(|(i, _)| i)
            .collect();
        if revision != self.revision || slots.len() != ordinals.len() || slots.is_empty() {
            return Err("stale policy decision batch".into());
        }
        for (&slot, &choice) in slots.iter().zip(ordinals) {
            let t = &self.episodes[slot].tree;
            if t.pending.is_some()
                || t.completed < self.simulations
                || !t.nodes[0].edges.get(choice).is_some_and(|e| e.visits > 0)
            {
                return Err("action outside completed root visits".into());
            }
        }
        // Prepare all replacements before committing any part of the batch.
        let replacements = slots
            .iter()
            .zip(ordinals)
            .map(|(&slot, &choice)| {
                let t = &self.episodes[slot].tree;
                let state = t.nodes[t.nodes[0].edges[choice].child.unwrap()]
                    .state
                    .clone();
                Ok((
                    Tree::new(t.space.clone(), state)?,
                    t.nodes[0].choices.input(choice)?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        for (slot, (tree, input)) in slots.into_iter().zip(replacements) {
            let ep = &mut self.episodes[slot];
            ep.transitions += ep.tree.transitions;
            ep.tree = tree;
            ep.decisions += 1;
            if let Some(input) = input {
                ep.inputs.push(input);
            }
        }
        self.revision = NEXT_REVISION.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    pub fn outcomes(&self) -> Vec<Outcome> {
        self.episodes
            .iter()
            .enumerate()
            .map(|(slot, ep)| {
                let state = &ep.tree.nodes[0].state;
                let c = &state.position().combat;
                let value = ep.tree.nodes[0].terminal;
                Outcome {
                    slot,
                    kind: match ep.tree.space.terminal(state) {
                        Some(true) => "win",
                        Some(false) => "loss",
                        None if ep.decisions >= self.max_decisions => "truncated",
                        None => "active",
                    },
                    value,
                    hp: c.entities.player.current_hp,
                    max_hp: c.entities.player.max_hp,
                    turns: c.turn.turn_count.saturating_add(1),
                    decisions: ep.decisions,
                    potion_uses: state.potion_uses(),
                    potions: c
                        .entities
                        .potions
                        .iter()
                        .map(|p| p.as_ref().map(|p| p.id))
                        .collect(),
                    search_transitions: ep.transitions + ep.tree.transitions,
                }
            })
            .collect()
    }
    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"objective": OBJECTIVE, "entity_schema": crate::entities::SCHEMA,
            "information": "oracle", "simulations": self.simulations,
            "source_revision": self.source_revision, "sources": self.leaf_sources,
            "specifications": self.specifications, "outcomes": self.outcomes(),
            "inputs": self.episodes.iter().map(|e| &e.inputs).collect::<Vec<_>>()})
    }
}

/// Fresh bounded demonstrations at an entry or a replayed student decision.
/// Prefix ordinals belong to this construction and the current choice grammar.
/// Their selected actions are imitation targets, never fabricated visit counts.
pub fn demonstrate(
    spec: &ConstructedCombat,
    work: usize,
    prefix_choices: &[usize],
) -> Result<Option<(Vec<EntityPosition>, Vec<usize>, f64)>, String> {
    use crate::solver::{CombatSolver, SolverConfig};
    if !(1..=100_000).contains(&work) || prefix_choices.len() > 1024 {
        return Err("invalid demonstration budget or prefix length".into());
    }
    let space = spec.max_potion_uses.map_or_else(
        CombatSearchSpace::default,
        CombatSearchSpace::with_potion_limit,
    );
    let mut state = CombatSearchState::new(spec.build()?)?;
    for &choice in prefix_choices {
        state = match space
            .prepare_choices(&state)?
            .transition(choice, limits())?
        {
            Transition::Settled { state, .. } => state,
            Transition::Interrupted { .. } => {
                return Err("student prefix replay interrupted".into())
            }
        };
    }
    // A PendingChoice is a real decision and is supported. An unfinished virtual
    // selection must not be discarded when restarting the whole-combat solver.
    if state.selection_family().is_some() {
        return Err("demonstration source has an unfinished virtual selection".into());
    }
    let remaining_potions = spec
        .max_potion_uses
        .map(|max| {
            max.checked_sub(state.potion_uses())
                .ok_or("prefix exceeds potion allowance")
        })
        .transpose()?;
    let mut solver = CombatSolver::new(
        state.position().clone(),
        SolverConfig {
            max_potion_uses: remaining_potions,
            ..Default::default()
        },
    )?;
    solver.advance(work, std::time::Duration::from_secs(15))?;
    let Some(plan) = solver
        .plans()
        .iter()
        .max_by_key(|p| p.successor.combat.entities.player.current_hp)
    else {
        return Ok(None);
    };
    let mut positions = vec![];
    let mut choices = vec![];
    for input in &plan.inputs {
        let path = space.prepare_choices(&state)?.input_path(input)?;
        for branch in path {
            let prepared = space.prepare_choices(&state)?;
            let selected = prepared
                .branches()
                .iter()
                .position(|b| b == &branch)
                .ok_or("demonstration branch absent")?;
            positions.push(encode_choices(
                &state,
                &prepared.branches().iter().collect::<Vec<_>>(),
                space.max_potion_uses(),
            )?);
            choices.push(selected);
            state = match prepared.transition(selected, limits())? {
                Transition::Settled { state, .. } => state,
                Transition::Interrupted { .. } => {
                    return Err("demonstration replay interrupted".into())
                }
            };
        }
    }
    let value = outcome_value(&space, &state)
        .filter(|v| *v > 0.)
        .ok_or("demonstration did not win")?;
    Ok(Some((positions, choices, value)))
}
