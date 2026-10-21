//! ExhumeAction checks the current hand/exhaust pile in update, including
//! changes caused by earlier card hooks. Constructed source-backed cases;
//! these are not live Java parity fixtures.
use super::*;
use crate::content::relics::{RelicId, RelicState};
use crate::engine::core::tick_engine;
use crate::runtime::combat::{CardLineageId, CombatState};
use crate::state::{ClientInput, EngineState, PendingChoice};

fn settle(engine: &mut EngineState, state: &mut CombatState) {
    for _ in 0..100 {
        if *engine != EngineState::CombatProcessing {
            return;
        }
        assert!(tick_engine(engine, state, None));
    }
    panic!("Exhume did not reach a stable boundary");
}

fn combat(exhaust: Vec<CombatCard>, upgrades: u8) -> CombatState {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.current_hp = 50;
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    let mut exhume = CombatCard::for_scenario(CardId::Exhume, 1);
    exhume.upgrades = upgrades;
    state.zones.hand = vec![exhume];
    state.zones.exhaust_pile = exhaust.into();
    state
}

fn pain_cube(state: &mut CombatState, drawn: CardId) {
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RunicCube));
    state
        .zones
        .hand
        .push(CombatCard::for_scenario(CardId::Pain, 2));
    state.zones.draw_pile = vec![CombatCard::for_scenario(drawn, 30)].into();
}

fn play(state: &mut CombatState) -> EngineState {
    let mut engine = EngineState::CombatPlayerTurn;
    assert!(tick_engine(
        &mut engine,
        state,
        Some(ClientInput::PlayCard {
            card_index: 0,
            target: None
        })
    ));
    settle(&mut engine, state);
    engine
}

fn candidates(engine: &EngineState) -> &[CardLineageId] {
    match engine {
        EngineState::PendingChoice(PendingChoice::GridSelect {
            candidate_lineages,
            source_pile: crate::state::PileType::Exhaust,
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::GridSelectReason::Exhume { upgrade: false },
        }) => candidate_lineages,
        other => panic!("expected Exhume selection, got {other:?}"),
    }
}

fn select(engine: &mut EngineState, state: &mut CombatState, lineage: u32) {
    assert!(tick_engine(
        engine,
        state,
        Some(ClientInput::SubmitSelection(
            crate::state::selection::SelectionResolution::card_lineages(
                crate::state::selection::SelectionScope::Grid,
                [crate::test_support::scenario_lineage(lineage)],
            ),
        ))
    ));
    settle(engine, state);
    assert_eq!(*engine, EngineState::CombatPlayerTurn);
}

fn lineages(cards: &[CombatCard]) -> Vec<CardLineageId> {
    cards.iter().map(|card| card.lineage).collect()
}

#[test]
fn exhume_returns_the_card_exhausted_by_a_preceding_draw_hook() {
    for upgrades in [0, 1] {
        let mut state = combat(vec![], upgrades);
        pain_cube(&mut state, CardId::DeusExMachina);
        assert_eq!(play(&mut state), EngineState::CombatPlayerTurn);
        assert_eq!(state.entities.player.current_hp, 49);
        assert!(
            state
                .zones
                .hand
                .iter()
                .any(|card| card.lineage == crate::test_support::scenario_lineage(30)),
            "Deus Ex Machina exhausted before Exhume resolves must be returned"
        );
        assert_eq!(
            state
                .zones
                .hand
                .iter()
                .filter(|card| card.id == CardId::Miracle)
                .count(),
            2
        );
        assert_eq!(
            lineages(&state.zones.exhaust_pile),
            [crate::test_support::scenario_lineage(1)]
        );
    }
}

#[test]
fn exhume_replaces_stale_auto_return_with_a_live_two_card_choice() {
    let mut state = combat(vec![CombatCard::for_scenario(CardId::Strike, 10)], 0);
    pain_cube(&mut state, CardId::DeusExMachina);
    let mut engine = play(&mut state);
    assert_eq!(
        candidates(&engine),
        &[10, 30].map(crate::test_support::scenario_lineage)
    );
    select(&mut engine, &mut state, 30);
    assert_eq!(
        lineages(&state.zones.exhaust_pile),
        [10, 1].map(crate::test_support::scenario_lineage)
    );
}

#[test]
fn exhume_still_opens_grid_when_filtering_leaves_one_candidate() {
    let mut state = combat(
        vec![
            CombatCard::for_scenario(CardId::Exhume, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ],
        1,
    );
    let mut engine = play(&mut state);
    assert_eq!(
        candidates(&engine),
        &[crate::test_support::scenario_lineage(11)]
    );
    select(&mut engine, &mut state, 11);
    assert_eq!(
        lineages(&state.zones.hand),
        [crate::test_support::scenario_lineage(11)]
    );
    assert_eq!(
        state.zones.hand[0].upgrades, 0,
        "Exhume+ reduces its own cost; it does not upgrade the retrieved card"
    );
}

#[test]
fn exhume_grid_and_continuation_keep_java_held_card_order() {
    let mut state = combat(
        vec![
            CombatCard::for_scenario(CardId::Exhume, 10),
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Exhume, 12),
            CombatCard::for_scenario(CardId::Defend, 13),
        ],
        0,
    );
    let mut engine = play(&mut state);
    assert_eq!(
        candidates(&engine),
        &[11, 13].map(crate::test_support::scenario_lineage)
    );
    // Shared Java projection appends the explicitly observed ExhumeAction
    // held cards after its remaining exhaust group at a pending boundary.
    assert_eq!(
        lineages(&state.zones.exhaust_pile),
        [11, 13, 10, 12].map(crate::test_support::scenario_lineage)
    );
    state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    select(&mut engine, &mut state, 13);
    assert_eq!(
        lineages(&state.zones.exhaust_pile),
        [11, 10, 12, 1].map(crate::test_support::scenario_lineage)
    );
    assert_eq!(
        lineages(&state.zones.hand),
        [crate::test_support::scenario_lineage(13)]
    );
}

#[test]
fn exhume_does_not_open_grid_when_preceding_draw_fills_hand() {
    let mut state = combat(
        vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ],
        0,
    );
    pain_cube(&mut state, CardId::Strike);
    state
        .zones
        .hand
        .extend((40..48).map(|id| CombatCard::for_scenario(CardId::Defend, id)));
    assert_eq!(state.zones.hand.len(), 10);
    assert_eq!(
        play(&mut state),
        EngineState::CombatPlayerTurn,
        "ExhumeAction checks hand capacity before opening any choice"
    );
    assert_eq!(state.zones.hand.len(), 10);
    assert_eq!(
        lineages(&state.zones.exhaust_pile),
        [10, 11, 1].map(crate::test_support::scenario_lineage)
    );
}

#[test]
fn exhume_empty_and_all_exhume_piles_finish_without_error_or_rng() {
    for count in 0..=2 {
        let mut state = combat(
            (10..10 + count)
                .map(|id| CombatCard::for_scenario(CardId::Exhume, id))
                .collect(),
            0,
        );
        let rng = state.rng.clone();
        assert_eq!(play(&mut state), EngineState::CombatPlayerTurn);
        assert!(state.zones.hand.is_empty());
        assert_eq!(state.zones.exhaust_pile.len(), count as usize + 1);
        assert_eq!(state.rng, rng);
        assert!(state
            .take_engine_diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.severity
                != crate::state::selection::EngineDiagnosticSeverity::Error));
    }
}

#[test]
fn exhume_singleton_preserves_instance_and_applies_corruption_cost() {
    let mut defend = CombatCard::for_scenario(CardId::Defend, 10);
    defend.upgrades = 1;
    defend.cost_modifier = 2;
    defend.cost_for_turn = Some(2);
    defend.base_block_override = Some(17);
    let mut state = combat(vec![], 0);
    state.queue_action_back(Action::ApplyPower {
        source: 0,
        target: 0,
        power_id: PowerId::Corruption,
        amount: 1,
    });
    settle(&mut EngineState::CombatProcessing, &mut state);
    // A constructed instance entering combat after Corruption's application
    // still needs Exhume's own setCostForTurn when it returns from exhaust.
    state.zones.exhaust_pile = vec![defend].into();
    let initial_energy = state.turn.energy;
    assert_eq!(play(&mut state), EngineState::CombatPlayerTurn);
    let returned = &state.zones.hand[0];
    assert_eq!(returned.lineage, crate::test_support::scenario_lineage(10));
    assert_eq!(
        (
            returned.upgrades,
            returned.cost_modifier,
            returned.base_block_override
        ),
        (1, 2, Some(17))
    );
    assert_eq!(returned.cost_for_turn_java(), 0);
    assert_eq!(state.turn.energy, initial_energy);
}
