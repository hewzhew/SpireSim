use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::content::relics::RelicId;
use crate::runtime::combat::{CombatCard, Power, PowerPayload};
use crate::state::core::EngineState;

#[test]
fn policy_weights_preserve_every_legal_input() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 11)];
    combat.turn.energy = 1;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = EngineCombatStepper.atomic_actions(&position);

    let weights = oracle_atomic_action_policy_weights(&position, &inputs);

    assert_eq!(weights.len(), inputs.len());
    assert!(weights
        .iter()
        .all(|weight| weight.is_finite() && *weight > 0.0));
}

#[test]
fn trusted_legal_policy_path_matches_validated_ordering() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 11),
        CombatCard::for_scenario(CardId::Defend, 12),
    ];
    combat.turn.energy = 1;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = EngineCombatStepper.atomic_actions(&position);
    let input_refs = inputs.iter().collect::<Vec<_>>();

    assert_eq!(
        oracle_legal_atomic_action_policy_weights_for_refs(&position, &input_refs),
        oracle_atomic_action_policy_weights(&position, &inputs)
    );
}

#[test]
fn equivalent_card_instances_share_one_policy_rank() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 11),
        CombatCard::for_scenario(CardId::Defend, 12),
        CombatCard::for_scenario(CardId::Defend, 13),
    ];
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 1,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 2,
            target: None,
        },
    ];
    let input_refs = inputs.iter().collect::<Vec<_>>();

    let validated = oracle_atomic_action_policy_weights(&position, &inputs);
    let trusted = oracle_legal_atomic_action_policy_weights_for_refs(&position, &input_refs);

    assert_eq!(validated, trusted);
    assert_eq!(validated, vec![1.0, 1.0, 1.0]);
}

#[test]
fn symmetric_sentry_targets_share_one_policy_rank() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut left = crate::test_support::planned_monster(EnemyId::Sentry, 1);
    left.id = 1;
    left.slot = 0;
    left.logical_position = -1;
    left.current_hp = 44;
    left.max_hp = 44;
    let mut middle = crate::test_support::planned_monster(EnemyId::Sentry, 2);
    middle.id = 2;
    middle.slot = 1;
    middle.logical_position = 0;
    middle.current_hp = 44;
    middle.max_hp = 44;
    let mut right = crate::test_support::planned_monster(EnemyId::Sentry, 1);
    right.id = 3;
    right.slot = 2;
    right.logical_position = 1;
    right.current_hp = 44;
    right.max_hp = 44;
    combat.entities.monsters = vec![left, middle, right];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 11)];
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(3),
        },
    ];
    let input_refs = inputs.iter().collect::<Vec<_>>();

    let validated = oracle_atomic_action_policy_weights(&position, &inputs);
    let trusted = oracle_legal_atomic_action_policy_weights_for_refs(&position, &input_refs);

    assert_eq!(validated, trusted);
    assert_eq!(validated[0], validated[1]);
}

#[test]
fn distinct_semantic_priorities_still_receive_distinct_weights() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.current_hp = 6;
    monster.max_hp = 6;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 11)];
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        ClientInput::EndTurn,
    ];

    let weights = oracle_atomic_action_policy_weights(&position, &inputs);

    assert!(weights[0] > weights[1], "weights={weights:?}");
}

#[test]
fn trusted_legal_policy_path_matches_validated_semantic_bias() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.player.current_hp = 80;
    let mut attacker = crate::test_support::planned_monster(EnemyId::TimeEater, 2);
    attacker.id = 2;
    combat.entities.monsters = vec![attacker];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::BurningPact, 11),
        CombatCard::for_scenario(CardId::Defend, 12),
    ];
    combat.turn.energy = 3;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = EngineCombatStepper.atomic_actions(&position);
    let input_refs = inputs.iter().collect::<Vec<_>>();

    let validated = oracle_atomic_action_policy_weights(&position, &inputs);
    let trusted = oracle_legal_atomic_action_policy_weights_for_refs(&position, &input_refs);
    assert_eq!(trusted, validated);

    let burning_pact_index = inputs
        .iter()
        .position(|input| {
            matches!(
                input,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: None
                }
            )
        })
        .expect("Burning Pact should be a legal atomic action");
    let diagnostics = oracle_atomic_action_policy_priority_diagnostics_v1(&position, &inputs);
    assert_eq!(
        diagnostics[burning_pact_index]
            .as_ref()
            .expect("Burning Pact should have priority diagnostics")
            .policy_log2_bias,
        6
    );
    assert!(trusted[burning_pact_index] > 1.0);
}

#[test]
fn connected_second_wind_wound_bias_reaches_both_policy_paths() {
    let mut combat = crate::test_support::blank_test_combat();
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
    let position = CombatPosition::new(
        EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
            candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::core::HandSelectReason::Exhaust,
        }),
        combat,
    );
    let inputs = vec![
        ClientInput::SubmitSelection(crate::state::selection::SelectionResolution::card_lineages(
            crate::state::selection::SelectionScope::Hand,
            [crate::test_support::scenario_lineage(10)],
        )),
        ClientInput::SubmitSelection(crate::state::selection::SelectionResolution::card_lineages(
            crate::state::selection::SelectionScope::Hand,
            [crate::test_support::scenario_lineage(20)],
        )),
    ];
    let input_refs = inputs.iter().collect::<Vec<_>>();

    let validated = oracle_atomic_action_policy_weights(&position, &inputs);
    let trusted = oracle_legal_atomic_action_policy_weights_for_refs(&position, &input_refs);
    assert_eq!(trusted, validated);
    assert!(
        trusted[1] > trusted[0] * 1_000.0,
        "Wound should be suppressed by a semantic scale: {trusted:?}"
    );
    let diagnostics = oracle_atomic_action_policy_priority_diagnostics_v1(&position, &inputs);
    assert_eq!(
        diagnostics[0]
            .as_ref()
            .expect("Wound choice should have priority diagnostics")
            .policy_log2_bias,
        -10
    );
}

#[test]
fn oracle_policy_uses_time_eater_haste_window_knowledge_automatically() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut eater = crate::test_support::test_monster(EnemyId::TimeEater);
    eater.id = 1;
    eater.current_hp = 200;
    eater.max_hp = 456;
    eater.time_eater.used_haste = true;
    eater.set_planned_move_id(5);
    combat.entities.monsters = vec![eater];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Cleave, 11),
        CombatCard::for_scenario(CardId::Defend, 12),
    ];
    combat.turn.energy = 3;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 1,
            target: None,
        },
    ];

    let weights = oracle_atomic_action_policy_weights(&position, &inputs);

    assert!(
        weights[1] > weights[0],
        "setup/mitigation must outrank nonlethal damage that Haste will erase"
    );
}

#[test]
fn awakened_policy_preserves_key_setup_without_promoting_every_power() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut awakened = crate::test_support::test_monster(EnemyId::AwakenedOne);
    awakened.id = 1;
    awakened.current_hp = 300;
    awakened.max_hp = 300;
    combat.entities.monsters = vec![awakened];
    combat
        .entities
        .power_db
        .insert(1, vec![test_power_amount(PowerId::Curiosity, 1)]);
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::DemonForm, 11),
        CombatCard::for_scenario(CardId::Evolve, 12),
        CombatCard::for_scenario(CardId::Strike, 13),
    ];
    combat.turn.energy = 6;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 1,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 2,
            target: Some(1),
        },
    ];

    let weights = oracle_atomic_action_policy_weights(&position, &inputs);

    assert!(
        weights[0] > weights[2],
        "Demon Form must retain long-fight setup relevance under Curiosity"
    );
    assert!(
        weights[2] > weights[1],
        "a low-impact Power must still pay the ordinary Curiosity penalty"
    );
}

#[test]
fn awakened_key_setup_prior_does_not_override_immediate_lethal() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut awakened = crate::test_support::test_monster(EnemyId::AwakenedOne);
    awakened.id = 1;
    awakened.current_hp = 6;
    awakened.max_hp = 300;
    combat.entities.monsters = vec![awakened];
    combat
        .entities
        .power_db
        .insert(1, vec![test_power_amount(PowerId::Curiosity, 1)]);
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::DemonForm, 11),
        CombatCard::for_scenario(CardId::Strike, 12),
    ];
    combat.turn.energy = 6;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 1,
            target: Some(1),
        },
    ];

    let weights = oracle_atomic_action_policy_weights(&position, &inputs);

    assert!(weights[1] > weights[0]);
}

#[test]
fn disarm_policy_prefers_a_target_without_artifact() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut blocked = crate::test_support::test_monster(EnemyId::Cultist);
    blocked.id = 1;
    let mut exposed = crate::test_support::test_monster(EnemyId::Cultist);
    exposed.id = 2;
    combat.entities.monsters = vec![blocked, exposed];
    combat
        .entities
        .power_db
        .insert(1, vec![test_power_amount(PowerId::Artifact, 1)]);
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Disarm, 11)];
    combat.turn.energy = 1;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(2),
        },
    ];

    let weights = oracle_atomic_action_policy_weights(&position, &inputs);

    assert!(weights[1] > weights[0]);
}

#[test]
fn dexterity_does_not_turn_artifact_blocked_disarm_into_a_block_card() {
    use crate::runtime::monster_move::{AttackSpec, DamageKind, MonsterMoveSpec};

    let mut combat = crate::test_support::blank_test_combat();
    let mut attacker = crate::test_support::test_monster(EnemyId::Deca);
    attacker.id = 1;
    let attack = MonsterMoveSpec::Attack(AttackSpec {
        base_damage: 16,
        hits: 2,
        damage_kind: DamageKind::Normal,
    });
    attacker.set_planned_steps(attack.to_steps());
    attacker.set_planned_visible_spec(Some(attack));
    combat.entities.monsters = vec![attacker];
    combat
        .entities
        .power_db
        .insert(1, vec![test_power_amount(PowerId::Artifact, 1)]);
    let player = combat.entities.player.id;
    combat.entities.power_db.insert(
        player,
        vec![
            test_power_amount(PowerId::FeelNoPain, 4),
            test_power_amount(PowerId::Dexterity, 6),
        ],
    );
    let mut bash = CombatCard::for_scenario(CardId::Bash, 10);
    bash.upgrades = 1;
    combat.zones.hand = vec![bash, CombatCard::for_scenario(CardId::Disarm, 11)];
    combat.turn.energy = 3;
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        ClientInput::PlayCard {
            card_index: 1,
            target: Some(1),
        },
    ];

    let exposed = CombatPosition::new(EngineState::CombatPlayerTurn, combat.clone());
    let exposed_weights = oracle_atomic_action_policy_weights(&exposed, &inputs);
    assert!(
        exposed_weights[0] > exposed_weights[1],
        "four incidental block must not outrank Bash while most damage remains"
    );

    assert!(
        exposed_weights[1].is_finite() && exposed_weights[1] > 0.0,
        "the blocked Disarm remains a legal searchable action"
    );
}

#[test]
fn donu_damage_outranks_equal_deca_damage_without_forcing_a_target() {
    let mut base = crate::test_support::blank_test_combat();
    let mut deca = crate::test_support::test_monster(EnemyId::Deca);
    deca.id = 1;
    deca.current_hp = 250;
    deca.max_hp = 250;
    let mut donu = crate::test_support::test_monster(EnemyId::Donu);
    donu.id = 2;
    donu.current_hp = 250;
    donu.max_hp = 250;
    base.entities.monsters = vec![deca, donu];

    let mut damaged_deca = base.clone();
    damaged_deca.entities.monsters[0].current_hp = 230;
    let mut damaged_donu = base;
    damaged_donu.entities.monsters[1].current_hp = 230;

    let deca_rank = oracle_combat_state_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        damaged_deca,
    ));
    let donu_rank = oracle_combat_state_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        damaged_donu,
    ));

    assert!(donu_rank > deca_rank);
}

#[test]
fn thief_damage_outranks_equal_damage_spread_to_an_ordinary_enemy() {
    let mut base = crate::test_support::blank_test_combat();
    let mut looter = crate::test_support::test_monster(EnemyId::Looter);
    looter.id = 1;
    looter.current_hp = 50;
    looter.max_hp = 50;
    let mut jaw_worm = crate::test_support::test_monster(EnemyId::JawWorm);
    jaw_worm.id = 2;
    jaw_worm.current_hp = 50;
    jaw_worm.max_hp = 50;
    base.entities.monsters = vec![looter, jaw_worm];

    let mut damaged_ordinary = base.clone();
    damaged_ordinary.entities.monsters[1].current_hp = 38;
    let mut damaged_thief = base;
    damaged_thief.entities.monsters[0].current_hp = 38;

    let ordinary_rank = oracle_combat_state_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        damaged_ordinary,
    ));
    let thief_rank = oracle_combat_state_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        damaged_thief,
    ));

    assert!(thief_rank > ordinary_rank);
}

#[test]
fn thief_completed_recovery_corridor_outranks_equal_distributed_damage() {
    let mut base = crate::test_support::blank_test_combat();
    let mut looter = crate::test_support::test_monster(EnemyId::Looter);
    looter.id = 1;
    looter.current_hp = 50;
    looter.max_hp = 50;
    let mut mugger = crate::test_support::test_monster(EnemyId::Mugger);
    mugger.id = 2;
    mugger.current_hp = 50;
    mugger.max_hp = 50;
    base.entities.monsters = vec![looter, mugger];

    let mut distributed = base.clone();
    distributed.entities.monsters[0].current_hp = 25;
    distributed.entities.monsters[1].current_hp = 25;
    let mut completed = base;
    completed.entities.monsters[0].current_hp = 0;

    let distributed_rank = oracle_combat_state_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        distributed.clone(),
    ));
    let completed_rank = oracle_combat_state_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        completed.clone(),
    ));

    assert!(completed_rank > distributed_rank);

    let distributed_turn_rank = oracle_combat_turn_generation_guide_components(
        &CombatPosition::new(EngineState::CombatPlayerTurn, distributed),
    );
    let completed_turn_rank = oracle_combat_turn_generation_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        completed,
    ));
    assert!(completed_turn_rank > distributed_turn_rank);
}

#[test]
fn thief_owner_progress_accumulates_across_looter_and_mugger_corridor() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut looter = crate::test_support::test_monster(EnemyId::Looter);
    looter.current_hp = 30;
    looter.max_hp = 50;
    let mut mugger = crate::test_support::test_monster(EnemyId::Mugger);
    mugger.current_hp = 0;
    mugger.max_hp = 48;
    combat.entities.monsters = vec![looter, mugger];

    assert_eq!(
        encounter_priority_owner_progress(&combat),
        EncounterPriorityOwnerProgress {
            completed_targets: 1,
            focused_damage: 48,
            total_damage: 68,
        }
    );
}

#[test]
fn ordinal_rank_guidance_is_weak_rather_than_exponential() {
    assert_eq!(oracle_ordinal_rank_weight(0), 1.0);
    assert_eq!(oracle_ordinal_rank_weight(1), 0.5);
    assert_eq!(oracle_ordinal_rank_weight(2), 1.0 / 3.0);
    assert_eq!(oracle_ordinal_rank_weight(15), 1.0 / 16.0);
}

#[test]
fn explicit_semantic_bias_uses_bounded_base_two_scale() {
    assert_eq!(oracle_rank_weight_with_semantic_bias(0, 6), 64.0);
    assert_eq!(oracle_rank_weight_with_semantic_bias(0, -10), 1.0 / 1_024.0);
    assert_eq!(
        oracle_rank_weight_with_semantic_bias(0, 21),
        oracle_rank_weight_with_semantic_bias(0, 20)
    );
    assert_eq!(
        oracle_rank_weight_with_semantic_bias(0, -21),
        oracle_rank_weight_with_semantic_bias(0, -20)
    );
}

#[test]
fn horizon_guide_exposes_turn_depth_as_its_primary_independent_rank() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    combat.turn.turn_count = 7;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

    let rank = oracle_combat_horizon_guide_components(&position);

    assert_eq!(rank.first(), Some(&7));
}

#[test]
fn turn_generation_horizon_uses_realized_in_turn_depth() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    let shallow = CombatPosition::new(EngineState::CombatPlayerTurn, combat.clone());
    combat.turn.counters.cards_played_this_turn = 3;
    let deep = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

    let shallow_rank = oracle_combat_turn_generation_guide_components(&shallow);
    let deep_rank = oracle_combat_turn_generation_guide_components(&deep);

    assert_eq!(shallow_rank.first(), Some(&0));
    assert_eq!(deep_rank.first(), Some(&3));
    assert!(deep_rank > shallow_rank);
}

#[test]
fn runic_cube_emergency_conversion_is_a_categorical_generation_opportunity() {
    let position = runic_cube_emergency_conversion_position();
    let value = oracle_combat_state_value(&position);

    assert!(
        value.survival_margin < 0,
        "expected visible lethal pressure, value={value:?}"
    );
    assert!(runic_cube_emergency_draw_conversion_ready(
        &position, &value
    ));
    let rank = oracle_combat_setup_guide_components(&position);
    assert_eq!(rank.first(), Some(&1));
}

#[test]
fn runic_cube_emergency_conversion_requires_every_tactical_dependency() {
    let ready = |position: &CombatPosition| {
        let value = oracle_combat_state_value(position);
        runic_cube_emergency_draw_conversion_ready(position, &value)
    };

    let mut no_cube = runic_cube_emergency_conversion_position();
    no_cube.combat.entities.player.relics.clear();
    assert!(!ready(&no_cube));

    let mut wrong_top_draw = runic_cube_emergency_conversion_position();
    wrong_top_draw.combat.zones.draw_pile =
        vec![CombatCard::for_scenario(CardId::Defend, 90)].into();
    assert!(!ready(&wrong_top_draw));

    let mut insufficient_energy = runic_cube_emergency_conversion_position();
    insufficient_energy.combat.turn.energy = 2;
    assert!(!ready(&insufficient_energy));

    let mut retaliation_is_blocked = runic_cube_emergency_conversion_position();
    retaliation_is_blocked.combat.entities.player.block = 5;
    assert!(!ready(&retaliation_is_blocked));

    let mut no_emergency = runic_cube_emergency_conversion_position();
    no_emergency.combat.entities.player.current_hp = 80;
    assert!(!ready(&no_emergency));
}

#[test]
fn dark_embrace_wound_access_is_a_categorical_state_opportunity() {
    let position = dark_embrace_wound_access_position();
    let value = oracle_combat_state_value(&position);

    assert!(dark_embrace_wound_access_conversion_ready(
        &position, &value
    ));
    let rank = oracle_combat_setup_guide_components(&position);
    assert_eq!(rank.first(), Some(&0));
    assert_eq!(rank.get(1), Some(&1));
}

#[test]
fn dark_embrace_wound_access_requires_the_immediate_engine_surface() {
    let ready = |position: &CombatPosition| {
        let value = oracle_combat_state_value(position);
        dark_embrace_wound_access_conversion_ready(position, &value)
    };

    let mut no_wound = dark_embrace_wound_access_position();
    no_wound
        .combat
        .zones
        .hand
        .retain(|card| card.id != CardId::Wound);
    assert!(!ready(&no_wound));

    let mut no_burning_pact = dark_embrace_wound_access_position();
    no_burning_pact
        .combat
        .zones
        .hand
        .retain(|card| card.id != CardId::BurningPact);
    assert!(!ready(&no_burning_pact));

    let mut no_exhaust_target = dark_embrace_wound_access_position();
    no_exhaust_target
        .combat
        .zones
        .hand
        .retain(|card| matches!(card.id, CardId::BurningPact | CardId::Wound));
    assert!(!ready(&no_exhaust_target));

    let mut no_emergency = dark_embrace_wound_access_position();
    no_emergency.combat.entities.player.current_hp = 80;
    assert!(!ready(&no_emergency));
}

#[test]
fn setup_summary_recognizes_connected_engines_and_remaining_fuel() {
    let mut combat = crate::test_support::blank_test_combat();
    let player = combat.entities.player.id;
    combat.entities.power_db.insert(
        player,
        vec![
            test_power(PowerId::DarkEmbrace),
            test_power(PowerId::Corruption),
            test_power(PowerId::Evolve),
        ],
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::ShrugItOff, 21),
        CombatCard::for_scenario(CardId::Wound, 22),
    ];
    combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::TrueGrit, 23)]).into();

    let summary = player_setup_summary(&combat);

    assert_eq!(summary.exhaust_engine_connected, 1);
    assert_eq!(summary.status_access_engine_connected, 1);
    assert_eq!(summary.exhaust_engine_fuel, 2);
}

#[test]
fn dark_embrace_connects_to_explicit_exhaust_sources_without_corruption() {
    let mut combat = crate::test_support::blank_test_combat();
    let player = combat.entities.player.id;
    combat
        .entities
        .power_db
        .insert(player, vec![test_power(PowerId::DarkEmbrace)]);
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::BurningPact, 24),
        CombatCard::for_scenario(CardId::Strike, 25),
    ];

    let connected = player_setup_summary(&combat);
    assert_eq!(connected.exhaust_engine_connected, 1);
    assert_eq!(connected.exhaust_engine_fuel, 1);

    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 26)];
    let disconnected = player_setup_summary(&combat);
    assert_eq!(disconnected.exhaust_engine_connected, 0);
    assert_eq!(disconnected.exhaust_engine_fuel, 0);
}

fn runic_cube_emergency_conversion_position() -> CombatPosition {
    use crate::content::relics::RelicState;

    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.player.current_hp = 24;
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RunicCube));
    combat.turn.energy = 3;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::SwordBoomerang, 10),
        CombatCard::for_scenario(CardId::SecondWind, 20),
    ];
    combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::PowerThrough, 30)].into();

    let mut spiker = crate::test_support::planned_monster(EnemyId::Spiker, 1);
    spiker.id = 1;
    let mut attacker = crate::test_support::planned_monster(EnemyId::TimeEater, 2);
    attacker.id = 2;
    combat.entities.monsters = vec![spiker, attacker];
    combat
        .entities
        .power_db
        .insert(1, vec![test_power_amount(PowerId::Thorns, 5)]);
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

fn dark_embrace_wound_access_position() -> CombatPosition {
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.player.current_hp = 18;
    combat.turn.energy = 3;
    combat.entities.power_db.insert(
        combat.entities.player.id,
        vec![test_power(PowerId::DarkEmbrace)],
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::BurningPact, 40),
        CombatCard::for_scenario(CardId::Wound, 41),
        CombatCard::for_scenario(CardId::Cleave, 42),
    ];
    combat.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::PowerThrough, 43),
        CombatCard::for_scenario(CardId::SecondWind, 44),
    ]
    .into();
    let mut spiker = crate::test_support::planned_monster(EnemyId::Spiker, 1);
    spiker.id = 1;
    let mut attacker = crate::test_support::planned_monster(EnemyId::TimeEater, 2);
    attacker.id = 2;
    combat.entities.monsters = vec![spiker, attacker];
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

#[test]
fn evolve_without_status_burden_is_not_a_connected_status_engine() {
    let mut combat = crate::test_support::blank_test_combat();
    let player = combat.entities.player.id;
    combat
        .entities
        .power_db
        .insert(player, vec![test_power(PowerId::Evolve)]);
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 31)];

    let summary = player_setup_summary(&combat);

    assert_eq!(summary.status_access_engine_connected, 0);
    assert_eq!(summary.exhaust_engine_connected, 0);
    assert_eq!(summary.exhaust_engine_fuel, 0);
}

#[test]
fn setup_recognizes_realized_exhaust_payoff_only_with_remaining_sources() {
    let mut combat = crate::test_support::blank_test_combat();
    let player = combat.entities.player.id;
    combat.entities.power_db.insert(
        player,
        vec![
            test_power_amount(PowerId::FeelNoPain, 4),
            test_power_amount(PowerId::DarkEmbrace, 1),
        ],
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::BurningPact, 41),
        CombatCard::for_scenario(CardId::Feed, 42),
    ];

    let connected = player_setup_summary(&combat);
    assert_eq!(connected.exhaust_payoff_engine_count, 2);
    assert_eq!(connected.exhaust_payoff_engine_fuel, 4);

    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 43)];
    let disconnected = player_setup_summary(&combat);
    assert_eq!(disconnected.exhaust_payoff_engine_count, 0);
    assert_eq!(disconnected.exhaust_payoff_engine_fuel, 0);
}

#[test]
fn realized_exhaust_payoff_gets_an_independent_setup_view() {
    let mut safer = crate::test_support::blank_test_combat();
    safer.entities.monsters = vec![crate::test_support::test_monster(EnemyId::AwakenedOne)];
    safer.entities.player.current_hp = 72;
    safer.zones.hand = vec![CombatCard::for_scenario(CardId::BurningPact, 51)];

    let mut connected = safer.clone();
    connected.entities.player.current_hp = 54;
    let player = connected.entities.player.id;
    connected
        .entities
        .power_db
        .insert(player, vec![test_power_amount(PowerId::FeelNoPain, 4)]);

    let safer_rank = oracle_combat_setup_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        safer,
    ));
    let connected_rank = oracle_combat_setup_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        connected,
    ));

    assert!(connected_rank > safer_rank);
}

#[test]
fn abundant_exhaust_fuel_does_not_override_realized_combat_progress() {
    let mut stockpiled = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.current_hp = 100;
    monster.max_hp = 100;
    stockpiled.entities.monsters = vec![monster];
    let player = stockpiled.entities.player.id;
    stockpiled
        .entities
        .power_db
        .insert(player, vec![test_power_amount(PowerId::FeelNoPain, 4)]);
    stockpiled.zones.draw_pile = ((0..12)
        .map(|index| CombatCard::for_scenario(CardId::Feed, 100 + index))
        .collect::<Vec<_>>())
    .into();

    let mut progressed = stockpiled.clone();
    progressed.zones.draw_pile.pop();
    progressed.entities.monsters[0].current_hp = 80;

    let stockpiled_rank = oracle_combat_setup_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        stockpiled,
    ));
    let progressed_rank = oracle_combat_setup_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        progressed,
    ));

    assert!(progressed_rank > stockpiled_rank);
}

#[test]
fn setup_prefers_realized_recurring_output_over_an_unset_safer_state() {
    let mut safer = crate::test_support::blank_test_combat();
    safer.entities.monsters = vec![crate::test_support::test_monster(EnemyId::TimeEater)];
    safer.entities.player.current_hp = 52;

    let mut scaled = safer.clone();
    scaled.entities.player.current_hp = 31;
    let player = scaled.entities.player.id;
    scaled
        .entities
        .power_db
        .insert(player, vec![test_power_amount(PowerId::DemonForm, 3)]);

    let safer_rank = oracle_combat_setup_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        safer,
    ));
    let scaled_rank = oracle_combat_setup_guide_components(&CombatPosition::new(
        EngineState::CombatPlayerTurn,
        scaled,
    ));

    assert!(scaled_rank > safer_rank);
}

#[test]
fn conditional_or_static_power_does_not_override_setup_survival() {
    let mut safer = crate::test_support::blank_test_combat();
    safer.entities.monsters = vec![crate::test_support::test_monster(EnemyId::TimeEater)];
    safer.entities.player.current_hp = 52;

    for power in [PowerId::Strength, PowerId::Rupture, PowerId::Combust] {
        let mut lower_hp = safer.clone();
        lower_hp.entities.player.current_hp = 31;
        let player = lower_hp.entities.player.id;
        lower_hp
            .entities
            .power_db
            .insert(player, vec![test_power_amount(power, 3)]);

        let safer_rank = oracle_combat_setup_guide_components(&CombatPosition::new(
            EngineState::CombatPlayerTurn,
            safer.clone(),
        ));
        let lower_hp_rank = oracle_combat_setup_guide_components(&CombatPosition::new(
            EngineState::CombatPlayerTurn,
            lower_hp,
        ));

        assert!(
            safer_rank > lower_hp_rank,
            "{power:?} must not claim recurring output"
        );
    }
}

#[test]
fn scheduled_strength_rollback_is_not_persistent_setup() {
    let mut combat = crate::test_support::blank_test_combat();
    let player = combat.entities.player.id;
    combat.entities.power_db.insert(
        player,
        vec![
            test_power_amount(PowerId::Strength, 5),
            test_power_amount(PowerId::LoseStrength, 5),
        ],
    );

    let summary = player_setup_summary(&combat);

    assert_eq!(summary.active_power_count, 0);
    assert_eq!(summary.active_power_mass, 0);
}

#[test]
fn setup_counts_only_strength_that_survives_a_scheduled_rollback() {
    let mut combat = crate::test_support::blank_test_combat();
    let player = combat.entities.player.id;
    combat.entities.power_db.insert(
        player,
        vec![
            test_power_amount(PowerId::Strength, 9),
            test_power_amount(PowerId::LoseStrength, 5),
        ],
    );

    let summary = player_setup_summary(&combat);

    assert_eq!(summary.active_power_count, 1);
    assert_eq!(summary.active_power_mass, 4);
}

#[test]
fn cleansed_strength_rollback_becomes_realized_setup() {
    let mut combat = crate::test_support::blank_test_combat();
    let player = combat.entities.player.id;
    combat
        .entities
        .power_db
        .insert(player, vec![test_power_amount(PowerId::Strength, 5)]);

    let summary = player_setup_summary(&combat);

    assert_eq!(summary.active_power_count, 1);
    assert_eq!(summary.active_power_mass, 5);
}

fn test_power(power_type: PowerId) -> Power {
    test_power_amount(power_type, -1)
}

fn test_power_amount(power_type: PowerId, amount: i32) -> Power {
    Power {
        power_type,
        instance_id: None,
        amount,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    }
}
