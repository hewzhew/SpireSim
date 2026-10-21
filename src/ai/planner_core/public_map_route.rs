//! Candidate-local facts derived only from the visible ordinary map graph.
//!
//! Each minimum and maximum ranges over complete ordinary continuations from
//! the selected node to a visible leaf. An earliest room can occur on a
//! different continuation from a minimum or maximum room count. Question marks
//! remain question marks; future Wing Boots flights are not assumed. Ordinary
//! combat rooms and uncertain rooms are counted separately from elite rooms.

use std::collections::BTreeMap;

use super::{PlannerMapRoom, PlannerPublicMap, PlannerPublicMapNode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicMapRouteSummary {
    pub min_elites: usize,
    pub max_elites: usize,
    pub min_combat_rooms: usize,
    pub max_combat_rooms: usize,
    pub min_unknown_rooms: usize,
    pub max_unknown_rooms: usize,
    pub min_rests: usize,
    pub max_rests: usize,
    pub min_shops: usize,
    pub max_shops: usize,
    pub first_elite_steps: Option<usize>,
    pub first_rest_steps: Option<usize>,
    pub first_shop_steps: Option<usize>,
    /// A shop reachable on one ordinary continuation before any elite room.
    /// Unlike `first_shop_steps` and `min_elites`, this is a joint path fact.
    pub first_shop_before_elite_steps: Option<usize>,
}

impl PublicMapRouteSummary {
    fn from_room(room: PlannerMapRoom) -> Self {
        let is_elite = usize::from(room == PlannerMapRoom::Elite);
        let is_combat = usize::from(matches!(
            room,
            PlannerMapRoom::Monster | PlannerMapRoom::Elite
        ));
        let is_unknown = usize::from(room == PlannerMapRoom::QuestionMark);
        let is_rest = usize::from(room == PlannerMapRoom::Rest);
        let is_shop = usize::from(room == PlannerMapRoom::Shop);
        Self {
            min_elites: is_elite,
            max_elites: is_elite,
            min_combat_rooms: is_combat,
            max_combat_rooms: is_combat,
            min_unknown_rooms: is_unknown,
            max_unknown_rooms: is_unknown,
            min_rests: is_rest,
            max_rests: is_rest,
            min_shops: is_shop,
            max_shops: is_shop,
            first_elite_steps: (is_elite != 0).then_some(0),
            first_rest_steps: (is_rest != 0).then_some(0),
            first_shop_steps: (is_shop != 0).then_some(0),
            first_shop_before_elite_steps: (is_shop != 0).then_some(0),
        }
    }

    fn with_children(mut self, children: &[Self]) -> Self {
        if children.is_empty() {
            return self;
        }
        self.min_elites += children.iter().map(|child| child.min_elites).min().unwrap();
        self.max_elites += children.iter().map(|child| child.max_elites).max().unwrap();
        self.min_combat_rooms += children
            .iter()
            .map(|child| child.min_combat_rooms)
            .min()
            .unwrap();
        self.max_combat_rooms += children
            .iter()
            .map(|child| child.max_combat_rooms)
            .max()
            .unwrap();
        self.min_unknown_rooms += children
            .iter()
            .map(|child| child.min_unknown_rooms)
            .min()
            .unwrap();
        self.max_unknown_rooms += children
            .iter()
            .map(|child| child.max_unknown_rooms)
            .max()
            .unwrap();
        self.min_rests += children.iter().map(|child| child.min_rests).min().unwrap();
        self.max_rests += children.iter().map(|child| child.max_rests).max().unwrap();
        self.min_shops += children.iter().map(|child| child.min_shops).min().unwrap();
        self.max_shops += children.iter().map(|child| child.max_shops).max().unwrap();
        if self.first_elite_steps.is_none() {
            self.first_elite_steps = earliest(children, |child| child.first_elite_steps);
        }
        if self.first_rest_steps.is_none() {
            self.first_rest_steps = earliest(children, |child| child.first_rest_steps);
        }
        if self.first_shop_steps.is_none() {
            self.first_shop_steps = earliest(children, |child| child.first_shop_steps);
        }
        if self.first_elite_steps == Some(0) {
            self.first_shop_before_elite_steps = None;
        } else if self.first_shop_before_elite_steps.is_none() {
            self.first_shop_before_elite_steps =
                earliest(children, |child| child.first_shop_before_elite_steps);
        }
        self
    }
}

fn earliest(
    children: &[PublicMapRouteSummary],
    field: impl Fn(&PublicMapRouteSummary) -> Option<usize>,
) -> Option<usize> {
    children
        .iter()
        .filter_map(field)
        .min()
        .map(|steps| steps + 1)
}

/// Summarize an ordinary-map suffix without reading the run state or hidden RNG.
pub fn summarize_public_map_route(
    map: &PlannerPublicMap,
    x: i32,
    y: i32,
) -> Result<PublicMapRouteSummary, String> {
    let mut nodes = BTreeMap::<(i32, i32), &PlannerPublicMapNode>::new();
    for node in &map.nodes {
        if nodes.insert((node.x, node.y), node).is_some() {
            return Err(format!(
                "duplicate public map node ({}, {})",
                node.x, node.y
            ));
        }
    }
    let mut ordered = nodes.values().copied().collect::<Vec<_>>();
    ordered.sort_by_key(|node| std::cmp::Reverse(node.y));
    let mut suffix = BTreeMap::<(i32, i32), PublicMapRouteSummary>::new();
    for node in ordered {
        let children = node
            .edges
            .iter()
            .map(|edge| {
                if edge.destination_y <= node.y {
                    return Err(format!(
                        "public map edge does not advance from ({}, {}) to ({}, {})",
                        node.x, node.y, edge.destination_x, edge.destination_y
                    ));
                }
                suffix
                    .get(&(edge.destination_x, edge.destination_y))
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "public map edge has no visible target ({}, {})",
                            edge.destination_x, edge.destination_y
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        suffix.insert(
            (node.x, node.y),
            PublicMapRouteSummary::from_room(node.room).with_children(&children),
        );
    }
    suffix
        .get(&(x, y))
        .copied()
        .ok_or_else(|| format!("route target ({x}, {y}) is absent from public map"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::planner_core::PlannerPublicMapEdge;

    fn node(x: i32, y: i32, room: PlannerMapRoom, next: &[(i32, i32)]) -> PlannerPublicMapNode {
        PlannerPublicMapNode {
            x,
            y,
            room,
            has_emerald_key: false,
            edges: next
                .iter()
                .map(|&(destination_x, destination_y)| PlannerPublicMapEdge {
                    destination_x,
                    destination_y,
                })
                .collect(),
        }
    }

    #[test]
    fn route_facts_bind_future_rooms_to_the_selected_branch() {
        let map = PlannerPublicMap {
            current_x: -1,
            current_y: -1,
            boss: None,
            nodes: vec![
                node(0, 0, PlannerMapRoom::Monster, &[(0, 1)]),
                node(2, 0, PlannerMapRoom::Monster, &[(2, 1)]),
                node(0, 1, PlannerMapRoom::Monster, &[(0, 2)]),
                node(2, 1, PlannerMapRoom::Rest, &[(2, 2)]),
                node(0, 2, PlannerMapRoom::Elite, &[]),
                node(2, 2, PlannerMapRoom::Shop, &[]),
            ],
        };
        let elite_route = summarize_public_map_route(&map, 0, 0).unwrap();
        let rest_route = summarize_public_map_route(&map, 2, 0).unwrap();
        assert_eq!((elite_route.min_elites, elite_route.max_elites), (1, 1));
        assert_eq!(
            (elite_route.min_combat_rooms, elite_route.max_combat_rooms),
            (3, 3)
        );
        assert_eq!(elite_route.first_elite_steps, Some(2));
        assert_eq!(elite_route.first_rest_steps, None);
        assert_eq!(elite_route.first_shop_before_elite_steps, None);
        assert_eq!((rest_route.min_elites, rest_route.max_elites), (0, 0));
        assert_eq!(
            (rest_route.min_combat_rooms, rest_route.max_combat_rooms),
            (1, 1)
        );
        assert_eq!(rest_route.first_rest_steps, Some(1));
        assert_eq!(rest_route.first_shop_steps, Some(2));
        assert_eq!(rest_route.first_shop_before_elite_steps, Some(2));
    }

    #[test]
    fn alternatives_are_reported_as_ranges_and_unknown_rooms_are_not_resolved() {
        let map = PlannerPublicMap {
            current_x: 0,
            current_y: -1,
            boss: None,
            nodes: vec![
                node(0, 0, PlannerMapRoom::QuestionMark, &[(0, 1), (1, 1)]),
                node(0, 1, PlannerMapRoom::Elite, &[]),
                node(1, 1, PlannerMapRoom::Rest, &[]),
            ],
        };
        let route = summarize_public_map_route(&map, 0, 0).unwrap();
        assert_eq!((route.min_elites, route.max_elites), (0, 1));
        assert_eq!((route.min_combat_rooms, route.max_combat_rooms), (0, 1));
        assert_eq!((route.min_unknown_rooms, route.max_unknown_rooms), (1, 1));
        assert_eq!((route.min_rests, route.max_rests), (0, 1));
        assert_eq!(route.first_elite_steps, Some(1));
        assert_eq!(route.first_rest_steps, Some(1));
    }

    #[test]
    fn independent_extrema_do_not_invent_an_elite_free_shop_path() {
        let map = PlannerPublicMap {
            current_x: -1,
            current_y: -1,
            boss: None,
            nodes: vec![
                node(0, 0, PlannerMapRoom::QuestionMark, &[(0, 1), (1, 1)]),
                node(0, 1, PlannerMapRoom::Elite, &[(0, 2)]),
                node(0, 2, PlannerMapRoom::Shop, &[]),
                node(1, 1, PlannerMapRoom::Monster, &[(1, 2)]),
                node(1, 2, PlannerMapRoom::Rest, &[]),
            ],
        };
        let route = summarize_public_map_route(&map, 0, 0).unwrap();
        assert_eq!(route.min_elites, 0);
        assert_eq!(route.first_shop_steps, Some(2));
        assert_eq!(route.first_shop_before_elite_steps, None);
        assert_eq!((route.min_combat_rooms, route.max_combat_rooms), (1, 1));
        assert_eq!((route.min_unknown_rooms, route.max_unknown_rooms), (1, 1));
    }
}
