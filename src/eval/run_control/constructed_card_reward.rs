//! Explicit first-act card rewards for condition diagnostics, not natural runs.

use serde::{Deserialize, Serialize};

use super::constructed_strategic::{
    apply_constructed_upgrades_v1, constructed_first_act_session_v1,
};
use super::{ConstructedFirstActLoadoutV1, LearningEnvV3};
use crate::content::cards::{
    colorless_pool_for_rarity, get_card_definition, ironclad_pool_for_rarity, CardId,
};
use crate::runtime::combat::CombatCard;
use crate::state::core::EngineState;
use crate::state::map::{MapEdge, MapRoomNode, MapState, Point, RoomType};
use crate::state::rewards::{RewardCard, RewardItem, RewardState};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedCardRewardRecipeV1 {
    pub loadout: ConstructedFirstActLoadoutV1,
    pub floor: u8,
    pub offers: Vec<ConstructedRewardOfferV1>,
    /// Explicit synthetic room sequence at map y0..14, ending in a campfire.
    /// Omission preserves the original ordinary-fight corridor exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corridor_rooms: Option<Vec<RoomType>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedRewardOfferV1 {
    pub id: CardId,
    #[serde(default)]
    pub upgrades: u8,
}

pub fn constructed_card_reward_env_v1(
    recipe: &ConstructedCardRewardRecipeV1,
) -> Result<LearningEnvV3, String> {
    if !(1..=14).contains(&recipe.floor) || !(1..=8).contains(&recipe.offers.len()) {
        return Err("constructed reward requires floor 1..14 and 1..8 offers".into());
    }
    if let Some(rooms) = &recipe.corridor_rooms {
        if rooms.len() != 15
            || rooms.last() != Some(&RoomType::RestRoom)
            || rooms
                .iter()
                .any(|room| matches!(room, RoomType::MonsterRoomBoss | RoomType::TrueVictoryRoom))
        {
            return Err(
                "constructed reward corridor requires 15 ordinary-act rooms ending in RestRoom"
                    .into(),
            );
        }
    }
    for (slot, offer) in recipe.offers.iter().enumerate() {
        let rarity = get_card_definition(offer.id).rarity;
        if !(ironclad_pool_for_rarity(rarity).contains(&offer.id)
            || colorless_pool_for_rarity(rarity).contains(&offer.id))
        {
            return Err("constructed Ironclad reward requires red or colorless offers".into());
        }
        apply_constructed_upgrades_v1(
            &mut CombatCard::for_scenario(offer.id, slot as u32),
            offer.upgrades,
        )?;
    }
    let mut session = constructed_first_act_session_v1(&recipe.loadout)?;
    let offers: Vec<_> = recipe
        .offers
        .iter()
        .map(|card| RewardCard::new(card.id, card.upgrades))
        .collect();
    let mut reward = RewardState::new();
    reward.items = vec![RewardItem::Card {
        cards: offers.clone(),
    }];
    reward.pending_card_choice = Some(offers);
    reward.pending_card_reward_index = Some(0);
    session.engine_state = EngineState::RewardScreen(reward);
    let run = &mut session.run_state;
    run.floor_num = i32::from(recipe.floor);
    // Declared synthetic corridor: caller rooms, or ordinary fights then F15 rest.
    // No route, event outcome, or combat history is attributed to a natural run.
    let graph: Vec<_> = (0..15)
        .map(|y| {
            let mut node = MapRoomNode::new(0, y);
            node.class = Some(recipe.corridor_rooms.as_ref().map_or_else(
                || {
                    if y == 14 {
                        RoomType::RestRoom
                    } else {
                        RoomType::MonsterRoom
                    }
                },
                |rooms| rooms[y as usize],
            ));
            if y < 14 {
                node.edges.insert(MapEdge::new(0, y, 0, y + 1));
            }
            if y > 0 {
                node.parents.push(Point::new(0, (y - 1) as usize));
            }
            vec![node]
        })
        .collect();
    run.map = MapState::new(graph);
    run.map.current_x = 0;
    run.map.current_y = i32::from(recipe.floor - 1);
    Ok(LearningEnvV3::from_session(session))
}
