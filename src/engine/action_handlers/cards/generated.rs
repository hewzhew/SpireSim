use crate::content::cards::{CardId, CardType};
use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::{CardBlueprint, CardLineageId, CombatCard, CombatState, PowerPayload};

fn apply_master_reality_to_generated_card(
    card: &mut CombatCard,
    state: &CombatState,
    upgrade_call_sites: u8,
) {
    crate::content::cards::apply_master_reality_to_generated_card(card, state, upgrade_call_sites);
}

fn make_generated_card_from_id(
    card_id: CardId,
    lineage: CardLineageId,
    upgraded: bool,
    state: &CombatState,
) -> CombatCard {
    let mut card = CombatCard::with_lineage(card_id, lineage);
    if upgraded {
        card.upgrades = 1;
    }
    crate::content::cards::configure_costs_on_new_card(&mut card, state);
    card
}

fn make_random_pool_card_from_id(
    card_id: CardId,
    lineage: CardLineageId,
    state: &CombatState,
) -> CombatCard {
    crate::content::cards::make_fresh_card_copy_for_combat(card_id, lineage, state)
}

fn apply_make_random_draw_pile_cost_override(card: &mut CombatCard, cost_for_turn: Option<u8>) {
    let Some(cost) = cost_for_turn else {
        return;
    };
    if cost == 0 && card.combat_cost_without_turn_override_java() > 0 {
        card.set_combat_and_turn_cost_java(0);
    } else {
        card.set_cost_for_turn_java(cost as i32);
    }
}

fn apply_make_random_draw_pile_blueprint_cost_override(
    card: &mut CardBlueprint,
    cost_for_turn: Option<u8>,
) {
    let Some(cost) = cost_for_turn else {
        return;
    };
    if cost == 0 && card.combat_cost_without_turn_override_java() > 0 {
        card.update_cost_java(-card.combat_cost_without_turn_override_java());
        card.set_cost_for_turn_java(0);
    } else {
        card.set_cost_for_turn_java(cost as i32);
    }
}

pub(super) fn materialize_random_class_card_in_hand_action(
    action: &mut Action,
    state: &mut CombatState,
) {
    let (card_type, cost_for_turn) = match action {
        Action::MakeRandomCardInHand {
            card_type,
            cost_for_turn,
        } => (*card_type, *cost_for_turn),
        _ => return,
    };

    let pool = class_card_pool_for_type(state.meta.player_class.as_str(), card_type);
    if pool.is_empty() {
        return;
    }

    let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
    let mut card = crate::content::cards::make_fresh_card_blueprint_for_combat(pool[idx], state);
    if let Some(cost) = cost_for_turn {
        card.set_cost_for_turn_java(cost as i32);
    }

    let constructed =
        crate::content::cards::prepare_make_temp_card_blueprint_in_hand_constructor(card, state);

    *action = Action::MakeConstructedCopyInHand {
        original: Box::new(constructed),
        amount: 1,
    };
}

pub(super) fn materialize_random_class_card_in_draw_pile_action(
    action: &mut Action,
    state: &mut CombatState,
) {
    let (card_type, cost_for_turn, random_spot) = match action {
        Action::MakeRandomCardInDrawPile {
            card_type,
            cost_for_turn,
            random_spot,
        } => (*card_type, *cost_for_turn, *random_spot),
        _ => return,
    };

    let pool = class_card_pool_for_type(state.meta.player_class.as_str(), card_type);
    if pool.is_empty() {
        return;
    }

    let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
    let mut card = crate::content::cards::make_fresh_card_blueprint_for_combat(pool[idx], state);
    apply_make_random_draw_pile_blueprint_cost_override(&mut card, cost_for_turn);

    *action = Action::MakeCopyInDrawPile {
        original: Box::new(card),
        amount: 1,
        random_spot,
        to_bottom: false,
    };
}

pub(super) fn materialize_random_colorless_card_in_hand_action(
    action: &mut Action,
    state: &mut CombatState,
) {
    let (cost_for_turn, upgraded) = match action {
        Action::MakeRandomColorlessCardInHand {
            cost_for_turn,
            upgraded,
        } => (*cost_for_turn, *upgraded),
        _ => return,
    };

    let pool = state.colorless_combat_pool();
    if pool.is_empty() {
        return;
    }

    let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
    let mut card = CardBlueprint::fresh(pool[idx]);
    if upgraded {
        card.upgrades = 1;
    }
    if let Some(cost) = cost_for_turn {
        card.set_cost_for_turn_java(cost as i32);
    }

    let constructed =
        crate::content::cards::prepare_make_temp_card_blueprint_in_hand_constructor(card, state);

    *action = Action::MakeConstructedCopyInHand {
        original: Box::new(constructed),
        amount: 1,
    };
}

fn apply_generated_card_entering_hand_mechanics(card: &mut CombatCard, state: &mut CombatState) {
    if store::has_power(state, 0, PowerId::Corruption) {
        // ShowCardAndAddToHandEffect calls onCardDrawOrDiscard before its
        // setCostForTurn(-9). That refresh changes the persistent cost of every
        // nonzero-cost skill in hand, including this incoming card. Skills
        // already free for the turn keep their previous combat cost.
        crate::content::cards::ironclad::corruption::corruption_refresh_hand_costs(
            state
                .zones
                .hand
                .iter_mut()
                .chain(std::iter::once(&mut *card)),
        );
        crate::content::cards::ironclad::corruption::corruption_on_card_draw(state, card);
    }
    crate::content::cards::evaluate_card(card, state, None);
}

#[cfg(test)]
mod corruption_generated_tests {
    use super::*;
    use crate::runtime::combat::{Power, PowerPayload};

    fn corrupt_state() -> CombatState {
        let mut state = crate::test_support::blank_test_combat();
        store::set_powers_for(
            &mut state,
            0,
            vec![Power {
                power_type: PowerId::Corruption,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        state
    }

    #[test]
    fn corruption_generated_skill_cost_persists_after_turn_reset() {
        for turn_cost in [None, Some(0), Some(2), Some(12)] {
            let mut state = corrupt_state();
            let mut card = CardBlueprint::fresh(CardId::PowerThrough);
            if let Some(cost) = turn_cost {
                card.set_cost_for_turn_java(cost);
            }
            handle_make_constructed_copy_in_hand(card, 1, &mut state);
            let generated = &mut state.zones.hand[0];
            let expected_combat_cost = match turn_cost {
                Some(0) => 1,
                Some(12) => 3,
                _ => 0,
            };
            assert_eq!(generated.cost_for_turn_java(), 0);
            assert_eq!(
                generated.combat_cost_without_turn_override_java(),
                expected_combat_cost
            );
            generated.reset_attributes_java();
            assert_eq!(generated.cost_for_turn_java(), expected_combat_cost);
        }
    }

    #[test]
    fn corruption_generated_hand_refresh_updates_existing_skills_only() {
        let mut state = corrupt_state();
        let paid = CombatCard::for_scenario(CardId::Defend, 1);
        let mut free = CombatCard::for_scenario(CardId::Shockwave, 2);
        free.set_cost_for_turn_java(0);
        let attack = CombatCard::for_scenario(CardId::Strike, 3);
        let x_skill = CombatCard::for_scenario(CardId::Transmutation, 4);
        state.zones.hand = vec![paid, free, attack, x_skill];

        // Even an incoming attack invokes the refresh on existing hand cards.
        handle_make_temp_card_in_hand(CardId::Anger, 1, false, &mut state);

        let costs: Vec<_> = state
            .zones
            .hand
            .iter()
            .map(|card| {
                (
                    card.combat_cost_without_turn_override_java(),
                    card.cost_for_turn_java(),
                )
            })
            .collect();
        assert_eq!(costs, [(0, 0), (2, 0), (1, 1), (-1, -1), (0, 0)]);
    }

    #[test]
    fn corruption_generated_overflow_keeps_discard_cost_and_skips_hand_refresh() {
        let mut state = corrupt_state();
        state.zones.hand = (1..=9)
            .map(|id| CombatCard::for_scenario(CardId::Strike, id))
            .collect();
        handle_make_temp_card_in_hand(CardId::PowerThrough, 2, false, &mut state);
        assert_eq!(state.zones.hand.len(), 10);
        assert_eq!(
            state.zones.hand[9].combat_cost_without_turn_override_java(),
            0
        );
        assert_eq!(
            state.zones.discard_pile[0].combat_cost_without_turn_override_java(),
            1
        );
        assert_eq!(state.zones.discard_pile[0].cost_for_turn_java(), 1);

        state.zones.hand[0] = CombatCard::for_scenario(CardId::Defend, 20);
        handle_make_temp_card_in_hand(CardId::PowerThrough, 1, false, &mut state);
        assert_eq!(state.zones.hand[0].cost_for_turn_java(), 1);
    }

    #[test]
    fn generated_skill_without_corruption_keeps_combat_cost() {
        let mut state = crate::test_support::blank_test_combat();
        let mut card = CombatCard::for_scenario(CardId::PowerThrough, 1);
        card.set_cost_for_turn_java(0);
        handle_make_copy_in_hand(card, 1, &mut state);
        assert_eq!(
            state.zones.hand[0].combat_cost_without_turn_override_java(),
            1
        );
        assert_eq!(state.zones.hand[0].cost_for_turn_java(), 0);
    }
}

fn add_generated_card_to_hand_or_discard(mut card: CombatCard, state: &mut CombatState) {
    if state.zones.hand.len() < 10 {
        apply_master_reality_to_generated_card(&mut card, state, 2);
        apply_generated_card_entering_hand_mechanics(&mut card, state);
        state.add_card_to_hand(card);
    } else {
        apply_master_reality_to_generated_card(&mut card, state, 1);
        state.add_card_to_discard_pile_top(card);
    }
}

pub fn handle_make_temp_card_in_hand(
    card_id: CardId,
    amount: u8,
    upgraded: bool,
    state: &mut CombatState,
) {
    for _ in 0..amount {
        let card = make_generated_card_from_id(
            card_id,
            state.allocate_combat_generated_lineage(),
            upgraded,
            state,
        );
        add_generated_card_to_hand_or_discard(card, state);
    }
}

pub fn handle_make_temp_card_in_discard(
    card_id: CardId,
    amount: u8,
    upgraded: bool,
    state: &mut CombatState,
) {
    if amount >= 6 {
        return;
    }
    for _ in 0..amount {
        let mut card = make_generated_card_from_id(
            card_id,
            state.allocate_combat_generated_lineage(),
            upgraded,
            state,
        );
        apply_master_reality_to_generated_card(&mut card, state, 1);
        state.add_card_to_discard_pile_top(card);
    }
}

pub fn handle_make_temp_card_in_draw_pile(
    card_id: CardId,
    amount: u8,
    random_spot: bool,
    to_bottom: bool,
    upgraded: bool,
    state: &mut CombatState,
) {
    for _ in 0..amount {
        let mut card = make_generated_card_from_id(
            card_id,
            state.allocate_combat_generated_lineage(),
            upgraded,
            state,
        );
        let upgrade_call_sites = if amount < 6 { 2 } else { 1 };
        apply_master_reality_to_generated_card(&mut card, state, upgrade_call_sites);
        if to_bottom {
            state.add_card_to_draw_pile_bottom(card);
        } else if random_spot {
            state.add_card_to_draw_pile_random_spot(card);
        } else {
            state.add_card_to_draw_pile_top(card);
        }
    }
}

pub fn handle_make_copy_in_hand(original: CombatCard, amount: u8, state: &mut CombatState) {
    for _ in 0..amount {
        let card = original
            .make_stat_equivalent_copy_with_lineage(state.allocate_combat_generated_lineage());
        add_generated_card_to_hand_or_discard(card, state);
    }
}

fn add_constructed_card_to_hand_or_discard(mut card: CombatCard, state: &mut CombatState) {
    if state.zones.hand.len() < 10 {
        apply_master_reality_to_generated_card(&mut card, state, 1);
        apply_generated_card_entering_hand_mechanics(&mut card, state);
        state.add_card_to_hand(card);
    } else {
        state.add_card_to_discard_pile_top(card);
    }
}

pub fn handle_make_constructed_copy_in_hand(
    original: CardBlueprint,
    amount: u8,
    state: &mut CombatState,
) {
    for _ in 0..amount {
        let card = original.materialize(state.allocate_combat_generated_lineage());
        add_constructed_card_to_hand_or_discard(card, state);
    }
}

pub fn handle_make_copy_in_draw_pile(
    original: CardBlueprint,
    amount: u8,
    random_spot: bool,
    to_bottom: bool,
    state: &mut CombatState,
) {
    for _ in 0..amount {
        let mut card = original.materialize(state.allocate_combat_generated_lineage());
        let upgrade_call_sites = if amount < 6 { 2 } else { 1 };
        apply_master_reality_to_generated_card(&mut card, state, upgrade_call_sites);
        if to_bottom {
            state.add_card_to_draw_pile_bottom(card);
        } else if random_spot {
            state.add_card_to_draw_pile_random_spot(card);
        } else {
            state.add_card_to_draw_pile_top(card);
        }
    }
}

fn nightmare_payload_copy(card: &CombatCard) -> CombatCard {
    let mut copy = card.make_stat_equivalent_copy_with_lineage(card.lineage);
    copy.reset_attributes_java();
    copy
}

pub fn queue_nightmare_power_front(card: &CombatCard, amount: u8, state: &mut CombatState) {
    let payload = PowerPayload::Card(nightmare_payload_copy(card));
    let instance_id = state.next_power_instance_id();
    state.queue_action_front(Action::ApplyPowerWithPayload {
        source: 0,
        target: 0,
        power_id: PowerId::Nightmare,
        amount: amount as i32,
        instance_id: Some(instance_id),
        extra_data: None,
        payload,
    });
}

pub fn handle_nightmare(amount: u8, state: &mut CombatState) {
    if state.zones.hand.is_empty() {
        return;
    }

    if state.zones.hand.len() == 1 {
        let card = state.zones.hand[0].clone();
        queue_nightmare_power_front(&card, amount, state);
        return;
    }

    state.queue_action_front(Action::SuspendForHandSelect {
        min: 1,
        max: 1,
        can_cancel: false,
        filter: crate::state::HandSelectFilter::Any,
        reason: crate::state::HandSelectReason::Nightmare { amount },
    });
}

pub fn handle_make_copy_in_discard(original: CombatCard, amount: u8, state: &mut CombatState) {
    if amount >= 6 {
        return;
    }
    for _ in 0..amount {
        let mut card = original
            .make_stat_equivalent_copy_with_lineage(state.allocate_combat_generated_lineage());
        apply_master_reality_to_generated_card(&mut card, state, 1);
        state.add_card_to_discard_pile_top(card);
    }
}

pub fn handle_return_stasis_card(
    card_lineage: CardLineageId,
    to_hand: bool,
    state: &mut CombatState,
) {
    let Some(pos) = state
        .zones
        .limbo
        .iter()
        .position(|card| card.lineage == card_lineage)
    else {
        return;
    };
    let held = state.zones.limbo.remove(pos);
    let mut card = held.make_same_instance_of_java();

    if to_hand {
        // Java StasisPower uses MakeTempCardInHandAction(card, false, true).
        // The action constructor upgrades once under Master Reality. If the
        // card actually enters hand, ShowCardAndAddToHandEffect upgrades again;
        // if hand overflow sends it to discard, only the constructor upgrade
        // has affected the same-UUID copy.
        apply_master_reality_to_generated_card(&mut card, state, 1);
        if state.zones.hand.len() < 10 {
            apply_master_reality_to_generated_card(&mut card, state, 1);
            apply_generated_card_entering_hand_mechanics(&mut card, state);
            state.add_card_to_hand(card);
        } else {
            state.add_card_to_discard_pile_top(card);
        }
    } else {
        // Java full-hand Stasis path uses MakeTempCardInDiscardAction(card, true);
        // that sameUUID constructor deliberately skips Master Reality.
        state.add_card_to_discard_pile_top(card);
    }
}

pub fn handle_make_temp_card_in_discard_and_deck(
    card_id: CardId,
    amount: u8,
    state: &mut CombatState,
) {
    for _ in 0..amount {
        // Java MakeTempCardInDiscardAndDeckAction queues the draw-pile effect
        // before the discard effect. Preserve that order for UUID/replay parity.
        let mut draw_card = make_generated_card_from_id(
            card_id,
            state.allocate_combat_generated_lineage(),
            false,
            state,
        );
        apply_master_reality_to_generated_card(&mut draw_card, state, 1);
        state.add_card_to_draw_pile_random_spot(draw_card);

        let mut discard_card = make_generated_card_from_id(
            card_id,
            state.allocate_combat_generated_lineage(),
            false,
            state,
        );
        apply_master_reality_to_generated_card(&mut discard_card, state, 1);
        state.add_card_to_discard_pile_top(discard_card);
    }
}

pub(crate) fn class_card_pool_for_type(
    player_class: &str,
    card_type: Option<CardType>,
) -> Vec<CardId> {
    crate::content::cards::class_combat_card_pool_for_type(player_class, card_type)
}

pub fn handle_make_random_card_in_hand(
    card_type: Option<CardType>,
    cost_for_turn: Option<u8>,
    state: &mut CombatState,
) {
    let pool = class_card_pool_for_type(state.meta.player_class.as_str(), card_type);
    if !pool.is_empty() {
        let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
        let card_id = pool[idx];
        let mut card = make_random_pool_card_from_id(
            card_id,
            state.allocate_combat_generated_lineage(),
            state,
        );
        if let Some(cost) = cost_for_turn {
            card.set_cost_for_turn_java(cost as i32);
        }
        add_generated_card_to_hand_or_discard(card, state);
    }
}

pub fn handle_make_random_card_in_draw_pile(
    card_type: Option<CardType>,
    cost_for_turn: Option<u8>,
    random_spot: bool,
    state: &mut CombatState,
) {
    let pool = class_card_pool_for_type(state.meta.player_class.as_str(), card_type);
    if !pool.is_empty() {
        let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
        let card_id = pool[idx];
        let mut card = make_random_pool_card_from_id(
            card_id,
            state.allocate_combat_generated_lineage(),
            state,
        );
        apply_make_random_draw_pile_cost_override(&mut card, cost_for_turn);
        apply_master_reality_to_generated_card(&mut card, state, 2);
        if random_spot {
            state.add_card_to_draw_pile_random_spot(card);
        } else {
            state.add_card_to_draw_pile_top(card);
        }
    }
}

pub fn handle_make_random_colorless_card_in_hand(
    cost_for_turn: Option<u8>,
    upgraded: bool,
    state: &mut CombatState,
) {
    let pool = state.colorless_combat_pool();
    if !pool.is_empty() {
        let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
        let card_id = pool[idx];
        let lineage = state.allocate_combat_generated_lineage();
        let mut card = CombatCard::with_lineage(card_id, lineage);
        if upgraded {
            card.upgrades = 1;
        }
        if let Some(cost) = cost_for_turn {
            card.set_cost_for_turn_java(cost as i32);
        }
        add_generated_card_to_hand_or_discard(card, state);
    }
}

pub fn handle_transmutation(
    upgraded: bool,
    free_to_play_once: bool,
    energy_on_use: i32,
    state: &mut CombatState,
) {
    let base_effect = if energy_on_use != -1 {
        energy_on_use
    } else {
        state.turn.energy as i32
    };
    let effect = crate::content::relics::hooks::on_calculate_x_cost(state, base_effect);

    if effect > 0 {
        for _ in 0..effect {
            let mut action = Action::MakeRandomColorlessCardInHand {
                cost_for_turn: Some(0),
                upgraded,
            };
            materialize_random_colorless_card_in_hand_action(&mut action, state);
            state.queue_action_back(action);
        }
        if !free_to_play_once {
            state.turn.spend_energy(state.turn.energy as i32);
        }
    }
}

pub fn handle_conjure_blade(free_to_play_once: bool, energy_on_use: i32, state: &mut CombatState) {
    let mut effect = if energy_on_use != -1 {
        energy_on_use
    } else {
        state.turn.energy as i32
    };
    if state
        .entities
        .player
        .has_relic(crate::content::relics::RelicId::ChemicalX)
    {
        effect += 2;
    }

    let mut card = make_generated_card_from_id(
        CardId::Expunger,
        state.allocate_combat_generated_lineage(),
        false,
        state,
    );
    card.misc_value = effect.max(0);
    apply_master_reality_to_generated_card(&mut card, state, 2);
    state.add_card_to_draw_pile_random_spot(card);

    if !free_to_play_once {
        state.turn.spend_energy(state.turn.energy as i32);
    }
}
