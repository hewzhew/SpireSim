//! Read-only comparison of a reconstructed reference with the resident search.
//! Counts describe this snapshot, not future work or negative training labels.
use super::{routes::CompleteRoute, Edge, PathPrior, PathSearch};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReferenceProgress {
    Reached {
        replay_pending: bool,
        retained_plan: bool,
    },
    AwaitingPreparation,
    AwaitingEnqueue,
    Queued {
        /// Uses the actual queue comparator, including depth and insertion ties.
        ahead: usize,
        strictly_lower_cost: usize,
        /// Includes the reference edge itself, regardless of depth/sequence ties.
        equal_cost: usize,
    },
    DepthDeferred,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DivergenceCount {
    /// Number of reference grammar choices shared before departing from it.
    pub after_decisions: usize,
    /// Includes terminal and exhausted stored nodes; excludes reference ancestors.
    /// Losses and rejected successors are not retained by PathSearch.
    pub retained_nodes: usize,
    /// Queued before the reference's first unreached edge in this snapshot.
    pub queued_ahead: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RouteInspection {
    pub reference_decisions: usize,
    pub reached_decisions: usize,
    pub next_input_index: Option<usize>,
    pub progress: ReferenceProgress,
    pub active_preparation: bool,
    pub active_replay: bool,
    pub frontier_edges: usize,
    pub depth_deferred_edges: usize,
    pub divergences: Vec<DivergenceCount>,
}

impl<P: PathPrior> PathSearch<P> {
    /// Inspect an independently reconstructed route at the same complete root
    /// and query. The caller owns reference provenance and reconstruction.
    /// No engine step, prior call, replay, queue mutation or budget grant occurs.
    /// A reached route need not be retained as a plan (e.g. dominated resources).
    pub fn inspect_route(&self, route: &CompleteRoute) -> Result<RouteInspection, String> {
        if self.root != route.root || self.goal != route.goal || route.decisions.is_empty() {
            return Err(
                "inspection needs a reconstructed route at the same exact root/query".into(),
            );
        }
        if let Some(failure) = &self.failure {
            return Err(format!("search stopped with an error: {failure}"));
        }
        // Edges are source-bound; equal-looking states or duplicate cards never
        // substitute for the recorded ancestry. No transposition lookup is used.
        let mut children = vec![Vec::new(); self.nodes.len()];
        for (i, node) in self.nodes.iter().enumerate().skip(1) {
            children[node.parent.unwrap()].push(i);
        }
        let mut reference_nodes = vec![false; self.nodes.len()];
        reference_nodes[0] = true;
        let mut parent = 0;
        let mut reached = 0;
        for decision in &route.decisions {
            if self.nodes[parent].state != decision.state
                || self.nodes[parent].spent_originals != decision.spent_originals_before
            {
                return Err("reference and resident prefix state differ".into());
            }
            let branch = decision
                .branches
                .get(decision.selected)
                .ok_or("invalid reference choice")?;
            let child = children[parent].iter().copied().find(|&i| {
                self.nodes[i]
                    .step
                    .as_ref()
                    .is_some_and(|s| &s.branch == branch)
            });
            let Some(child) = child else { break };
            reference_nodes[child] = true;
            parent = child;
            reached += 1;
        }
        // Count actual stored divergence families, even when there is no missing
        // reference edge. Totals never pretend that discarded losses were stored.
        let mut shared = vec![0; self.nodes.len()];
        let mut divergences: Vec<_> = (0..=reached)
            .map(|after_decisions| DivergenceCount {
                after_decisions,
                ..Default::default()
            })
            .collect();
        for (i, node) in self.nodes.iter().enumerate().skip(1) {
            if reference_nodes[i] {
                shared[i] = node.depth;
            } else {
                shared[i] = shared[node.parent.unwrap()];
                divergences[shared[i]].retained_nodes += 1;
            }
        }
        let progress = if reached == route.decisions.len() {
            if self.nodes[parent].state.position() != &route.successor {
                return Err("reference and resident terminal successor differ".into());
            }
            ReferenceProgress::Reached {
                replay_pending: self.replay.as_ref().is_some_and(|r| r.node == parent),
                retained_plan: self
                    .plans
                    .iter()
                    .any(|p| p.inputs == route.inputs && p.successor == route.successor),
            }
        } else if let Some(expansion) = &self.nodes[parent].expansion {
            let decision = &route.decisions[reached];
            if expansion.choices.branches() != decision.branches {
                return Err("reference choices differ from resident admission".into());
            }
            let choice = decision.selected;
            if choice >= expansion.cursor {
                ReferenceProgress::AwaitingEnqueue
            } else {
                let matches = |edge: &&Edge| edge.parent == parent && edge.choice == choice;
                if let Some(target) = self.frontier.iter().find(matches) {
                    let mut ahead = 0;
                    let mut strictly_lower_cost = 0;
                    let mut equal_cost = 0;
                    for edge in &self.frontier {
                        if edge.priority < target.priority {
                            strictly_lower_cost += 1;
                        }
                        if edge.priority.total_cmp(&target.priority).is_eq() {
                            equal_cost += 1;
                        }
                        if edge > target {
                            ahead += 1;
                            divergences[shared[edge.parent]].queued_ahead += 1;
                        }
                    }
                    ReferenceProgress::Queued {
                        ahead,
                        strictly_lower_cost,
                        equal_cost,
                    }
                } else if self.depth_deferred.values().flatten().any(|e| matches(&e)) {
                    ReferenceProgress::DepthDeferred
                } else {
                    return Err(
                        "reference edge is neither retained, pending nor depth-deferred".into(),
                    );
                }
            }
        } else {
            ReferenceProgress::AwaitingPreparation
        };
        divergences.retain(|d| d.retained_nodes > 0 || d.queued_ahead > 0);
        Ok(RouteInspection {
            reference_decisions: route.decisions.len(),
            reached_decisions: reached,
            next_input_index: route.decisions.get(reached).map(|d| d.input_index),
            progress,
            active_preparation: self.expanding.is_some(),
            active_replay: self.replay.is_some(),
            frontier_edges: self.frontier.len(),
            depth_deferred_edges: self.depth_deferred_count,
            divergences,
        })
    }
}
