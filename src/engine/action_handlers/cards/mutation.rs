use crate::content::cards::CardId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CardLineageId, CombatCard, CombatState};

pub fn handle_apply_bullet_time(state: &mut CombatState) {
    for card in &mut state.zones.hand {
        card.set_cost_for_turn_java(0);
    }
}

pub fn handle_reduce_all_hand_costs(amount: u8, state: &mut CombatState) {
    for card in state.zones.hand.iter_mut() {
        if card.cost_for_turn_java() > 0 {
            let current = card.cost_for_turn_java();
            card.set_cost_for_turn_java(current - amount as i32);
        }
    }
}

pub fn handle_reduce_retained_hand_costs(amount: i32, state: &mut CombatState) {
    if amount <= 0 {
        return;
    }
    for card in state.zones.hand.iter_mut() {
        if card.retain_override == Some(true) || crate::content::cards::is_self_retain(card) {
            card.modify_cost_for_combat_java(-amount);
        }
    }
}

pub fn handle_enlightenment(permanent: bool, state: &mut CombatState) {
    for card in state.zones.hand.iter_mut() {
        let base_cost = card.base_cost_for_combat_java();
        if base_cost < 0 {
            continue;
        }

        let current = card.cost_for_turn_java();
        if current > 1 {
            card.set_cost_for_turn_java(1);
        }

        if permanent && base_cost > 1 {
            card.set_combat_cost_preserving_turn_java(1);
        }
    }
}

pub fn handle_madness(state: &mut CombatState) {
    let better_possible = state
        .zones
        .hand
        .iter()
        .any(|card| card.cost_for_turn_java() > 0);
    if !better_possible
        && !state
            .zones
            .hand
            .iter()
            .any(|card| card.combat_cost_without_turn_override_java() > 0)
    {
        return;
    }

    // Java MadnessAction retries CardGroup.getRandomCard over the entire hand.
    // Filtering first changes both the chosen card and the RNG call sequence.
    loop {
        let pick = state
            .rng
            .card_random_rng
            .random(state.zones.hand.len() as i32 - 1) as usize;
        let card = &mut state.zones.hand[pick];
        let eligible = if better_possible {
            card.cost_for_turn_java() > 0
        } else {
            card.combat_cost_without_turn_override_java() > 0
        };
        if eligible {
            card.set_combat_and_turn_cost_java(0);
            break;
        }
    }
}

pub fn handle_upgrade_all_in_hand(state: &mut CombatState) {
    for card in state.zones.hand.iter_mut() {
        crate::content::cards::upgrade_card_once_java(card);
    }
}

pub fn handle_upgrade_all_cards_in_combat(state: &mut CombatState) {
    for card in state
        .zones
        .hand
        .iter_mut()
        .chain(state.zones.draw_pile.iter_mut())
        .chain(state.zones.discard_pile.iter_mut())
        .chain(state.zones.exhaust_pile.iter_mut())
    {
        crate::content::cards::upgrade_card_once_java(card);
    }
}

pub fn handle_upgrade_all_burns(state: &mut CombatState) {
    for card in state
        .zones
        .draw_pile
        .iter_mut()
        .chain(state.zones.discard_pile.iter_mut())
    {
        if card.id == CardId::Burn {
            card.upgrades += 1;
        }
    }
}

pub fn handle_upgrade_card(card_lineage: CardLineageId, state: &mut CombatState) {
    for card in state
        .zones
        .hand
        .iter_mut()
        .chain(state.zones.draw_pile.iter_mut())
        .chain(state.zones.discard_pile.iter_mut())
    {
        if card.lineage == card_lineage {
            crate::content::cards::upgrade_card_once_java(card);
            break;
        }
    }
}

pub fn handle_upgrade_random_card(state: &mut CombatState) {
    let upgradeable_lineages: Vec<CardLineageId> = state
        .zones
        .hand
        .iter()
        .filter(|c| crate::content::cards::can_upgrade_card_once(c))
        .map(|c| c.lineage)
        .collect();
    if !upgradeable_lineages.is_empty() {
        let mut shuffled = upgradeable_lineages;
        crate::runtime::rng::shuffle_with_random_long(&mut shuffled, &mut state.rng.shuffle_rng);
        let target_lineage = shuffled[0];
        if let Some(card) = state
            .zones
            .hand
            .iter_mut()
            .find(|c| c.lineage == target_lineage)
        {
            crate::content::cards::upgrade_card_once_java(card);
        }
    }
}

pub fn handle_modify_card_misc(card_lineage: CardLineageId, amount: i32, state: &mut CombatState) {
    // Java IncreaseMiscAction first looks up the UUID in the master deck.
    // Stat-equivalent combat copies have fresh identities: they still grow in
    // battle, but must not create a permanent mutation for their source card.
    if state
        .meta
        .master_deck_snapshot
        .iter()
        .any(|card| card.lineage == card_lineage)
    {
        state.meta.master_deck_snapshot.with_cards_mut(|cards| {
            let original = cards
                .iter_mut()
                .find(|card| card.lineage == card_lineage)
                .expect("master-deck instance was just found");
            original.misc_value += amount;
        });
        state
            .meta
            .meta_changes
            .push(crate::runtime::combat::MetaChange::ModifyCardMisc {
                card_lineage,
                amount,
            });
    }

    for_each_battle_instance_and_replay_mut(state, card_lineage, |card| {
        card.misc_value += amount;
    });
}

pub fn handle_modify_card_damage(
    card_lineage: CardLineageId,
    amount: i32,
    state: &mut CombatState,
) {
    let apply = |card: &mut crate::runtime::combat::CombatCard| {
        let def = crate::content::cards::get_card_definition(card.id);
        let upgraded_base = def.base_damage + (card.upgrades as i32) * def.upgrade_damage;
        let current = card.base_damage_override.unwrap_or(upgraded_base);
        card.base_damage_override = Some((current + amount).max(0));
    };
    for_each_battle_instance_and_replay_mut(state, card_lineage, apply);
}

fn for_each_battle_instance_and_replay_mut(
    state: &mut CombatState,
    card_lineage: CardLineageId,
    mut apply: impl FnMut(&mut CombatCard),
) {
    state
        .zones
        .for_each_java_battle_instance_mut_by_lineage(card_lineage, &mut apply);
    state
        .zones
        .for_each_queued_instance_mut_by_lineage(card_lineage, &mut apply);
    // Java places replay copies in limbo during onUseCard, before the
    // original damage/misc growth settles. Rust holds those same-instance copies
    // in a deferred enqueue action until the original actions drain.
    // They are already battle instances, so growth must reach them here too.
    for action in &mut state.engine.action_queue {
        if let Action::EnqueueCardPlay { item, .. } = action {
            if item.card.lineage == card_lineage {
                apply(&mut item.card);
            }
        }
    }
}

pub fn handle_gash(card_lineage: CardLineageId, amount: i32, state: &mut CombatState) {
    let apply = |card: &mut crate::runtime::combat::CombatCard| {
        let def = crate::content::cards::get_card_definition(card.id);
        let upgraded_base = def.base_damage + (card.upgrades as i32) * def.upgrade_damage;
        let current = card.base_damage_override.unwrap_or(upgraded_base);
        card.base_damage_override = Some((current + amount).max(0));
    };

    for card in state
        .zones
        .hand
        .iter_mut()
        .chain(state.zones.draw_pile.iter_mut())
        .chain(state.zones.discard_pile.iter_mut())
        .chain(state.zones.limbo.iter_mut())
    {
        if card.id == CardId::Claw || card.lineage == card_lineage {
            apply(card);
        }
    }
}

pub fn handle_modify_card_block(card_lineage: CardLineageId, amount: i32, state: &mut CombatState) {
    state
        .zones
        .for_each_java_battle_instance_mut_by_lineage(card_lineage, |card| {
            let def = crate::content::cards::get_card_definition(card.id);
            let upgraded_base = def.base_block + (card.upgrades as i32) * def.upgrade_block;
            let current = card.base_block_override.unwrap_or(upgraded_base);
            card.base_block_override = Some(current + amount);
        });
}

pub fn handle_reduce_card_cost_for_combat(
    card_lineage: CardLineageId,
    amount: i32,
    state: &mut CombatState,
) {
    if amount <= 0 {
        return;
    }
    let delta = -amount;
    state
        .zones
        .for_each_java_battle_instance_mut_by_lineage(card_lineage, |card| {
            card.modify_cost_for_combat_java(delta);
        });
    state
        .zones
        .for_each_queued_instance_mut_by_lineage(card_lineage, |card| {
            card.modify_cost_for_combat_java(delta);
        });
}

pub fn handle_randomize_hand_costs(state: &mut CombatState) {
    for card in state.zones.hand.iter_mut() {
        let current_cost = card.combat_cost_without_turn_override_java();
        if current_cost >= 0 {
            let new_cost = state.rng.card_random_rng.random(3);
            if current_cost != new_cost {
                card.set_combat_and_turn_cost_java(new_cost);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::CombatCard;
    use crate::runtime::rng::{begin_sts_rng_trace, StsRng, StsRngCall};

    #[test]
    fn misc_growth_distinguishes_temporary_copies_from_same_instance_replays() {
        for id in [CardId::RitualDagger, CardId::GeneticAlgorithm] {
            let mut state = crate::test_support::blank_test_combat();
            let original =
                CombatCard::with_lineage(id, state.meta.master_deck_lineage_allocator.allocate());
            let copy =
                original.make_stat_equivalent_copy_with_lineage(CardLineageId::combat_generated(0));
            state.meta.master_deck_snapshot = vec![original.clone()].into();
            state.zones.hand = vec![original.clone(), copy.clone()];
            state.zones.limbo = vec![original.make_same_instance_of_java()];

            handle_modify_card_misc(copy.lineage, 3, &mut state);

            assert!(state.meta.meta_changes.is_empty());
            assert_eq!(
                state.meta.master_deck_snapshot[0].misc_value,
                original.misc_value
            );
            assert_eq!(state.zones.hand[0].misc_value, original.misc_value);
            assert_eq!(state.zones.hand[1].misc_value, copy.misc_value + 3);
            assert_eq!(state.zones.limbo[0].misc_value, original.misc_value);

            handle_modify_card_misc(original.lineage, 2, &mut state);

            assert_eq!(
                state.meta.master_deck_snapshot[0].misc_value,
                original.misc_value + 2
            );
            assert_eq!(state.zones.hand[0].misc_value, original.misc_value + 2);
            assert_eq!(state.zones.limbo[0].misc_value, original.misc_value + 2);
            assert_eq!(state.zones.hand[1].misc_value, copy.misc_value + 3);
            assert_eq!(
                state.meta.meta_changes,
                vec![crate::runtime::combat::MetaChange::ModifyCardMisc {
                    card_lineage: original.lineage,
                    amount: 2,
                }]
            );
        }
    }

    #[test]
    fn madness_replays_java_rejected_hand_draws_before_modifying_the_target() {
        let mut state = crate::test_support::blank_test_combat();
        // Captured immediately before MadnessAction in the natural Time Eater
        // counterexample, after the played Madness leaves the hand. Java rejects
        // Battle Trance twice, then selects the second Defend, not Clothesline.
        state.zones.hand = [
            CardId::Defend,
            CardId::Clothesline,
            CardId::Whirlwind,
            CardId::Defend,
            CardId::FlameBarrier,
            CardId::AscendersBane,
            CardId::BattleTrance,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32))
        .collect();
        state.rng.card_random_rng = StsRng::from_state(0xBAB625AEC1436208, 0x6F57D431495630E8, 14);
        let before = state.zones.hand.clone();
        let trace = begin_sts_rng_trace().unwrap();

        handle_madness(&mut state);

        let calls: Vec<_> = trace
            .finish()
            .unwrap()
            .into_iter()
            .map(|e| e.call)
            .collect();
        assert_eq!(
            calls,
            [6, 6, 3].map(|result| StsRngCall::RandomInt { range: 6, result })
        );
        assert_eq!(state.rng.card_random_rng.counter, 17);
        for (i, card) in state.zones.hand.iter().enumerate() {
            assert_eq!(card.lineage, before[i].lineage);
            if i == 3 {
                assert_eq!(card.combat_cost_without_turn_override_java(), 0);
                assert_eq!(card.cost_for_turn_java(), 0);
            } else {
                assert_eq!(card, &before[i]);
            }
        }
    }

    #[test]
    fn madness_prefers_positive_turn_cost_over_a_temporarily_free_card() {
        let mut state = crate::test_support::blank_test_combat();
        let mut temporarily_free = CombatCard::for_scenario(CardId::Defend, 1);
        temporarily_free.set_cost_for_turn_java(0);
        let mut temporarily_expensive = CombatCard::for_scenario(CardId::Anger, 2);
        temporarily_expensive.set_cost_for_turn_java(2);
        state.zones.hand = vec![temporarily_free.clone(), temporarily_expensive];

        handle_madness(&mut state);

        assert_eq!(state.zones.hand[0], temporarily_free);
        assert_eq!(
            state.zones.hand[1].combat_cost_without_turn_override_java(),
            0
        );
        assert_eq!(state.zones.hand[1].cost_for_turn_java(), 0);
    }

    #[test]
    fn madness_fallback_uses_mutated_combat_cost_instead_of_printed_cost() {
        let mut state = crate::test_support::blank_test_combat();
        let mut already_free = CombatCard::for_scenario(CardId::Strike, 1);
        already_free.set_combat_and_turn_cost_java(0);
        let mut temporarily_free = CombatCard::for_scenario(CardId::Anger, 2);
        temporarily_free.set_combat_and_turn_cost_java(2);
        temporarily_free.set_cost_for_turn_java(0);
        state.zones.hand = vec![already_free.clone(), temporarily_free];

        handle_madness(&mut state);

        assert_eq!(state.zones.hand[0], already_free);
        assert_eq!(
            state.zones.hand[1].combat_cost_without_turn_override_java(),
            0
        );
        assert_eq!(state.zones.hand[1].cost_for_turn_java(), 0);
        assert!(state.rng.card_random_rng.counter > 0);
    }

    #[test]
    fn madness_without_an_eligible_card_does_not_consume_rng() {
        let mut state = crate::test_support::blank_test_combat();
        handle_madness(&mut state);
        assert_eq!(state.rng.card_random_rng.counter, 0);
        state.zones.hand = [
            CardId::Whirlwind,
            CardId::AscendersBane,
            CardId::Anger,
            CardId::Strike,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32))
        .collect();
        state.zones.hand[3].set_combat_and_turn_cost_java(0);
        let before = state.zones.hand.clone();

        handle_madness(&mut state);

        assert_eq!(state.zones.hand, before);
        assert_eq!(state.rng.card_random_rng.counter, 0);
    }
}
