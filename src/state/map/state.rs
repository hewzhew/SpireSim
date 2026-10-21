use super::node::{Map, RoomType};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MapState {
    pub graph: Map,
    /// current_y = -1 indicates the player has not selected a starting node at the bottom yet.
    /// current_y = 14 is the final visible map row; current_y = 15 is the synthetic boss room.
    pub current_y: i32,
    pub current_x: i32,
    pub boss_node_available: bool,
    pub has_emerald_key: bool,
}

impl MapState {
    pub fn checkpoint_externalized_placeholder() -> Self {
        Self {
            graph: Vec::new(),
            current_y: -1,
            current_x: -1,
            boss_node_available: false,
            has_emerald_key: false,
        }
    }

    pub fn is_checkpoint_externalized_placeholder(&self) -> bool {
        self.graph.is_empty()
            && self.current_y == -1
            && self.current_x == -1
            && !self.boss_node_available
            && !self.has_emerald_key
    }

    pub fn new(graph: Map) -> Self {
        Self {
            graph,
            current_y: -1,
            current_x: -1,
            boss_node_available: false,
            has_emerald_key: false, // In a real run, this is injected from the Act / Run manager
        }
    }

    /// Checks if the player can legally travel to the target point based on edges.
    /// If `has_flight` is true, allows traveling to any valid node on the next
    /// row reached by the current node's outgoing edges, not to arbitrary later
    /// rows.
    ///
    /// Java: `MapRoomNode.wingedIsConnectedTo()` checks `node.y == edge.dstY`
    /// and ignores `dstX` while Winged Greaves has charges.
    pub fn can_travel_to(&self, target_x: i32, target_y: i32, has_flight: bool) -> bool {
        // Handle initial map entry at y=0
        if self.current_y == -1 {
            if target_y == 0 && target_x >= 0 && (target_x as usize) < self.graph[0].len() {
                // You can enter any node at the bottom row (y=0) that natively has an edge or parent
                return !self.graph[0][target_x as usize].edges.is_empty();
            }
            return false;
        }

        // Handle normal edge traversal
        if target_y == self.current_y + 1
            && self.current_y >= 0
            && (self.current_y as usize) < self.graph.len()
        {
            let current_node = &self.graph[self.current_y as usize][self.current_x as usize];
            for edge in current_node.edges.iter() {
                if edge.dst_x == target_x && edge.dst_y == target_y {
                    return true;
                }
            }
        }

        // WingBoots flight: allow any valid node on a row the current node can
        // already reach vertically. Java compares only the target row to each
        // outgoing edge's dstY, but DungeonMapScreen only exposes map nodes
        // with outgoing edges. Rust represents the final campfire row without
        // explicit boss edges, so an incoming edge is its visibility proof.
        if has_flight
            && self.current_y >= 0
            && (self.current_y as usize) < self.graph.len()
            && target_y == self.current_y + 1
            && (target_y as usize) < self.graph.len()
            && target_x >= 0
            && (target_x as usize) < self.graph[target_y as usize].len()
        {
            let current_node = &self.graph[self.current_y as usize][self.current_x as usize];
            let target_node = &self.graph[target_y as usize][target_x as usize];
            let target_is_visible = !target_node.edges.is_empty()
                || (target_y == 14 && !target_node.parents.is_empty());
            if current_node.edges.iter().any(|edge| edge.dst_y == target_y) && target_is_visible {
                return true;
            }
        }

        // Handling Boss Node Phase
        if target_y == 15 && self.current_y == 14 {
            return true;
        }

        false
    }

    pub fn travel_to(
        &mut self,
        target_x: i32,
        target_y: i32,
        has_flight: bool,
    ) -> Result<(), &'static str> {
        if !self.can_travel_to(target_x, target_y, has_flight) {
            return Err("Invalid map traversal path");
        }
        self.current_y = target_y;
        self.current_x = target_x;
        Ok(())
    }

    pub fn get_current_room_type(&self) -> Option<RoomType> {
        if self.current_y == 15 {
            return Some(RoomType::MonsterRoomBoss);
        }
        self.get_current_node().and_then(|n| n.class)
    }

    pub fn boss_node_available_now(&self) -> bool {
        if self.current_y == 14 {
            return true;
        }
        let Some(current_node) = self.get_current_node() else {
            return false;
        };
        current_node.edges.iter().any(|edge| {
            self.graph
                .get(edge.dst_y.max(0) as usize)
                .and_then(|row| row.get(edge.dst_x.max(0) as usize))
                .is_some_and(|node| node.class == Some(RoomType::MonsterRoomBoss))
        })
    }

    pub fn get_current_node(&self) -> Option<&super::node::MapRoomNode> {
        if self.current_y >= 0
            && self.current_x >= 0
            && (self.current_y as usize) < self.graph.len()
        {
            let row = &self.graph[self.current_y as usize];
            if (self.current_x as usize) < row.len() {
                return Some(&row[self.current_x as usize]);
            }
        }
        None
    }

    pub fn set_current_room_type(&mut self, room_type: RoomType) -> Result<(), &'static str> {
        if self.current_y >= 0
            && self.current_x >= 0
            && (self.current_y as usize) < self.graph.len()
        {
            let row = &mut self.graph[self.current_y as usize];
            if (self.current_x as usize) < row.len() {
                row[self.current_x as usize].class = Some(room_type);
                return Ok(());
            }
        }
        Err("No current map node")
    }
}

impl Default for MapState {
    fn default() -> Self {
        Self::checkpoint_externalized_placeholder()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};

    fn node(x: i32, y: i32, class: Option<RoomType>) -> MapRoomNode {
        let mut node = MapRoomNode::new(x, y);
        node.class = class;
        node
    }

    #[test]
    fn wing_boots_matches_java_next_row_only_semantics() {
        let mut start = node(0, 0, Some(RoomType::MonsterRoom));
        start.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut normal_target = node(0, 1, Some(RoomType::MonsterRoom));
        normal_target.edges.insert(MapEdge::new(0, 1, 0, 2));
        let mut winged_target = node(1, 1, Some(RoomType::ShopRoom));
        winged_target.edges.insert(MapEdge::new(1, 1, 1, 2));

        let graph = vec![
            vec![start, node(1, 0, None)],
            vec![normal_target, winged_target],
            vec![
                node(0, 2, Some(RoomType::RestRoom)),
                node(1, 2, Some(RoomType::MonsterRoom)),
            ],
        ];
        let mut map = MapState::new(graph);
        map.current_x = 0;
        map.current_y = 0;

        assert!(map.can_travel_to(0, 1, false));
        assert!(!map.can_travel_to(1, 1, false));
        assert!(
            map.can_travel_to(1, 1, true),
            "Java Winged Greaves ignores dstX but keeps the target on an outgoing edge row"
        );
        assert!(
            !map.can_travel_to(0, 2, true),
            "Java Winged Greaves does not skip arbitrary future rows"
        );
    }

    #[test]
    fn wing_boots_cannot_land_on_an_invisible_map_placeholder() {
        let mut start = node(0, 7, Some(RoomType::MonsterRoom));
        start.edges.insert(MapEdge::new(0, 7, 3, 8));
        let mut visible_chest = node(3, 8, Some(RoomType::TreasureRoom));
        visible_chest.edges.insert(MapEdge::new(3, 8, 3, 9));
        let graph = vec![
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![start],
            vec![
                node(0, 8, Some(RoomType::TreasureRoom)),
                node(1, 8, Some(RoomType::TreasureRoom)),
                node(2, 8, Some(RoomType::TreasureRoom)),
                visible_chest,
            ],
        ];
        let mut map = MapState::new(graph);
        map.current_x = 0;
        map.current_y = 7;

        assert!(map.can_travel_to(3, 8, false));
        assert!(
            !map.can_travel_to(0, 8, true),
            "Java only offers nodes present in DungeonMapScreen.visibleMapNodes"
        );
    }

    #[test]
    fn boss_node_availability_is_derived_from_java_map_position() {
        let mut normal = MapState::new(vec![vec![node(0, 14, Some(RoomType::RestRoom))]]);
        normal.current_x = 0;
        normal.current_y = 14;
        assert!(
            normal.boss_node_available_now(),
            "Java DungeonMap makes the boss hitbox available from row 14"
        );

        let mut elite = node(0, 2, Some(RoomType::MonsterRoomElite));
        elite.edges.insert(MapEdge::new(0, 2, 0, 3));
        let ending = vec![
            vec![node(0, 0, Some(RoomType::RestRoom))],
            vec![node(0, 1, Some(RoomType::ShopRoom))],
            vec![elite],
            vec![node(0, 3, Some(RoomType::MonsterRoomBoss))],
        ];
        let mut ending_map = MapState::new(ending);
        ending_map.current_x = 0;
        ending_map.current_y = 2;
        assert!(
            ending_map.boss_node_available_now(),
            "Java TheEnding also exposes the boss hitbox from the Shield/Spear node"
        );
    }
}
