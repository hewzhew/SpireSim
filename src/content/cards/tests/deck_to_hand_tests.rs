//! Source-backed regressions for typed and unrestricted deck-to-hand actions. These are
//! constructed runtime cases, not recorded Java parity fixtures.
use super::*;
use crate::content::relics::{RelicId, RelicState};
use crate::engine::core::tick_engine;
use crate::state::{ClientInput, EngineState};

fn settle(engine: &mut EngineState, state: &mut crate::runtime::combat::CombatState) {
    for _ in 0..100 {
        if *engine != EngineState::CombatProcessing {
            return;
        }
        assert!(tick_engine(engine, state, None));
    }
    panic!("deck-to-hand case did not settle");
}

fn play_with_pain_cube(
    secret: CardId,
    matching: CardId,
    draw_count: u32,
    discard_count: u32,
) -> (EngineState, crate::runtime::combat::CombatState) {
    play_upgraded_with_pain_cube(secret, 0, matching, draw_count, discard_count)
}

fn play_upgraded_with_pain_cube(
    card_id: CardId,
    upgrades: u8,
    matching: CardId,
    draw_count: u32,
    discard_count: u32,
) -> (EngineState, crate::runtime::combat::CombatState) {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.current_hp = 50;
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RunicCube));
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    state.zones.hand = vec![
        CombatCard::for_scenario(card_id, 1),
        CombatCard::for_scenario(CardId::Pain, 2),
        CombatCard::for_scenario(CardId::Pain, 3),
    ];
    state.zones.hand[0].upgrades = upgrades;
    state.zones.draw_pile = (4..4 + draw_count)
        .map(|i| CombatCard::for_scenario(matching, i))
        .collect::<Vec<_>>()
        .into();
    state.zones.discard_pile = (4 + draw_count..4 + draw_count + discard_count)
        .map(|i| CombatCard::for_scenario(matching, i))
        .collect::<Vec<_>>()
        .into();
    let mut engine = EngineState::CombatPlayerTurn;
    assert!(tick_engine(
        &mut engine,
        &mut state,
        Some(ClientInput::PlayCard {
            card_index: 0,
            target: None
        })
    ));
    settle(&mut engine, &mut state);
    assert_eq!(state.entities.player.current_hp, 48);
    (engine, state)
}

fn secret_card_after_pain_cube_reshuffle(secret: CardId, matching: CardId) {
    let (engine, state) = play_with_pain_cube(secret, matching, 1, 2);
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    // Java: each Pain adds LoseHPAction to the front; Runic Cube adds a draw
    // to the front. The second draw shuffles the discard pile. Only then does
    // the secret card's queued action inspect the NEW draw pile and auto-move
    // its sole remaining matching card. An identity captured at use time is stale.
    assert!(
        state.zones.draw_pile.is_empty(),
        "{secret:?} must inspect the draw pile after Pain/Runic Cube resolution"
    );
    let actual: std::collections::BTreeSet<_> = state
        .zones
        .hand
        .iter()
        .filter(|card| card.id == matching)
        .map(|card| card.lineage)
        .collect();
    assert_eq!(
        actual,
        [4, 5, 6]
            .map(crate::test_support::scenario_lineage)
            .into_iter()
            .collect()
    );
    assert_eq!(state.zones.exhaust_pile.len(), 1);
    assert_eq!(state.zones.exhaust_pile[0].id, secret);
}

#[test]
fn secret_technique_rechecks_draw_pile_after_pain_and_runic_cube() {
    secret_card_after_pain_cube_reshuffle(CardId::SecretTechnique, CardId::Defend);
}

#[test]
fn secret_weapon_rechecks_draw_pile_after_pain_and_runic_cube() {
    secret_card_after_pain_cube_reshuffle(CardId::SecretWeapon, CardId::Strike);
}

#[test]
fn seek_rechecks_singleton_identity_after_pain_and_runic_cube() {
    secret_card_after_pain_cube_reshuffle(CardId::Seek, CardId::Strike);
}

#[test]
fn seek_empty_at_play_can_take_a_card_from_the_replacement_draw_pile() {
    let (engine, state) = play_with_pain_cube(CardId::Seek, CardId::Strike, 0, 3);
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert!(
        state.zones.draw_pile.is_empty(),
        "Seek must inspect the pile after its preceding hooks even when empty at use time"
    );
    assert_eq!(
        state
            .zones
            .hand
            .iter()
            .filter(|c| c.id == CardId::Strike)
            .count(),
        3
    );
}

#[test]
fn seek_plus_auto_moves_the_current_remaining_pile_without_grid_or_rng() {
    for draw_count in [3, 4] {
        let (engine, state) =
            play_upgraded_with_pain_cube(CardId::Seek, 1, CardId::Strike, draw_count, 0);
        assert_eq!(
            engine,
            EngineState::CombatPlayerTurn,
            "Seek+ must auto-take the current one/two-card pile"
        );
        assert!(state.zones.draw_pile.is_empty());
        assert_eq!(
            state
                .zones
                .hand
                .iter()
                .filter(|c| c.id == CardId::Strike)
                .count(),
            draw_count as usize
        );
        assert_eq!(state.rng.card_random_rng.counter, 0);
    }
}

#[test]
fn seek_plus_opens_current_grid_when_reshuffle_adds_more_than_requested() {
    let (engine, state) = play_upgraded_with_pain_cube(CardId::Seek, 1, CardId::Strike, 1, 5);
    let EngineState::PendingChoice(crate::state::PendingChoice::GridSelect {
        candidate_lineages,
        min_cards,
        max_cards,
        can_cancel,
        ..
    }) = engine
    else {
        panic!("Seek+ must select from the current four-card pile")
    };
    assert_eq!((min_cards, max_cards, can_cancel), (2, 2, false));
    assert_eq!(candidate_lineages.len(), 4);
    let candidates: std::collections::BTreeSet<_> = candidate_lineages.into_iter().collect();
    assert_eq!(
        candidates,
        state.zones.draw_pile.iter().map(|c| c.lineage).collect()
    );
    assert_eq!(state.rng.card_random_rng.counter, 0);
}

#[test]
fn seek_drawn_empty_before_resolution_is_a_normal_empty_effect() {
    let (engine, mut state) = play_with_pain_cube(CardId::Seek, CardId::Strike, 2, 0);
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert!(state.zones.draw_pile.is_empty());
    assert!(state
        .take_engine_diagnostics()
        .iter()
        .all(|d| d.severity != crate::state::selection::EngineDiagnosticSeverity::Error));
}

#[test]
fn seek_plus_auto_move_preserves_pile_order_cost_and_full_hand_overflow_under_no_draw() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    let mut seek = CombatCard::for_scenario(CardId::Seek, 1);
    seek.upgrades = 1;
    state.zones.hand = std::iter::once(seek)
        .chain((100..109).map(|i| CombatCard::for_scenario(CardId::Defend, i)))
        .collect();
    let mut top = CombatCard::for_scenario(CardId::Defend, 30);
    let mut bottom = CombatCard::for_scenario(CardId::Strike, 40);
    top.cost_for_turn = Some(0);
    bottom.cost_for_turn = Some(0);
    bottom.cost_modifier = 1;
    state.zones.draw_pile = vec![top, bottom].into();
    crate::engine::action_handlers::execute_action(
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::NoDraw,
            amount: 1,
        },
        &mut state,
    );
    let mut engine = EngineState::CombatPlayerTurn;
    assert!(tick_engine(
        &mut engine,
        &mut state,
        Some(ClientInput::PlayCard {
            card_index: 0,
            target: None
        })
    ));
    settle(&mut engine, &mut state);
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert!(state.zones.draw_pile.is_empty());
    assert_eq!(state.zones.hand.len(), 10);
    assert_eq!(
        state.zones.hand.last().unwrap().lineage,
        crate::test_support::scenario_lineage(40),
        "auto-all uses Java group order, not sorted grid order"
    );
    assert_eq!(state.zones.hand.last().unwrap().cost_for_turn, Some(0));
    assert_eq!(state.zones.hand.last().unwrap().cost_modifier, 1);
    assert_eq!(
        state.zones.discard_pile[0].lineage,
        crate::test_support::scenario_lineage(30)
    );
    assert_eq!(state.zones.discard_pile[0].cost_for_turn, Some(0));
    assert_eq!(state.rng.card_random_rng.counter, 0);
}

#[test]
fn vanished_matches_fizzle_without_a_broken_engine_diagnostic() {
    for (secret, matching) in [
        (CardId::SecretTechnique, CardId::Defend),
        (CardId::SecretWeapon, CardId::Strike),
    ] {
        let (engine, mut state) = play_with_pain_cube(secret, matching, 2, 0);
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert!(state.zones.draw_pile.is_empty());
        assert_eq!(
            state.zones.hand.iter().filter(|c| c.id == matching).count(),
            2
        );
        assert_eq!(state.rng.card_random_rng.counter, 0);
        assert!(state
            .take_engine_diagnostics()
            .iter()
            .all(|d| d.severity != crate::state::selection::EngineDiagnosticSeverity::Error));
    }
}

#[test]
fn replacement_matches_open_a_grid_and_consume_rng_for_the_current_pile() {
    for (secret, matching) in [
        (CardId::SecretTechnique, CardId::Defend),
        (CardId::SecretWeapon, CardId::Strike),
    ] {
        let (mut engine, mut state) = play_with_pain_cube(secret, matching, 1, 4);
        let EngineState::PendingChoice(crate::state::PendingChoice::GridSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            ..
        }) = &engine
        else {
            panic!("three current matches must open a grid for {secret:?}")
        };
        assert_eq!((*min_cards, *max_cards, *can_cancel), (1, 1, false));
        assert_eq!(candidate_lineages.len(), 3);
        let candidates: std::collections::BTreeSet<_> =
            candidate_lineages.iter().copied().collect();
        assert_eq!(
            candidates,
            state.zones.draw_pile.iter().map(|c| c.lineage).collect()
        );
        // First addToRandomSpot inserts directly; the following two each
        // consume cardRandomRng once, independently of shuffleRng.
        assert_eq!(state.rng.card_random_rng.counter, 2);
        let selected = candidate_lineages[0];
        assert!(tick_engine(
            &mut engine,
            &mut state,
            Some(ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    crate::state::selection::SelectionScope::Grid,
                    [selected]
                )
            ))
        ));
        settle(&mut engine, &mut state);
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert!(state.zones.hand.iter().any(|card| card.lineage == selected));
        assert_eq!(state.zones.draw_pile.len(), 2);
        assert_eq!(
            state.rng.card_random_rng.counter, 2,
            "selection does not recollect the candidates"
        );
    }
}

#[test]
fn secret_cards_require_a_matching_draw_pile_card_before_play() {
    let mut state = crate::test_support::blank_test_combat();
    for (secret, matching) in [
        (CardId::SecretTechnique, CardId::Defend),
        (CardId::SecretWeapon, CardId::Strike),
    ] {
        let card = CombatCard::for_scenario(secret, 1);
        state.zones.draw_pile.clear();
        assert!(can_play_card(&card, &state).is_err());
        state.zones.draw_pile = vec![CombatCard::for_scenario(matching, 2)].into();
        assert!(can_play_card(&card, &state).is_ok());
    }
}
