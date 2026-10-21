use super::*;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::runtime::combat::CombatCard;
use crate::runtime::combat::{Power, PowerPayload};
use crate::state::selection::{SelectionResolution, SelectionScope};
use crate::test_support::{blank_test_combat, planned_monster};

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
fn move_to_draw_prefers_higher_value_card() {
    let mut combat = blank_test_combat();
    combat.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Carnage, 20),
    ]
    .into();
    let engine = EngineState::PendingChoice(PendingChoice::GridSelect {
        source_pile: PileType::Discard,
        candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: GridSelectReason::MoveToDrawPile,
    });

    let strike = pending_choice_ordering_hint(&engine, &combat, &grid_select([10]))
        .expect("strike candidate should rank");
    let carnage = pending_choice_ordering_hint(&engine, &combat, &grid_select([20]))
        .expect("carnage candidate should rank");

    assert!(carnage.primary > strike.primary);
}

#[test]
fn upgrade_selection_prefers_higher_upgrade_delta() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Bash, 20),
    ];
    let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: HandSelectReason::Upgrade,
    });

    let strike = pending_choice_ordering_hint(&engine, &combat, &hand_select([10]))
        .expect("strike upgrade candidate should rank");
    let bash = pending_choice_ordering_hint(&engine, &combat, &hand_select([20]))
        .expect("bash upgrade candidate should rank");

    assert!(bash.primary > strike.primary);
}

#[test]
fn scry_discard_prefers_status_over_empty_selection() {
    let combat = blank_test_combat();
    let engine = EngineState::PendingChoice(PendingChoice::ScrySelect {
        cards: vec![CardId::Slimed, CardId::Bash],
        card_lineages: crate::test_support::scenario_lineages([10, 20]),
    });

    let keep_all =
        pending_choice_ordering_hint(&engine, &combat, &ClientInput::SubmitScryDiscard(vec![]))
            .expect("empty scry discard should rank");
    let discard_slimed =
        pending_choice_ordering_hint(&engine, &combat, &ClientInput::SubmitScryDiscard(vec![0]))
            .expect("slimed scry discard should rank");

    assert!(discard_slimed.primary > keep_all.primary);
    assert_eq!(
        discard_slimed.role,
        PendingChoiceOrderingRole::RemovalSelection
    );
}

#[test]
fn cancel_is_explicitly_low_priority_but_still_ranked() {
    let combat = blank_test_combat();
    let engine = EngineState::PendingChoice(PendingChoice::DiscoverySelect(
        crate::state::core::DiscoveryChoiceState {
            cards: vec![CardId::Carnage],
            colorless: false,
            card_type: None,
            amount: 1,
            can_skip: true,
        },
    ));

    let cancel = pending_choice_ordering_hint(&engine, &combat, &ClientInput::Cancel)
        .expect("cancel should rank");
    let pick =
        pending_choice_ordering_hint(&engine, &combat, &ClientInput::SubmitDiscoverChoice(0))
            .expect("pick should rank");

    assert_eq!(cancel.role, PendingChoiceOrderingRole::Cancel);
    assert!(pick.primary > cancel.primary);
}

#[test]
fn connected_second_wind_engine_preserves_wound_for_batch_conversion() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Wound, 10),
        CombatCard::for_scenario(CardId::Cleave, 20),
    ];
    combat.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::SecondWind, 30),
        CombatCard::for_scenario(CardId::PowerThrough, 40),
    ]
    .into();
    combat.entities.power_db.insert(
        combat.entities.player.id,
        vec![Power {
            power_type: PowerId::DarkEmbrace,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });

    let wound = pending_choice_ordering_hint(&engine, &combat, &hand_select([10]))
        .expect("Wound candidate should rank");
    let cleave = pending_choice_ordering_hint(&engine, &combat, &hand_select([20]))
        .expect("Cleave candidate should rank");

    assert!(
        cleave.primary > wound.primary,
        "connected batch conversion should preserve Wound: wound={wound:?} cleave={cleave:?}"
    );
    assert_eq!(wound.policy_log2_bias, -10);
    assert_eq!(cleave.policy_log2_bias, 0);
}

#[test]
fn ordinary_exhaust_still_prefers_removing_wound() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Wound, 10),
        CombatCard::for_scenario(CardId::Cleave, 20),
    ];
    let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });

    let wound = pending_choice_ordering_hint(&engine, &combat, &hand_select([10]))
        .expect("Wound candidate should rank");
    let cleave = pending_choice_ordering_hint(&engine, &combat, &hand_select([20]))
        .expect("Cleave candidate should rank");

    assert!(wound.primary > cleave.primary);
    assert_eq!(wound.policy_log2_bias, 0);
    assert_eq!(cleave.policy_log2_bias, 0);
}

#[test]
fn exhaust_preserves_visible_block_and_strength_control_over_ordinary_damage() {
    let mut combat = blank_test_combat();
    combat.turn.energy = 2;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Disarm, 10),
        CombatCard::for_scenario(CardId::Defend, 20),
        CombatCard::for_scenario(CardId::Cleave, 30),
    ];
    combat.entities.monsters = vec![planned_monster(EnemyId::TimeEater, 2)];
    let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([10, 20, 30]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });

    let disarm = pending_choice_ordering_hint(&engine, &combat, &hand_select([10]))
        .expect("Disarm candidate should rank");
    let defend = pending_choice_ordering_hint(&engine, &combat, &hand_select([20]))
        .expect("Defend candidate should rank");
    let cleave = pending_choice_ordering_hint(&engine, &combat, &hand_select([30]))
        .expect("Cleave candidate should rank");

    assert!(cleave.primary > disarm.primary);
    assert!(cleave.primary > defend.primary);
    assert_eq!(disarm.policy_log2_bias, -6);
    assert_eq!(defend.policy_log2_bias, -6);
    assert_eq!(cleave.policy_log2_bias, 0);
}

#[test]
fn exhaust_does_not_protect_block_without_visible_pressure() {
    let mut combat = blank_test_combat();
    combat.turn.energy = 2;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 20),
        CombatCard::for_scenario(CardId::Cleave, 30),
    ];
    let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([20, 30]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });

    let defend = pending_choice_ordering_hint(&engine, &combat, &hand_select([20]))
        .expect("Defend candidate should rank");
    let cleave = pending_choice_ordering_hint(&engine, &combat, &hand_select([30]))
        .expect("Cleave candidate should rank");

    assert_eq!(defend.primary, cleave.primary);
    assert_eq!(defend.policy_log2_bias, 0);
    assert_eq!(cleave.policy_log2_bias, 0);
}
