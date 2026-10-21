//! Read-only tactical summaries for the current hand and next draw window.
//!
//! These facts describe one exact combat state. They do not decide legality,
//! rank actions, expand search nodes, or claim an outcome.

use sts_core::content::cards::{get_card_definition, is_ethereal, is_self_retain};
use sts_core::content::relics::RelicId;
use sts_core::engine::core::{
    compute_player_turn_start_draw_count, player_skips_ordinary_end_turn_discard,
};
use sts_core::runtime::combat::{CombatCard, CombatState};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatTacticalCardPileValueV1 {
    pub damage: i32,
    pub block: i32,
    pub playable_cards: i32,
    pub low_cost: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatTacticalChokerCapacityV1 {
    pub has_velvet_choker: bool,
    pub cards_played_this_turn: u8,
    pub remaining_slots: Option<u8>,
    pub affordable_hand_cards: u8,
    pub representable_affordable_cards: u8,
    pub stranded_affordable_cards: u8,
}

pub fn combat_tactical_hand_value_v1(combat: &CombatState) -> CombatTacticalCardPileValueV1 {
    let mut value = card_pile_value(combat.zones.hand.iter(), combat.turn.energy as i32);
    value.playable_cards =
        combat_tactical_choker_capacity_v1(combat).representable_affordable_cards as i32;
    value
}

pub fn combat_tactical_choker_capacity_v1(combat: &CombatState) -> CombatTacticalChokerCapacityV1 {
    let choker = combat
        .entities
        .player
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::VelvetChoker);
    let has_velvet_choker = choker.is_some();
    let cards_played_this_turn = combat.turn.counters.cards_played_this_turn;
    let remaining_slots = choker.map(|relic| {
        sts_core::content::relics::velvet_choker::remaining_card_plays(relic.counter)
            .min(u8::MAX as u32) as u8
    });
    let affordable_hand_cards = combat
        .zones
        .hand
        .iter()
        .filter(|card| {
            let cost = card.cost_for_turn_java();
            cost >= 0 && cost <= combat.turn.energy as i32
        })
        .count()
        .min(u8::MAX as usize) as u8;
    let representable_affordable_cards = remaining_slots.map_or(affordable_hand_cards, |slots| {
        affordable_hand_cards.min(slots)
    });
    CombatTacticalChokerCapacityV1 {
        has_velvet_choker,
        cards_played_this_turn,
        remaining_slots,
        affordable_hand_cards,
        representable_affordable_cards,
        stranded_affordable_cards: affordable_hand_cards
            .saturating_sub(representable_affordable_cards),
    }
}

pub fn combat_tactical_next_draw_value_v1(combat: &CombatState) -> CombatTacticalCardPileValueV1 {
    let requested = compute_player_turn_start_draw_count(combat).max(0) as usize;
    let retained = projected_retained_hand_count(combat);
    let hand_capacity = 10usize.saturating_sub(retained);
    let draw_count = requested
        .min(hand_capacity)
        .min(combat.zones.draw_pile.len());
    card_pile_value(
        combat.zones.draw_pile.iter().take(draw_count),
        combat.entities.player.energy_master as i32,
    )
}

fn projected_retained_hand_count(combat: &CombatState) -> usize {
    let skip_discard = player_skips_ordinary_end_turn_discard(combat);
    combat
        .zones
        .hand
        .iter()
        .filter(|card| {
            let explicitly_retained = card.retain_override == Some(true) || is_self_retain(card);
            explicitly_retained || (skip_discard && !is_ethereal(card))
        })
        .count()
}

fn card_pile_value<'a>(
    cards: impl Iterator<Item = &'a CombatCard>,
    playable_energy: i32,
) -> CombatTacticalCardPileValueV1 {
    cards.fold(
        CombatTacticalCardPileValueV1::default(),
        |mut value, card| {
            let def = get_card_definition(card.id);
            let cost = card.cost_for_turn_java();
            if cost >= 0 && cost <= playable_energy {
                value.playable_cards += 1;
            }
            value.low_cost -= cost.max(0);
            value.damage += card
                .base_damage_override
                .unwrap_or(def.base_damage + def.upgrade_damage * card.upgrades as i32)
                .max(0);
            value.block += card
                .base_block_override
                .unwrap_or(def.base_block + def.upgrade_block * card.upgrades as i32)
                .max(0);
            value
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::cards::CardId;
    use sts_core::content::relics::{RelicId, RelicState};
    use sts_core::test_support::blank_test_combat;

    #[test]
    fn next_draw_value_uses_turn_start_draw_modifier_and_next_turn_energy() {
        let mut combat = blank_test_combat();
        combat.turn.energy = 0;
        combat.entities.player.energy_master = 3;
        combat.turn.turn_start_draw_modifier = -4;
        combat.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Carnage, 12),
        ])
        .into();

        let value = combat_tactical_next_draw_value_v1(&combat);

        assert_eq!(value.damage, 6);
        assert_eq!(value.playable_cards, 1);
    }

    #[test]
    fn hand_value_uses_current_turn_energy_for_playability() {
        let mut combat = blank_test_combat();
        combat.turn.energy = 1;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Carnage, 12),
        ];

        let value = combat_tactical_hand_value_v1(&combat);

        assert_eq!(value.damage, 26);
        assert_eq!(value.playable_cards, 1);
    }

    #[test]
    fn choker_capacity_reports_affordable_cards_stranded_by_the_cap() {
        let mut combat = blank_test_combat();
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::VelvetChoker));
        combat.turn.energy = 3;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Strike, 12),
            CombatCard::for_scenario(CardId::Strike, 13),
        ];
        combat.turn.counters.cards_played_this_turn = 5;
        combat.entities.player.relics[0].counter = 5;

        let capacity = combat_tactical_choker_capacity_v1(&combat);

        assert!(capacity.has_velvet_choker);
        assert_eq!(capacity.cards_played_this_turn, 5);
        assert_eq!(capacity.remaining_slots, Some(1));
        assert_eq!(capacity.affordable_hand_cards, 3);
        assert_eq!(capacity.representable_affordable_cards, 1);
        assert_eq!(capacity.stranded_affordable_cards, 2);
        assert_eq!(combat_tactical_hand_value_v1(&combat).playable_cards, 1);
    }

    #[test]
    fn choker_capacity_is_not_binding_below_remaining_slots() {
        let mut combat = blank_test_combat();
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::VelvetChoker));
        combat.turn.energy = 1;
        combat.turn.counters.cards_played_this_turn = 4;
        combat.entities.player.relics[0].counter = 4;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Carnage, 12),
        ];

        let capacity = combat_tactical_choker_capacity_v1(&combat);

        assert_eq!(capacity.remaining_slots, Some(2));
        assert_eq!(capacity.affordable_hand_cards, 1);
        assert_eq!(capacity.representable_affordable_cards, 1);
        assert_eq!(capacity.stranded_affordable_cards, 0);
    }

    #[test]
    fn choker_capacity_uses_relic_counter_without_rewriting_play_history() {
        let mut combat = blank_test_combat();
        let mut relic = RelicState::new(RelicId::VelvetChoker);
        relic.counter = 6;
        combat.entities.player.add_relic(relic);
        combat.turn.counters.cards_played_this_turn = 1;
        let capacity = combat_tactical_choker_capacity_v1(&combat);
        assert_eq!(capacity.cards_played_this_turn, 1);
        assert_eq!(capacity.remaining_slots, Some(0));
        combat.entities.player.relics[0].counter = -1;
        assert_eq!(
            combat_tactical_choker_capacity_v1(&combat).remaining_slots,
            Some(7)
        );
        assert_eq!(combat.turn.counters.cards_played_this_turn, 1);
    }

    #[test]
    fn absent_choker_reports_unbounded_capacity_without_stranding() {
        let mut combat = blank_test_combat();
        combat.turn.energy = 3;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Strike, 12),
        ];

        let capacity = combat_tactical_choker_capacity_v1(&combat);

        assert!(!capacity.has_velvet_choker);
        assert_eq!(capacity.remaining_slots, None);
        assert_eq!(capacity.affordable_hand_cards, 2);
        assert_eq!(capacity.representable_affordable_cards, 2);
        assert_eq!(capacity.stranded_affordable_cards, 0);
    }

    #[test]
    fn pyramid_retained_hand_caps_next_turn_draw() {
        let mut combat = blank_test_combat();
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicPyramid));
        combat.zones.hand = (0..8)
            .map(|index| CombatCard::for_scenario(CardId::Defend, 100 + index))
            .collect();
        combat.zones.draw_pile = ((0..5)
            .map(|index| CombatCard::for_scenario(CardId::Strike, 200 + index))
            .collect::<Vec<_>>())
        .into();

        let value = combat_tactical_next_draw_value_v1(&combat);

        assert_eq!(value.playable_cards, 2);
        assert_eq!(value.damage, 12);
    }

    #[test]
    fn ethereal_apparitions_release_pyramid_draw_capacity_unless_explicitly_retained() {
        let mut combat = blank_test_combat();
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicPyramid));
        combat.zones.hand = (0..5)
            .map(|index| CombatCard::for_scenario(CardId::Defend, 100 + index))
            .chain((0..3).map(|index| CombatCard::for_scenario(CardId::Apparition, 200 + index)))
            .collect();
        combat.zones.draw_pile = ((0..5)
            .map(|index| CombatCard::for_scenario(CardId::Strike, 300 + index))
            .collect::<Vec<_>>())
        .into();

        assert_eq!(combat_tactical_next_draw_value_v1(&combat).damage, 30);

        combat.zones.hand[5].retain_override = Some(true);
        assert_eq!(combat_tactical_next_draw_value_v1(&combat).damage, 24);
    }

    #[test]
    fn equilibrium_keeps_unmarked_cards_in_next_draw_capacity_projection() {
        let mut combat = blank_test_combat();
        sts_core::engine::action_handlers::powers::handle_apply_power(
            0,
            0,
            sts_core::content::powers::PowerId::Equilibrium,
            1,
            &mut combat,
        );
        combat.zones.hand = (0..8)
            .map(|index| CombatCard::for_scenario(CardId::Defend, 100 + index))
            .collect();
        combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::Apparition, 108));
        combat.zones.draw_pile = (0..5)
            .map(|index| CombatCard::for_scenario(CardId::Strike, 200 + index))
            .collect::<Vec<_>>()
            .into();
        assert_eq!(combat_tactical_next_draw_value_v1(&combat).damage, 12);
        combat.zones.hand[8].retain_override = Some(true);
        assert_eq!(combat_tactical_next_draw_value_v1(&combat).damage, 6);
    }
}
