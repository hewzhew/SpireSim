use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;

use super::*;
use crate::ai::combat_search_v2::{
    CombatSearchV2OutcomeOrderKeyReport, SearchCoverageStatus, SearchTerminalLabel,
};
use crate::content::cards::CardId;
use crate::content::monsters::encounter_pool::EncounterPoolTier;
use crate::content::monsters::factory::EncounterId;
use crate::content::relics::{RelicId, RelicState};
use crate::engine::campfire_candidates::CampfireCandidate;
use crate::eval::campfire_evaluation::{
    build_campfire_evaluation_batch, CampfireContinuationProfile, CampfireEvaluationHorizon,
    CampfireEvaluationSpec, CampfireRunGoal,
};
use crate::eval::campfire_survival_scenarios::{CampfireSurvivalLens, CampfireSurvivalSubject};
use crate::eval::combat_lab_v1::{
    CombatLabCommonBudgetV1, CombatLabOutcomeClassV1, CombatLabProfileSpecV1,
    CombatLabReplayedCandidateV1, CombatLabShuffleGeneratorV1, CombatLabShuffleScheduleV1,
};
use crate::eval::source_identity::SourceIdentity;
use crate::runtime::combat::CombatCard;
use crate::state::core::ClientInput;
use crate::state::run::RunState;

fn profile() -> CombatLabProfileSpecV1 {
    serde_json::from_value(json!({
        "id": "immediate",
        "label": "Immediate",
        "information_scope": "exact_state_oracle",
        "potion_policy": "semantic_budgeted",
        "rollout_policy": "enemy_mechanics_adaptive_no_potion",
        "child_rollout_policy": "immediate",
        "turn_plan_policy": "disabled",
        "phase_guard_policy": "default",
        "setup_bias_policy": "default"
    }))
    .unwrap()
}

fn budget() -> CombatLabCommonBudgetV1 {
    CombatLabCommonBudgetV1 {
        max_nodes: 2_000,
        max_actions_per_line: 200,
        max_engine_steps_per_action: 250,
        wall_ms: None,
        satisfaction: crate::ai::combat_search_v2::CombatSearchV2Satisfaction::ZeroLossOrBudget,
        max_potions_used: Some(1),
        rollout_max_evaluations: 64,
        rollout_max_actions: 40,
        rollout_beam_width: 2,
        turn_plan_probe_max_inner_nodes: None,
        turn_plan_probe_max_end_states: None,
        turn_plan_probe_per_bucket_limit: None,
    }
}

fn panel_spec() -> CampfireThreatPanelSpecV1 {
    CampfireThreatPanelSpecV1 {
        schema_version: CAMPFIRE_THREAT_PANEL_SCHEMA_VERSION,
        experiment_id: "threat-panel-test".to_string(),
        analysis_seed: 77,
        encounter_sources: vec![CampfireThreatEncounterSourceV1::PublicPool {
            act: 3,
            tier: EncounterPoolTier::Strong,
        }],
        schedule: CombatLabShuffleScheduleV1 {
            generator: CombatLabShuffleGeneratorV1::SplitMix64V1,
            seed: 91,
        },
        lenses: vec![
            CampfireSurvivalLens::ActualConsequence,
            CampfireSurvivalLens::RootHpCapability,
        ],
        include_unchanged_root: true,
        profile: profile(),
        common_budget: budget(),
    }
}

fn evaluation_spec() -> CampfireEvaluationSpec {
    CampfireEvaluationSpec {
        run_goal: CampfireRunGoal::Act3Victory,
        horizon: CampfireEvaluationHorizon::UntilNextCampfireOrActTerminal {
            route_horizon_nodes: 5,
        },
        route_path_budget: 2_000,
        continuation_profile: CampfireContinuationProfile {
            profile_id: "threat-panel-test".to_string(),
            source_identity: "test-source".to_string(),
        },
        public_scenario_distribution_id: "act3-strong-public-pool".to_string(),
        mechanics_version: "test-v1".to_string(),
    }
}

fn root() -> RunState {
    let mut root = RunState::new(17, 0, false, "Ironclad");
    root.current_hp = 20;
    root.master_deck = vec![
        CombatCard::for_scenario(CardId::Strike, 101),
        CombatCard::for_scenario(CardId::Defend, 102),
    ];
    root.relics = vec![
        RelicState::new(RelicId::Girya),
        RelicState::new(RelicId::Shovel),
        RelicState::new(RelicId::PeacePipe),
    ];
    root
}

#[test]
fn public_pool_contract_expands_all_act3_strong_entries() {
    let resolved = resolve_campfire_threat_panel_spec_v1(panel_spec()).unwrap();

    assert_eq!(resolved.encounters.len(), 8);
    assert_eq!(
        resolved.encounters[0].encounter_id,
        EncounterId::SpireGrowth
    );
    assert_eq!(
        resolved.encounters[7].encounter_id,
        EncounterId::WrithingMass
    );
    assert!(resolved.encounters.iter().all(|encounter| {
        matches!(
            encounter.provenance,
            CampfireThreatEncounterProvenanceV1::PublicPool {
                normalized_weight,
                ..
            } if (normalized_weight - 0.125).abs() < f64::EPSILON
        )
    }));
}

#[test]
fn panel_contract_rejects_wall_clock_budget_noise() {
    let mut spec = panel_spec();
    spec.common_budget.wall_ms = Some(50);

    let error = resolve_campfire_threat_panel_spec_v1(spec).unwrap_err();

    assert!(error.contains("forbids wall_ms"), "{error}");
}

#[test]
fn compiled_panel_covers_every_alignable_subject_lens_and_encounter() {
    let root = root();
    let evaluation = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
    let resolved = resolve_campfire_threat_panel_spec_v1(panel_spec()).unwrap();

    let sample = compile_campfire_threat_panel_sample_v1(&root, &evaluation, &resolved, 0).unwrap();

    // unchanged root + Rest + two Smith targets + Lift = five alignable subjects.
    assert_eq!(sample.cells.len(), 8 * 2 * 5);
    assert_eq!(sample.gaps.len(), 3); // Dig + two deck-changing Toke targets.
    for encounter in &resolved.encounters {
        for lens in &resolved.spec.lenses {
            assert_eq!(
                sample
                    .cells
                    .iter()
                    .filter(|(candidate_encounter, cell)| {
                        candidate_encounter == encounter && cell.lens == *lens
                    })
                    .count(),
                5
            );
        }
    }
}

#[test]
fn summary_reports_encounter_direction_reversals_without_resolving_limited_cells() {
    let root = root();
    let evaluation = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
    let mut spec = panel_spec();
    spec.encounter_sources = vec![
        CampfireThreatEncounterSourceV1::Explicit {
            encounter_id: EncounterId::JawWorm,
            room_type: crate::state::map::node::RoomType::MonsterRoom,
            label: "jaw-worm-probe".to_string(),
        },
        CampfireThreatEncounterSourceV1::Explicit {
            encounter_id: EncounterId::Transient,
            room_type: crate::state::map::node::RoomType::MonsterRoom,
            label: "transient-probe".to_string(),
        },
    ];
    spec.lenses = vec![CampfireSurvivalLens::ActualConsequence];
    let resolved = resolve_campfire_threat_panel_spec_v1(spec).unwrap();
    let sample = compile_campfire_threat_panel_sample_v1(&root, &evaluation, &resolved, 0).unwrap();
    let rest = CampfireSurvivalSubject::Candidate {
        candidate: CampfireCandidate::Rest,
    };
    let smith = CampfireSurvivalSubject::Candidate {
        candidate: CampfireCandidate::Smith {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(101),
        },
    };
    let mut cells = Vec::new();
    for sample_index in 0..2 {
        for encounter in &resolved.encounters {
            let rest_final = if encounter.encounter_id == EncounterId::JawWorm {
                70
            } else {
                50
            };
            let smith_final = 60;
            let rest_scenario = sample
                .cells
                .iter()
                .find(|(cell_encounter, cell)| {
                    cell_encounter.encounter_id == encounter.encounter_id && cell.subject == rest
                })
                .unwrap();
            let smith_scenario = sample
                .cells
                .iter()
                .find(|(cell_encounter, cell)| {
                    cell_encounter.encounter_id == encounter.encounter_id && cell.subject == smith
                })
                .unwrap();
            cells.push(fake_cell(
                &resolved,
                rest_scenario,
                sample_index,
                rest_final,
            ));
            cells.push(fake_cell(
                &resolved,
                smith_scenario,
                sample_index,
                smith_final,
            ));
        }
    }

    let summary = summarize_campfire_threat_panel_v1(&resolved.contract_hash, &cells, 2).unwrap();

    assert_eq!(summary.strata.len(), 4);
    assert!(summary
        .strata
        .iter()
        .all(|stratum| stratum.coverage_limited == 2 && stratum.resolved_cells == 0));
    let reversal = summary
        .reversals
        .iter()
        .find(|reversal| reversal.left == rest && reversal.right == smith)
        .or_else(|| {
            summary
                .reversals
                .iter()
                .find(|reversal| reversal.left == smith && reversal.right == rest)
        })
        .expect("opposite encounter medians should create one direction reversal");
    assert_eq!(reversal.left_better_encounters.len(), 1);
    assert_eq!(reversal.right_better_encounters.len(), 1);
}

#[test]
fn artifact_store_repairs_partial_tail_and_rejects_duplicate_append() {
    let root = root();
    let evaluation = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
    let mut spec = panel_spec();
    spec.encounter_sources = vec![CampfireThreatEncounterSourceV1::Explicit {
        encounter_id: EncounterId::JawWorm,
        room_type: crate::state::map::node::RoomType::MonsterRoom,
        label: "journal-test".to_string(),
    }];
    spec.lenses = vec![CampfireSurvivalLens::ActualConsequence];
    let resolved = resolve_campfire_threat_panel_spec_v1(spec).unwrap();
    let sample = compile_campfire_threat_panel_sample_v1(&root, &evaluation, &resolved, 0).unwrap();
    let subjects = sample
        .cells
        .iter()
        .map(|(_, cell)| cell.subject)
        .collect::<Vec<_>>();
    let manifest = CampfireThreatPanelManifestV1::new(
        resolved.clone(),
        evaluation.context.clone(),
        subjects,
        sample.gaps.clone(),
        SourceIdentity {
            git_commit: Some("test".to_string()),
            git_dirty: Some(false),
        },
        123,
    );
    let output = std::env::temp_dir().join(format!(
        "campfire-threat-panel-artifact-{}-{}",
        std::process::id(),
        resolved.contract_hash
    ));
    if output.exists() {
        fs::remove_dir_all(&output).unwrap();
    }
    let scenario = &sample.cells[0];
    let mut record = fake_cell(&resolved, scenario, 0, 17);
    record.context_fingerprint = evaluation.context.context_fingerprint.clone();
    record.cell_key = campfire_threat_panel_cell_key_v1(
        &resolved.contract_hash,
        &record.context_fingerprint,
        record.subject,
        record.lens,
        &record.encounter,
        0,
        record.analysis_seed,
        record.shuffle_seed,
        &record.profile_id,
    );

    {
        let mut store =
            CampfireThreatPanelArtifactStoreV1::create_or_resume(&output, manifest.clone())
                .unwrap();
        let mut legacy = record.clone();
        legacy.schema_version = 1;
        let error = store
            .append_cell(&legacy)
            .expect_err("legacy fingerprint cell must be rejected");
        assert!(error.contains("unsupported Campfire threat panel cell schema_version"));
        store.append_cell(&record).unwrap();
        assert!(store.append_cell(&record).is_err());
    }
    OpenOptions::new()
        .append(true)
        .open(output.join("cells.jsonl"))
        .unwrap()
        .write_all(br#"{"partial":"#)
        .unwrap();

    let recovered =
        CampfireThreatPanelArtifactStoreV1::create_or_resume(&output, manifest.clone()).unwrap();

    assert_eq!(recovered.cells().len(), 1);
    assert!(recovered.contains_cell(&record.cell_key));
    assert!(fs::read(output.join("cells.jsonl"))
        .unwrap()
        .ends_with(b"\n"));
    drop(recovered);

    fs::write(
        output.join("cells.jsonl"),
        b"{\"schema_version\":1,\"legacy_fingerprint\":{}}\n",
    )
    .expect("write legacy panel journal");
    let error = CampfireThreatPanelArtifactStoreV1::create_or_resume(&output, manifest)
        .err()
        .expect("legacy panel cell must fail resume");
    assert!(error.contains("unsupported Campfire threat panel cell schema_version"));
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn identical_exact_state_reuse_preserves_result_and_rebinds_cell_identity() {
    let root = root();
    let evaluation = build_campfire_evaluation_batch(&root, evaluation_spec()).unwrap();
    let mut spec = panel_spec();
    spec.encounter_sources = vec![CampfireThreatEncounterSourceV1::Explicit {
        encounter_id: EncounterId::JawWorm,
        room_type: crate::state::map::node::RoomType::MonsterRoom,
        label: "reuse-test".to_string(),
    }];
    let resolved = resolve_campfire_threat_panel_spec_v1(spec).unwrap();
    let sample = compile_campfire_threat_panel_sample_v1(&root, &evaluation, &resolved, 0).unwrap();
    let smith = CampfireSurvivalSubject::Candidate {
        candidate: CampfireCandidate::Smith {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(101),
        },
    };
    let actual = sample
        .cells
        .iter()
        .find(|(_, cell)| {
            cell.subject == smith && cell.lens == CampfireSurvivalLens::ActualConsequence
        })
        .unwrap();
    let capability = sample
        .cells
        .iter()
        .find(|(_, cell)| {
            cell.subject == smith && cell.lens == CampfireSurvivalLens::RootHpCapability
        })
        .unwrap();
    assert_eq!(actual.1.state_fingerprint, capability.1.state_fingerprint);
    let source = fake_cell(&resolved, actual, 0, 13);

    let reused = super::runner::reuse_identical_state_result(
        &source,
        "reused-key",
        &capability.0,
        0,
        &capability.1,
    )
    .unwrap();

    assert_eq!(reused.cell_key, "reused-key");
    assert_eq!(reused.lens, CampfireSurvivalLens::RootHpCapability);
    assert_eq!(
        serde_json::to_value(&reused.replayed_candidate).unwrap(),
        serde_json::to_value(&source.replayed_candidate).unwrap()
    );
    assert_eq!(
        reused.execution_reuse,
        Some(CampfireThreatPanelExecutionReuseV1::IdenticalExactState {
            source_cell_key: source.cell_key
        })
    );
}

fn fake_cell(
    resolved: &ResolvedCampfireThreatPanelSpecV1,
    scenario: &(
        CampfireThreatEncounterV1,
        crate::eval::campfire_survival_scenarios::CampfireSurvivalScenarioCell,
    ),
    sample_index: u64,
    final_hp: i32,
) -> CampfireThreatPanelCellV1 {
    let (encounter, cell) = scenario;
    CampfireThreatPanelCellV1 {
        schema_version: CAMPFIRE_THREAT_PANEL_CELL_SCHEMA_VERSION,
        cell_key: format!(
            "fake-{sample_index}-{:?}-{:?}",
            encounter.encounter_id, cell.subject
        ),
        contract_hash: resolved.contract_hash.clone(),
        context_fingerprint: "context".to_string(),
        subject: cell.subject,
        lens: cell.lens,
        encounter: encounter.clone(),
        sample_index,
        analysis_seed: cell.analysis_seed,
        shuffle_seed: cell.shuffle_seed,
        profile_id: resolved.spec.profile.id.clone(),
        state_fingerprint: cell.state_fingerprint.clone(),
        start_hp: cell.start.combat.entities.player.current_hp,
        search_terminal: Some(SearchTerminalLabel::Win),
        coverage_status: SearchCoverageStatus::TimeBudgetLimited,
        outcome_class: CombatLabOutcomeClassV1::CoverageLimited,
        replay_validated: true,
        replayed_candidate: Some(CombatLabReplayedCandidateV1 {
            terminal: SearchTerminalLabel::Win,
            outcome_order_key: CombatSearchV2OutcomeOrderKeyReport {
                terminal_rank: 0,
                run_hygiene: 0,
                persistent_adjusted_hp: final_hp,
                final_hp,
                persistent_run_value: 0,
                potion_conservation: 0,
                faster_turns: 0,
                fewer_cards_played: 0,
                enemy_progress: 0,
                shorter_line: 0,
            },
            final_hp,
            hp_loss: (cell.start.combat.entities.player.current_hp - final_hp).max(0),
            turns: 3,
            actions: 2,
            cards_played: 1,
            potions_used: 0,
            draw_history: Vec::new(),
            action_history: vec![ClientInput::EndTurn],
        }),
        execution_reuse: None,
        expanded_nodes: 100,
        generated_nodes: 150,
        nodes_to_first_win: Some(50),
        node_budget_exhausted: false,
        deadline_exhausted: true,
        error: None,
    }
}
