//! ArmamentsAction inspects the hand after prior actions, not at card use.
//! Constructed runtime regressions; no recorded live Java boundaries.
use super::*;
use crate::content::relics::{RelicId, RelicState};
use crate::engine::core::tick_engine;
use crate::state::{ClientInput, EngineState, PendingChoice};

fn settle(engine: &mut EngineState, state: &mut crate::runtime::combat::CombatState) {
    for _ in 0..100 {
        if *engine != EngineState::CombatProcessing {
            return;
        }
        assert!(tick_engine(engine, state, None));
    }
    panic!("Armaments did not reach a stable boundary");
}

fn play(
    upgrades: u8,
    others: Vec<CombatCard>,
    no_draw: bool,
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
    let mut armaments = CombatCard::for_scenario(CardId::Armaments, 1);
    armaments.upgrades = upgrades;
    state.zones.hand = vec![armaments, CombatCard::for_scenario(CardId::Pain, 2)];
    state.zones.hand.extend(others);
    state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 50)].into();
    if no_draw {
        crate::engine::action_handlers::execute_action(
            Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::NoDraw,
                amount: 1,
            },
            &mut state,
        );
    }
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
    assert_eq!(state.entities.player.current_hp, 49);
    assert_eq!(state.entities.player.block, 5);
    assert_eq!(state.rng.card_random_rng.counter, 0);
    (engine, state)
}

#[test]
fn armaments_upgrades_the_only_card_drawn_by_preceding_pain_cube() {
    let (engine, state) = play(0, vec![], false);
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(
        state
            .zones
            .hand
            .iter()
            .find(|c| c.id == CardId::Strike)
            .unwrap()
            .upgrades,
        1,
        "Armaments must include the card drawn before its upgrade action"
    );
}

#[test]
fn armaments_plus_upgrades_new_draw_and_existing_repeat_upgrade_card() {
    let mut blow = CombatCard::for_scenario(CardId::SearingBlow, 10);
    blow.upgrades = 5;
    let (engine, state) = play(
        1,
        vec![blow, CombatCard::for_scenario(CardId::Wound, 11)],
        false,
    );
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(
        state
            .zones
            .hand
            .iter()
            .find(|c| c.id == CardId::Strike)
            .unwrap()
            .upgrades,
        1,
        "Armaments+ must include newly drawn cards"
    );
    assert_eq!(
        state
            .zones
            .hand
            .iter()
            .find(|c| c.id == CardId::SearingBlow)
            .unwrap()
            .upgrades,
        6
    );
    assert!(state
        .zones
        .hand
        .iter()
        .filter(|c| matches!(c.id, CardId::Pain | CardId::Wound))
        .all(|c| c.upgrades == 0));
}

#[test]
fn armaments_uses_current_selection_and_returns_cards_in_java_order() {
    let (mut engine, mut state) =
        play(0, vec![CombatCard::for_scenario(CardId::Defend, 10)], false);
    let EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages,
        min_cards,
        max_cards,
        can_cancel,
        ..
    }) = &engine
    else {
        panic!("newly drawn card must turn Armaments into a two-candidate selection")
    };
    assert_eq!(
        candidate_lineages,
        &[10, 50].map(crate::test_support::scenario_lineage)
    );
    assert_eq!((*min_cards, *max_cards, *can_cancel), (1, 1, false));
    assert!(tick_engine(
        &mut engine,
        &mut state,
        Some(ClientInput::SubmitSelection(
            crate::state::selection::SelectionResolution::card_lineages(
                crate::state::selection::SelectionScope::Hand,
                [crate::test_support::scenario_lineage(50)]
            )
        ))
    ));
    settle(&mut engine, &mut state);
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(
        state
            .zones
            .hand
            .iter()
            .map(|c| (c.id, c.upgrades))
            .collect::<Vec<_>>(),
        [(CardId::Defend, 0), (CardId::Strike, 1), (CardId::Pain, 0)]
    );
    assert_eq!(state.rng.card_random_rng.counter, 0);
}

#[test]
fn armaments_empty_after_no_draw_remains_a_normal_no_upgrade_effect() {
    for upgrades in [0, 1] {
        let (engine, mut state) = play(upgrades, vec![], true);
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert_eq!(state.zones.draw_pile.len(), 1);
        assert_eq!(state.zones.draw_pile[0].upgrades, 0);
        assert!(state
            .take_engine_diagnostics()
            .iter()
            .all(|d| d.severity != crate::state::selection::EngineDiagnosticSeverity::Error));
    }
}
