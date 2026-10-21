use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Hash, Copy)]
pub enum RoomType {
    EventRoom,
    MonsterRoom,
    MonsterRoomElite,
    MonsterRoomBoss,
    RestRoom,
    ShopRoom,
    TreasureRoom,
    TrueVictoryRoom,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct Point {
    pub x: usize,
    pub y: usize,
}

impl Point {
    pub fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }

    /// Index into a map grid to get the node at this point.
    pub fn node<'a>(&self, map: &'a Map) -> &'a MapRoomNode {
        &map[self.y][self.x]
    }

    /// Get the parent points of this node in the map.
    pub fn parents<'a>(&self, map: &'a Map) -> &'a Vec<Point> {
        &map[self.y][self.x].parents
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct MapEdge {
    pub src_x: i32,
    pub src_y: i32,
    pub dst_x: i32,
    pub dst_y: i32,
}

impl MapEdge {
    pub fn new(src_x: i32, src_y: i32, dst_x: i32, dst_y: i32) -> Self {
        Self {
            src_x,
            src_y,
            dst_x,
            dst_y,
        }
    }
}

impl PartialOrd for MapEdge {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MapEdge {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.dst_x.cmp(&other.dst_x) {
            Ordering::Equal => self.dst_y.cmp(&other.dst_y),
            res => res,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MapRoomNode {
    pub x: i32,
    pub y: i32,
    pub class: Option<RoomType>,
    #[serde(default)]
    pub has_emerald_key: bool,

    #[serde(default)]
    pub edges: BTreeSet<MapEdge>,
    #[serde(default)]
    pub parents: Vec<Point>,
}

impl PartialEq for MapRoomNode {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}
impl Eq for MapRoomNode {}

impl MapRoomNode {
    pub fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            class: None,
            has_emerald_key: false,
            edges: BTreeSet::new(),
            parents: vec![],
        }
    }
}

pub type Map = Vec<Vec<MapRoomNode>>;
