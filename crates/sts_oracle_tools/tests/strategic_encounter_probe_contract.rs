use std::collections::HashSet;
use sts_oracle_runtime::ai::combat_search_v2::{
    run_combat_mechanism_horizon_probe_v1, CombatMechanismHorizonProbeConfigV1,
};
use sts_oracle_runtime::ai::noncombat_strategy_v1::StrategyCapabilityKindV1;
use sts_oracle_runtime::content::cards::CardId;
use sts_oracle_runtime::content::monsters::factory::EncounterId;
use sts_oracle_runtime::content::potions::{Potion, PotionId};
use sts_oracle_runtime::eval::run_control::{
    run_strategic_checkpoint_probe_decomposition_v1, run_strategic_encounter_probe_suite_v1,
    run_strategic_encounter_probes_v1, run_strategic_mechanism_probes_v1,
    run_strategic_probe_calibration_v1, strategic_encounter_probe_plan_v1,
    strategic_encounter_probe_scenarios_v1, strategic_mechanism_probe_plan_v1,
    validate_strategic_probe_shadow_ordering_v1, StrategicCheckpointProbeVariantKindV1,
    StrategicCheckpointReferenceRelationV1, StrategicEncounterPrimaryEvidenceV1,
    StrategicEncounterProbeBudgetV1, StrategicEncounterProbeHpBasisV1,
    StrategicEncounterProbePotionUseV1, StrategicEncounterProbeSpecV1,
    StrategicMechanismProbeOutcomeV1, StrategicProbeCalibrationPartitionV1,
    StrategicProbeFidelityV1, StrategicProbeOrderingCalibrationCaseV1,
    StrategicProbeOwnerAuthorityV1, StrategicProbeResolvedLabelV1,
    StrategicProbeSchedulingAuthorityV1, StrategicProbeShadowFidelityV1,
    StrategicProbeShadowOrderKeyV1, STRATEGIC_ENCOUNTER_PROBE_SCHEMA_VERSION,
};
use sts_oracle_runtime::sim::combat_start::build_natural_combat_start;
use sts_oracle_runtime::state::map::node::RoomType;
use sts_oracle_runtime::state::run::RunState;

#[test]
fn act_two_battery_keeps_distinct_elite_tests_and_uses_the_visible_boss() {
    let mut run = RunState::new(7, 0, false, "Ironclad");
    run.act_num = 2;
    run.boss_key = Some(EncounterId::TheChamp);

    let plan = strategic_encounter_probe_plan_v1(&run);

    assert_eq!(plan.len(), 5);
    assert_eq!(plan[0].encounter, EncounterId::ThreeByrds);
    assert_eq!(plan[1].encounter, EncounterId::Slavers);
    assert_eq!(plan[2].encounter, EncounterId::GremlinLeader);
    assert_eq!(plan[3].encounter, EncounterId::BookOfStabbing);
    assert_eq!(plan[4].encounter, EncounterId::TheChamp);
    assert!(plan[4]
        .capabilities_under_test
        .contains(&StrategyCapabilityKindV1::PhaseControl));
}

#[test]
fn checkpoint_decomposition_changes_only_named_resource_or_deck_fields() {
    let mut observed = RunState::new(23, 0, false, "Ironclad");
    observed.act_num = 2;
    observed.floor_num = 32;
    observed.current_hp = 35;
    observed.max_hp = 94;
    observed.add_card_to_deck(CardId::Cleave);
    let mut reference = observed.clone();
    reference.floor_num = 17;
    reference.current_hp = 80;
    reference.master_deck.pop();
    let observed_before = observed.clone();
    let reference_before = reference.clone();
    let probe = StrategicEncounterProbeSpecV1 {
        probe_id: "contract_cultist",
        encounter: EncounterId::Cultist,
        room_type: RoomType::MonsterRoom,
        probe_seed: 9001,
        capabilities_under_test: vec![StrategyCapabilityKindV1::SingleTargetFrontload],
    };

    let decomposition = run_strategic_checkpoint_probe_decomposition_v1(
        &observed,
        Some(&reference),
        Some(StrategicCheckpointReferenceRelationV1::StateOnlyCounterfactual),
        &[probe],
        StrategicEncounterProbeBudgetV1 {
            max_nodes_per_encounter: 0,
            wall_ms_per_encounter: 1,
            hp_basis: StrategicEncounterProbeHpBasisV1::Full,
            potion_use: StrategicEncounterProbePotionUseV1::SemanticBudgeted { max_uses: 1 },
        },
    )
    .expect("compatible checkpoints");

    assert_eq!(decomposition.variants.len(), 3);
    assert_eq!(decomposition.omitted_variants.len(), 2);
    assert_eq!(
        decomposition.reference_relation,
        Some(StrategicCheckpointReferenceRelationV1::StateOnlyCounterfactual)
    );
    let observed_variant = &decomposition.variants[0];
    assert_eq!(
        observed_variant.kind,
        StrategicCheckpointProbeVariantKindV1::Observed
    );
    assert_eq!(observed_variant.state.current_hp, 35);
    let full_hp_variant = &decomposition.variants[1];
    assert_eq!(
        full_hp_variant.kind,
        StrategicCheckpointProbeVariantKindV1::FullHpOnly
    );
    assert_eq!(full_hp_variant.state.current_hp, 94);
    let deck_variant = decomposition
        .variants
        .iter()
        .find(|variant| variant.kind == StrategicCheckpointProbeVariantKindV1::DeckFromReference)
        .expect("reference deck variant");
    assert_eq!(deck_variant.state.current_hp, 35);
    assert_eq!(deck_variant.state.deck_size, reference.master_deck.len());
    assert_eq!(observed, observed_before);
    assert_eq!(reference, reference_before);
}

#[test]
fn potion_counterfactual_rejects_a_no_op_policy_and_reports_the_restored_resource() {
    let observed = RunState::new(29, 0, false, "Ironclad");
    let mut reference = observed.clone();
    reference.potions[0] = Some(Potion::new(PotionId::BlockPotion, 91));
    let probe = StrategicEncounterProbeSpecV1 {
        probe_id: "contract_cultist",
        encounter: EncounterId::Cultist,
        room_type: RoomType::MonsterRoom,
        probe_seed: 9_002,
        capabilities_under_test: vec![StrategyCapabilityKindV1::SustainedDefense],
    };

    let disabled = run_strategic_checkpoint_probe_decomposition_v1(
        &observed,
        Some(&reference),
        Some(StrategicCheckpointReferenceRelationV1::StateOnlyCounterfactual),
        std::slice::from_ref(&probe),
        StrategicEncounterProbeBudgetV1 {
            max_nodes_per_encounter: 0,
            wall_ms_per_encounter: 1,
            hp_basis: StrategicEncounterProbeHpBasisV1::Current,
            potion_use: StrategicEncounterProbePotionUseV1::Disabled,
        },
    );
    assert!(disabled
        .expect_err("restoring an unusable potion would be a placebo counterfactual")
        .contains("enabled paired potion-use policy"));

    let report = run_strategic_checkpoint_probe_decomposition_v1(
        &observed,
        Some(&reference),
        Some(StrategicCheckpointReferenceRelationV1::StateOnlyCounterfactual),
        &[probe],
        StrategicEncounterProbeBudgetV1 {
            max_nodes_per_encounter: 0,
            wall_ms_per_encounter: 1,
            hp_basis: StrategicEncounterProbeHpBasisV1::Current,
            potion_use: StrategicEncounterProbePotionUseV1::SemanticBudgeted { max_uses: 1 },
        },
    )
    .expect("paired potion policy makes the intervention executable");
    let restored = report
        .variants
        .iter()
        .find(|variant| variant.kind == StrategicCheckpointProbeVariantKindV1::PotionsFromReference)
        .expect("restored potion variant");
    assert_eq!(restored.state.potion_ids, vec![PotionId::BlockPotion]);
    assert_eq!(
        restored.probe.budget.potion_use,
        StrategicEncounterProbePotionUseV1::SemanticBudgeted { max_uses: 1 }
    );
}

#[test]
fn fixed_battery_realizes_all_counterfactual_encounters_without_errors() {
    let mut run = RunState::new(17, 0, false, "Ironclad");
    run.act_num = 2;
    run.boss_key = Some(EncounterId::TheChamp);

    let report = run_strategic_encounter_probe_suite_v1(
        &run,
        StrategicEncounterProbeBudgetV1 {
            max_nodes_per_encounter: 0,
            wall_ms_per_encounter: 1,
            hp_basis: StrategicEncounterProbeHpBasisV1::Current,
            potion_use: StrategicEncounterProbePotionUseV1::Disabled,
        },
    );

    assert_eq!(report.observations.len(), 5);
    assert!(report.observations.iter().all(|observation| !matches!(
        &observation.primary_evidence,
        StrategicEncounterPrimaryEvidenceV1::SetupError { .. }
    )));
    assert!(report.observations.iter().all(|observation| matches!(
        &observation.primary_evidence,
        StrategicEncounterPrimaryEvidenceV1::BudgetUnknown
            | StrategicEncounterPrimaryEvidenceV1::ExactWitness { .. }
    )));
}

#[test]
fn probe_is_counterfactual_and_does_not_consume_the_run_rng_or_mutate_hp() {
    let mut run = RunState::new(11, 0, false, "Ironclad");
    run.current_hp = 73;
    run.max_hp = 80;
    run.add_card_to_deck(CardId::Cleave);
    let before = run.clone();
    let probe = StrategicEncounterProbeSpecV1 {
        probe_id: "contract_cultist",
        encounter: EncounterId::Cultist,
        room_type: RoomType::MonsterRoom,
        probe_seed: 1234,
        capabilities_under_test: vec![StrategyCapabilityKindV1::SingleTargetFrontload],
    };

    let report = run_strategic_encounter_probes_v1(
        &run,
        &[probe],
        StrategicEncounterProbeBudgetV1 {
            max_nodes_per_encounter: 0,
            wall_ms_per_encounter: 1,
            hp_basis: StrategicEncounterProbeHpBasisV1::Current,
            potion_use: StrategicEncounterProbePotionUseV1::Disabled,
        },
    );

    assert_eq!(run, before);
    assert_eq!(
        report.schema_version,
        STRATEGIC_ENCOUNTER_PROBE_SCHEMA_VERSION
    );
    assert_eq!(report.observations.len(), 1);
    assert_eq!(report.observations[0].initial_hp, 73);
    assert!(matches!(
        &report.observations[0].primary_evidence,
        StrategicEncounterPrimaryEvidenceV1::BudgetUnknown
    ));
    assert_eq!(
        report.information_boundary,
        "offline_shadow_counterfactual_fixed_act_pool_no_successor_authority"
    );
}

#[test]
fn scenario_battery_keeps_unknown_separate_and_does_not_change_the_run() {
    let mut run = RunState::new(19, 20, false, "Ironclad");
    run.current_hp = 47;
    run.potions[0] = Some(Potion::new(PotionId::FirePotion, 11));
    let before = run.clone();
    let probes = strategic_encounter_probe_scenarios_v1(
        &[StrategicEncounterProbeSpecV1 {
            probe_id: "act1_two_louse_diagnostic",
            encounter: EncounterId::TwoLouse,
            room_type: RoomType::MonsterRoom,
            probe_seed: 12_345,
            capabilities_under_test: vec![StrategyCapabilityKindV1::MultiTargetControl],
        }],
        3,
    )
    .expect("three reproducible cases");
    assert_eq!(
        probes
            .iter()
            .map(|probe| probe.probe_seed)
            .collect::<Vec<_>>(),
        vec![12_345, 77_881, 143_417]
    );
    assert!(strategic_encounter_probe_scenarios_v1(&probes, 0).is_err());

    let report = run_strategic_encounter_probes_v1(
        &run,
        &probes,
        StrategicEncounterProbeBudgetV1 {
            max_nodes_per_encounter: 0,
            wall_ms_per_encounter: 1,
            hp_basis: StrategicEncounterProbeHpBasisV1::Current,
            potion_use: StrategicEncounterProbePotionUseV1::Disabled,
        },
    );
    assert_eq!(run, before);
    assert_eq!(report.ascension_level, 20);
    assert_eq!(report.master_deck, run.master_deck);
    assert_eq!(report.relics, run.relics);
    assert_eq!(report.entry_potions[0], Some(PotionId::FirePotion));
    assert_eq!(report.observations.len(), 3);
    let summary = &report.encounter_summaries[0];
    assert_eq!(summary.cases, 3);
    assert_eq!(summary.budget_unknown, 3);
    assert_eq!(summary.exact_witnesses, 0);
    assert_eq!(summary.exhaustive_refutations, 0);
    assert_eq!(summary.setup_errors, 0);
}

#[test]
fn verified_probe_win_projects_persistent_resources() {
    let mut run = RunState::new(43, 0, false, "Ironclad");
    run.master_deck.clear();
    run.add_card_to_deck(CardId::Immolate);
    run.potions[0] = Some(Potion::new(PotionId::FirePotion, 19));
    let report = run_strategic_encounter_probes_v1(
        &run,
        &[StrategicEncounterProbeSpecV1 {
            probe_id: "two_louse_immediate_win",
            encounter: EncounterId::TwoLouse,
            room_type: RoomType::MonsterRoom,
            probe_seed: 987,
            capabilities_under_test: vec![StrategyCapabilityKindV1::MultiTargetControl],
        }],
        StrategicEncounterProbeBudgetV1 {
            max_nodes_per_encounter: 1_000,
            wall_ms_per_encounter: 500,
            hp_basis: StrategicEncounterProbeHpBasisV1::Current,
            potion_use: StrategicEncounterProbePotionUseV1::SemanticBudgeted { max_uses: 1 },
        },
    );
    let StrategicEncounterPrimaryEvidenceV1::ExactWitness { witness } =
        &report.observations[0].primary_evidence
    else {
        panic!("one Immolate should give an exact two-louse win");
    };
    assert!(witness.resource_projection_error.is_none());
    let resources = witness
        .resources
        .as_ref()
        .expect("verified resource replay");
    assert!(resources.final_gold >= run.gold);
    assert!(resources.total_potion_expenditures <= 1);
    assert_eq!(report.encounter_summaries[0].exact_witnesses, 1);
}

#[test]
fn finite_horizon_mechanism_probe_returns_distinct_exact_endpoints_without_a_scalar_score() {
    let mut run = RunState::new(31, 0, false, "Ironclad");
    let (engine, combat) =
        build_natural_combat_start(&mut run, EncounterId::Cultist, RoomType::MonsterRoom)
            .expect("cultist start");
    let engine_before = engine.clone();
    let combat_before = combat.clone();

    let report = run_combat_mechanism_horizon_probe_v1(
        &engine,
        &combat,
        CombatMechanismHorizonProbeConfigV1 {
            horizon_turns: 1,
            max_active_states_per_depth: 32,
            max_inner_nodes_per_turn: 128,
            max_end_states_per_turn: 16,
            per_bucket_limit: 4,
            max_engine_steps_per_action: 250,
        },
    );

    assert_eq!(report.depths.len(), 1);
    assert!(!report.depths[0].endpoints.is_empty());
    let hashes = report.depths[0]
        .endpoints
        .iter()
        .map(|endpoint| endpoint.state.exact_state_hash.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(hashes.len(), report.depths[0].endpoints.len());
    assert_eq!(
        report.evidence_policy,
        "bounded_exact_transitions_diverse_endpoint_surface_no_scalar_or_whole_combat_claim"
    );
    assert_eq!(engine, engine_before);
    assert_eq!(combat, combat_before);
}

#[test]
fn strategic_mechanism_battery_keeps_phase_burst_out_until_a_controlled_fixture_exists() {
    let run = RunState::new(37, 0, false, "Ironclad");
    let plan = strategic_mechanism_probe_plan_v1();

    assert_eq!(plan.len(), 5);
    assert!(plan.iter().all(|probe| !probe.probe_id.contains("phase")));
    let report = run_strategic_mechanism_probes_v1(&run, &plan[..1]);
    assert_eq!(report.observations.len(), 1);
    assert!(matches!(
        report.observations[0].outcome,
        StrategicMechanismProbeOutcomeV1::HeuristicEstimate { .. }
    ));
    assert!(report
        .unsupported_questions
        .contains(&"phase_burst_requires_a_controlled_pre_transition_combat_fixture"));
}

#[test]
fn calibration_reuses_the_same_case_across_fidelities_without_gaining_authority() {
    let run = RunState::new(41, 0, false, "Ironclad");
    let probe = StrategicEncounterProbeSpecV1 {
        probe_id: "contract_cultist",
        encounter: EncounterId::Cultist,
        room_type: RoomType::MonsterRoom,
        probe_seed: 4_242,
        capabilities_under_test: vec![StrategyCapabilityKindV1::SingleTargetFrontload],
    };
    let before = run.clone();
    let report = run_strategic_probe_calibration_v1(
        &run,
        &[probe],
        StrategicEncounterProbeHpBasisV1::Current,
        &[
            StrategicProbeFidelityV1 {
                max_nodes: 0,
                wall_ms: 1,
            },
            StrategicProbeFidelityV1 {
                max_nodes: 1,
                wall_ms: 1,
            },
        ],
        &[StrategicProbeShadowFidelityV1 {
            horizon_turns: 1,
            max_active_states_per_depth: 2,
            max_inner_nodes_per_turn: 2,
            max_end_states_per_turn: 2,
            per_bucket_limit: 1,
        }],
    )
    .expect("ordered fidelity ladder");

    assert_eq!(report.observations.len(), 1);
    assert_eq!(report.observations[0].probe_seed, 4_242);
    assert_eq!(report.observations[0].fidelity_observations.len(), 2);
    assert_eq!(report.observations[0].shadows.len(), 1);
    assert!(report.authority.contains("no_candidate_elimination"));
    assert_eq!(run, before);
}

#[test]
fn caller_provided_held_out_rows_measure_ordering_without_granting_authority() {
    let key = |enemy_delta, survival_margin| StrategicProbeShadowOrderKeyV1 {
        terminal_win_seen: false,
        non_loss_endpoint_seen: true,
        living_enemy_delta: 0,
        total_enemy_hp_delta: enemy_delta,
        survival_margin,
        pollution_avoidance: 0,
        depth_turns: 2,
    };
    let cases = vec![
        StrategicProbeOrderingCalibrationCaseV1 {
            case_id: "development-only".to_string(),
            seed_group: "seed-dev".to_string(),
            partition: StrategicProbeCalibrationPartitionV1::Development,
            shadow_order_key: Some(key(10, 40)),
            exact_label: StrategicProbeResolvedLabelV1::ExactWitness,
        },
        StrategicProbeOrderingCalibrationCaseV1 {
            case_id: "held-out-win".to_string(),
            seed_group: "seed-held-win".to_string(),
            partition: StrategicProbeCalibrationPartitionV1::HeldOut,
            shadow_order_key: Some(key(30, 50)),
            exact_label: StrategicProbeResolvedLabelV1::ExactWitness,
        },
        StrategicProbeOrderingCalibrationCaseV1 {
            case_id: "held-out-refutation".to_string(),
            seed_group: "seed-held-loss".to_string(),
            partition: StrategicProbeCalibrationPartitionV1::HeldOut,
            shadow_order_key: Some(key(5, 5)),
            exact_label: StrategicProbeResolvedLabelV1::ExhaustiveRefutation,
        },
        StrategicProbeOrderingCalibrationCaseV1 {
            case_id: "held-out-unknown".to_string(),
            seed_group: "seed-held-unknown".to_string(),
            partition: StrategicProbeCalibrationPartitionV1::HeldOut,
            shadow_order_key: Some(key(0, -100)),
            exact_label: StrategicProbeResolvedLabelV1::BudgetUnknown,
        },
    ];

    let validation =
        validate_strategic_probe_shadow_ordering_v1(&cases).expect("disjoint seed groups");
    assert_eq!(validation.development_seed_groups, 1);
    assert_eq!(validation.held_out_seed_groups, 3);
    assert_eq!(validation.held_out_budget_unknown, 1);
    assert_eq!(validation.informative_pairs, 1);
    assert_eq!(validation.concordant_pairs, 1);
    assert_eq!(validation.discordant_pairs, 0);
    assert_eq!(
        validation.scheduling_authority,
        StrategicProbeSchedulingAuthorityV1::WithheldPendingHeldOutCalibration
    );
    assert_eq!(
        validation.owner_authority,
        StrategicProbeOwnerAuthorityV1::NotGranted
    );
}

#[test]
fn held_out_validation_rejects_seed_group_leakage() {
    let cases = [
        StrategicProbeOrderingCalibrationCaseV1 {
            case_id: "dev".to_string(),
            seed_group: "same-seed".to_string(),
            partition: StrategicProbeCalibrationPartitionV1::Development,
            shadow_order_key: None,
            exact_label: StrategicProbeResolvedLabelV1::BudgetUnknown,
        },
        StrategicProbeOrderingCalibrationCaseV1 {
            case_id: "held".to_string(),
            seed_group: "same-seed".to_string(),
            partition: StrategicProbeCalibrationPartitionV1::HeldOut,
            shadow_order_key: None,
            exact_label: StrategicProbeResolvedLabelV1::BudgetUnknown,
        },
    ];

    assert!(validate_strategic_probe_shadow_ordering_v1(&cases)
        .expect_err("one seed group cannot tune and validate the same hint")
        .contains("both development and held-out"));
}
