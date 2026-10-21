//! Explicit, bounded constructed first-act boss campfires for condition diagnostics.
//! These are not natural run checkpoints or sampled trajectories.

use super::constructed_strategic::constructed_first_act_session_v1;
use super::{ConstructedFirstActLoadoutV1, LearningEnvV3};
use crate::state::core::EngineState;
use crate::state::map::{MapRoomNode, MapState, RoomType};

pub fn constructed_campfire_env_v1(
    recipe: &ConstructedFirstActLoadoutV1,
) -> Result<LearningEnvV3, String> {
    let mut session = constructed_first_act_session_v1(recipe)?;
    session.engine_state = EngineState::Campfire;
    let run = &mut session.run_state;
    run.floor_num = 15;
    let mut graph = vec![vec![]; 15];
    graph[14] = vec![MapRoomNode::new(0, 14)];
    graph[14][0].class = Some(RoomType::RestRoom);
    run.map = MapState::new(graph);
    run.map.current_x = 0;
    run.map.current_y = 14;
    run.map.boss_node_available = true;
    Ok(LearningEnvV3::from_session(session))
}

#[cfg(test)]
mod tests {
    use super::super::ConstructedStrategicCardV1;
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::relics::RelicId;
    use crate::engine::campfire_candidates::{legal_campfire_candidates, CampfireCandidate};

    fn recipe() -> ConstructedFirstActLoadoutV1 {
        ConstructedFirstActLoadoutV1 {
            seed: 1,
            ascension_level: 20,
            current_hp: 10,
            max_hp: 78,
            gold: 0,
            boss: EncounterId::Hexaghost,
            cards: [CardId::Bash, CardId::LimitBreak]
                .into_iter()
                .map(|id| ConstructedStrategicCardV1 {
                    id,
                    upgrades: 0,
                    misc_value: None,
                    base_damage_override: None,
                    base_block_override: None,
                    cost_modifier: 0,
                })
                .collect(),
            relics: vec![],
            potions: vec![],
        }
    }

    #[test]
    fn constructed_campfire_uses_real_candidate_gates_and_observation() {
        let mut recipe = recipe();
        recipe.relics = vec![RelicId::CoffeeDripper];
        let env = constructed_campfire_env_v1(&recipe).unwrap();
        assert!(matches!(
            env.observe().unwrap(),
            super::super::LearningBoundaryV1::Strategic { .. }
        ));
        let run = env.session_checkpoint().into_session().unwrap().run_state;
        assert!(!legal_campfire_candidates(&run)
            .iter()
            .any(|c| matches!(c, CampfireCandidate::Rest)));
        recipe.relics = vec![RelicId::FusionHammer];
        let env = constructed_campfire_env_v1(&recipe).unwrap();
        let run = env.session_checkpoint().into_session().unwrap().run_state;
        assert!(!legal_campfire_candidates(&run)
            .iter()
            .any(|c| matches!(c, CampfireCandidate::Smith { .. })));
    }

    #[test]
    fn constructed_campfire_rejects_invalid_or_unbounded_recipes() {
        let mut recipe = recipe();
        recipe.current_hp = 79;
        assert!(constructed_campfire_env_v1(&recipe).is_err());
        recipe.current_hp = 10;
        recipe.cards[0].upgrades = 2;
        assert!(constructed_campfire_env_v1(&recipe).is_err());
        recipe.cards[0].upgrades = 0;
        recipe.boss = EncounterId::TheHeart;
        assert!(constructed_campfire_env_v1(&recipe).is_err());
    }

    #[test]
    fn constructed_campfire_retains_explicit_public_gold() {
        let mut recipe = recipe();
        recipe.gold = 217;
        let env = constructed_campfire_env_v1(&recipe).unwrap();
        let run = env.session_checkpoint().into_session().unwrap().run_state;
        assert_eq!(run.gold, 217);
        for gold in [-1, 1_000_001] {
            recipe.gold = gold;
            assert!(constructed_campfire_env_v1(&recipe).is_err());
        }
    }

    #[test]
    fn constructed_inventory_preserves_slots_and_core_potion_belt_capacity() {
        use crate::content::potions::PotionId;
        let mut recipe = recipe();
        recipe.potions = vec![None, Some(PotionId::EnergyPotion)];
        let env = constructed_campfire_env_v1(&recipe).unwrap();
        let run = env.session_checkpoint().into_session().unwrap().run_state;
        assert_eq!(run.potions.len(), 2);
        assert!(run.potions[0].is_none());
        assert_eq!(run.potions[1].as_ref().unwrap().id, PotionId::EnergyPotion);
        recipe.potions.push(Some(PotionId::FirePotion));
        assert!(constructed_campfire_env_v1(&recipe).is_err());
        recipe.relics.push(RelicId::PotionBelt);
        let env = constructed_campfire_env_v1(&recipe).unwrap();
        let run = env.session_checkpoint().into_session().unwrap().run_state;
        assert_eq!(run.potions.len(), 4);
        assert_eq!(run.potions[2].as_ref().unwrap().id, PotionId::FirePotion);
        assert!(run.potions[3].is_none());
        recipe.potions.extend([None, None]);
        assert!(constructed_campfire_env_v1(&recipe).is_err());
    }
}
