use super::*;
use std::collections::HashMap;

fn grid_select(uuids: impl IntoIterator<Item = u32>) -> ClientInput {
    ClientInput::SubmitSelection(crate::state::selection::SelectionResolution::card_lineages(
        crate::state::selection::SelectionScope::Grid,
        crate::test_support::scenario_lineages(uuids),
    ))
}

#[test]
fn combat_ordering_keeps_original_action_ids_after_reordering() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::Cultist);
    monster.current_hp = 6;
    monster.max_hp = 6;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let choices = vec![
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 1);
    assert!(matches!(
        ordered.choices[0].choice.input,
        ClientInput::PlayCard { .. }
    ));
    assert_eq!(ordered.choices[1].original_action_id, 0);
    assert_eq!(ordered.summary.max_position_shift, 1);
    assert_eq!(
        ordered.summary.first_role,
        Some(ActionOrderingRole::LethalCard)
    );
}

#[test]
fn root_action_prior_reorders_equal_role_actions_without_pruning() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.id = 1;
    monster.current_hp = 40;
    monster.max_hp = 40;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Strike, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];
    let favored_action_key = choices[1].action_key.clone();
    let exact_state_hash = combat_exact_state_hash_v2(&EngineState::CombatPlayerTurn, &combat);
    let prior = CombatSearchV2RootActionPrior::from_scores(HashMap::from([(
        exact_state_hash,
        HashMap::from([(favored_action_key, 0.9)]),
    )]));

    let ordered = order_indexed_action_choices_with_prior(
        &EngineState::CombatPlayerTurn,
        &combat,
        choices
            .into_iter()
            .enumerate()
            .map(|(original_action_id, choice)| IndexedActionChoice {
                original_action_id,
                choice,
            })
            .collect(),
        Some(&prior),
        CombatSearchPhaseGuardPluginId::Default,
        CombatSearchActionPriorPluginId::Default,
    );

    assert_eq!(ordered.choices.len(), 2);
    assert_eq!(ordered.summary.root_action_prior_scored_actions, 1);
    assert_eq!(ordered.choices[0].original_action_id, 1);
    assert_eq!(ordered.choices[1].original_action_id, 0);
}

#[test]
fn root_action_prior_can_reorder_within_the_same_semantic_role() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.id = 1;
    monster.current_hp = 40;
    monster.max_hp = 40;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::HeavyBlade, 10),
        CombatCard::for_scenario(CardId::Strike, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];
    let favored_action_key = choices[1].action_key.clone();
    let exact_state_hash = combat_exact_state_hash_v2(&EngineState::CombatPlayerTurn, &combat);
    let prior = CombatSearchV2RootActionPrior::from_scores(HashMap::from([(
        exact_state_hash,
        HashMap::from([(favored_action_key, 0.9)]),
    )]));

    let ordered = order_indexed_action_choices_with_prior(
        &EngineState::CombatPlayerTurn,
        &combat,
        choices
            .into_iter()
            .enumerate()
            .map(|(original_action_id, choice)| IndexedActionChoice {
                original_action_id,
                choice,
            })
            .collect(),
        Some(&prior),
        CombatSearchPhaseGuardPluginId::Default,
        CombatSearchActionPriorPluginId::Default,
    );

    assert_eq!(
        ordered.summary.first_role,
        Some(ActionOrderingRole::DamageProgress)
    );
    assert_eq!(ordered.choices[0].original_action_id, 1);
    assert_eq!(ordered.choices[1].original_action_id, 0);
}

#[test]
fn lethal_persistent_payoff_precedes_ordinary_lethal_even_when_prior_favors_ordinary() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.id = 1;
    monster.current_hp = 5;
    monster.max_hp = 40;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::HandOfGreed, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];
    let favored_ordinary_action_key = choices[0].action_key.clone();
    let exact_state_hash = combat_exact_state_hash_v2(&EngineState::CombatPlayerTurn, &combat);
    let prior = CombatSearchV2RootActionPrior::from_scores(HashMap::from([(
        exact_state_hash,
        HashMap::from([(favored_ordinary_action_key, 1.0)]),
    )]));

    let ordered = order_indexed_action_choices_with_prior(
        &EngineState::CombatPlayerTurn,
        &combat,
        choices
            .into_iter()
            .enumerate()
            .map(|(original_action_id, choice)| IndexedActionChoice {
                original_action_id,
                choice,
            })
            .collect(),
        Some(&prior),
        CombatSearchPhaseGuardPluginId::Default,
        CombatSearchActionPriorPluginId::Default,
    );

    assert_eq!(ordered.choices[0].original_action_id, 1);
    assert_eq!(ordered.choices[1].original_action_id, 0);
}

#[test]
fn nonlethal_persistent_payoff_does_not_precede_ordinary_lethal() {
    let mut combat = blank_test_combat();
    let mut low_hp = test_monster(EnemyId::LouseNormal);
    low_hp.id = 1;
    low_hp.current_hp = 5;
    low_hp.max_hp = 5;
    let mut high_hp = test_monster(EnemyId::JawWorm);
    high_hp.id = 2;
    high_hp.current_hp = 40;
    high_hp.max_hp = 40;
    combat.entities.monsters = vec![low_hp, high_hp];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::HandOfGreed, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(2),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 1);
}

#[test]
fn lethal_card_is_ordered_before_nonlethal_damage() {
    let mut combat = blank_test_combat();
    let mut low_hp = test_monster(EnemyId::LouseNormal);
    low_hp.current_hp = 6;
    low_hp.max_hp = 6;
    low_hp.id = 1;
    let mut high_hp = test_monster(EnemyId::JawWorm);
    high_hp.current_hp = 30;
    high_hp.max_hp = 30;
    high_hp.id = 2;
    combat.entities.monsters = vec![low_hp, high_hp];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(2),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 1);
}

#[test]
fn block_that_prevents_visible_lethal_is_ordered_before_damage() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 5;
    combat.entities.player.block = 0;
    let mut monster = test_monster(EnemyId::Cultist);
    monster.set_planned_move_id(1);
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Defend, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: None,
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 1);
}

#[test]
fn visible_lethal_still_precedes_timed_threat_damage_progress() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 5;
    let mut exploder = test_monster(EnemyId::Exploder);
    exploder.id = 1;
    exploder.current_hp = 30;
    exploder.max_hp = 30;
    exploder.set_planned_move_id(1);
    combat.entities.monsters = vec![exploder];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Explosive,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Defend, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: None,
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 1);
}

#[test]
fn persistent_enemy_strength_down_is_ordered_before_plain_damage() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::Cultist);
    monster.id = 1;
    monster.current_hp = 50;
    monster.max_hp = 50;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Disarm, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 1);
    assert_eq!(
        ordered.summary.first_role,
        Some(ActionOrderingRole::SustainedMitigation)
    );
}

#[test]
fn reactive_enemy_scaling_risk_orders_damage_before_nonlethal_skill_block() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 80;
    let mut nob = test_monster(EnemyId::GremlinNob);
    nob.id = 1;
    nob.current_hp = 83;
    nob.max_hp = 83;
    nob.set_planned_move_id(1);
    combat.entities.monsters = vec![nob];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Anger,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 10),
        CombatCard::for_scenario(CardId::Strike, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert!(matches!(
        ordered.choices[0].choice.input,
        ClientInput::PlayCard {
            card_index: 1,
            target: Some(1)
        }
    ));
    assert_eq!(
        ordered.summary.first_role,
        Some(ActionOrderingRole::DamageProgress)
    );
}

#[test]
fn current_turn_strength_setup_orders_before_available_attack() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::Cultist);
    monster.id = 1;
    monster.current_hp = 50;
    monster.max_hp = 50;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Flex, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: None,
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 1);
    assert_eq!(
        ordered.summary.first_role,
        Some(ActionOrderingRole::CurrentTurnAttackSetup)
    );
}

#[test]
fn current_turn_strength_setup_requires_available_attack() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 10),
        CombatCard::for_scenario(CardId::Flex, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: None,
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 0);
    assert_eq!(ordered.summary.first_role, Some(ActionOrderingRole::Block));
}

#[test]
fn split_trigger_risk_orders_safe_damage_first_without_dropping_actions() {
    let mut combat = blank_test_combat();
    let mut slime = test_monster(EnemyId::AcidSlimeL);
    slime.id = 1;
    slime.current_hp = 40;
    slime.max_hp = 65;
    combat.entities.monsters = vec![slime];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Split,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Carnage, 10),
        CombatCard::for_scenario(CardId::Strike, 11),
    ];
    let choices = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];

    let ordered = order_action_choices(&EngineState::CombatPlayerTurn, &combat, choices);

    assert_eq!(ordered.choices.len(), 2);
    assert_eq!(ordered.choices[0].original_action_id, 1);
    assert_eq!(ordered.choices[1].original_action_id, 0);
}

#[test]
fn non_player_turn_choices_keep_existing_order() {
    let combat = blank_test_combat();
    let choices = vec![
        CombatActionChoice::from_input(&combat, ClientInput::Proceed),
        CombatActionChoice::from_input(&combat, ClientInput::Cancel),
    ];

    let ordered = order_action_choices(&EngineState::CombatProcessing, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 0);
    assert_eq!(ordered.choices[1].original_action_id, 1);
    assert_eq!(ordered.summary.max_position_shift, 0);
}

#[test]
fn pending_choice_actions_are_ordered_without_losing_original_ids() {
    let mut combat = blank_test_combat();
    combat.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Carnage, 20),
    ]
    .into();
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
        source_pile: crate::state::core::PileType::Discard,
        candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: crate::state::core::GridSelectReason::MoveToDrawPile,
    });
    let choices = vec![
        CombatActionChoice::from_input(&combat, grid_select([10])),
        CombatActionChoice::from_input(&combat, grid_select([20])),
    ];

    let ordered = order_action_choices(&engine, &combat, choices);

    assert_eq!(ordered.choices[0].original_action_id, 1);
    assert_eq!(
        ordered.summary.first_role,
        Some(ActionOrderingRole::PendingChoiceValueSelection)
    );
    assert_eq!(ordered.summary.max_position_shift, 1);
}
