//! Separate actor navigation topology from source-oriented map comparison.
//!
//! Java's MapGenerator keeps drawing anchors and assigns rooms to unused cells.
//! Those bytes matter to parity, but are not selectable rooms on an actor's map.

use std::collections::BTreeSet;

use crate::ai::planner_core::{
    PlannerMapRoom, PlannerPublicMap, PlannerPublicMapEdge, PlannerPublicMapNode,
};
use crate::content::monsters::factory::EncounterId;

use super::RunControlSession;

/// Comparison input retains assigned cells and original drawing edges. It is
/// not an actor navigation graph or a source of legal route actions. The active
/// synthetic boss node follows the established run comparison representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunMapComparisonTopologyV1 {
    pub current_x: i32,
    pub current_y: i32,
    pub boss: Option<EncounterId>,
    pub nodes: Vec<PlannerPublicMapNode>,
}

pub fn run_map_comparison_topology_v1(session: &RunControlSession) -> RunMapComparisonTopologyV1 {
    let mut nodes = session
        .run_state
        .map
        .graph
        .iter()
        .flat_map(|row| row.iter())
        .filter_map(|node| {
            node.class.map(|room| PlannerPublicMapNode {
                x: node.x,
                y: node.y,
                room: PlannerMapRoom::from(room),
                has_emerald_key: node.has_emerald_key,
                edges: node
                    .edges
                    .iter()
                    .map(|edge| PlannerPublicMapEdge {
                        destination_x: edge.dst_x,
                        destination_y: edge.dst_y,
                    })
                    .collect(),
            })
        })
        .collect::<Vec<_>>();
    if (session.run_state.map.can_travel_to(0, 15, false) || session.run_state.map.current_y == 15)
        && !nodes.iter().any(|node| node.x == 0 && node.y == 15)
    {
        nodes.push(boss_node());
    }
    RunMapComparisonTopologyV1 {
        current_x: session.run_state.map.current_x,
        current_y: session.run_state.map.current_y,
        boss: session.run_state.boss_key,
        nodes,
    }
}

/// Public navigation topology, not the generator's drawing matrix. No RNG or
/// future room outcomes are read. Normal candidates still belong to the action
/// owner; reachability alone does not grant a current action or future flight.
pub fn planner_public_map_v1(session: &RunControlSession) -> PlannerPublicMap {
    let RunMapComparisonTopologyV1 {
        current_x,
        current_y,
        boss,
        mut nodes,
    } = run_map_comparison_topology_v1(session);
    let incoming = nodes
        .iter()
        .flat_map(|node| node.edges.iter())
        .map(|edge| (edge.destination_x, edge.destination_y))
        .collect::<BTreeSet<_>>();
    // DungeonMapScreen.updateImage displays nodes with edges. A referenced
    // terminal or current node also remains explicit in the navigation graph:
    // e.g. the Act 4 boss, or a current TrueVictoryRoom with no outgoing edge.
    nodes.retain(|node| {
        !node.edges.is_empty()
            || incoming.contains(&(node.x, node.y))
            || (node.x, node.y) == (current_x, current_y)
    });

    let standard_act_map =
        matches!(session.run_state.act_num, 1..=3) && session.run_state.map.graph.len() == 15;
    if standard_act_map {
        let mut publishes_boss = current_y == 15;
        for node in &mut nodes {
            if node.y == 14 {
                // The normal action owner exposes SelectMapNode(0) at y=15.
                // MapGenerator's (3,16) endpoint is just the drawn boss anchor;
                // it must not become an Unknown navigation destination. This
                // changes no raw edge or map comparison evidence.
                node.edges = vec![PlannerPublicMapEdge {
                    destination_x: 0,
                    destination_y: 15,
                }];
                publishes_boss = true;
            }
        }
        if publishes_boss && !nodes.iter().any(|node| node.x == 0 && node.y == 15) {
            nodes.push(boss_node());
        }
    }
    PlannerPublicMap {
        current_x,
        current_y,
        boss,
        nodes,
    }
}

fn boss_node() -> PlannerPublicMapNode {
    PlannerPublicMapNode {
        x: 0,
        y: 15,
        room: PlannerMapRoom::Boss,
        has_emerald_key: false,
        edges: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
