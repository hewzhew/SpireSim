use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::potions::{Potion, PotionId};
use crate::runtime::combat::CombatCard;
use crate::sim::combat_action::CombatActionChoice;
use crate::test_support::{blank_test_combat, test_monster};

mod defensive_resource;
mod offensive;

fn attacking_monster() -> MonsterEntity {
    let mut monster = test_monster(EnemyId::Cultist);
    monster.set_planned_move_id(1);
    monster
}

#[test]
fn liquid_memories_without_a_discard_target_is_retained_only_as_low_policy_work() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![attacking_monster()];
    combat.entities.potions = vec![
        Some(Potion::with_affordance_truth(
            PotionId::LiquidMemories,
            1,
            true,
            true,
            false,
        )),
        None,
        None,
    ];
    let input = ClientInput::UsePotion {
        potion_index: 0,
        target: None,
    };

    assert!(!semantic_potion_action_allowed(&combat, &input));
    combat
        .zones
        .discard_pile
        .push(CombatCard::for_scenario(CardId::Strike, 11));
    assert!(semantic_potion_action_allowed(&combat, &input));
}
