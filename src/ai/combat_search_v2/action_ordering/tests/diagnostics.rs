use super::*;

#[test]
fn ordering_collector_reports_awakened_strength_transition_window() {
    let mut combat = blank_test_combat();
    combat.turn.energy = 1;
    let mut awakened = test_monster(EnemyId::AwakenedOne);
    awakened.id = 7;
    awakened.current_hp = 6;
    awakened.max_hp = 300;
    awakened.awakened_one.form1 = true;
    combat.entities.monsters = vec![awakened];
    combat.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 4,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::DarkShackles, 10),
        CombatCard::for_scenario(CardId::Strike, 11),
    ];
    let ordered = order_action_choices(
        &EngineState::CombatPlayerTurn,
        &combat,
        vec![
            CombatActionChoice::from_input(
                &combat,
                ClientInput::PlayCard {
                    card_index: 1,
                    target: Some(7),
                },
            ),
            CombatActionChoice::from_input(
                &combat,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(7),
                },
            ),
        ],
    );
    let mut collector = ActionOrderingDiagnosticsCollector::default();
    collector.observe(&ordered.summary);
    let report = collector.finish();

    let sample = report
        .action_effect_samples
        .iter()
        .find(|sample| sample.action_key.contains("card:DarkShackles"))
        .expect("transition setup sample");
    assert!(sample.phase.awakened_one_strength_transition);
    assert_eq!(sample.phase.convertible_positive_strength, 4);
    assert!(sample.phase.remaining_damage_upper_bound >= sample.phase.phase_one_hp_with_block);
}

#[test]
fn ordering_collector_reports_role_counts_without_action_tree() {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::Cultist);
    monster.current_hp = 6;
    monster.max_hp = 6;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let ordered = order_action_choices(
        &EngineState::CombatPlayerTurn,
        &combat,
        vec![
            CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
            CombatActionChoice::from_input(
                &combat,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(1),
                },
            ),
        ],
    );
    let mut collector = ActionOrderingDiagnosticsCollector::default();

    collector.observe(&ordered.summary);
    let report = collector.finish();

    assert_eq!(
        report.behavioral_effect,
        "child_generation_order_only_no_prune_no_merge"
    );
    assert_eq!(report.states_reordered, 1);
    assert_eq!(report.max_position_shift, 1);
    assert_eq!(report.largest_reorders.len(), 1);
    assert_eq!(report.largest_reorders[0].first_role, "lethal_card");
    assert_eq!(report.attack_retaliation_actions, 0);
    assert_eq!(report.attack_retaliation_trigger_count_hint, 0);
    assert_eq!(report.attack_retaliation_raw_player_damage_hint, 0);
    assert_eq!(report.attack_retaliation_player_block_loss_hint, 0);
    assert_eq!(report.attack_retaliation_player_hp_loss_hint, 0);
    assert_eq!(report.max_attack_retaliation_player_hp_loss_hint, 0);
}

#[test]
fn ordering_collector_reports_attack_retaliation_attribution_and_exposure() {
    let mut combat = blank_test_combat();
    combat.entities.player.block = 5;
    let mut target = test_monster(EnemyId::Spiker);
    target.id = 1;
    target.current_hp = 40;
    target.max_hp = 40;
    combat.entities.monsters = vec![target];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Thorns,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::TwinStrike, 11),
    ];
    let ordered = order_action_choices(
        &EngineState::CombatPlayerTurn,
        &combat,
        vec![
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
        ],
    );
    let mut collector = ActionOrderingDiagnosticsCollector::default();

    collector.observe(&ordered.summary);
    let report = collector.finish();

    assert_eq!(report.attack_retaliation_actions, 2);
    assert_eq!(report.attack_retaliation_trigger_count_hint, 3);
    assert_eq!(report.attack_retaliation_raw_player_damage_hint, 9);
    assert_eq!(report.attack_retaliation_player_block_loss_hint, 8);
    assert_eq!(report.attack_retaliation_player_hp_loss_hint, 1);
    assert_eq!(report.max_attack_retaliation_player_hp_loss_hint, 1);
    let twin = report
        .action_effect_samples
        .iter()
        .find(|sample| sample.action_key.contains("card:TwinStrike"))
        .expect("Twin Strike retaliation sample");
    assert_eq!(twin.reactive.attack_retaliation_trigger_count_hint, 2);
    assert_eq!(twin.reactive.attack_retaliation_raw_player_damage_hint, 6);
    assert_eq!(twin.reactive.attack_retaliation_player_block_loss_hint, 5);
    assert_eq!(twin.reactive.attack_retaliation_player_hp_loss_hint, 1);
}

#[test]
fn ordering_collector_caps_action_effect_samples() {
    let mut combat = blank_test_combat();
    let mut nob = test_monster(EnemyId::GremlinNob);
    nob.id = 1;
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
    let ordered = order_action_choices(
        &EngineState::CombatPlayerTurn,
        &combat,
        vec![
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
        ],
    );
    let mut collector = ActionOrderingDiagnosticsCollector::default();

    for _ in 0..(ACTION_EFFECT_SAMPLE_LIMIT + 3) {
        collector.observe(&ordered.summary);
    }
    let report = collector.finish();

    assert_eq!(
        report.action_effect_actions,
        (ACTION_EFFECT_SAMPLE_LIMIT + 3) as u64
    );
    assert_eq!(
        report.action_effect_samples.len(),
        ACTION_EFFECT_SAMPLE_LIMIT
    );
    assert!(report.action_effect_samples[0].reactive.enemy_strength_gain > 0);
    assert!(report.action_effect_samples[0].derived.reactive_risk_score > 0);
}

#[test]
fn ordering_collector_counts_phase_hint_actions() {
    let mut combat = blank_test_combat();
    let mut lagavulin = test_monster(EnemyId::Lagavulin);
    lagavulin.id = 1;
    lagavulin.lagavulin.is_out = false;
    combat.entities.monsters = vec![lagavulin];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let ordered = order_action_choices(
        &EngineState::CombatPlayerTurn,
        &combat,
        vec![CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        )],
    );
    let mut collector = ActionOrderingDiagnosticsCollector::default();

    collector.observe(&ordered.summary);
    let report = collector.finish();

    assert_eq!(report.phase_action_hint_actions, 1);
}
