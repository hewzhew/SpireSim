use super::*;
use crate::runtime::combat::CardBlueprint;

#[test]
fn cost_upgrades_apply_java_base_cost_and_turn_delta_rules() {
    // AbstractCard.upgradeBaseCost resets cost, preserves a positive turn
    // cost's delta, and leaves a zero turn cost at zero. BloodForBlood alone
    // requests cost - 1 below four, then clamps its actual cost to zero.
    for (id, modifier, turn, cost_after, turn_after) in [
        (CardId::Corruption, -3, None, 2, 2),
        (CardId::Corruption, 1, Some(5), 2, 3),
        (CardId::Corruption, -1, Some(0), 2, 0),
        (CardId::BodySlam, 2, Some(2), 0, 0),
        (CardId::BloodForBlood, -2, Some(3), 1, 2),
        (CardId::BloodForBlood, -4, Some(2), 0, 1),
        (CardId::BloodForBlood, 2, Some(6), 3, 3),
        (CardId::Strike, -1, Some(0), 0, 0),
    ] {
        let mut card = CombatCard::for_scenario(id, 1);
        card.cost_modifier = modifier;
        card.cost_for_turn = turn;
        card.free_to_play_once = true;
        let mut blueprint = CardBlueprint::from_stat_equivalent(&card);
        assert!(upgrade_card_once_java(&mut card));
        assert!(upgrade_card_blueprint_once_java(&mut blueprint));
        assert_eq!(
            card.combat_cost_without_turn_override_java(),
            cost_after,
            "{id:?} modifier={modifier} turn={turn:?}"
        );
        assert_eq!(
            card.cost_for_turn_java(),
            turn_after,
            "{id:?} modifier={modifier} turn={turn:?}"
        );
        assert!(card.free_to_play_once);
        assert_eq!(blueprint, CardBlueprint::from_stat_equivalent(&card));
        let after = card.clone();
        assert!(!upgrade_card_once_java(&mut card));
        assert_eq!(card, after);
    }
}

#[test]
fn colorless_cost_upgrades_follow_java_card_sources() {
    for id in [CardId::Magnetism, CardId::Mayhem, CardId::MindBlast] {
        let mut card = CombatCard::for_scenario(id, 1);
        assert_eq!(card.combat_cost_without_turn_override_java(), 2);
        assert!(upgrade_card_once_java(&mut card));
        assert_eq!(card.combat_cost_without_turn_override_java(), 1, "{id:?}");
    }
}
