//! Root-scoped executed edges, observed turn boundaries and verified returns.
//! Eviction forgets guidance; it never establishes a negative result.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct Key {
    state: CombatSearchState,
    expenditures: CombatInputExpendituresV1,
    hash: u64,
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
            && self.expenditures == other.expenditures
            && (self.state.shares_exact_state(&other.state) || self.state == other.state)
    }
}

impl Key {
    pub(super) fn new(state: CombatSearchState, expenditures: CombatInputExpendituresV1) -> Self {
        let hash = state_index_key(&state);
        Self {
            state,
            expenditures,
            hash,
        }
    }

    #[cfg(test)]
    fn node(node: &Node) -> Self {
        Self::new(node.state.clone(), node.expenditures())
    }
}

struct Edge {
    source: Key,
    branch: CombatBranch,
    destination: Key,
    propagated: Vec<Return>,
    boundaries: Vec<Boundary>,
}

/// An observed reachable boundary, not a terminal resource return or a bound
/// on unsearched alternatives. Every lane keeps a complete, real endpoint.
#[derive(Clone)]
struct Boundary {
    endpoint: Key,
    trace: Option<Arc<Trace>>,
    input_count: usize,
    ranks: [(i64, i64, i64); retention::SCORE_LANES],
    total_potion_expenditures: usize,
}

impl Boundary {
    fn turn(&self) -> u32 {
        self.endpoint.state.position().combat.turn.turn_count
    }

    fn permitted(&self, source: &Key, card_only: bool, max_potions: Option<usize>) -> bool {
        (!card_only || self.endpoint.expenditures == source.expenditures)
            && max_potions.is_none_or(|max| self.total_potion_expenditures <= max)
    }

    fn covers(&self, other: &Self) -> bool {
        self.endpoint.expenditures.explicit_count() <= other.endpoint.expenditures.explicit_count()
            && self.total_potion_expenditures <= other.total_potion_expenditures
            && self.ranks.iter().zip(other.ranks).all(|(a, b)| *a >= b)
    }
}

fn boundary_front_covers(a: &[&Boundary], b: &[&Boundary]) -> bool {
    b.iter().all(|b| a.iter().any(|a| a.covers(b)))
}

#[derive(Clone)]
struct Return {
    outcome: Arc<CombatWitnessMaterializedOutcomeV1>,
    final_expenditures: CombatInputExpendituresV1,
}

impl Return {
    fn covers(&self, other: &Self) -> bool {
        // A suffix that uses a potion cannot replace a no-more-potion suffix,
        // even when passive expenditures make their final resources equal.
        self.final_expenditures.explicit_count() <= other.final_expenditures.explicit_count()
            && (self.outcome.strictly_dominates(&other.outcome)
                || self
                    .outcome
                    .has_same_materialized_resources_v1(&other.outcome))
    }

    fn permitted(&self, source: &Key, card_only: bool, max_potions: Option<usize>) -> bool {
        (!card_only || self.final_expenditures == source.expenditures)
            && max_potions.is_none_or(|max| self.outcome.total_potion_expenditures as usize <= max)
    }
}

struct Record {
    key: Key,
    returns: Vec<Return>,
    boundaries: Vec<Boundary>,
}

enum Backup {
    Seed(Key, Return),
    Edge(u64, Return),
    BoundarySeed(Key, Boundary),
    BoundaryEdge(u64, u32),
}

pub(super) struct Graph {
    capacity: usize,
    next_id: u64,
    edges: HashMap<u64, Edge>,
    edge_order: VecDeque<u64>,
    outgoing: HashMap<u64, Vec<u64>>,
    incoming: HashMap<u64, Vec<u64>>,
    records: HashMap<u64, Record>,
    record_order: VecDeque<u64>,
    record_index: HashMap<u64, Vec<u64>>,
    pending: VecDeque<Backup>,
    pending_boundaries: std::collections::HashSet<(u64, u32)>,
    revision: u64,
}

#[derive(Clone, Copy)]
pub(super) struct RetentionGuide<'a> {
    graph: &'a Graph,
    card_only: bool,
    max_potions: Option<usize>,
}

impl RetentionGuide<'_> {
    pub(super) fn observed_boundaries(&self, node: &Node) -> Vec<serde_json::Value> {
        let key = Key::new(node.state.clone(), node.expenditures());
        let turn = node.state.position().combat.turn.turn_count;
        self.graph
            .boundaries(&key)
            .iter()
            .filter(|b| b.turn() > turn && b.permitted(&key, self.card_only, self.max_potions))
            .map(|b| {
                let witness = Node {
                    state: b.endpoint.state.clone(),
                    card_baseline: node.card_baseline.clone(),
                    potion_budget: node.potion_budget.clone(),
                    trace: b.trace.clone(),
                    input_count: b.input_count,
                };
                serde_json::json!({"target_turn":b.turn(), "ranks":b.ranks,
                    "realized_hp":b.endpoint.state.position().combat.entities.player.current_hp,
                    "total_potion_expenditures":b.total_potion_expenditures,
                    "inputs":witness.inputs(), "successor":b.endpoint.state.position()})
            })
            .collect()
    }

    /// Replace relative ranks only where candidates have observed results at
    /// the same future turn. Unobserved candidates keep their existing slots;
    /// a deeper observation cannot compete numerically with a shorter horizon.
    /// These are witnessed best-so-far estimates, never dominance pruning.
    pub(super) fn order_by_boundaries(
        &self,
        nodes: &[Node],
        index_keys: &[u64],
        order: &mut [usize],
        stats: &mut SolverStats,
    ) {
        if self.graph.records.is_empty() {
            return;
        }
        let source_turn = nodes[order[0]].state.position().combat.turn.turn_count;
        debug_assert!(nodes
            .iter()
            .all(|n| n.state.position().combat.turn.turn_count == source_turn));
        for horizon in 1..=Graph::BOUNDARY_HORIZONS {
            let Some(turn) = source_turn.checked_add(horizon) else {
                continue;
            };
            let mut slots = vec![];
            let mut known = vec![];
            for (slot, &i) in order.iter().enumerate() {
                let key = Key {
                    state: nodes[i].state.clone(),
                    expenditures: nodes[i].expenditures(),
                    hash: index_keys[i],
                };
                let front = self
                    .graph
                    .boundaries(&key)
                    .iter()
                    .filter(|b| {
                        b.turn() == turn && b.permitted(&key, self.card_only, self.max_potions)
                    })
                    .collect::<Vec<_>>();
                if !front.is_empty() {
                    slots.push(slot);
                    known.push((i, front));
                }
            }
            if known.len() < 2 {
                continue;
            }
            // A single-lane improvement can hide a real tactical tradeoff.
            // Only whole observed fronts covering every continuation objective
            // alter priorities. Each coverage requires one complete endpoint;
            // no best-HP/best-damage synthetic endpoint is assembled.
            let mut ranked = Vec::with_capacity(known.len());
            let mut remaining = (0..known.len()).collect::<Vec<_>>();
            while !remaining.is_empty() {
                let layer = remaining
                    .iter()
                    .copied()
                    .filter(|&i| {
                        !remaining.iter().any(|&j| {
                            i != j
                                && boundary_front_covers(&known[j].1, &known[i].1)
                                && !boundary_front_covers(&known[i].1, &known[j].1)
                        })
                    })
                    .collect::<Vec<_>>();
                debug_assert!(!layer.is_empty());
                ranked.extend(layer.iter().map(|&i| known[i].0));
                remaining.retain(|i| !layer.contains(i));
            }
            if slots
                .iter()
                .zip(&ranked)
                .any(|(&slot, &i)| order[slot] != i)
            {
                stats.boundary_retention_reorders += 1;
            }
            stats.boundary_retention_comparisons += known.len();
            for (slot, i) in slots.into_iter().zip(ranked) {
                order[slot] = i;
            }
        }
    }

    pub(super) fn outcomes(&self, node: &Node) -> Vec<CombatWitnessMaterializedOutcomeV1> {
        let key = Key::new(node.state.clone(), node.expenditures());
        self.graph
            .returns(&key)
            .iter()
            .filter(|r| r.permitted(&key, self.card_only, self.max_potions))
            .map(|r| (*r.outcome).clone())
            .collect()
    }

    /// Nominate one representative for each non-dominated feasible return front.
    /// Redundant evidence cannot claim repeated slots in this lane. Every exact
    /// candidate still competes in the original lanes, including suppressed
    /// nominees: coverage of known returns is not dominance of future potential.
    pub(super) fn nominations(
        &self,
        nodes: &[Node],
        index_keys: &[u64],
        stats: &mut SolverStats,
    ) -> Vec<usize> {
        if self.graph.records.is_empty() {
            return vec![];
        }
        debug_assert_eq!(nodes.len(), index_keys.len());
        let mut known = Vec::new();
        for (i, (node, &hash)) in nodes.iter().zip(index_keys).enumerate() {
            if !self.graph.record_index.contains_key(&hash) {
                continue;
            }
            let key = Key {
                state: node.state.clone(),
                expenditures: node.expenditures(),
                hash,
            };
            let returns = self
                .graph
                .returns(&key)
                .iter()
                .filter(|r| r.permitted(&key, self.card_only, self.max_potions))
                .collect::<Vec<_>>();
            if !returns.is_empty() {
                known.push((i, returns));
            }
        }
        if known.is_empty() {
            return vec![];
        }
        stats.backed_retention_batches += 1;
        stats.backed_retention_candidates += known.len();
        let mut nominees: Vec<usize> = Vec::new();
        for i in pareto_front(&known) {
            if nominees.iter().all(|&j| {
                !front_covers(&known[j].1, &known[i].1) || !front_covers(&known[i].1, &known[j].1)
            }) {
                nominees.push(i);
            }
        }
        stats.backed_retention_nominations += nominees.len();
        stats.backed_retention_suppressed_nominations += known.len() - nominees.len();
        nominees.into_iter().map(|i| known[i].0).collect()
    }
}

fn front_covers(a: &[&Return], b: &[&Return]) -> bool {
    b.iter().all(|b| a.iter().any(|a| a.covers(b)))
}

fn pareto_front(known: &[(usize, Vec<&Return>)]) -> Vec<usize> {
    (0..known.len())
        .filter(|&i| {
            !(0..known.len()).any(|j| {
                i != j
                    && front_covers(&known[j].1, &known[i].1)
                    && !front_covers(&known[i].1, &known[j].1)
            })
        })
        .collect()
}

fn pareto_order(mut known: Vec<(usize, Vec<&Return>)>) -> Vec<usize> {
    let mut ordered = Vec::with_capacity(known.len());
    while !known.is_empty() {
        let layer = pareto_front(&known);
        debug_assert!(!layer.is_empty());
        ordered.extend(layer.iter().map(|&i| known[i].0));
        for &i in layer.iter().rev() {
            known.remove(i);
        }
    }
    ordered
}

impl Graph {
    const RETURNS_PER_KEY: usize = 8;
    const BOUNDARY_HORIZONS: u32 = 2;

    pub(super) fn new(max_width: usize) -> Self {
        Self::with_capacity(max_width.saturating_mul(256).clamp(256, 32_768))
    }

    pub(super) fn retention_guide(
        &self,
        card_only: bool,
        max_potions: Option<usize>,
    ) -> RetentionGuide<'_> {
        RetentionGuide {
            graph: self,
            card_only,
            max_potions,
        }
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            next_id: 0,
            edges: HashMap::new(),
            edge_order: VecDeque::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            records: HashMap::new(),
            record_order: VecDeque::new(),
            record_index: HashMap::new(),
            pending: VecDeque::new(),
            pending_boundaries: std::collections::HashSet::new(),
            revision: 1,
        }
    }

    fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("search graph ID overflow");
        id
    }

    fn edge(&self, source: &Key, branch: &CombatBranch) -> Option<&Edge> {
        self.outgoing.get(&source.hash)?.iter().find_map(|id| {
            self.edges
                .get(id)
                .filter(|edge| edge.source == *source && edge.branch == *branch)
        })
    }

    pub(super) fn transition(
        &mut self,
        source: &Key,
        choices: &crate::combat::PreparedCombatChoices,
        branch: usize,
        input: Option<&ClientInput>,
        limits: CombatStepLimits,
        stats: &mut SolverStats,
    ) -> Result<Transition<CombatSearchState>, String> {
        if limits
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Ok(Transition::Interrupted {
                engine_steps: 0,
                cause: crate::TransitionInterruption::Deadline,
            });
        }
        let action = &choices.branches()[branch];
        if let Some(edge) = self.edge(source, action) {
            stats.transition_cache_hits += 1;
            record_potion_transition(source, input, true, 0, stats);
            return Ok(Transition::Settled {
                state: edge.destination.state.clone(),
                engine_steps: 0,
            });
        }
        let result = choices.transition(branch, limits)?;
        if let Transition::Settled {
            state,
            engine_steps,
        } = &result
        {
            record_potion_transition(source, input, false, *engine_steps, stats);
            let mut expenditures = source.expenditures;
            if let Some(input) = input {
                expenditures.record(input);
            }
            self.record_edge(
                source.clone(),
                action.clone(),
                Key::new(state.clone(), expenditures),
                stats,
            );
        }
        Ok(result)
    }

    fn remove_index(index: &mut HashMap<u64, Vec<u64>>, hash: u64, id: u64) {
        if let Some(bucket) = index.get_mut(&hash) {
            bucket.retain(|&candidate| candidate != id);
            if bucket.is_empty() {
                index.remove(&hash);
            }
        }
    }

    fn record_edge(
        &mut self,
        source: Key,
        branch: CombatBranch,
        destination: Key,
        stats: &mut SolverStats,
    ) {
        if self.edges.len() == self.capacity {
            let id = self.edge_order.pop_front().unwrap();
            let old = self.edges.remove(&id).unwrap();
            Self::remove_index(&mut self.outgoing, old.source.hash, id);
            Self::remove_index(&mut self.incoming, old.destination.hash, id);
            stats.graph_edges_evicted += 1;
        }
        let id = self.id();
        self.outgoing.entry(source.hash).or_default().push(id);
        self.incoming.entry(destination.hash).or_default().push(id);
        self.edge_order.push_back(id);
        // Newly encountered incoming edges may arrive after their destination
        // acquired a return. Reopen that propagation without replaying the win.
        let returns = self.returns(&destination).to_vec();
        let boundaries = self.boundaries(&destination).to_vec();
        if !returns.is_empty() {
            self.revision += 1;
        }
        self.edges.insert(
            id,
            Edge {
                source,
                branch,
                destination,
                propagated: vec![],
                boundaries: vec![],
            },
        );
        for result in returns {
            self.enqueue(Backup::Edge(id, result), stats);
        }
        for boundary in boundaries {
            let source_turn = self.edges[&id]
                .source
                .state
                .position()
                .combat
                .turn
                .turn_count;
            if boundary.turn() >= source_turn
                && boundary.turn() - source_turn <= Self::BOUNDARY_HORIZONS
            {
                self.enqueue(Backup::BoundaryEdge(id, boundary.turn()), stats);
            }
        }
        stats.graph_edges_recorded += 1;
        stats.graph_peak_edges = stats.graph_peak_edges.max(self.edges.len());
    }

    fn returns(&self, key: &Key) -> &[Return] {
        self.record_index
            .get(&key.hash)
            .and_then(|ids| {
                ids.iter()
                    .find_map(|id| self.records.get(id).filter(|r| r.key == *key))
            })
            .map_or(&[], |record| &record.returns)
    }

    fn record_id(&self, key: &Key) -> Option<u64> {
        self.record_index
            .get(&key.hash)?
            .iter()
            .copied()
            .find(|id| self.records.get(id).is_some_and(|r| r.key == *key))
    }

    fn boundaries(&self, key: &Key) -> &[Boundary] {
        self.record_id(key)
            .map_or(&[], |id| &self.records[&id].boundaries)
    }

    /// Seed only a settled, unresolved player turn reached after a turn change.
    /// The caller supplies shared-owner expenditure accounting at this endpoint.
    pub(super) fn observe_boundary(
        &mut self,
        node: &Node,
        total_potion_expenditures: usize,
        model: Option<&crate::resource_value::ValueModel>,
        stats: &mut SolverStats,
    ) {
        debug_assert_eq!(node.state.position().engine, EngineState::CombatPlayerTurn);
        let endpoint = Key::new(node.state.clone(), node.expenditures());
        if self
            .boundaries(&endpoint)
            .iter()
            .any(|b| b.endpoint == endpoint)
        {
            return;
        }
        let boundary = Boundary {
            endpoint: endpoint.clone(),
            trace: node.trace.clone(),
            input_count: node.input_count,
            ranks: retention::boundary_ranks(node, model),
            total_potion_expenditures,
        };
        self.enqueue(Backup::BoundarySeed(endpoint, boundary), stats);
        stats.observed_turn_boundaries += 1;
    }

    fn ensure_record(&mut self, key: &Key) -> u64 {
        if let Some(id) = self.record_id(key) {
            return id;
        }
        if self.records.len() == self.capacity {
            let id = self.record_order.pop_front().unwrap();
            let old = self.records.remove(&id).unwrap();
            Self::remove_index(&mut self.record_index, old.key.hash, id);
        }
        let id = self.id();
        self.record_index.entry(key.hash).or_default().push(id);
        self.record_order.push_back(id);
        self.records.insert(
            id,
            Record {
                key: key.clone(),
                returns: vec![],
                boundaries: vec![],
            },
        );
        id
    }

    /// Retain lane extrema for this horizon, separately for no-more-potion and
    /// unrestricted continuations. This summarizes evidence without combining
    /// the resources of different endpoints or changing exact state identity.
    fn retain_boundaries(pool: &mut Vec<Boundary>, source: &Key, new: Boundary) -> bool {
        let turn = new.turn();
        if pool.iter().any(|b| b.endpoint == new.endpoint) {
            return false;
        }
        pool.push(new);
        let newest = pool.len() - 1;
        let mut selected = vec![];
        for card_only in [true, false] {
            for lane in 0..retention::SCORE_LANES {
                // Earlier equal evidence wins, preventing cycles and retaining
                // deterministic witnesses rather than creating synthetic ranks.
                let best = (0..pool.len())
                    .filter(|&i| {
                        pool[i].turn() == turn && pool[i].permitted(source, card_only, None)
                    })
                    .min_by_key(|&i| (std::cmp::Reverse(pool[i].ranks[lane]), i));
                if let Some(i) = best {
                    if !selected.contains(&i) {
                        selected.push(i);
                    }
                }
            }
        }
        let accepted = selected.contains(&newest);
        let mut index = 0;
        pool.retain(|b| {
            let keep = b.turn() != turn || selected.contains(&index);
            index += 1;
            keep
        });
        accepted
    }

    fn backup_boundary(&mut self, key: Key, boundary: Boundary, stats: &mut SolverStats) {
        let source_turn = key.state.position().combat.turn.turn_count;
        if boundary.turn() < source_turn || boundary.turn() - source_turn > Self::BOUNDARY_HORIZONS
        {
            return;
        }
        let id = self.ensure_record(&key);
        if !Self::retain_boundaries(
            &mut self.records.get_mut(&id).unwrap().boundaries,
            &key,
            boundary.clone(),
        ) {
            return;
        }
        stats.boundary_backup_updates += 1;
        stats.backed_peak_records = stats.backed_peak_records.max(self.records.len());
        let incoming = self.incoming.get(&key.hash).map_or_else(Vec::new, |ids| {
            ids.iter()
                .copied()
                .filter(|id| {
                    self.edges.get(id).is_some_and(|e| {
                        e.destination == key
                            && boundary
                                .turn()
                                .saturating_sub(e.source.state.position().combat.turn.turn_count)
                                <= Self::BOUNDARY_HORIZONS
                    })
                })
                .collect()
        });
        for id in incoming {
            self.enqueue(Backup::BoundaryEdge(id, boundary.turn()), stats);
        }
    }

    fn enqueue(&mut self, backup: Backup, stats: &mut SolverStats) {
        if let Backup::BoundaryEdge(id, turn) = &backup {
            if self.pending_boundaries.contains(&(*id, *turn)) {
                stats.boundary_updates_coalesced += 1;
                return;
            }
        }
        if self.pending.len() < self.capacity.saturating_mul(Self::RETURNS_PER_KEY) {
            if let Backup::BoundaryEdge(id, turn) = &backup {
                self.pending_boundaries.insert((*id, *turn));
            }
            self.pending.push_back(backup);
        } else {
            stats.backed_queue_limit_dropped += 1;
        }
    }

    /// Called only after independent full-root replay, successor equality and
    /// canonical materialization. Observed terminals must never call this.
    pub(super) fn verified_win(
        &mut self,
        boundaries: Vec<Key>,
        outcome: CombatWitnessMaterializedOutcomeV1,
        final_expenditures: CombatInputExpendituresV1,
        stats: &mut SolverStats,
    ) {
        let result = Return {
            outcome: Arc::new(outcome),
            final_expenditures,
        };
        for key in boundaries {
            self.enqueue(Backup::Seed(key, result.clone()), stats);
        }
    }

    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn boundary_step(&mut self, backup: Backup, stats: &mut SolverStats) {
        stats.boundary_backup_operations += 1;
        match backup {
            Backup::BoundarySeed(key, boundary) => {
                self.backup_boundary(key, boundary, stats);
            }
            Backup::BoundaryEdge(id, turn) => {
                self.pending_boundaries.remove(&(id, turn));
                let Some(edge) = self.edges.get(&id) else {
                    return;
                };
                let source = edge.source.clone();
                let source_turn = edge.source.state.position().combat.turn.turn_count;
                if turn < source_turn || turn - source_turn > Self::BOUNDARY_HORIZONS {
                    return;
                }
                // Read the newest bounded envelope once, not every historical
                // endpoint queued while this edge was waiting for service.
                let boundaries = self
                    .boundaries(&edge.destination)
                    .iter()
                    .filter(|b| b.turn() == turn)
                    .cloned()
                    .collect::<Vec<_>>();
                for boundary in boundaries {
                    let edge = self.edges.get_mut(&id).unwrap();
                    if Self::retain_boundaries(&mut edge.boundaries, &source, boundary.clone()) {
                        self.backup_boundary(source.clone(), boundary, stats);
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    /// A terminal seed/edge or a bounded bookkeeping quantum. No boundary
    /// quantum simulates an input or crosses a queued terminal return. Small
    /// batches amortize graph administration; exact operation counts remain
    /// visible. Monotonic edge IDs and forwarded envelopes stop stale cycles.
    pub(super) fn backup_step(&mut self, stats: &mut SolverStats) {
        let Some(backup) = self.pending.pop_front() else {
            return;
        };
        stats.result_backup_work += 1;
        if matches!(backup, Backup::BoundarySeed(..) | Backup::BoundaryEdge(..)) {
            stats.boundary_backup_work += 1;
            self.boundary_step(backup, stats);
            for _ in 1..retention::SCORE_LANES {
                if !matches!(
                    self.pending.front(),
                    Some(Backup::BoundarySeed(..) | Backup::BoundaryEdge(..))
                ) {
                    break;
                }
                let backup = self.pending.pop_front().unwrap();
                self.boundary_step(backup, stats);
            }
            return;
        }
        let (key, result) = match backup {
            Backup::Seed(key, result) => (key, result),
            Backup::Edge(id, result) => {
                let Some(edge) = self.edges.get_mut(&id) else {
                    return;
                };
                if edge.propagated.iter().any(|old| old.covers(&result)) {
                    return;
                }
                edge.propagated.retain(|old| !result.covers(old));
                if edge.propagated.len() == Self::RETURNS_PER_KEY {
                    stats.backed_return_limit_dropped += 1;
                    return;
                }
                edge.propagated.push(result.clone());
                (edge.source.clone(), result)
            }
            Backup::BoundarySeed(..) | Backup::BoundaryEdge(..) => unreachable!(),
        };
        if self.returns(&key).iter().any(|old| old.covers(&result)) {
            return;
        }
        let existing = self.record_id(&key);
        let id = if let Some(id) = existing {
            let record = self.records.get_mut(&id).unwrap();
            record.returns.retain(|old| !result.covers(old));
            if record.returns.len() == Self::RETURNS_PER_KEY {
                stats.backed_return_limit_dropped += 1;
                return;
            }
            id
        } else {
            self.ensure_record(&key)
        };
        self.records
            .get_mut(&id)
            .unwrap()
            .returns
            .push(result.clone());
        stats.backed_return_updates += 1;
        stats.backed_peak_records = stats.backed_peak_records.max(self.records.len());
        self.revision += 1;
        let incoming = self.incoming.get(&key.hash).map_or_else(Vec::new, |ids| {
            ids.iter()
                .copied()
                .filter(|id| {
                    self.edges
                        .get(id)
                        .is_some_and(|edge| edge.destination == key)
                })
                .collect()
        });
        for id in incoming {
            self.enqueue(Backup::Edge(id, result.clone()), stats);
        }
    }

    /// Stable Pareto layers of known feasible branches, then unseen branches.
    /// This schedules evidence, not a value label for unsearched alternatives.
    pub(super) fn reorder(
        &self,
        expansion: &mut Expansion,
        card_only: bool,
        max_potions: Option<usize>,
        stats: &mut SolverStats,
    ) {
        if expansion.backed_revision == self.revision {
            return;
        }
        expansion.backed_revision = self.revision;
        if expansion.branches.len() - expansion.index < 2 {
            return;
        }
        let source = &expansion.graph_key;
        // A destination's result will enqueue its source. Until that charged
        // backup arrives, there is no reason to scan every branch here.
        if self.returns(source).is_empty() {
            return;
        }
        let remaining = &mut expansion.branches[expansion.index..];
        let mut known = Vec::new();
        let mut unseen = Vec::new();
        for &branch in remaining.iter() {
            let returns = self
                .edge(source, &expansion.choices.branches()[branch])
                .map_or_else(Vec::new, |edge| {
                    self.returns(&edge.destination)
                        .iter()
                        .filter(|r| r.permitted(source, card_only, max_potions))
                        .collect::<Vec<_>>()
                });
            if returns.is_empty() {
                unseen.push(branch);
            } else {
                known.push((branch, returns));
            }
        }
        if known.is_empty() {
            return;
        }
        let mut ordered = pareto_order(known);
        ordered.extend(unseen);
        if remaining != ordered {
            remaining.copy_from_slice(&ordered);
            stats.backed_branch_reorders += 1;
        }
    }
}

fn record_potion_transition(
    source: &Key,
    input: Option<&ClientInput>,
    cached: bool,
    engine_steps: usize,
    stats: &mut SolverStats,
) {
    let p = &mut stats.potion_search;
    if matches!(input, Some(ClientInput::UsePotion { .. })) {
        p.use_transitions += 1;
        p.cached_use_transitions += usize::from(cached);
    }
    if matches!(input, Some(ClientInput::DiscardPotion(_))) {
        p.discard_transitions += 1;
        p.cached_discard_transitions += usize::from(cached);
    }
    if source.expenditures.explicit_count() > 0 {
        p.after_expenditure_transitions += 1;
        p.after_expenditure_engine_steps += engine_steps;
    }
}

#[cfg(test)]
mod tests;
