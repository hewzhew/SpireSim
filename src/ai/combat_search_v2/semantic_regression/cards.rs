use super::*;
use crate::state::selection::{SelectionResolution, SelectionScope};

fn hand_select(uuids: impl IntoIterator<Item = u32>) -> ClientInput {
    ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Hand,
        crate::test_support::scenario_lineages(uuids),
    ))
}

fn grid_select(uuids: impl IntoIterator<Item = u32>) -> ClientInput {
    ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Grid,
        crate::test_support::scenario_lineages(uuids),
    ))
}

#[test]
fn stepper_dropkick_against_vulnerable_draws_and_refunds_energy() {
    let mut combat = blank_test_combat();
    combat.turn.energy = 1;
    combat.entities.monsters = vec![monster(EnemyId::JawWorm, 10, 0, 30)];
    store::set_powers_for(&mut combat, 10, vec![power(PowerId::Vulnerable, 3)]);
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Dropkick, 100)];
    combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 101)]).into();

    let step = apply_from_player_turn(
        combat,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(10),
        },
    );

    assert_stable_player_turn(&step);
    assert_eq!(step.terminal, CombatTerminal::Unresolved);
    assert_eq!(
        step.position.combat.turn.energy, 1,
        "Dropkick should spend 1 energy then refund 1 when the target is Vulnerable"
    );
    assert_eq!(
        step.position.combat.entities.monsters[0].current_hp, 23,
        "Dropkick damage should use the target's Vulnerable state at execution"
    );
    assert_eq!(
        card_snapshots(&step.position.combat.zones.hand),
        vec![(CardId::Strike, crate::test_support::scenario_lineage(101))],
        "Dropkick should draw one card after the damage/effect action resolves"
    );
}

#[test]
fn stepper_headbutt_grid_select_moves_selected_discard_card_to_draw_top() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![monster(EnemyId::JawWorm, 10, 0, 40)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Headbutt, 100)];
    combat.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 201),
        CombatCard::for_scenario(CardId::Defend, 202),
    ]
    .into();

    let after_headbutt = apply_from_player_turn(
        combat,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(10),
        },
    );

    assert!(!after_headbutt.truncated);
    match &after_headbutt.position.engine {
        EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile,
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        }) => {
            assert_eq!(*source_pile, PileType::Discard);
            assert_eq!(
                candidate_lineages,
                &crate::test_support::scenario_lineages([201, 202])
            );
            assert_eq!((*min_cards, *max_cards, *can_cancel), (1, 1, false));
            assert_eq!(*reason, GridSelectReason::MoveToDrawPile);
        }
        other => panic!("Headbutt should suspend on a discard grid select, got {other:?}"),
    }

    assert!(is_legal_action(
        &after_headbutt.position,
        &grid_select([202])
    ));
    assert!(
        !is_legal_action(&after_headbutt.position, &ClientInput::Proceed),
        "pending grid select must not expose fake Proceed to search"
    );

    let after_select = apply(&after_headbutt.position, grid_select([202]));

    assert_stable_player_turn(&after_select);
    assert_eq!(
        card_snapshots(&after_select.position.combat.zones.draw_pile),
        vec![(CardId::Defend, crate::test_support::scenario_lineage(202))]
    );
    let discard = card_snapshots(&after_select.position.combat.zones.discard_pile);
    assert!(discard.contains(&(CardId::Headbutt, crate::test_support::scenario_lineage(100))));
    assert!(discard.contains(&(CardId::Strike, crate::test_support::scenario_lineage(201))));
    assert!(
        !discard.contains(&(CardId::Defend, crate::test_support::scenario_lineage(202))),
        "the selected Headbutt card should no longer remain in discard"
    );
}

#[test]
fn stepper_armaments_base_resolves_upgrade_pending_choice() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![monster(EnemyId::JawWorm, 10, 0, 40)];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Armaments, 100),
        CombatCard::for_scenario(CardId::Strike, 101),
        CombatCard::for_scenario(CardId::Bash, 102),
    ];

    let after_armaments = apply_from_player_turn(
        combat,
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
    );

    assert!(!after_armaments.truncated);
    assert_eq!(after_armaments.position.combat.entities.player.block, 5);
    match &after_armaments.position.engine {
        EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        }) => {
            assert_eq!(
                candidate_lineages,
                &crate::test_support::scenario_lineages([101, 102])
            );
            assert_eq!((*min_cards, *max_cards, *can_cancel), (1, 1, false));
            assert_eq!(*reason, HandSelectReason::Upgrade);
        }
        other => panic!("base Armaments should suspend on an upgrade hand select, got {other:?}"),
    }

    assert!(is_legal_action(
        &after_armaments.position,
        &hand_select([102])
    ));
    assert!(
        !is_legal_action(&after_armaments.position, &ClientInput::Proceed),
        "pending hand select must not expose fake Proceed to search"
    );

    let after_select = apply(&after_armaments.position, hand_select([102]));

    assert_stable_player_turn(&after_select);
    let bash = after_select
        .position
        .combat
        .zones
        .hand
        .iter()
        .find(|card| card.lineage == crate::test_support::scenario_lineage(102))
        .expect("selected Bash should remain in hand after Armaments resolves");
    assert_eq!(bash.upgrades, 1);
}

#[test]
fn stepper_armaments_plus_upgrades_all_eligible_hand_cards_without_pending_choice() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![monster(EnemyId::JawWorm, 10, 0, 40)];
    let mut armaments_plus = CombatCard::for_scenario(CardId::Armaments, 100);
    armaments_plus.upgrades = 1;
    let mut already_upgraded_defend = CombatCard::for_scenario(CardId::Defend, 102);
    already_upgraded_defend.upgrades = 1;
    combat.zones.hand = vec![
        armaments_plus,
        CombatCard::for_scenario(CardId::Strike, 101),
        already_upgraded_defend,
        CombatCard::for_scenario(CardId::Wound, 103),
    ];

    let step = apply_from_player_turn(
        combat,
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
    );

    assert_stable_player_turn(&step);
    assert_eq!(step.position.combat.entities.player.block, 5);
    let hand = &step.position.combat.zones.hand;
    assert_eq!(
        hand.iter()
            .find(|card| card.lineage == crate::test_support::scenario_lineage(101))
            .map(|card| card.upgrades),
        Some(1),
        "Armaments+ should upgrade eligible cards left in hand"
    );
    assert_eq!(
        hand.iter()
            .find(|card| card.lineage == crate::test_support::scenario_lineage(102))
            .map(|card| card.upgrades),
        Some(1),
        "already-upgraded non-Searing Blow cards should not gain an extra upgrade"
    );
    assert_eq!(
        hand.iter()
            .find(|card| card.lineage == crate::test_support::scenario_lineage(103))
            .map(|card| card.upgrades),
        Some(0),
        "statuses are not Armaments upgrade targets"
    );
}
