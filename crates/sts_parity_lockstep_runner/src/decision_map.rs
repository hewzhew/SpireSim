//! A view of the existing public navigation graph. Never a route policy or
//! another map projection; unknown destinations and all branches remain visible.
use std::collections::BTreeMap;
use sts_oracle_run_control::{
    ai::planner_core::{PlannerMapRoom, PlannerPublicMap},
    content::identity::gameplay_display_label_v1,
};

pub(super) fn render(map: &PlannerPublicMap, act: u64) -> String {
    let mut lines = vec![format!(
        "Act {act} map: Boss {}",
        map.boss.map(gameplay_display_label_v1).unwrap_or("Unknown")
    )];
    if map.nodes.is_empty() {
        lines.push("Map unavailable".into());
        return lines.join("\n");
    }
    let nodes = map
        .nodes
        .iter()
        .map(|n| ((n.y, n.x), n))
        .collect::<BTreeMap<_, _>>();
    let coordinate = |x, y| format!("R{}C{}", y + 1, x + 1);
    lines.push(format!(
        "Current: {}",
        if map.current_y < 0 {
            "Entrance".into()
        } else {
            coordinate(map.current_x, map.current_y)
        }
    ));
    let mut rows = BTreeMap::<i32, Vec<String>>::new();
    for (&(y, x), node) in &nodes {
        let room = match node.room {
            PlannerMapRoom::QuestionMark => "?",
            PlannerMapRoom::Monster => "Monster",
            PlannerMapRoom::Elite => "Elite",
            PlannerMapRoom::Boss => "Boss",
            PlannerMapRoom::Rest => "Rest Site",
            PlannerMapRoom::Shop => "Shop",
            PlannerMapRoom::Treasure => "Treasure",
            PlannerMapRoom::Victory => "Victory",
        };
        let mut text = format!(
            "C{} {room}{}",
            x + 1,
            if node.has_emerald_key {
                " (Emerald key)"
            } else {
                ""
            }
        );
        let mut edges = node
            .edges
            .iter()
            .map(|e| (e.destination_y, e.destination_x))
            .collect::<Vec<_>>();
        edges.sort_unstable();
        if !edges.is_empty() {
            text.push_str(" -> ");
            text.push_str(
                &edges
                    .into_iter()
                    .map(|(y, x)| {
                        format!(
                            "{}{}",
                            coordinate(x, y),
                            if nodes.contains_key(&(y, x)) {
                                ""
                            } else {
                                " (Unknown)"
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        rows.entry(y).or_default().push(text);
    }
    lines.extend(
        rows.into_iter()
            .map(|(y, nodes)| format!("Row {}: {}", y + 1, nodes.join("; "))),
    );
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_run_control::ai::planner_core::{PlannerPublicMapEdge, PlannerPublicMapNode};

    #[test]
    fn reading_preserves_branches_merges_and_unpublished_destinations() {
        let node = |x, y, room, points: &[(i32, i32)]| PlannerPublicMapNode {
            x,
            y,
            room,
            has_emerald_key: false,
            edges: points
                .iter()
                .map(|&(x, y)| PlannerPublicMapEdge {
                    destination_x: x,
                    destination_y: y,
                })
                .collect(),
        };
        let mut map = PlannerPublicMap {
            current_x: -1,
            current_y: -1,
            boss: None,
            nodes: vec![
                node(0, 0, PlannerMapRoom::Monster, &[(0, 1), (2, 1)]),
                node(0, 1, PlannerMapRoom::Rest, &[(1, 2)]),
                node(2, 1, PlannerMapRoom::Elite, &[(1, 2)]),
                node(1, 2, PlannerMapRoom::QuestionMark, &[(1, 3)]),
            ],
        };
        let text = render(&map, 2);
        assert!(text.contains("C1 Monster -> R2C1, R2C3"));
        assert!(text.contains("C1 Rest Site -> R3C2; C3 Elite -> R3C2"));
        assert!(text.contains("C2 ? -> R4C2 (Unknown)"));
        map.nodes.reverse();
        assert_eq!(text, render(&map, 2));
    }
}
