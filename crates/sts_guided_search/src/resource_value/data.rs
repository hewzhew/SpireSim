//! Offline transition collection from fresh solver trajectories and actual responses.
use super::*;
use crate::{solver::CombatSolver, CombatSearchSpace, SearchSpace, Transition};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use sts_core::{
    sim::combat::CombatStepLimits,
    state::core::{ClientInput, EngineState},
};

#[derive(Serialize)]
pub struct Row {
    pub origin: Option<(usize, crate::CombatBranch)>,
    pub features: Vec<f32>,
    pub terminal: Option<f32>,
    /// Best actually completed continuation in this collection, not V*.
    pub observed_return: Option<f32>,
    /// Some means every admitted one-step branch was actually executed.
    pub children: Option<Vec<usize>>,
}
#[derive(Serialize)]
pub struct Trace {
    pub inputs: Vec<ClientInput>,
    pub rows: Vec<usize>,
    pub terminal: Option<f32>,
}
#[derive(Serialize)]
pub struct Dataset {
    pub schema: u32,
    pub objective: &'static str,
    pub feature_names: Vec<String>,
    pub rows: Vec<Row>,
    pub traces: Vec<Trace>,
    pub transitions: usize,
    pub replay_inputs: usize,
    pub continuation_work: usize,
    pub continuation_sources: usize,
    pub response_work: usize,
    pub responses: Vec<Response>,
}

#[derive(Serialize)]
pub struct Response {
    pub source: usize,
    /// False when even this source's action set did not fit the shared budget.
    pub source_expanded: bool,
    pub alternatives: Vec<ResponseAlternative>,
}

#[derive(Serialize)]
pub struct ResponseAlternative {
    pub first: usize,
    /// Every reachable decision before the response boundary was expanded.
    /// This is graph coverage, not a guarantee that all policies terminate.
    pub complete: bool,
    pub unresolved: usize,
    pub boundaries: usize,
    /// Independently replayed from the original collection root; see traces.
    pub best_boundary: Option<usize>,
    /// Trace follows this source and first branch, even if the endpoint was
    /// already known through a different trajectory.
    pub witness_trace: Option<usize>,
}

// A cheap bucket only. Full state (including order, RNG and selection) decides equality.
type Bucket = (u32, u8, i32, i32, usize, usize);
struct Collector {
    states: Vec<CombatSearchState>,
    index: HashMap<Bucket, Vec<usize>>,
    data: Dataset,
    space: CombatSearchSpace,
}
impl Collector {
    fn insert(
        &mut self,
        state: CombatSearchState,
        origin: Option<(usize, crate::CombatBranch)>,
    ) -> usize {
        let combat = &state.position().combat;
        let key = (
            combat.turn.turn_count,
            combat.turn.energy,
            combat.entities.player.current_hp,
            combat.entities.player.block,
            combat.zones.hand.len(),
            combat.zones.draw_pile.len(),
        );
        if let Some(i) = self
            .index
            .get(&key)
            .and_then(|bucket| bucket.iter().copied().find(|&i| self.states[i] == state))
        {
            return i;
        }
        let terminal = self.space.terminal(&state).map(|win| {
            if win {
                state.position().combat.entities.player.current_hp as f32 / 75.
            } else {
                -1.
            }
        });
        let i = self.states.len();
        self.index.entry(key).or_default().push(i);
        self.data.rows.push(Row {
            origin,
            features: features(&state),
            terminal,
            observed_return: terminal,
            children: None,
        });
        self.states.push(state);
        i
    }
    fn step(
        &mut self,
        state: &CombatSearchState,
        branch: &crate::CombatBranch,
    ) -> Result<CombatSearchState, String> {
        self.data.transitions += 1;
        match self.space.transition(
            state,
            branch,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        )? {
            Transition::Settled { state, .. } => Ok(state),
            Transition::Interrupted { .. } => {
                Err("value data encountered interrupted transition".into())
            }
        }
    }
    fn trace(&mut self, root: &CombatSearchState, inputs: &[ClientInput]) -> Result<(), String> {
        let mut state = root.clone();
        let mut rows = vec![self
            .states
            .iter()
            .position(|s| s == root)
            .ok_or("missing trace source")?];
        for input in inputs {
            let path = self.space.prepare_choices(&state)?.input_path(input)?;
            for branch in path {
                state = self.step(&state, &branch)?;
                rows.push(self.insert(state.clone(), Some((*rows.last().unwrap(), branch))));
            }
        }
        // Independent materialized-input replay, including losing routes.
        let final_replay = crate::shuffle_order::replay(root.position(), inputs)?
            .pop()
            .unwrap();
        self.data.replay_inputs += inputs.len();
        if state.position() != &final_replay {
            return Err("value trace replay mismatch".into());
        }
        let terminal = self.data.rows[*rows.last().unwrap()].terminal;
        if let Some(value) = terminal {
            for &i in &rows {
                let old = self.data.rows[i].observed_return;
                self.data.rows[i].observed_return = Some(old.map_or(value, |v| v.max(value)));
            }
        }
        self.data.traces.push(Trace {
            inputs: inputs.to_vec(),
            rows,
            terminal,
        });
        Ok(())
    }

    fn expand(&mut self, i: usize, remaining: usize) -> Result<bool, String> {
        if self.data.rows[i].children.is_some() {
            return Ok(true);
        }
        let state = self.states[i].clone();
        let prepared = self.space.prepare_choices(&state)?;
        if prepared.branches().len() > remaining {
            return Ok(false);
        }
        let mut children = vec![];
        for branch in prepared.branches() {
            let next = self.step(&state, branch)?;
            children.push(self.insert(next, Some((i, branch.clone()))));
        }
        if children.is_empty() {
            return Err("nonterminal value source has no branches".into());
        }
        self.data.rows[i].children = Some(children);
        Ok(true)
    }

    fn boundary(&self, i: usize, turn: u32) -> bool {
        self.data.rows[i].terminal.is_some()
            || (self.states[i].position().combat.turn.turn_count > turn
                && self.states[i].selected_members().is_none()
                && matches!(
                    self.states[i].position().engine,
                    EngineState::CombatPlayerTurn
                ))
    }

    /// Include recorded turn entries as well as observed one-step mode forks.
    /// Entries from continuation traces cover setup decisions before any card
    /// can change the mode. No hand, RNG state or favorable outcome is invented.
    fn response_sources(&self) -> Vec<usize> {
        self.data
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                if !matches!(
                    self.states[i].position().engine,
                    EngineState::CombatPlayerTurn
                ) || self.states[i].selected_members().is_some()
                {
                    return None;
                }
                let turn = self.states[i].position().combat.turn.turn_count;
                let mut cursor = i;
                let mut entry = true;
                while let Some((parent, _)) = &self.data.rows[cursor].origin {
                    let state = &self.states[*parent];
                    if state.position().combat.turn.turn_count < turn {
                        break;
                    }
                    if state.selected_members().is_none()
                        && matches!(state.position().engine, EngineState::CombatPlayerTurn)
                    {
                        entry = false;
                        break;
                    }
                    cursor = *parent;
                }
                if entry {
                    return Some(i);
                }
                let phases: HashSet<_> = row
                    .children
                    .as_ref()?
                    .iter()
                    .filter_map(|&j| {
                        let c = &self.states[j].position().combat;
                        if c.turn.turn_count != turn || self.data.rows[j].terminal.is_some() {
                            return None;
                        }
                        let g = &c.entities.monsters[0].guardian;
                        Some((g.is_open, g.close_up_triggered))
                    })
                    .collect();
                (phases.len() > 1).then_some(i)
            })
            .collect()
    }

    fn verify_response(&mut self, source: usize, suffix: &[usize]) -> Result<usize, String> {
        let mut row = source;
        let mut path = vec![row];
        let mut inputs = vec![];
        while let Some((parent, branch)) = &self.data.rows[row].origin {
            if let Some(input) = self.space.input(&self.states[*parent], branch)? {
                inputs.push(input);
            }
            row = *parent;
            path.push(row);
        }
        if row != 0 {
            return Err("response witness disconnected from root".into());
        }
        inputs.reverse();
        path.reverse();
        row = source;
        for &next in suffix {
            let index = self.data.rows[row]
                .children
                .as_ref()
                .and_then(|children| children.iter().position(|&i| i == next))
                .ok_or("missing response witness edge")?;
            if let Some(input) = self
                .space
                .prepare_choices(&self.states[row])?
                .input(index)?
            {
                inputs.push(input);
            }
            path.push(next);
            row = next;
        }
        let final_replay = crate::shuffle_order::replay(self.states[0].position(), &inputs)?
            .pop()
            .ok_or("empty response witness")?;
        self.data.replay_inputs += inputs.len();
        if &final_replay != self.states[row].position() {
            return Err("response witness replay mismatch".into());
        }
        self.data.traces.push(Trace {
            inputs,
            rows: path,
            terminal: self.data.rows[row].terminal,
        });
        Ok(self.data.traces.len() - 1)
    }

    fn responses(&mut self, sources: &[usize], work: usize) -> Result<(), String> {
        // Finish a local response graph before widening to another source.
        // A shared BFS can spend the entire budget on shallow, unlabelled rows.
        // Permute row IDs independently of outcomes so early saved wins do not
        // always monopolize collection. This does not touch the game's RNG.
        let mut order = sources.to_vec();
        order.sort_by_cached_key(|&i| {
            let key: [u8; 32] = Sha256::digest((i as u64).to_le_bytes()).into();
            (key, i)
        });
        let mut seen = HashSet::new();
        let start = self.data.transitions;
        for source in order {
            let turn = self.states[source].position().combat.turn.turn_count;
            let mut stack = vec![source];
            while let Some(i) = stack.pop() {
                if !seen.insert((i, turn)) || self.boundary(i, turn) {
                    continue;
                }
                if self.expand(i, work.saturating_sub(self.data.transitions - start))? {
                    stack.extend(self.data.rows[i].children.as_ref().unwrap().iter().rev());
                }
            }
        }
        self.data.response_work = self.data.transitions - start;
        for &source in sources {
            let turn = self.states[source].position().combat.turn.turn_count;
            let mut alternatives = vec![];
            for first in self.data.rows[source].children.clone().unwrap_or_default() {
                let mut seen = HashSet::new();
                let mut parents = HashMap::from([(first, source)]);
                let mut pending = vec![first];
                let mut boundaries = vec![];
                let mut unresolved = 0;
                while let Some(i) = pending.pop() {
                    if !seen.insert(i) {
                        continue;
                    }
                    if self.boundary(i, turn) {
                        boundaries.push(i);
                    } else if let Some(children) = &self.data.rows[i].children {
                        for &j in children {
                            if let std::collections::hash_map::Entry::Vacant(entry) =
                                parents.entry(j)
                            {
                                entry.insert(i);
                                pending.push(j);
                            }
                        }
                    } else {
                        unresolved += 1;
                    }
                }
                let best = boundaries.iter().copied().max_by(|&a, &b| {
                    let ceiling = |i: usize| {
                        self.data.rows[i]
                            .terminal
                            .unwrap_or(self.data.rows[i].features[7])
                    };
                    ceiling(a).total_cmp(&ceiling(b)).then_with(|| b.cmp(&a))
                });
                let witness_trace = if let Some(mut i) = best {
                    let mut suffix = vec![];
                    while i != first {
                        suffix.push(i);
                        i = parents[&i];
                    }
                    suffix.push(first);
                    suffix.reverse();
                    Some(self.verify_response(source, &suffix)?)
                } else {
                    None
                };
                alternatives.push(ResponseAlternative {
                    first,
                    complete: unresolved == 0,
                    unresolved,
                    boundaries: boundaries.len(),
                    best_boundary: best,
                    witness_trace,
                });
            }
            self.data.responses.push(Response {
                source,
                source_expanded: self.data.rows[source].children.is_some(),
                alternatives,
            });
        }
        Ok(())
    }
}

/// Search preparation and this collection have separate, reported costs. Rows
/// share an entire source family and must never be split randomly across folds.
pub fn collect(
    solver: &CombatSolver,
    branch_work: usize,
    continuation_work: usize,
    response_work: usize,
) -> Result<Dataset, String> {
    validate_position(solver.root())?;
    let root = CombatSearchState::new(solver.root().clone())?;
    let mut c = Collector {
        states: vec![],
        index: HashMap::new(),
        space: CombatSearchSpace::with_potion_limit(0),
        data: Dataset {
            schema: SCHEMA,
            objective: OBJECTIVE,
            feature_names: feature_names(&root),
            rows: vec![],
            traces: vec![],
            transitions: 0,
            replay_inputs: 0,
            continuation_work: 0,
            continuation_sources: 0,
            response_work: 0,
            responses: vec![],
        },
    };
    c.insert(root.clone(), None);
    for plan in solver.plans() {
        c.trace(&root, &plan.inputs)?;
    }
    if let Some(loss) = solver.loss_sample() {
        c.trace(&root, &loss.inputs)?;
    }
    for prefix in solver.sample_prefixes(12) {
        c.trace(&root, &prefix.inputs)?;
    }
    // A deliberately passive control is real executed defeat data, not a label
    // saying the root is hopeless. It also covers the mechanism's full cycle.
    let mut state = root.clone();
    let mut inputs = vec![];
    for _ in 0..256 {
        if c.space.terminal(&state).is_some() {
            break;
        }
        let prepared = c.space.prepare_choices(&state)?;
        let index = prepared
            .branches()
            .iter()
            .position(|b| matches!(b, crate::CombatBranch::Input(ClientInput::EndTurn)))
            .unwrap_or(0);
        if let Some(input) = prepared.input(index)? {
            inputs.push(input);
        }
        state = c.step(&state, &prepared.branches()[index])?;
    }
    c.trace(&root, &inputs)?;
    // Expand the captured sources, from the end backwards so terminal-adjacent
    // sources receive attention first. New children do not recursively expand.
    let sources = c.states.len();
    let start = c.data.transitions;
    for i in (0..sources).rev() {
        if c.data.rows[i].terminal.is_some() {
            continue;
        }
        c.expand(i, branch_work.saturating_sub(c.data.transitions - start))?;
    }
    // A fixed, outcome-blind sample of actual source/alternative states. This
    // labels more than one already successful path; each continuation is still
    // an empirical bounded solver result, not an optimal state value.
    if continuation_work > 0 {
        let eligible: Vec<_> = (0..c.states.len())
            .filter(|&i| {
                c.data.rows[i].terminal.is_none() && c.states[i].selected_members().is_none()
            })
            .collect();
        let count = eligible.len().min(64);
        for k in 0..count {
            let index = eligible[k * eligible.len() / count];
            let state = c.states[index].clone();
            let mut continuation = CombatSolver::new(
                state.position().clone(),
                crate::solver::SolverConfig {
                    max_width: 16,
                    max_potion_uses: Some(0),
                    ..Default::default()
                },
            )?;
            continuation.advance(continuation_work, std::time::Duration::from_secs(10))?;
            c.data.continuation_work += continuation.stats().work();
            c.data.continuation_sources += 1;
            for plan in continuation.plans() {
                c.trace(&state, &plan.inputs)?;
            }
            if let Some(loss) = continuation.loss_sample() {
                c.trace(&state, &loss.inputs)?;
            }
        }
    }
    if response_work > 0 {
        // Continuations contribute genuine new turn entries; selecting sources
        // earlier would systematically omit those already observed decisions.
        let response_sources = c.response_sources();
        c.responses(&response_sources, response_work)?;
    }
    Ok(c.data)
}
