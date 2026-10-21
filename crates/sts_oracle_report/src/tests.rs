use super::*;

const SAMPLE: &str = r#"
{
  "schema": "sts-public-actor-census-v9",
  "summary": {"episodes": 1, "successes": 0, "unsupported_decision_surface": 1},
  "ignored_large_field": {"map": [1, 2, 3]},
  "episodes": [{
    "seed": 17,
    "actor": {"actor_name": "reference", "actor_revision": 5},
    "outcome": {"status": "failure", "reason": {"kind": "unsupported_decision_surface"}},
    "combat_research_plan_attempts": [{
      "act": 1, "floor": 7, "encounter_id": "Lagavulin", "hp": 75, "max_hp": 80, "turn": 0,
      "lanes": [{"provenance": {"producer_name": "resident", "producer_revision": 1}, "work_unit": "turn_graph_generation_work", "work_consumed": 4096, "exit": "witness"}],
      "exit": "witness",
      "candidates": [
        {"supporting_provenances": [{"producer_name": "resident", "producer_revision": 1}], "plan_steps": 22, "satisfies_requested_quality": false, "outcome": {"final_hp": 26, "final_gold": 36, "persistent": {"max_hp": 80, "recoverable_gold_delta": 0, "unrecovered_stolen_gold": 0, "ritual_dagger_value": 0, "genetic_algorithm_value": 0, "external_burden_count": 0}, "consumed_entry_potion_slots": [], "total_potions_consumed": 0, "generated_potions": [], "final_relics": [{"id": "Nunchaku", "counter": 5}], "meta_changes": [], "pending_rewards": [], "combat_mugged": false}},
        {"supporting_provenances": [{"producer_name": "resident", "producer_revision": 1}], "plan_steps": 46, "satisfies_requested_quality": false, "outcome": {"final_hp": 28, "final_gold": 36, "persistent": {"max_hp": 80, "recoverable_gold_delta": 0, "unrecovered_stolen_gold": 0, "ritual_dagger_value": 0, "genetic_algorithm_value": 0, "external_burden_count": 0}, "consumed_entry_potion_slots": [], "total_potions_consumed": 0, "generated_potions": [], "final_relics": [{"id": "Nunchaku", "counter": 9}], "meta_changes": [], "pending_rewards": [], "combat_mugged": false}}
      ],
      "non_dominated_candidate_ordinals": [1],
      "selection": "unique_non_dominated_candidate",
      "selected_candidate_ordinal": 1,
      "selected_supporting_provenances": [{"producer_name": "resident", "producer_revision": 1}],
      "plan_steps": 46
    }]
  }]
}
"#;

#[test]
fn compact_projection_ignores_unneeded_payload_and_marks_frontier() {
    let report = parse_public_actor_census_v9(SAMPLE.as_bytes()).unwrap();
    let inspection = inspect_public_actor_census_v9(
        &report,
        &PublicActorCensusInspectFilterV1 {
            encounter: Some("lagavulin".to_string()),
            ..PublicActorCensusInspectFilterV1::default()
        },
    )
    .unwrap();

    let attempt = &inspection.episodes[0].combat_attempts[0];
    assert!(!attempt.candidates[0].non_dominated);
    assert!(attempt.candidates[1].non_dominated);
    assert_eq!(attempt.selected_candidate_ordinal, Some(1));
    assert!(!attempt.selected_candidate_projection_ambiguous);
    assert!(attempt.candidates[1].selected_by_unique_report_projection);
    assert!(render_public_actor_census_inspection_v1(&inspection)
        .contains("C1 [ND] | 46 steps | HP 28"));
    assert!(render_public_actor_census_inspection_v1(&inspection)
        .contains("relic differences [#0=Nunchaku(counter=9)]"));
    let json = serde_json::to_value(&inspection).unwrap();
    assert_eq!(
        json["episodes"][0]["combat_attempts"][0]["candidates"][1]["persistent"]["max_hp"],
        80
    );
    assert_eq!(
        json["episodes"][0]["combat_attempts"][0]["candidates"][1]["final_relics"][0]["counter"],
        9
    );
}

#[test]
fn exact_selected_ordinal_disambiguates_same_producer_and_length() {
    let report = SAMPLE.replace("\"plan_steps\": 22", "\"plan_steps\": 46");
    let report = parse_public_actor_census_v9(report.as_bytes()).unwrap();
    let inspection =
        inspect_public_actor_census_v9(&report, &PublicActorCensusInspectFilterV1::default())
            .unwrap();
    let attempt = &inspection.episodes[0].combat_attempts[0];
    assert_eq!(attempt.selected_candidate_ordinal, Some(1));
    assert!(!attempt.selected_candidate_projection_ambiguous);
}

#[test]
fn validation_rejects_selected_ordinal_that_disagrees_with_receipt_identity() {
    let invalid = SAMPLE.replace(
        "\"selected_candidate_ordinal\": 1",
        "\"selected_candidate_ordinal\": 0",
    );
    let error = parse_public_actor_census_v9(invalid.as_bytes()).unwrap_err();
    assert!(error.contains("disagrees with supporting provenances or plan length"));
}

#[test]
fn validation_rejects_selected_ordinal_outside_fact_frontier() {
    let invalid = SAMPLE
        .replace(
            "\"selected_candidate_ordinal\": 1",
            "\"selected_candidate_ordinal\": 0",
        )
        .replace("\"plan_steps\": 22", "\"plan_steps\": 46");
    let error = parse_public_actor_census_v9(invalid.as_bytes()).unwrap_err();
    assert!(error.contains("outside the non-dominated frontier"));
}

#[test]
fn validation_rejects_noncanonical_resource_slot_order() {
    let invalid = SAMPLE.replacen(
        "\"consumed_entry_potion_slots\": []",
        "\"consumed_entry_potion_slots\": [1, 1]",
        1,
    );
    let error = parse_public_actor_census_v9(invalid.as_bytes()).unwrap_err();
    assert!(error.contains("unsorted or duplicate consumed potion slots"));
}

#[test]
fn outcome_projection_rejects_an_unreported_comparator_field() {
    let invalid = SAMPLE.replace(
        "\"combat_mugged\": false",
        "\"combat_mugged\": false, \"future_comparator_input\": 7",
    );
    let error = parse_public_actor_census_v9(invalid.as_bytes()).unwrap_err();
    assert!(error.contains("future_comparator_input"));
}

#[test]
fn outcome_projection_rejects_a_missing_comparator_field() {
    let invalid = SAMPLE.replace(", \"combat_mugged\": false", "");
    let error = parse_public_actor_census_v9(invalid.as_bytes()).unwrap_err();
    assert!(error.contains("combat_mugged"));
}

#[test]
fn validation_rejects_out_of_range_frontier_ordinal() {
    let invalid = SAMPLE.replace(
        "\"non_dominated_candidate_ordinals\": [1]",
        "\"non_dominated_candidate_ordinals\": [2]",
    );
    let error = parse_public_actor_census_v9(invalid.as_bytes()).unwrap_err();
    assert!(error.contains("only 2 candidates exist"));
}

#[test]
fn validation_rejects_another_schema() {
    let invalid = SAMPLE.replace("sts-public-actor-census-v9", "some-other-schema");
    let error = parse_public_actor_census_v9(invalid.as_bytes()).unwrap_err();
    assert!(error.contains("unsupported census schema"));
}

#[test]
fn resident_corridor_validation_and_render_find_first_missing_successor() {
    let report = ResidentCorridorAuditV1 {
        schema: RESIDENT_CORRIDOR_AUDIT_SCHEMA_V1.to_owned(),
        root_exact_state_hash: "root".to_owned(),
        from_turn_index: 0,
        configured_generation_work: 4096,
        configured_wall_ms: 500,
        configured_satisfaction: "zero_loss_or_budget".to_owned(),
        public_plan_turn_proposal_enabled: true,
        elapsed_ms: 12,
        search_status: "Partial(GenerationWorkBudget)".to_owned(),
        witness_found: false,
        witness: None,
        counters: ResidentCorridorSearchCountersV1 {
            generation_work: 4096,
            exact_nodes: 20,
            exact_edges: 19,
            ..ResidentCorridorSearchCountersV1::default()
        },
        turns: vec![
            ResidentCorridorTurnAuditV1 {
                teacher_turn_index: 0,
                from_turn: 0,
                teacher_action_count: 2,
                source_exact_state_hash: "root".to_owned(),
                successor_exact_state_hash: "t1".to_owned(),
                source: Some(sample_resident_state(64)),
                edge: Some(ResidentCorridorEdgeAuditV1 {
                    negative_log_policy: 1.0,
                    plan_prefix_proposed: false,
                    visits: 1,
                    anchor_visits: 1,
                    backed_visits: 0,
                    successor_path_cost: 1.0,
                    successor_anchor_ordinal_rank: Some(2),
                    successor_anchor_candidate_count: 24,
                    successor_anchor_service_cost: Some(1.0),
                    best_anchor_service_cost: Some(0.0),
                    guide_service: Vec::new(),
                }),
                successor: Some(sample_resident_state(0)),
            },
            ResidentCorridorTurnAuditV1 {
                teacher_turn_index: 1,
                from_turn: 1,
                teacher_action_count: 3,
                source_exact_state_hash: "t1".to_owned(),
                successor_exact_state_hash: "t2".to_owned(),
                source: Some(sample_resident_state(0)),
                edge: None,
                successor: None,
            },
        ],
    };
    validate_resident_corridor_audit_v1(&report).unwrap();
    let rendered = render_resident_corridor_audit_v1(&report);
    assert!(rendered.contains("teacher T0"));
    assert!(rendered.contains("First missing teacher successor: T1"));
}

#[test]
fn resident_corridor_render_surfaces_typed_witness_quality() {
    let report = ResidentCorridorAuditV1 {
        schema: RESIDENT_CORRIDOR_AUDIT_SCHEMA_V1.to_owned(),
        root_exact_state_hash: "root".to_owned(),
        from_turn_index: 0,
        configured_generation_work: 4096,
        configured_wall_ms: 500,
        configured_satisfaction: "zero_loss_or_budget".to_owned(),
        public_plan_turn_proposal_enabled: true,
        elapsed_ms: 12,
        search_status: "Partial(GenerationWorkBudget)".to_owned(),
        witness_found: true,
        witness: Some(ResidentCorridorWitnessAuditV1 {
            starting_player_hp: 75,
            final_player_hp: 42,
            max_player_hp: 80,
            action_count: 21,
            potion_expenditures: 0,
            final_exact_state_hash: "win".to_owned(),
        }),
        counters: ResidentCorridorSearchCountersV1::default(),
        turns: Vec::new(),
    };

    validate_resident_corridor_audit_v1(&report).unwrap();
    let rendered = render_resident_corridor_audit_v1(&report);
    assert!(rendered.contains("42/80 HP from 75 | 21 actions | 0 potions"));
}

fn sample_resident_state(generation_work: usize) -> ResidentCorridorStateAuditV1 {
    ResidentCorridorStateAuditV1 {
        relative_turn_depth: 0,
        visits: 1,
        first_service_selection: (generation_work > 0).then_some(0),
        first_guide_service_selection: None,
        generation_work,
        retained_generator_work_items: 0,
        generator_anchor_work_pops: 0,
        generator_guided_work_pops: 0,
        generated_options: 0,
        children: 0,
        exhausted: false,
    }
}
