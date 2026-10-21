//! A bounded scheduling demonstration, not labels of unsearched outcome quality.
use super::{
    inspection::ReferenceProgress, linear, routes::CompleteRoute, PathContext, PathOrder,
    PathPrior, PathSearch,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize)]
pub struct QueueNode {
    pub parent: Option<usize>,
    pub incoming_choice: Option<usize>,
    pub features: linear::LinearInput,
}
#[derive(Serialize)]
pub struct QueueEdge {
    pub parent: usize,
    pub choice: usize,
    pub depth: usize,
    pub sequence: u64,
    /// Diagnostic source cost; never an input feature or a target value.
    pub source_cost: f64,
    /// Indices in the caller's independently reconstructed references.
    /// Empty means unknown, not a bad action or an unsolvable continuation.
    pub witnesses: Vec<usize>,
}
#[derive(Serialize)]
pub struct WitnessedQueue {
    pub schema: u32,
    pub evidence_kind: &'static str,
    pub root_id: String,
    pub goal: crate::ResourceRequirement,
    pub prior_revision: String,
    pub frontier_size: usize,
    pub leading_edges_requested: usize,
    pub nodes: Vec<QueueNode>,
    pub edges: Vec<QueueEdge>,
}

impl<P: PathPrior> PathSearch<P> {
    /// Read a ready queue without advancing search. Only exact reconstructed
    /// same-query victories can witness an edge. Caller owns replay provenance.
    /// None means no eligible example, never a negative reachability label.
    /// Includes the first K queue edges plus witnessed edges and shared ancestry;
    /// it is a censored sample, not all competing prefixes or a work bound.
    pub fn witnessed_queue(
        &self,
        references: &[CompleteRoute],
        leading_edges: usize,
    ) -> Result<Option<WitnessedQueue>, String> {
        if leading_edges == 0 || references.is_empty() || self.order != PathOrder::RelativePrior {
            return Err("queue data needs explicit references, positive sample size and relative-prior order".into());
        }
        let mut reviews = Vec::new();
        for route in references {
            let review = self.inspect_route(route)?;
            if !route.goal.accepts(&route.outcome) {
                return Err("queue witness must satisfy its full resource query".into());
            }
            reviews.push(review);
        }
        if self.expanding.is_some() || self.replay.is_some() {
            return Ok(None);
        }
        let nondominated: Vec<_> = (0..references.len())
            .filter(|&i| {
                !references
                    .iter()
                    .any(|r| r.outcome.strictly_dominates(&references[i].outcome))
            })
            .collect();
        if nondominated
            .iter()
            .any(|&i| matches!(reviews[i].progress, ReferenceProgress::Reached { .. }))
        {
            return Ok(None);
        }
        let mut witnessed = BTreeMap::<u64, Vec<usize>>::new();
        for i in nondominated {
            if !matches!(reviews[i].progress, ReferenceProgress::Queued { .. }) {
                continue;
            }
            let mut node = 0;
            for d in references[i]
                .decisions
                .iter()
                .take(reviews[i].reached_decisions)
            {
                node = self
                    .nodes
                    .iter()
                    .enumerate()
                    .find(|(_, n)| {
                        n.parent == Some(node)
                            && n.step
                                .as_ref()
                                .is_some_and(|s| s.branch == d.branches[d.selected])
                    })
                    .map(|(n, _)| n)
                    .ok_or("reference prefix disappeared")?;
            }
            let choice = references[i].decisions[reviews[i].reached_decisions].selected;
            let edge = self
                .frontier
                .iter()
                .find(|e| e.parent == node && e.choice == choice)
                .ok_or("witnessed queue edge disappeared")?;
            witnessed.entry(edge.sequence).or_default().push(i);
        }
        if witnessed.is_empty() {
            return Ok(None);
        }
        let mut edges = self.frontier.iter().collect::<Vec<_>>();
        edges.sort_by(|a, b| b.cmp(a));
        let mut sampled = edges
            .iter()
            .take(leading_edges)
            .map(|e| e.sequence)
            .collect::<BTreeSet<_>>();
        sampled.extend(witnessed.keys());
        edges.retain(|e| sampled.contains(&e.sequence));
        let mut ancestors = BTreeSet::new();
        for edge in &edges {
            let mut node = Some(edge.parent);
            while let Some(i) = node {
                if !ancestors.insert(i) {
                    break;
                }
                node = self.nodes[i].parent;
            }
        }
        let remap = ancestors
            .iter()
            .enumerate()
            .map(|(new, &old)| (old, new))
            .collect::<BTreeMap<_, _>>();
        let mut nodes = Vec::new();
        for &old in &ancestors {
            let n = &self.nodes[old];
            let expansion = n.expansion.as_ref().ok_or("queue ancestor is unprepared")?;
            let incoming_choice = n
                .parent
                .map(|p| {
                    self.nodes[p]
                        .expansion
                        .as_ref()
                        .unwrap()
                        .choices
                        .branches()
                        .iter()
                        .position(|b| b == &n.step.as_ref().unwrap().branch)
                        .ok_or("ancestor choice disappeared")
                })
                .transpose()?;
            nodes.push(QueueNode {
                parent: n.parent.map(|p| remap[&p]),
                incoming_choice,
                features: linear::encode(
                    &n.state,
                    PathContext {
                        root: &self.root,
                        goal: &self.goal,
                        spent_originals: n.spent_originals,
                    },
                    &expansion.choices.branches(),
                )?,
            });
        }
        let root = serde_json::to_value(&self.root).map_err(|e| e.to_string())?;
        Ok(Some(WitnessedQueue {
            schema: 1,
            evidence_kind: "witnessed_queue",
            root_id: format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&root).map_err(|e| e.to_string())?)
            ),
            goal: self.goal.clone(),
            prior_revision: self.prior_revision.clone(),
            frontier_size: self.frontier.len(),
            leading_edges_requested: leading_edges,
            nodes,
            edges: edges
                .into_iter()
                .map(|e| QueueEdge {
                    parent: remap[&e.parent],
                    choice: e.choice,
                    depth: self.nodes[e.parent].depth + 1,
                    sequence: e.sequence,
                    source_cost: e.priority,
                    witnesses: witnessed.get(&e.sequence).cloned().unwrap_or_default(),
                })
                .collect(),
        }))
    }
}
