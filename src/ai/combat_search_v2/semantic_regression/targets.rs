use super::*;

#[test]
fn stepper_legal_card_targets_exclude_zero_hp_dying_and_half_dead_monsters() {
    let mut combat = blank_test_combat();
    let mut zero_hp_leftover = monster(EnemyId::AcidSlimeM, 11, 0, 0);
    zero_hp_leftover.is_dying = false;
    zero_hp_leftover.half_dead = false;
    let mut dying_leftover = monster(EnemyId::AcidSlimeL, 12, 1, 0);
    dying_leftover.is_dying = true;
    let mut half_dead_leftover = monster(EnemyId::Darkling, 13, 2, 1);
    half_dead_leftover.half_dead = true;
    let alive = monster(EnemyId::JawWorm, 14, 3, 25);
    combat.entities.monsters = vec![zero_hp_leftover, dying_leftover, half_dead_leftover, alive];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];

    let target_actions = legal_actions(&player_turn_position(combat))
        .into_iter()
        .filter_map(|input| match input {
            ClientInput::PlayCard { card_index, target } => Some((card_index, target)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        target_actions,
        vec![(0, Some(14))],
        "search-facing legal actions must not target dead/split-leftover monsters"
    );
}
