use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::powers::store;
use crate::runtime::combat::CombatCard;
use crate::runtime::combat::{Power, PowerPayload};
use crate::state::core::{DiscoveryChoiceState, PendingChoice};
use crate::testing::support::{
    blank_test_combat, combat_with_monsters, planned_monster, test_monster,
};
use sts_combat_planner::UniformCombatActionPolicy;

struct ConstantPolicy(f64);

impl CombatActionPolicy for ConstantPolicy {
    fn weights(&self, _position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        vec![self.0; choices.len()]
    }
}

fn synthetic_example(positive: f64, negative: f64) -> RankingExample {
    RankingExample {
        target_probabilities: vec![1.0, 0.0],
        neutral_indices: Vec::new(),
        top1_accepted_indices: vec![0],
        base_logits: vec![0.0; 2],
        candidates: vec![
            BTreeMap::from([("signal".to_string(), positive)]),
            BTreeMap::from([("signal".to_string(), negative)]),
        ],
    }
}

#[test]
fn sparse_softmax_learns_demonstrated_ranking() {
    let examples = vec![synthetic_example(1.0, -1.0)];
    let config = CombatActionImitationTrainingConfigV1::default();
    let weights = train_sparse_softmax(&examples, config);
    assert_eq!(
        runtime_candidate_index(
            &weights,
            &examples[0],
            config.logit_scale,
            config.max_abs_log_factor,
        ),
        0
    );
    assert!(weights["signal"] > 0.0);
}

#[test]
fn exact_alternative_is_excluded_from_negative_training() {
    let examples = vec![RankingExample {
        target_probabilities: vec![1.0, 0.0, 0.0],
        neutral_indices: vec![1],
        top1_accepted_indices: vec![0, 1],
        base_logits: vec![0.0; 3],
        candidates: vec![
            BTreeMap::from([("demonstrated".to_string(), 1.0)]),
            BTreeMap::from([("accepted_alternative".to_string(), 1.0)]),
            BTreeMap::from([("negative".to_string(), 1.0)]),
        ],
    }];
    let weights = train_sparse_softmax(&examples, CombatActionImitationTrainingConfigV1::default());

    assert!(weights["demonstrated"] > 0.0);
    assert!(weights["negative"] < 0.0);
    assert_eq!(weights["accepted_alternative"], 0.0);
}

#[test]
fn all_exact_wins_do_not_create_v1_feasibility_preference() {
    assert!(!combat_action_reanalysis_has_v1_preference_evidence(&[
        CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 1 },
        CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 50 },
    ]));
    assert!(combat_action_reanalysis_has_v1_preference_evidence(&[
        CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 50 },
        CombatActionReanalysisEvidenceV1::BudgetUnknown,
    ]));
    assert!(combat_action_reanalysis_has_v1_preference_evidence(&[
        CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 50 },
        CombatActionReanalysisEvidenceV1::ExactNonWin,
    ]));
}

#[test]
fn indexed_sparse_softmax_matches_string_map_reference() {
    let examples = vec![
        RankingExample {
            target_probabilities: vec![0.0, 1.0, 0.0],
            neutral_indices: vec![2],
            top1_accepted_indices: vec![1, 2],
            base_logits: vec![0.25, -0.5, 0.0],
            candidates: vec![
                BTreeMap::from([("shared".to_string(), 1.0), ("alpha".to_string(), -0.5)]),
                BTreeMap::from([("shared".to_string(), 0.25), ("beta".to_string(), 2.0)]),
                BTreeMap::from([("gamma".to_string(), 1.5)]),
            ],
        },
        RankingExample {
            target_probabilities: vec![1.0, 0.0],
            neutral_indices: Vec::new(),
            top1_accepted_indices: vec![0],
            base_logits: vec![-0.1, 0.2],
            candidates: vec![
                BTreeMap::from([("alpha".to_string(), 0.75), ("gamma".to_string(), -1.0)]),
                BTreeMap::from([("beta".to_string(), 0.5), ("shared".to_string(), -0.25)]),
            ],
        },
    ];
    let config = CombatActionImitationTrainingConfigV1 {
        epochs: 17,
        ..CombatActionImitationTrainingConfigV1::default()
    };

    let indexed = train_sparse_softmax(&examples, config);
    let reference = train_sparse_softmax_reference(&examples, config);

    assert_eq!(indexed, reference);
}

#[test]
fn exact_winning_adjacent_swap_excludes_the_alternative_from_negatives() {
    let mut monster = planned_monster(EnemyId::JawWorm, 1);
    monster.current_hp = 6;
    let mut combat = combat_with_monsters(vec![monster]);
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 11),
        CombatCard::for_scenario(CardId::Inflame, 12),
        CombatCard::for_scenario(CardId::Strike, 13),
    ];
    combat.entities.player.current_hp = 1;
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let demonstrated = ClientInput::PlayCard {
        card_index: 0,
        target: None,
    };
    let next = ClientInput::PlayCard {
        card_index: 0,
        target: None,
    };
    let lethal = ClientInput::PlayCard {
        card_index: 0,
        target: Some(1),
    };
    let candidates =
        concrete_combat_action_candidates_for_witness_v1(&position, &demonstrated, 256);
    let demonstrated_index = candidates
        .iter()
        .position(|candidate| candidate == &demonstrated)
        .expect("Defend is legal");
    let inflame_index = candidates
        .iter()
        .position(|candidate| {
            matches!(
                candidate,
                ClientInput::PlayCard {
                    card_index: 1,
                    target: None
                }
            )
        })
        .expect("Inflame is legal");
    let end_turn_index = candidates
        .iter()
        .position(|candidate| matches!(candidate, ClientInput::EndTurn))
        .expect("end turn is legal");
    let accepted = exact_witness_adjacent_accepted_indices_v1(
        &EngineCombatStepper,
        &position,
        &[demonstrated, next, lethal],
        0,
        &candidates,
        demonstrated_index,
        250,
    );

    assert!(accepted.contains(&demonstrated_index));
    assert!(accepted.contains(&inflame_index));
    assert!(!accepted.contains(&end_turn_index));
}

#[test]
fn runtime_ranking_applies_base_and_bounded_residual_together() {
    let learned = vec![10.0, 0.0];
    let base = vec![0.0, 4.0];
    let combined = runtime_combined_logits(&learned, &base, 3.0);

    assert_eq!(combined, vec![0.0, 1.0]);
}

#[test]
fn warm_start_can_escape_the_runtime_residual_floor() {
    let example = RankingExample {
        target_probabilities: vec![0.0, 1.0],
        neutral_indices: Vec::new(),
        top1_accepted_indices: vec![1],
        base_logits: vec![0.0, 0.0],
        candidates: vec![
            BTreeMap::from([("first".to_string(), 1.0)]),
            BTreeMap::from([("second".to_string(), 1.0)]),
        ],
    };
    let config = CombatActionImitationTrainingConfigV1 {
        epochs: 80,
        learning_rate: 0.2,
        l2_penalty: 0.0,
        ..CombatActionImitationTrainingConfigV1::default()
    };
    let initial = vec![
        CombatActionImitationCoefficientV1 {
            feature: "first".to_string(),
            weight: 10.0,
        },
        CombatActionImitationCoefficientV1 {
            feature: "second".to_string(),
            weight: 0.0,
        },
    ];
    let learned =
        train_sparse_softmax_with_initial(std::slice::from_ref(&example), config, Some(&initial));

    assert_eq!(
        runtime_candidate_index(
            &learned,
            &example,
            config.logit_scale,
            config.max_abs_log_factor,
        ),
        1
    );
}

#[test]
fn card_semantics_ignore_hand_index_and_uuid() {
    let mut left = blank_test_combat();
    left.zones.hand = vec![
        CombatCard::for_scenario(CardId::Warcry, 11),
        CombatCard::for_scenario(CardId::Defend, 12),
    ];
    let mut right = left.clone();
    right.zones.hand.swap(0, 1);
    right.zones.hand[1].lineage = crate::test_support::scenario_lineage(99);
    let left = CombatPosition::new(EngineState::CombatPlayerTurn, left);
    let right = CombatPosition::new(EngineState::CombatPlayerTurn, right);
    let left_features = action_feature_vector(
        &left,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
    );
    let right_features = action_feature_vector(
        &right,
        &ClientInput::PlayCard {
            card_index: 1,
            target: None,
        },
    );
    assert_eq!(left_features, right_features);
    assert!(left_features.keys().all(|feature| !feature.contains("99")));
}

#[test]
fn strength_sources_share_mechanic_semantics_without_losing_timing() {
    let mut combat = blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Inflame, 11),
        CombatCard::for_scenario(CardId::DemonForm, 12),
    ];
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let inflame = action_semantic_tokens(
        &position,
        &ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
    );
    let demon_form = action_semantic_tokens(
        &position,
        &ClientInput::PlayCard {
            card_index: 1,
            target: None,
        },
    );

    assert!(inflame.contains(&"semantic/provides/strength".to_string()));
    assert!(demon_form.contains(&"semantic/provides/strength".to_string()));
    assert!(inflame.contains(&"semantic/provides_immediately/strength".to_string()));
    assert!(!demon_form.contains(&"semantic/provides_immediately/strength".to_string()));
    assert!(demon_form.contains(&"semantic/provides_on/turn_start/strength".to_string()));
}

#[test]
fn discovery_choices_expose_selected_card_semantics() {
    let combat = blank_test_combat();
    let engine = EngineState::PendingChoice(PendingChoice::DiscoverySelect(DiscoveryChoiceState {
        cards: vec![CardId::Bash, CardId::Defend],
        colorless: false,
        card_type: None,
        amount: 1,
        can_skip: false,
    }));
    let position = CombatPosition::new(engine, combat);

    let bash = action_feature_vector(&position, &ClientInput::SubmitDiscoverChoice(0));
    let defend = action_feature_vector(&position, &ClientInput::SubmitDiscoverChoice(1));

    assert_ne!(bash, defend);
    assert!(bash.contains_key("action/choice/card/Bash+0"));
    assert!(defend.contains_key("action/choice/card/Defend+0"));
}

#[test]
fn targeted_card_semantics_include_target_local_state() {
    let mut combat = blank_test_combat();
    let artifact = test_monster(EnemyId::Cultist);
    let mut exposed = test_monster(EnemyId::Cultist);
    exposed.id = 2;
    exposed.slot = 1;
    combat.entities.monsters = vec![artifact, exposed];
    store::set_powers_for(
        &mut combat,
        1,
        vec![Power {
            power_type: PowerId::Artifact,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 11)];
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

    let into_artifact = action_feature_vector(
        &position,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
    );
    let into_exposed = action_feature_vector(
        &position,
        &ClientInput::PlayCard {
            card_index: 0,
            target: Some(2),
        },
    );

    assert_ne!(into_artifact, into_exposed);
    assert!(into_artifact.contains_key("action/interaction/card/Bash+0/target/power/Artifact/1"));
    assert!(!into_exposed.contains_key("action/interaction/card/Bash+0/target/power/Artifact/1"));
}

#[test]
fn compiled_runtime_score_matches_sparse_training_features() {
    let mut combat = blank_test_combat();
    let mut target = test_monster(EnemyId::Cultist);
    target.current_hp = 19;
    combat.entities.monsters = vec![target];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 11)];
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let input = ClientInput::PlayCard {
        card_index: 0,
        target: Some(1),
    };
    let state = typed_combat_feature_components_v1(&position);
    let features = action_feature_vector_with_state(&position, &input, &state);
    let coefficients = features
        .keys()
        .enumerate()
        .map(|(index, feature)| CombatActionImitationCoefficientV1 {
            feature: feature.clone(),
            weight: (index as f64 + 1.0) / 97.0,
        })
        .collect::<Vec<_>>();
    let sparse = coefficients
        .iter()
        .map(|coefficient| (coefficient.feature.clone(), coefficient.weight))
        .collect::<HashMap<_, _>>();
    let expected = sparse_score(&sparse, &features);
    let actual =
        CompiledActionImitationWeightsV1::new(&coefficients).score(&position, &input, &state);

    assert!(
        (expected - actual).abs() < 1.0e-10,
        "{expected} != {actual}"
    );
}

#[test]
fn learned_policy_preserves_positive_weights() {
    let artifact = CombatActionImitationArtifactV1 {
        schema_name: COMBAT_ACTION_IMITATION_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_ACTION_IMITATION_SCHEMA_VERSION,
        feature_schema: COMBAT_ACTION_FEATURE_SCHEMA.to_string(),
        runtime_compatibility_id: COMBAT_ACTION_IMITATION_RUNTIME_ID.to_string(),
        training_authority: "test".to_string(),
        source_trajectory_count: 1,
        source_action_count: 1,
        source_terminal_final_hp: 1,
        ranked_decision_count: 1,
        pairwise_comparison_count: 1,
        skipped_forced_decision_count: 0,
        training_top1_correct: 1,
        training_top1_total: 1,
        logit_scale: 1.0,
        max_abs_log_factor: 3.0,
        base_weight_exponent: 0.0,
        coefficients: vec![CombatActionImitationCoefficientV1 {
            feature: "action/kind/end_turn".to_string(),
            weight: -100.0,
        }],
    };
    let policy = combat_action_imitation_policy_v1(Arc::new(UniformCombatActionPolicy), artifact)
        .expect("valid learned policy");
    let position = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
    let input = ClientInput::EndTurn;
    let weights = policy.weights(&position, &[CombatPolicyChoice::Atomic(&input)]);
    assert_eq!(weights.len(), 1);
    assert!(weights[0].is_finite() && weights[0] > 0.0);
}

#[test]
fn specialized_action_prior_stops_after_the_root_player_turn() {
    let mut position = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
    let root_turn = position.combat.turn.turn_count;
    let policy = root_player_turn_action_policy_v1(
        root_turn,
        Arc::new(ConstantPolicy(7.0)),
        Arc::new(ConstantPolicy(2.0)),
    );
    let input = ClientInput::EndTurn;
    let choices = [CombatPolicyChoice::Atomic(&input)];
    assert_eq!(policy.weights(&position, &choices), vec![7.0]);

    position.combat.turn.turn_count = root_turn.saturating_add(1);
    assert_eq!(policy.weights(&position, &choices), vec![2.0]);
}

#[test]
fn artifact_rejects_nonfinite_coefficients() {
    let artifact = CombatActionImitationArtifactV1 {
        schema_name: COMBAT_ACTION_IMITATION_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_ACTION_IMITATION_SCHEMA_VERSION,
        feature_schema: COMBAT_ACTION_FEATURE_SCHEMA.to_string(),
        runtime_compatibility_id: COMBAT_ACTION_IMITATION_RUNTIME_ID.to_string(),
        training_authority: "test".to_string(),
        source_trajectory_count: 1,
        source_action_count: 1,
        source_terminal_final_hp: 1,
        ranked_decision_count: 1,
        pairwise_comparison_count: 1,
        skipped_forced_decision_count: 0,
        training_top1_correct: 0,
        training_top1_total: 1,
        logit_scale: 1.0,
        max_abs_log_factor: 3.0,
        base_weight_exponent: 0.0,
        coefficients: vec![CombatActionImitationCoefficientV1 {
            feature: "broken".to_string(),
            weight: f64::NAN,
        }],
    };
    assert!(artifact.validate().is_err());
}

#[test]
fn artifact_rejects_changed_runtime_contract() {
    let artifact = CombatActionImitationArtifactV1 {
        schema_name: COMBAT_ACTION_IMITATION_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_ACTION_IMITATION_SCHEMA_VERSION,
        feature_schema: COMBAT_ACTION_FEATURE_SCHEMA.to_string(),
        runtime_compatibility_id: "stale-runtime".to_string(),
        training_authority: "test".to_string(),
        source_trajectory_count: 1,
        source_action_count: 1,
        source_terminal_final_hp: 1,
        ranked_decision_count: 1,
        pairwise_comparison_count: 1,
        skipped_forced_decision_count: 0,
        training_top1_correct: 1,
        training_top1_total: 1,
        logit_scale: 1.0,
        max_abs_log_factor: 3.0,
        base_weight_exponent: 0.0,
        coefficients: vec![CombatActionImitationCoefficientV1 {
            feature: "action/kind/end_turn".to_string(),
            weight: 1.0,
        }],
    };
    let error = artifact.validate().expect_err("stale contract must fail");
    assert!(error.contains("runtime mismatch"));
    assert!(error.contains("rebuild"));

    let mut payload = serde_json::to_value(&artifact).expect("serialize artifact");
    payload
        .as_object_mut()
        .unwrap()
        .remove("runtime_compatibility_id");
    let error = serde_json::from_value::<CombatActionImitationArtifactV1>(payload)
        .expect_err("current imitation artifacts require the runtime contract field");
    assert!(error.to_string().contains("runtime_compatibility_id"));
}
