//! Explicit short-route intervention; ordinary movement and settlement thereafter.
use super::*;
use crate::local::ConstructedCombat;
use sts_core::content::{
    cards::CardId, monsters::factory::EncounterId, potions::PotionId, relics::RelicId,
};
use sts_core::state::map::{MapEdge, MapRoomNode, MapState, Point};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentRoom {
    pub encounter: EncounterId,
    pub room: RoomType,
}

/// Constructed Act 1 corridor, not a naturally acquired loadout or map prefix.
/// Rewards are generated normally and require explicit choices. No setup or
/// inventory intervention occurs after initialization.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentRecipe {
    pub hp: i32,
    pub deck: Vec<(CardId, u8)>,
    pub relics: Vec<RelicId>,
    pub potions: Vec<PotionId>,
    pub route: Vec<SegmentRoom>,
}

impl NaturalEpisode {
    pub fn from_segment(
        seed: u64,
        max_actions: usize,
        recipe: &SegmentRecipe,
    ) -> Result<Self, String> {
        use EncounterId::*;
        if !(2..=8).contains(&recipe.route.len()) {
            return Err("supplied segment requires 2..8 rooms".into());
        }
        for r in &recipe.route {
            let valid = match r.room {
                RoomType::MonsterRoom => matches!(
                    r.encounter,
                    Cultist
                        | JawWorm
                        | TwoLouse
                        | SmallSlimes
                        | BlueSlaver
                        | Looter
                        | GremlinGang
                        | RedSlaver
                        | LargeSlime
                        | ExordiumThugs
                        | ExordiumWildlife
                        | ThreeLouse
                        | TwoFungiBeasts
                        | LotsOfSlimes
                ),
                RoomType::MonsterRoomElite => {
                    matches!(r.encounter, GremlinNob | Lagavulin | ThreeSentries)
                }
                _ => false,
            };
            if !valid {
                return Err("supplied segment requires matching Act 1 room encounters".into());
            }
        }
        let first = &recipe.route[0];
        let mut run = sts_core::state::run::with_suppressed_obtain_logs(|| {
            ConstructedCombat {
                seed,
                encounter: first.encounter,
                act: 1,
                room: first.room,
                hp: recipe.hp,
                deck: recipe.deck.clone(),
                relics: recipe.relics.clone(),
                potions: recipe.potions.clone(),
                max_potion_uses: None,
            }
            .build_run()
        })?;
        // Replace only the declared initial map and encounter schedule. Keep the
        // generated RNG streams/pools. Floor transitions still reseed exactly as
        // the shared run loop specifies; they are not reset by this experiment.
        run.map = MapState::new(
            recipe
                .route
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let mut n = MapRoomNode::new(0, i as i32);
                    n.class = Some(r.room);
                    if i > 0 {
                        n.parents.push(Point::new(0, i - 1));
                    }
                    if i + 1 < recipe.route.len() {
                        n.edges.insert(MapEdge::new(0, i as i32, 0, i as i32 + 1));
                    }
                    vec![n]
                })
                .collect(),
        );
        run.monster_list = recipe
            .route
            .iter()
            .filter(|r| r.room == RoomType::MonsterRoom)
            .map(|r| r.encounter)
            .collect();
        run.elite_monster_list = recipe
            .route
            .iter()
            .filter(|r| r.room == RoomType::MonsterRoomElite)
            .map(|r| r.encounter)
            .collect();
        run.event_state = None;
        let mut episode = Self::from_initial_run(run, max_actions)?;
        episode.engine = EngineState::MapNavigation;
        episode.segment = Some(recipe.clone());
        Ok(episode)
    }
}

#[cfg(test)]
mod tests;
