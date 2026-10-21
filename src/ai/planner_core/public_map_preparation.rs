//! Connected preparation prefixes over the published ordinary map only.
//!
//! At most four states per reachable node retain Rest/Shop availability on the
//! same path. One shortest example per endpoint and availability class is kept;
//! these examples are not path frequencies, combat forecasts or event outcomes.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::{PlannerMapRoom, PlannerPublicMap, PlannerPublicMapNode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicMapPreparationEndpoint {
    Elite,
    KnownEnd,
    UnresolvedEnd,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicMapPreparationPath {
    pub nodes: Vec<(i32, i32)>,
    pub has_rest: bool,
    pub has_shop: bool,
    pub endpoint: PublicMapPreparationEndpoint,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PublicMapPreparationSummary {
    /// Indices: neither, Rest only, Shop only, both on one connected prefix.
    pub elite_steps_by_preparation: [Option<usize>; 4],
    /// Published Boss/Victory reached before any published Elite. A question
    /// mark on the way remains unresolved, so this is not a safe-route claim.
    pub first_known_elite_free_end_steps: Option<usize>,
    /// Missing destinations or nonterminal recorded leaves remain incomplete.
    pub first_unresolved_end_steps: Option<usize>,
}

/// Enumerate preparation classes from an explicit selected node, including it.
/// The first Elite, Boss, Victory or visible/invisible endpoint ends a prefix.
pub fn public_map_preparation_paths(
    map: &PlannerPublicMap,
    x: i32,
    y: i32,
) -> Result<Vec<PublicMapPreparationPath>, String> {
    type State = ((i32, i32), bool, bool);
    let mut nodes = BTreeMap::<(i32, i32), &PlannerPublicMapNode>::new();
    for node in &map.nodes {
        if nodes.insert((node.x, node.y), node).is_some() {
            return Err(format!(
                "duplicate public map node ({}, {})",
                node.x, node.y
            ));
        }
    }
    let state = |point, rest, shop| {
        let room = nodes.get(&point).map(|node| node.room);
        (
            point,
            rest || room == Some(PlannerMapRoom::Rest),
            shop || room == Some(PlannerMapRoom::Shop),
        )
    };
    let root = state((x, y), false, false);
    let mut parents: BTreeMap<State, Option<State>> = BTreeMap::from([(root, None)]);
    let mut queue = VecDeque::from([root]);
    let mut endpoints = Vec::new();
    while let Some(current @ (point, rest, shop)) = queue.pop_front() {
        let node = nodes.get(&point);
        let endpoint = match node {
            Some(node) if node.room == PlannerMapRoom::Elite => {
                Some(PublicMapPreparationEndpoint::Elite)
            }
            Some(node) if matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory) => {
                Some(PublicMapPreparationEndpoint::KnownEnd)
            }
            None => Some(PublicMapPreparationEndpoint::UnresolvedEnd),
            Some(node) if node.edges.is_empty() => {
                Some(PublicMapPreparationEndpoint::UnresolvedEnd)
            }
            _ => None,
        };
        if let Some(endpoint) = endpoint {
            endpoints.push((current, endpoint));
            continue;
        }
        let node = node.unwrap();
        let mut children = node
            .edges
            .iter()
            .map(|edge| (edge.destination_x, edge.destination_y))
            .collect::<Vec<_>>();
        if children.iter().copied().collect::<BTreeSet<_>>().len() != children.len() {
            return Err(format!(
                "duplicate public map edge at ({}, {})",
                point.0, point.1
            ));
        }
        if children.iter().any(|child| child.1 <= point.1) {
            return Err(format!(
                "public map edge does not advance from ({}, {})",
                point.0, point.1
            ));
        }
        children.sort_unstable();
        for point in children {
            let next = state(point, rest, shop);
            if let std::collections::btree_map::Entry::Vacant(entry) = parents.entry(next) {
                entry.insert(Some(current));
                queue.push_back(next);
            }
        }
    }
    endpoints.sort_by_key(|(((x, y), rest, shop), _)| (*y, *x, *rest, *shop));
    Ok(endpoints
        .into_iter()
        .map(|(last, endpoint)| {
            let mut points = Vec::new();
            let mut cursor = Some(last);
            while let Some(current) = cursor {
                points.push(current.0);
                cursor = parents[&current];
            }
            points.reverse();
            PublicMapPreparationPath {
                nodes: points,
                has_rest: last.1,
                has_shop: last.2,
                endpoint,
            }
        })
        .collect())
}

/// Ordinary next targets after the current room. Absence means the cursor is
/// not observable; an empty known list means no ordinary continuation is shown.
/// Future Wing Boots jumps are outside this projection.
pub fn public_map_preparation_starts_after_current(
    map: &PlannerPublicMap,
) -> Option<Vec<(i32, i32)>> {
    let mut starts = if let Some(current) = map
        .nodes
        .iter()
        .find(|node| (node.x, node.y) == (map.current_x, map.current_y))
    {
        current
            .edges
            .iter()
            .map(|edge| (edge.destination_x, edge.destination_y))
            .collect::<Vec<_>>()
    } else if map.current_y == -1 {
        map.nodes
            .iter()
            .filter(|node| node.y == 0)
            .map(|node| (node.x, node.y))
            .collect()
    } else {
        return None;
    };
    starts.sort_unstable();
    starts.dedup();
    Some(starts)
}

/// Earliest published endpoint per joint class over alternative start nodes.
/// `initial_steps` is 0 for a selected node, 1 for the current room's next node.
pub fn summarize_public_map_preparation(
    map: &PlannerPublicMap,
    starts: &[(i32, i32)],
    initial_steps: usize,
) -> Result<PublicMapPreparationSummary, String> {
    let mut summary = PublicMapPreparationSummary::default();
    for &(x, y) in starts {
        for path in public_map_preparation_paths(map, x, y)? {
            let steps = path.nodes.len() - 1 + initial_steps;
            let field = match path.endpoint {
                PublicMapPreparationEndpoint::Elite => {
                    let class = usize::from(path.has_rest) + 2 * usize::from(path.has_shop);
                    &mut summary.elite_steps_by_preparation[class]
                }
                PublicMapPreparationEndpoint::KnownEnd => {
                    &mut summary.first_known_elite_free_end_steps
                }
                PublicMapPreparationEndpoint::UnresolvedEnd => {
                    &mut summary.first_unresolved_end_steps
                }
            };
            *field = Some(field.map_or(steps, |prior| prior.min(steps)));
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests;
