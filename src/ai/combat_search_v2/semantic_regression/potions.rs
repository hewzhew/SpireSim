use super::*;

#[test]
fn stepper_fruit_juice_consumes_slot_and_increases_hp_once() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 13;
    combat.entities.player.max_hp = 87;
    combat.entities.monsters = vec![monster(EnemyId::JawWorm, 10, 0, 30)];
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::FruitJuice, 300)),
        Some(Potion::new(PotionId::FirePotion, 301)),
        None,
    ];

    let step = apply_from_player_turn(
        combat,
        ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    );

    assert_stable_player_turn(&step);
    assert_eq!(step.position.combat.entities.player.current_hp, 18);
    assert_eq!(step.position.combat.entities.player.max_hp, 92);
    assert!(
        step.position.combat.entities.potions[0].is_none(),
        "the used Fruit Juice slot must be empty after the max-hp effect resolves"
    );
    assert_eq!(
        step.position.combat.entities.potions[1]
            .as_ref()
            .map(|potion| potion.id),
        Some(PotionId::FirePotion)
    );
}
