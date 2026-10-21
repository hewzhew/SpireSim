use super::*;
use sts_combat_outcome::combat_witness_materialization::{
    project_replay_checked_combat_outcome_v1, CombatWitnessMaterializedOutcomeV1,
};

mod node;
mod priority;
mod queue;
pub(in crate::ai::combat_search_v2) mod resources;

pub(super) use node::{RootLineage, RootLineageId, SearchNode};
pub(super) use priority::QueueEntry;
pub(super) use queue::FrontierQueue;
pub(super) use resources::{is_resource_covered, ResourceVector};

#[derive(Clone)]
pub(super) struct RetainedWinCandidate {
    pub(super) node: SearchNode,
    resources: CombatWitnessMaterializedOutcomeV1,
}

pub(super) fn remember_best_complete(best: &mut Option<SearchNode>, candidate: SearchNode) {
    let replace = best
        .as_ref()
        .map(|existing| compare_nodes(&candidate, existing) == Ordering::Greater)
        .unwrap_or(true);
    if replace {
        *best = Some(candidate);
    }
}

pub(super) fn remember_win_candidate(
    root: &CombatPosition,
    candidates: &mut Vec<RetainedWinCandidate>,
    candidate: &SearchNode,
) -> bool {
    let candidate_resources = materialized_win_resources(root, candidate);
    if candidates
        .iter()
        .any(|existing| existing.resources.strictly_dominates(&candidate_resources))
    {
        return false;
    }

    if let Some(equal_index) = candidates.iter().position(|existing| {
        existing
            .resources
            .has_same_materialized_resources_v1(&candidate_resources)
    }) {
        if compare_nodes(candidate, &candidates[equal_index].node) == Ordering::Greater {
            candidates[equal_index] = RetainedWinCandidate {
                node: candidate.clone(),
                resources: candidate_resources,
            };
            return true;
        }
        return false;
    }

    candidates.retain(|existing| !candidate_resources.strictly_dominates(&existing.resources));
    candidates.push(RetainedWinCandidate {
        node: candidate.clone(),
        resources: candidate_resources,
    });
    true
}

/// Search has already settled every edge in this complete line. Reuse the
/// terminal resource owner without searching or replaying it again. The root
/// binds potion identity, including passive expenditure and generated potions;
/// action count and tactical ordering are only tie-breaks for equal resources.
fn materialized_win_resources(
    root: &CombatPosition,
    node: &SearchNode,
) -> CombatWitnessMaterializedOutcomeV1 {
    project_replay_checked_combat_outcome_v1(
        root,
        &node
            .actions
            .iter()
            .map(|action| action.input.clone())
            .collect::<Vec<_>>(),
        &CombatPosition::new(node.engine.clone(), node.combat.clone()),
    )
}

pub(super) fn remember_best_frontier(best: &mut Option<SearchNode>, candidate: &SearchNode) {
    let replace = best
        .as_ref()
        .map(|existing| compare_nodes(candidate, existing) == Ordering::Greater)
        .unwrap_or(true);
    if replace {
        *best = Some(candidate.clone());
    }
}

fn compare_nodes(left: &SearchNode, right: &SearchNode) -> Ordering {
    CombatOutcomeScore::from_node(left).cmp(&CombatOutcomeScore::from_node(right))
}

#[cfg(test)]
mod tests;
