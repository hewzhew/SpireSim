//! Original EmptyDeckShuffleAction/ShuffleAllAction notify relics in their
//! constructors; CardGroup.shuffle consumes a seed even for an empty group.
//! Source-backed constructed regressions, not recorded live Java boundaries.
use super::*;
use crate::content::relics::{RelicId, RelicState};
use crate::engine::core::tick_engine;
use crate::runtime::combat::{CardLineageId, CombatState};
use crate::runtime::rng::{shuffle_with_random_long, StsRng};
use crate::state::{ClientInput, EngineState, PendingChoice};

fn settle(engine: &mut EngineState, state: &mut CombatState) {
    for _ in 0..200 {
        if *engine != EngineState::CombatProcessing {
            return;
        }
        assert!(tick_engine(engine, state, None));
    }
    panic!("shuffle did not reach a stable boundary");
}

fn combat() -> CombatState {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    state
}

fn cards(ids: &[u32]) -> Vec<CombatCard> {
    ids.iter()
        .map(|&id| CombatCard::for_scenario(CardId::Strike, id))
        .collect()
}

fn lineages(cards: &[CombatCard]) -> Vec<CardLineageId> {
    cards.iter().map(|card| card.lineage).collect()
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

// Java CardGroup uses bottom-to-top order. EmptyDeckShuffleAction shuffles
// only discard, Soul.shuffle appends it to draw, then ShuffleAction shuffles
// that entire group. Conversion to Rust order belongs only at the boundary.
fn java_deep_breath_order(
    draw: &[CombatCard],
    discard: &[CombatCard],
    rng: &mut StsRng,
) -> Vec<CombatCard> {
    let mut group = draw.iter().rev().cloned().collect::<Vec<_>>();
    let mut discarded = discard.to_vec();
    shuffle_with_random_long(&mut discarded, rng);
    group.extend(discarded);
    shuffle_with_random_long(&mut group, rng);
    group.reverse();
    group
}

#[test]
fn deep_breath_uses_two_shuffles_and_java_group_order() {
    for seed in [1, 17, 20260906] {
        for upgrades in [0, 1] {
            let mut state = combat();
            state.rng.shuffle_rng = StsRng::new(seed);
            let mut breath = CombatCard::for_scenario(CardId::DeepBreath, 1);
            breath.upgrades = upgrades;
            state.zones.hand = vec![breath];
            state.zones.draw_pile = cards(&[10, 11, 12]).into();
            state.zones.discard_pile = cards(&[20, 21, 22]).into();
            let mut expected_rng = state.rng.shuffle_rng.clone();
            let expected = java_deep_breath_order(
                &state.zones.draw_pile,
                &state.zones.discard_pile.clone().into_vec(),
                &mut expected_rng,
            );
            assert_eq!(play(&mut state), EngineState::CombatPlayerTurn);
            assert_eq!(
                state.rng.shuffle_rng.counter, 2,
                "each CardGroup shuffle consumes its own seed"
            );
            let drawn = upgrades as usize + 1;
            assert_eq!(lineages(&state.zones.hand), lineages(&expected[..drawn]));
            assert_eq!(
                lineages(&state.zones.draw_pile),
                lineages(&expected[drawn..])
            );
            assert_eq!(state.rng.shuffle_rng, expected_rng);
            assert_eq!(state.zones.discard_pile.len(), 1);
            assert_eq!(state.zones.discard_pile[0].id, CardId::DeepBreath);
        }
    }
}

#[test]
fn deep_breath_melange_scries_before_shuffle_and_checkpoint_keeps_one_notification() {
    let mut state = combat();
    for relic in [RelicId::Melange, RelicId::Sundial, RelicId::Abacus] {
        state.entities.player.add_relic(RelicState::new(relic));
    }
    state.zones.hand = vec![CombatCard::for_scenario(CardId::DeepBreath, 1)];
    state.zones.draw_pile = cards(&[10, 11, 12, 13]).into();
    state.zones.discard_pile = cards(&[20, 21]).into();
    let mut engine = play(&mut state);
    let EngineState::PendingChoice(PendingChoice::ScrySelect { card_lineages, .. }) = &engine
    else {
        panic!("Melange should open a choice before Deep Breath shuffles");
    };
    assert_eq!(
        card_lineages,
        &[10, 11, 12].map(crate::test_support::scenario_lineage)
    );
    assert_eq!(
        state.rng.shuffle_rng.counter, 0,
        "no shuffle before the constructor's ScryAction"
    );
    assert_eq!(
        state
            .entities
            .player
            .relics
            .iter()
            .find(|r| r.id == RelicId::Sundial)
            .unwrap()
            .counter,
        1
    );

    // Restoring an already-constructed pending action must not notify twice.
    state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let mut expected_rng = state.rng.shuffle_rng.clone();
    let expected = java_deep_breath_order(
        &cards(&[10, 12, 13]),
        &cards(&[20, 21, 11]),
        &mut expected_rng,
    );
    assert!(tick_engine(
        &mut engine,
        &mut state,
        Some(ClientInput::SubmitScryDiscard(vec![1]))
    ));
    settle(&mut engine, &mut state);
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(lineages(&state.zones.hand), lineages(&expected[..1]));
    assert_eq!(lineages(&state.zones.draw_pile), lineages(&expected[1..]));
    assert_eq!(state.rng.shuffle_rng, expected_rng);
    assert_eq!(
        state.entities.player.block, 6,
        "one relic notification despite two shuffles"
    );
    assert_eq!(
        state
            .entities
            .player
            .relics
            .iter()
            .find(|r| r.id == RelicId::Sundial)
            .unwrap()
            .counter,
        1
    );
}

#[test]
fn deep_breath_keeps_queued_shuffles_when_pain_draw_empties_discard() {
    let mut state = combat();
    state.entities.player.current_hp = 50;
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RunicCube));
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Abacus));
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::DeepBreath, 1),
        CombatCard::for_scenario(CardId::Pain, 2),
    ];
    state.zones.discard_pile = cards(&[20]).into();
    assert_eq!(play(&mut state), EngineState::CombatPlayerTurn);
    assert_eq!(state.entities.player.current_hp, 49);
    assert_eq!(
        state.rng.shuffle_rng.counter, 3,
        "Pain draw shuffles once; both queued Deep Breath shuffles still consume a seed"
    );
    assert_eq!(
        state.entities.player.block, 12,
        "the explicit and draw reshuffles each notify relics once"
    );
    assert_eq!(
        lineages(&state.zones.hand),
        [2, 20].map(crate::test_support::scenario_lineage)
    );
}

#[test]
fn deep_breath_empty_discard_at_use_only_draws() {
    let mut state = combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Melange));
    state.zones.hand = vec![CombatCard::for_scenario(CardId::DeepBreath, 1)];
    state.zones.draw_pile = cards(&[10, 11]).into();
    assert_eq!(play(&mut state), EngineState::CombatPlayerTurn);
    assert_eq!(state.rng.shuffle_rng.counter, 0);
    assert_eq!(
        lineages(&state.zones.hand),
        [crate::test_support::scenario_lineage(10)]
    );
}

#[test]
fn empty_deck_shuffle_moves_only_shuffled_discard_above_existing_draw() {
    let mut state = combat();
    state.zones.draw_pile = cards(&[10, 11]).into();
    state.zones.discard_pile = cards(&[20, 21, 22]).into();
    let mut expected_rng = state.rng.shuffle_rng.clone();
    let mut expected = cards(&[20, 21, 22]);
    shuffle_with_random_long(&mut expected, &mut expected_rng);
    expected.reverse();
    expected.extend(cards(&[10, 11]));
    state.queue_action_back(Action::EmptyDeckShuffle);
    let mut engine = EngineState::CombatProcessing;
    settle(&mut engine, &mut state);
    assert_eq!(lineages(&state.zones.draw_pile), lineages(&expected));
    assert!(state.zones.discard_pile.is_empty());
    assert_eq!(state.rng.shuffle_rng, expected_rng);
}

#[test]
fn shuffle_constructors_interleave_relic_actions_with_neighboring_actions() {
    for shuffle in [Action::EmptyDeckShuffle, Action::ShuffleAllIntoDraw] {
        let mut state = combat();
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::Melange));
        state.zones.draw_pile = cards(&[10, 11]).into();
        let initial_energy = state.turn.energy;
        state.queue_actions(smallvec::smallvec![
            ActionInfo {
                action: Action::GainEnergy { amount: 7 },
                insertion_mode: AddTo::Bottom
            },
            ActionInfo {
                action: shuffle,
                insertion_mode: AddTo::Bottom
            },
            ActionInfo {
                action: Action::GainEnergy { amount: 11 },
                insertion_mode: AddTo::Bottom
            },
        ]);
        let mut engine = EngineState::CombatProcessing;
        settle(&mut engine, &mut state);
        assert!(matches!(
            engine,
            EngineState::PendingChoice(PendingChoice::ScrySelect { .. })
        ));
        assert_eq!(state.turn.energy, initial_energy + 7, "the preceding action runs before constructor-queued Scry, the following action after it");
        assert_eq!(state.rng.shuffle_rng.counter, 0);
    }
}

#[test]
fn shuffle_draw_pile_consumes_seed_for_empty_and_singleton_groups() {
    for size in 0..=1 {
        let mut state = combat();
        state.zones.draw_pile = cards(&[10][..size]).into();
        let before = state.zones.draw_pile.clone();
        let mut expected_rng = state.rng.shuffle_rng.clone();
        expected_rng.random_long();
        crate::engine::action_handlers::execute_action(
            Action::ShuffleDrawPile {
                trigger_relics: false,
            },
            &mut state,
        );
        assert_eq!(state.zones.draw_pile, before);
        assert_eq!(
            state.rng.shuffle_rng, expected_rng,
            "Java seeds Collections.shuffle even if there are no swaps"
        );
    }
}

#[test]
fn reboot_empty_discard_still_shuffles_twice() {
    let mut state = combat();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Reboot, 1)];
    state.zones.draw_pile = cards(&[10]).into();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Abacus));
    assert_eq!(play(&mut state), EngineState::CombatPlayerTurn);
    assert_eq!(
        state.rng.shuffle_rng.counter, 3,
        "two explicit shuffles, then oversized draw's empty reshuffle"
    );
    assert_eq!(state.entities.player.block, 12);
    assert_eq!(state.zones.exhaust_pile.len(), 1);
    assert_eq!(state.zones.exhaust_pile[0].id, CardId::Reboot);
    assert_eq!(
        lineages(&state.zones.hand),
        [crate::test_support::scenario_lineage(10)]
    );
}
