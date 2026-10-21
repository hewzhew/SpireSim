use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use sts_parity_lockstep::*;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

#[test]
fn natural_rng_lifecycle_excludes_discarded_floor_streams_before_room_entry() {
    let neow =
        natural_run_active_rng_owners_v4("natural_neow", true, NaturalRunRngRoomV3::Other, false)
            .unwrap();
    let neow_grid = natural_run_active_rng_owners_v4(
        "natural_neow_grid_select",
        true,
        NaturalRunRngRoomV3::Other,
        false,
    )
    .unwrap();
    let map =
        natural_run_active_rng_owners_v4("natural_map", false, NaturalRunRngRoomV3::Other, false)
            .unwrap();
    let event =
        natural_run_active_rng_owners_v4("natural_event", false, NaturalRunRngRoomV3::Other, false)
            .unwrap();
    let grid = natural_run_active_rng_owners_v4(
        "natural_grid_select",
        false,
        NaturalRunRngRoomV3::Other,
        false,
    )
    .unwrap();
    let combat = natural_run_active_rng_owners_v4(
        "natural_combat",
        false,
        NaturalRunRngRoomV3::Other,
        false,
    )
    .unwrap();
    let hand = natural_run_active_rng_owners_v4(
        "natural_hand_select",
        false,
        NaturalRunRngRoomV3::Other,
        false,
    )
    .unwrap();

    assert_eq!(neow.len(), 9);
    assert!(neow.contains(&"run.neow"));
    assert!(neow.iter().all(|owner| !owner.starts_with("floor.")));
    assert_eq!(neow_grid, neow);
    assert_eq!(map, NATURAL_RUN_PERSISTENT_RNG_OWNERS_V2);
    assert_eq!(event, combat);
    assert_eq!(grid, event);
    assert_eq!(hand, combat);
    assert_eq!(combat.len(), 13);
    assert!(NATURAL_RUN_FLOOR_RNG_OWNERS_V2
        .iter()
        .all(|owner| combat.contains(owner)));
    assert!(natural_run_active_rng_owners_v4(
        "natural_map",
        true,
        NaturalRunRngRoomV3::Other,
        false
    )
    .is_err());
}

#[test]
fn natural_campfire_boundaries_keep_the_current_floor_rng_family_active() {
    for boundary in [
        "natural_campfire",
        "natural_campfire_smith",
        "natural_campfire_smith_confirm",
        "natural_campfire_complete",
    ] {
        let owners =
            natural_run_active_rng_owners_v4(boundary, false, NaturalRunRngRoomV3::Other, false)
                .unwrap();
        assert!(owners.contains(&"floor.misc"), "{boundary}");
        assert!(!owners.contains(&"run.neow"), "{boundary}");
    }
}

#[test]
fn deck_selection_choice_keeps_only_source_binding_and_stable_card_facts() {
    let choice = natural_deck_card_choice_v1("natural_run/starter/3", "strike", 2).unwrap();

    assert_eq!(choice.kind, "deck_card");
    assert_eq!(choice.id, "natural_run/starter/3");
    assert_eq!(
        choice.attributes,
        BTreeMap::from([
            (
                "card_id".to_owned(),
                SemanticScalarV1::Text("strike".to_owned()),
            ),
            ("upgrades".to_owned(), SemanticScalarV1::Integer(2)),
        ])
    );
    assert!(natural_deck_card_choice_v1("java-runtime-uuid", "strike", 0).is_err());
}

#[test]
fn hand_selection_choice_keeps_source_identity_without_endpoint_card_ids() {
    let choice = natural_hand_card_choice_v1("natural_run/starter/5", "defend_r", 1).unwrap();

    assert_eq!(choice.kind, "hand_card");
    assert_eq!(choice.id, "natural_run/starter/5");
    assert_eq!(choice.occurrence, 0);
    assert_eq!(
        choice.attributes.get("card_id"),
        Some(&SemanticScalarV1::Text("defend_r".to_owned()))
    );
    assert_eq!(
        choice.attributes.get("upgrades"),
        Some(&SemanticScalarV1::Integer(1))
    );
    assert!(natural_hand_card_choice_v1("java-runtime-uuid", "defend_r", 0).is_err());
}

#[test]
fn natural_bootstrap_rejects_duplicate_source_slots_with_distinct_bindings() {
    let cards = ["left", "right"]
        .into_iter()
        .map(|binding| NaturalRunBootstrapCardV1 {
            binding: binding.to_owned(),
            source: NaturalRunCardSourceV1::StarterDeck {
                player_class: "IRONCLAD".to_owned(),
                source_slot: 0,
            },
            card_id: "strike".to_owned(),
            upgrades: 0,
        })
        .collect();
    let error = NaturalRunBootstrapV1::new(
        "7",
        0,
        false,
        NaturalRunContentProfileV1::AllUnlocked,
        NaturalRunNeowEligibilityV1::Rich,
        "IRONCLAD",
        cards,
    )
    .unwrap_err();
    assert!(error.contains("repeats or omits"));
}

#[test]
fn neow_drawback_card_source_rejects_invalid_shapes() {
    for source in [
        NaturalRunCardSourceV1::NeowDrawback {
            reward_type: String::new(),
            drawback: "curse".to_owned(),
            product_slot: 0,
        },
        NaturalRunCardSourceV1::NeowDrawback {
            reward_type: "one_rare_relic".to_owned(),
            drawback: "no_gold".to_owned(),
            product_slot: 0,
        },
        NaturalRunCardSourceV1::NeowDrawback {
            reward_type: "one_rare_relic".to_owned(),
            drawback: "curse".to_owned(),
            product_slot: 1,
        },
    ] {
        assert!(natural_run_card_source_key_v1(&source).is_err());
        assert!(natural_run_card_binding_v1(&source).is_err());
    }
}

#[test]
fn dynamic_neow_card_source_cannot_enter_the_natural_bootstrap() {
    let cards = vec![
        NaturalRunBootstrapCardV1 {
            binding: "natural_run/starter/0".to_owned(),
            source: NaturalRunCardSourceV1::StarterDeck {
                player_class: "IRONCLAD".to_owned(),
                source_slot: 0,
            },
            card_id: "strike".to_owned(),
            upgrades: 0,
        },
        NaturalRunBootstrapCardV1 {
            binding: "natural_run/neow_drawback/one_rare_relic/curse/0".to_owned(),
            source: NaturalRunCardSourceV1::NeowDrawback {
                reward_type: "one_rare_relic".to_owned(),
                drawback: "curse".to_owned(),
                product_slot: 0,
            },
            card_id: "injury".to_owned(),
            upgrades: 0,
        },
    ];
    let error = NaturalRunBootstrapV1::new(
        "7",
        0,
        false,
        NaturalRunContentProfileV1::AllUnlocked,
        NaturalRunNeowEligibilityV1::Rich,
        "IRONCLAD",
        cards,
    )
    .unwrap_err();

    assert!(error.contains("invalid card source"));
}

#[test]
fn combat_birth_source_uses_room_action_and_product_as_its_stable_binding() {
    let source = NaturalRunCardSourceV1::CombatBirth {
        room_id: "F01X03".to_owned(),
        action_ordinal: 4,
        cause: NaturalRunCombatCardBirthCauseV1::EndTurnResolution,
        product_slot: 0,
    };

    assert_eq!(
        natural_run_card_source_key_v1(&source).unwrap(),
        "combat_birth:F01X03:4:0"
    );
    assert_eq!(
        natural_run_card_binding_v1(&source).unwrap(),
        "natural_run/combat_birth/F01X03/4/0"
    );
    for invalid in [
        NaturalRunCardSourceV1::CombatBirth {
            room_id: "F00X03".to_owned(),
            action_ordinal: 4,
            cause: NaturalRunCombatCardBirthCauseV1::EndTurnResolution,
            product_slot: 0,
        },
        NaturalRunCardSourceV1::CombatBirth {
            room_id: "F01X03".to_owned(),
            action_ordinal: 4,
            cause: NaturalRunCombatCardBirthCauseV1::CardPlay {
                producer_binding: "foreign/card".to_owned(),
            },
            product_slot: 0,
        },
        NaturalRunCardSourceV1::CombatBirth {
            room_id: "F01X03".to_owned(),
            action_ordinal: 4,
            cause: NaturalRunCombatCardBirthCauseV1::PotionUse {
                potion_id: String::new(),
                slot: 0,
            },
            product_slot: 0,
        },
    ] {
        assert!(natural_run_card_source_key_v1(&invalid).is_err());
        assert!(natural_run_card_binding_v1(&invalid).is_err());
    }
}

#[test]
fn card_reward_source_uses_room_and_acquisition_slot_as_its_stable_binding() {
    let source = NaturalRunCardSourceV1::CardReward {
        room_id: "F02X03".to_owned(),
        product_slot: 1,
    };
    assert_eq!(
        natural_run_card_source_key_v1(&source).unwrap(),
        "card_reward:F02X03:1"
    );
    assert_eq!(
        natural_run_card_binding_v1(&source).unwrap(),
        "natural_run/card_reward/F02X03/1"
    );
    for room_id in ["F00X03", "F2X03", "F02X3", "foreign"] {
        assert!(
            natural_run_card_source_key_v1(&NaturalRunCardSourceV1::CardReward {
                room_id: room_id.to_owned(),
                product_slot: 0,
            })
            .is_err()
        );
    }
}

#[test]
fn neow_drawback_source_requires_the_closed_semantic_action_shape() {
    let mut missing_drawback = SemanticChoiceV1::new("neow_reward", "one_rare_relic", 0);
    let action = SemanticActionV1::Select {
        surface: DecisionSurfaceV1::Neow,
        choice: missing_drawback.clone(),
    };
    assert!(natural_run_neow_drawback_card_source_v1(&action).is_err());

    missing_drawback.attributes.insert(
        "drawback".to_owned(),
        SemanticScalarV1::Text("curse".to_owned()),
    );
    missing_drawback.occurrence = 1;
    let action = SemanticActionV1::Select {
        surface: DecisionSurfaceV1::Neow,
        choice: missing_drawback,
    };
    assert!(natural_run_neow_drawback_card_source_v1(&action).is_err());
}

struct FakeAdapter {
    initial: Option<ParitySnapshotV1>,
    successors: VecDeque<Result<ParitySnapshotV1, AdapterFailureV1>>,
    prepare_failure: Option<AdapterFailureV1>,
    rng_call_ledger: Option<NaturalRunRngCallLedgerV1>,
}

impl LockstepAdapterV1 for FakeAdapter {
    type PreparedAction = SemanticActionV1;

    fn observe(&mut self) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        self.initial.take().ok_or_else(|| {
            AdapterFailureV1::new(AdapterFailureKindV1::Evidence, "missing initial snapshot")
        })
    }

    fn prepare(
        &mut self,
        action: &SemanticActionV1,
        _based_on: &ParitySnapshotV1,
    ) -> Result<Self::PreparedAction, AdapterFailureV1> {
        if let Some(failure) = self.prepare_failure.take() {
            Err(failure)
        } else {
            Ok(action.clone())
        }
    }

    fn execute(
        &mut self,
        _prepared: Self::PreparedAction,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        self.successors.pop_front().unwrap_or_else(|| {
            Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Evidence,
                "missing successor",
            ))
        })
    }

    fn last_rng_call_ledger_v1(&self) -> Option<&NaturalRunRngCallLedgerV1> {
        self.rng_call_ledger.as_ref()
    }
}

#[test]
fn exact_committed_action_advances_and_persists_one_step() {
    let root = temp_path("exact");
    let initial = snapshot("root", json!({"floor": 1}), true);
    let successor = snapshot("next", json!({"floor": 2}), true);
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(initial.clone(), successor.clone()),
        ok_adapter(initial, successor),
    );
    let entry = runner.step(SemanticActionV1::EndTurn).unwrap();
    assert!(matches!(
        entry.outcome,
        TransitionOutcomeV1::Continued {
            with_coverage_gap: false
        }
    ));
    assert!(entry.rng_call_comparison.as_ref().unwrap().exact);
    assert!(!runner.is_stopped());
    assert_eq!(
        runner.current_comparison().status,
        ComparisonStatusV1::Exact
    );
    assert_eq!(runner.stop_reason(), None);
    assert!(root.join("session.json").is_file());
    let persisted = fs::read_to_string(root.join("steps/00000000.json")).unwrap();
    assert!(persisted.contains("sts-lockstep-transition-v4"));
    assert!(persisted.contains("rng_call_comparison"));
    assert!(persisted.contains("java_digest"));
    assert!(!persisted.contains("\"floor\""));
    assert!(!persisted.contains("java_before"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn exact_journal_does_not_repeat_large_projected_values() {
    let root = temp_path("compact-journal");
    let initial = snapshot("root", json!({"marker": "i".repeat(100_000)}), true);
    let successor = snapshot("next", json!({"marker": "s".repeat(100_000)}), true);
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(initial.clone(), successor.clone()),
        ok_adapter(initial, successor),
    );

    runner.step(SemanticActionV1::EndTurn).unwrap();

    let session = fs::read(root.join("session.json")).unwrap();
    let step = fs::read(root.join("steps/00000000.json")).unwrap();
    assert!(
        session.len() < 10_000,
        "session receipt was {} bytes",
        session.len()
    );
    assert!(step.len() < 10_000, "step receipt was {} bytes", step.len());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_initializes_beside_preexisting_raw_evidence() {
    let root = temp_path("raw-evidence-sibling");
    let evidence = root.join("java-evidence");
    fs::create_dir_all(&evidence).unwrap();
    fs::write(evidence.join("boundary.json"), b"raw evidence\n").unwrap();
    let initial = snapshot("root", json!({"floor": 1}), true);

    let _runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(initial.clone(), initial.clone()),
        ok_adapter(initial.clone(), initial),
    );

    assert_eq!(
        fs::read(evidence.join("boundary.json")).unwrap(),
        b"raw evidence\n"
    );
    assert!(root.join("session.json").is_file());
    assert!(root.join("steps").is_dir());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn audit_allows_only_noncritical_unobserved_domains() {
    let java = snapshot_with_gap("java", false);
    let rust = snapshot("rust", json!({"floor": 1}), false);
    let comparison = compare_snapshots_v1(&java, &rust).unwrap();
    assert_eq!(comparison.status, ComparisonStatusV1::Unobserved);
    assert!(comparison.may_continue(LockstepModeV1::Audit));
    assert!(!comparison.may_continue(LockstepModeV1::Certification));

    let critical = compare_snapshots_v1(&snapshot_with_gap("java2", true), &rust).unwrap();
    assert!(!critical.may_continue(LockstepModeV1::Audit));
}

#[test]
fn rust_failure_after_java_step_keeps_java_successor_for_offline_recompare() {
    let root = temp_path("partial");
    let initial = snapshot("root", json!({"floor": 1}), true);
    let java_after = snapshot("java-next", json!({"floor": 2}), true);
    let rust_failure = AdapterFailureV1::new(
        AdapterFailureKindV1::Unsupported,
        "Rust action is not implemented",
    );
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(initial.clone(), java_after.clone()),
        FakeAdapter {
            initial: Some(initial),
            successors: VecDeque::from([Err(rust_failure)]),
            prepare_failure: None,
            rng_call_ledger: Some(empty_rng_call_ledger()),
        },
    );
    let entry = runner.step(SemanticActionV1::EndTurn).unwrap();
    assert!(runner.is_stopped());
    assert_eq!(entry.java_after.as_ref(), Some(&java_after));
    assert_eq!(runner.last_transition_outcome(), Some(&entry.outcome));
    assert!(entry.rust_after.is_none());
    assert!(matches!(
        entry.outcome,
        TransitionOutcomeV1::StoppedByAdapter {
            endpoint: EndpointV1::Rust,
            phase: AdapterPhaseV1::Execute,
            ..
        }
    ));
    assert_eq!(
        runner.stop_reason(),
        Some("Rust execute failed: Rust action is not implemented")
    );
    assert_eq!(
        runner.step(SemanticActionV1::EndTurn).unwrap_err(),
        "Rust execute failed: Rust action is not implemented"
    );

    let repaired = recompare_recorded_java_snapshot_after_v1(&entry, &java_after).unwrap();
    assert_eq!(repaired.status, ComparisonStatusV1::Exact);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mismatched_successor_stops_at_the_first_committed_transition() {
    let root = temp_path("mismatch");
    let initial = snapshot("root", json!({"floor": 1}), true);
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(
            initial.clone(),
            snapshot("java-next", json!({"gold": 100}), true),
        ),
        ok_adapter(initial, snapshot("rust-next", json!({"gold": 99}), true)),
    );
    let entry = runner.step(SemanticActionV1::EndTurn).unwrap();
    assert_eq!(runner.last_transition_outcome(), Some(&entry.outcome));
    assert!(runner.is_stopped());
    assert!(matches!(
        entry.outcome,
        TransitionOutcomeV1::StoppedByComparison { .. }
    ));
    assert_eq!(
        entry.comparison.unwrap().status,
        ComparisonStatusV1::Mismatch
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rng_call_mismatch_stops_even_when_successor_snapshots_are_exact() {
    let root = temp_path("rng-call-mismatch");
    let initial = snapshot("root", json!({"floor": 1}), true);
    let successor = snapshot("next", json!({"floor": 2}), true);
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        adapter_with_rng_call(initial.clone(), successor.clone(), true),
        adapter_with_rng_call(initial, successor, false),
    );

    let entry = runner.step(SemanticActionV1::EndTurn).unwrap();

    assert!(runner.is_stopped());
    assert_eq!(
        entry.comparison.as_ref().unwrap().status,
        ComparisonStatusV1::Exact
    );
    let rng = entry.rng_call_comparison.as_ref().unwrap();
    assert!(!rng.exact);
    assert_eq!(rng.first_difference.as_ref().unwrap().call_index, 0);
    assert!(runner
        .stop_reason()
        .unwrap()
        .contains("RNG calls differ at transition call 0"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_rust_rng_ledger_stops_and_keeps_both_successors() {
    let root = temp_path("missing-rust-rng-ledger");
    let initial = snapshot("root", json!({"floor": 1}), true);
    let successor = snapshot("next", json!({"floor": 2}), true);
    let mut rust = ok_adapter(initial.clone(), successor.clone());
    rust.rng_call_ledger = None;
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(initial.clone(), successor.clone()),
        rust,
    );

    let entry = runner.step(SemanticActionV1::EndTurn).unwrap();

    assert!(runner.is_stopped());
    assert_eq!(entry.java_after.as_ref(), Some(&successor));
    assert_eq!(entry.rust_after.as_ref(), Some(&successor));
    assert!(matches!(
        entry.outcome,
        TransitionOutcomeV1::StoppedByAdapter {
            endpoint: EndpointV1::Rust,
            phase: AdapterPhaseV1::ValidateSuccessor,
            ..
        }
    ));
    assert_eq!(
        runner.stop_reason(),
        Some(
            "Rust successor validation failed: Rust adapter omitted the committed RNG-call ledger"
        )
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn producer_epoch_change_is_recorded_and_stops_before_rust_execution() {
    let root = temp_path("epoch-change");
    let initial = snapshot("root", json!({"floor": 1}), true);
    let mut changed = snapshot("java-next", json!({"floor": 2}), true);
    changed.producer.session_epoch = "different-epoch".to_owned();
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(initial.clone(), changed.clone()),
        ok_adapter(initial, snapshot("rust-next", json!({"floor": 2}), true)),
    );
    let entry = runner.step(SemanticActionV1::EndTurn).unwrap();
    assert!(runner.is_stopped());
    assert_eq!(entry.java_after.as_ref(), Some(&changed));
    assert!(entry.rust_after.is_none());
    assert!(matches!(
        entry.outcome,
        TransitionOutcomeV1::StoppedByAdapter {
            endpoint: EndpointV1::Java,
            phase: AdapterPhaseV1::ValidateSuccessor,
            ..
        }
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn shared_action_space_assigns_stable_short_handles_without_hashes() {
    let mut choice = SemanticChoiceV1::new("map_node", "F1X3", 0);
    choice.attributes.insert(
        "room".to_owned(),
        SemanticScalarV1::Text("monster".to_owned()),
    );
    let select = SemanticActionV1::Select {
        surface: DecisionSurfaceV1::Map,
        choice,
    };
    let space = SemanticActionSpaceV1::new([SemanticActionV1::EndTurn, select.clone()]).unwrap();
    assert_eq!(space.actions.len(), 2);
    assert_eq!(space.resolve("A1"), Some(&select));
    assert!(space
        .compact_lines()
        .iter()
        .all(|line| !line.to_ascii_lowercase().contains("sha")));
    assert!(SemanticActionSpaceV1::new([select.clone(), select]).is_err());
}

#[test]
fn canonical_selection_indices_preserve_source_order_and_allowed_counts() {
    assert_eq!(
        canonical_selection_indices_v1(3, [0, 2]).unwrap(),
        vec![vec![], vec![0, 1], vec![0, 2], vec![1, 2]]
    );
    assert!(canonical_selection_indices_v1(3, [2, 1]).is_err());
    assert!(canonical_selection_indices_v1(11, [1]).is_err());
}

#[test]
fn empty_typed_selection_is_left_for_the_surface_adapter_to_validate() {
    SemanticActionV1::SubmitSelection {
        surface: DecisionSurfaceV1::HandSelect,
        choices: Vec::new(),
    }
    .validate()
    .unwrap();
}

#[test]
fn snapshot_contract_rejects_a_domain_omitted_by_both_endpoints() {
    let mut java = snapshot("java", json!({"floor": 1}), true);
    let mut rust = snapshot("rust", json!({"floor": 1}), true);
    java.contract
        .domains
        .insert(ParityDomainV1::RngStreams, true);
    rust.contract
        .domains
        .insert(ParityDomainV1::RngStreams, true);
    assert!(compare_snapshots_v1(&java, &rust)
        .unwrap_err()
        .contains("exactly cover"));
}

#[test]
fn different_snapshot_contracts_are_unsupported_not_exact() {
    let java = snapshot("java", json!({"floor": 1}), true);
    let mut rust = snapshot("rust", json!({"floor": 1}), true);
    rust.contract.schema = "different-v1".to_owned();
    let comparison = compare_snapshots_v1(&java, &rust).unwrap();
    assert_eq!(comparison.status, ComparisonStatusV1::Unsupported);
    assert!(!comparison.may_continue(LockstepModeV1::Audit));
}

#[test]
fn initial_mismatch_remains_available_without_reading_the_journal() {
    let root = temp_path("initial-stop");
    let java = snapshot("java", json!({"floor": 1}), true);
    let rust = snapshot("rust", json!({"floor": 2}), true);
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(java.clone(), java),
        ok_adapter(rust.clone(), rust),
    );

    assert!(runner.is_stopped());
    assert_eq!(
        runner.stop_reason(),
        Some("Run values differ at $.floor: Java=1 Rust=2")
    );
    assert_eq!(
        runner.current_comparison().status,
        ComparisonStatusV1::Mismatch
    );
    assert_eq!(
        runner.step(SemanticActionV1::EndTurn).unwrap_err(),
        "Run values differ at $.floor: Java=1 Rust=2"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mismatch_reason_names_the_first_nested_field_without_dumping_the_domain() {
    let java = snapshot(
        "java",
        json!({"encounter_schedule": {"normal": ["2louse", "jawworm"]}}),
        true,
    );
    let rust = snapshot(
        "rust",
        json!({"encounter_schedule": {"normal": ["twolouse", "jawworm"]}}),
        true,
    );
    let comparison = compare_snapshots_v1(&java, &rust).unwrap();

    assert_eq!(
        comparison.first_stop(LockstepModeV1::Audit).as_deref(),
        Some(
            "Run values differ at $.encounter_schedule.normal[0]: Java=\"2louse\" Rust=\"twolouse\""
        )
    );
}

#[test]
fn action_references_expire_after_one_transition_even_with_the_same_actions() {
    let root = temp_path("action-handle");
    let initial = action_snapshot("initial", [SemanticActionV1::EndTurn]);
    let successor = action_snapshot("successor", [SemanticActionV1::EndTurn]);
    let mut runner = runner(
        &root,
        LockstepModeV1::Audit,
        ok_adapter(initial.clone(), successor.clone()),
        ok_adapter(initial, successor),
    );

    assert_eq!(runner.stop_reason(), None);
    let view = runner.current_action_view().unwrap();
    let handle = view.actions().actions[0].handle.clone();
    assert_eq!(
        runner.resolve_action_handle(&handle).unwrap(),
        SemanticActionV1::EndTurn
    );
    assert!(runner.resolve_action_handle("A0").is_err());
    let (java, rust) = runner.current_snapshots();
    assert_eq!(java.observation_id, "initial");
    assert_eq!(rust.observation_id, "initial");

    let entry = runner.step_handle(&handle).unwrap();
    assert_eq!(entry.action, SemanticActionV1::EndTurn);
    let (java, rust) = runner.current_snapshots();
    assert_eq!(java.observation_id, "successor");
    assert_eq!(rust.observation_id, "successor");
    assert!(runner.step_handle(&handle).is_err());
    assert!(view
        .resolve_for(&runner.current_action_view().unwrap(), &handle)
        .is_err());
    let same_snapshot = runner.current_snapshots().0;
    let other_owner = ActionViewV1::new("another-branch", 1, same_snapshot).unwrap();
    assert!(other_owner
        .resolve_for(&runner.current_action_view().unwrap(), &handle)
        .is_err());
    fs::remove_dir_all(root).unwrap();
}

fn runner(
    root: &Path,
    mode: LockstepModeV1,
    java: FakeAdapter,
    rust: FakeAdapter,
) -> LockstepRunnerV1<FakeAdapter, FakeAdapter> {
    LockstepRunnerV1::start_new(java, rust, mode, root, "test-session").unwrap()
}

fn ok_adapter(initial: ParitySnapshotV1, successor: ParitySnapshotV1) -> FakeAdapter {
    FakeAdapter {
        initial: Some(initial),
        successors: VecDeque::from([Ok(successor)]),
        prepare_failure: None,
        rng_call_ledger: Some(empty_rng_call_ledger()),
    }
}

fn adapter_with_rng_call(
    initial: ParitySnapshotV1,
    successor: ParitySnapshotV1,
    result: bool,
) -> FakeAdapter {
    FakeAdapter {
        initial: Some(initial),
        successors: VecDeque::from([Ok(successor)]),
        prepare_failure: None,
        rng_call_ledger: Some(
            NaturalRunRngCallLedgerV1::new(vec![NaturalRunRngCallV1 {
                owner: "run.card".to_owned(),
                owner_ordinal: 0,
                operation: NaturalRunRngOperationV1::RandomBoolean { result },
            }])
            .unwrap(),
        ),
    }
}

fn empty_rng_call_ledger() -> NaturalRunRngCallLedgerV1 {
    NaturalRunRngCallLedgerV1::new(Vec::new()).unwrap()
}

struct InputAdapter {
    inner: FakeAdapter,
    inputs: ExecutionInputsV1,
}

impl LockstepAdapterV1 for InputAdapter {
    type PreparedAction = SemanticActionV1;
    fn observe(&mut self) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        self.inner.observe()
    }
    fn prepare(
        &mut self,
        action: &SemanticActionV1,
        before: &ParitySnapshotV1,
    ) -> Result<Self::PreparedAction, AdapterFailureV1> {
        self.inner.prepare(action, before)
    }
    fn execute(
        &mut self,
        prepared: Self::PreparedAction,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        self.inner.execute(prepared)
    }
    fn last_rng_call_ledger_v1(&self) -> Option<&NaturalRunRngCallLedgerV1> {
        self.inner.last_rng_call_ledger_v1()
    }
    fn last_execution_inputs_v1(&self) -> ExecutionInputsV1 {
        self.inputs.clone()
    }
    fn execute_with_inputs_v1(
        &mut self,
        prepared: Self::PreparedAction,
        inputs: &ExecutionInputsV1,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        assert_eq!(inputs, &self.inputs);
        self.inner.execute(prepared)
    }
}

#[test]
fn execution_inputs_are_forwarded_and_journaled_only_after_java_execution() {
    for prepare_fails in [false, true] {
        let root = temp_path("execution-inputs");
        let initial = snapshot("root", json!({"floor": 1}), true);
        let successor = snapshot("next", json!({"floor": 2}), true);
        let inputs = ExecutionInputsV1 {
            secret_portal_playtime_bits: Some(vec![800.0f32.to_bits()]),
        };
        let mut java = InputAdapter {
            inner: ok_adapter(initial.clone(), successor.clone()),
            inputs: inputs.clone(),
        };
        if prepare_fails {
            java.inner.prepare_failure = Some(AdapterFailureV1::new(
                AdapterFailureKindV1::Rejected,
                "stale",
            ));
        }
        let rust = InputAdapter {
            inner: ok_adapter(initial, successor),
            inputs: inputs.clone(),
        };
        let mut runner =
            LockstepRunnerV1::start_new(java, rust, LockstepModeV1::Audit, &root, "time-input")
                .unwrap();
        let entry = runner.step(SemanticActionV1::EndTurn).unwrap();
        assert_eq!(entry.execution_inputs, (!prepare_fails).then_some(inputs));
        assert_eq!(runner.is_stopped(), prepare_fails);
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("steps/00000000.json")).unwrap()).unwrap();
        assert_eq!(
            stored["execution_inputs"],
            serde_json::to_value(&entry.execution_inputs).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn snapshot(id: &str, value: serde_json::Value, critical: bool) -> ParitySnapshotV1 {
    ParitySnapshotV1 {
        observation_id: id.to_owned(),
        boundary_kind: "player_decision".to_owned(),
        producer: ProducerIdentityV1 {
            implementation: "fake".to_owned(),
            version: "1".to_owned(),
            session_epoch: "epoch".to_owned(),
        },
        contract: SnapshotContractV1::new("test-v1", [(ParityDomainV1::Run, critical)]).unwrap(),
        domains: BTreeMap::from([(
            ParityDomainV1::Run,
            DomainObservationV1::observed("run-v1", value),
        )]),
    }
}

fn snapshot_with_gap(id: &str, critical: bool) -> ParitySnapshotV1 {
    ParitySnapshotV1 {
        observation_id: id.to_owned(),
        boundary_kind: "player_decision".to_owned(),
        producer: ProducerIdentityV1 {
            implementation: "fake".to_owned(),
            version: "1".to_owned(),
            session_epoch: "epoch".to_owned(),
        },
        contract: SnapshotContractV1::new("test-v1", [(ParityDomainV1::Run, critical)]).unwrap(),
        domains: BTreeMap::from([(
            ParityDomainV1::Run,
            DomainObservationV1::unobserved("not projected yet"),
        )]),
    }
}

fn action_snapshot(
    id: &str,
    actions: impl IntoIterator<Item = SemanticActionV1>,
) -> ParitySnapshotV1 {
    let (_, observation) = SemanticActionSpaceV1::new(actions)
        .unwrap()
        .domain_observation()
        .unwrap();
    ParitySnapshotV1 {
        observation_id: id.to_owned(),
        boundary_kind: "player_decision".to_owned(),
        producer: ProducerIdentityV1 {
            implementation: "fake".to_owned(),
            version: "1".to_owned(),
            session_epoch: "epoch".to_owned(),
        },
        contract: SnapshotContractV1::new("test-v1", [(ParityDomainV1::ActionSpace, true)])
            .unwrap(),
        domains: BTreeMap::from([(ParityDomainV1::ActionSpace, observation)]),
    }
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sts-parity-lockstep-{label}-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ))
}
