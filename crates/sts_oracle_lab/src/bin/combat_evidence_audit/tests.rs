use std::collections::BTreeSet;
use std::path::PathBuf;

use sts_oracle_runtime::content::cards::{CardId, CardType};
use sts_oracle_runtime::sim::combat::CombatTerminal;
use sts_oracle_runtime::state::core::ClientInput;

use super::artifacts::is_trace_artifact_name;
use super::query::{execute_query_batch, CombatEvidenceQueryBatch};
use super::replay::{build_fiend_fire_observation, replay_pair};
use super::{
    ActionObservation, CardObservation, EvidenceRecord, FiendFireClassification,
    MonsterObservation, PairCandidate, PlayerObservation, PreviousCardBypassObservation,
    PreviousCardBypassStatus, StateObservation,
};

fn monster(id: usize, hp: i32, block: i32, terminal: bool) -> MonsterObservation {
    MonsterObservation {
        id,
        hp,
        max_hp: hp.max(1),
        block,
        slot: id.saturating_sub(1) as u8,
        is_dying: terminal,
        half_dead: false,
        is_escaped: false,
    }
}

fn state(turn: u32, target: MonsterObservation) -> StateObservation {
    StateObservation {
        turn,
        energy: 3,
        player: PlayerObservation { hp: 80, block: 0 },
        hand: Vec::new(),
        monsters: vec![target],
    }
}

fn card(id: CardId, recipe_slot: u32) -> CardObservation {
    CardObservation {
        id,
        lineage: sts_oracle_runtime::runtime::combat::CardLineageId::constructed_scenario(
            recipe_slot,
        ),
        upgrades: 0,
        cost_for_turn: None,
        free_to_play_once: false,
    }
}

#[test]
fn a3f37_shape_requires_nonlethal_immediate_fiend_fire() {
    let actions = vec![
        ActionObservation {
            index: 0,
            input: ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            card: Some(card(CardId::SwordBoomerang, 10)),
            card_type: Some(CardType::Attack),
            before: state(2, monster(3, 20, 27, false)),
            after: state(2, monster(3, 20, 21, false)),
            terminal_after: CombatTerminal::Unresolved,
            previous_card_bypass: None,
        },
        ActionObservation {
            index: 1,
            input: ClientInput::PlayCard {
                card_index: 0,
                target: Some(3),
            },
            card: Some(card(CardId::FiendFire, 11)),
            card_type: Some(CardType::Attack),
            before: state(2, monster(3, 20, 21, false)),
            after: state(2, monster(3, 0, 0, true)),
            terminal_after: CombatTerminal::Unresolved,
            previous_card_bypass: None,
        },
    ];
    let observation = build_fiend_fire_observation(
        "record",
        "root",
        &actions,
        1,
        PreviousCardBypassObservation {
            previous_action_index: Some(0),
            status: PreviousCardBypassStatus::Applied,
            terminal_after: Some(CombatTerminal::Unresolved),
            after: Some(state(2, monster(3, 4, 0, false))),
        },
        CombatTerminal::Win,
    );
    assert_eq!(
        observation.classification,
        FiendFireClassification::ConfirmedBlockConversionWindow
    );
}

#[test]
fn bronze_preparation_without_positive_target_block_stays_out_of_scope() {
    let actions = vec![
        ActionObservation {
            index: 0,
            input: ClientInput::PlayCard {
                card_index: 0,
                target: Some(2),
            },
            card: Some(card(CardId::Clothesline, 20)),
            card_type: Some(CardType::Attack),
            before: state(1, monster(3, 57, 0, false)),
            after: state(1, monster(3, 57, 0, false)),
            terminal_after: CombatTerminal::Unresolved,
            previous_card_bypass: None,
        },
        ActionObservation {
            index: 1,
            input: ClientInput::PlayCard {
                card_index: 0,
                target: Some(3),
            },
            card: Some(card(CardId::FiendFire, 21)),
            card_type: Some(CardType::Attack),
            before: state(1, monster(3, 57, 0, false)),
            after: state(1, monster(3, 1, 0, false)),
            terminal_after: CombatTerminal::Unresolved,
            previous_card_bypass: None,
        },
    ];
    let observation = build_fiend_fire_observation(
        "record",
        "root",
        &actions,
        1,
        PreviousCardBypassObservation {
            previous_action_index: Some(0),
            status: PreviousCardBypassStatus::Applied,
            terminal_after: Some(CombatTerminal::Unresolved),
            after: Some(state(1, monster(3, 9, 0, false))),
        },
        CombatTerminal::Win,
    );
    assert_eq!(
        observation.classification,
        FiendFireClassification::NoPositiveBlockBeforePreviousAttack
    );
}

#[test]
fn typed_batch_query_matches_observed_preparation_and_exact_bypass() {
    let previous = ActionObservation {
        index: 0,
        input: ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        card: Some(card(CardId::SwordBoomerang, 10)),
        card_type: Some(CardType::Attack),
        before: state(2, monster(3, 20, 27, false)),
        after: state(2, monster(3, 20, 21, false)),
        terminal_after: CombatTerminal::Unresolved,
        previous_card_bypass: Some(PreviousCardBypassObservation {
            previous_action_index: None,
            status: PreviousCardBypassStatus::NoPreviousCardBoundary,
            terminal_after: None,
            after: None,
        }),
    };
    let current = ActionObservation {
        index: 1,
        input: ClientInput::PlayCard {
            card_index: 0,
            target: Some(3),
        },
        card: Some(card(CardId::FiendFire, 11)),
        card_type: Some(CardType::Attack),
        before: state(2, monster(3, 20, 21, false)),
        after: state(2, monster(3, 0, 0, true)),
        terminal_after: CombatTerminal::Win,
        previous_card_bypass: Some(PreviousCardBypassObservation {
            previous_action_index: Some(0),
            status: PreviousCardBypassStatus::Applied,
            terminal_after: Some(CombatTerminal::Unresolved),
            after: Some(state(2, monster(3, 4, 0, false))),
        }),
    };
    let record = EvidenceRecord {
        schema_name: "CombatEvidenceReplayV3".to_string(),
        schema_version: 3,
        record_id: "record".to_string(),
        root_exact_state_hash: "root".to_string(),
        case_identity: None,
        action_sequence_blake2b_512: "actions".to_string(),
        provenance: BTreeSet::new(),
        source_paths: BTreeSet::new(),
        case_path: None,
        action_paths: Vec::new(),
        replay_exact: true,
        supplied_action_count: 2,
        consumed_action_count: 2,
        final_terminal: CombatTerminal::Win,
        final_player_hp: 80,
        actions: vec![previous, current],
        fiend_fire_observations: Vec::new(),
    };
    let batch: CombatEvidenceQueryBatch = serde_json::from_value(serde_json::json!({
        "schema_name": "CombatEvidenceQueryBatchV1",
        "schema_version": 1,
        "queries": [{
            "query_id": "block_conversion",
            "record": {"replay_exact": true, "final_terminal": "win"},
            "current": {
                "card_id": "FiendFire",
                "query_target": {"after": {"terminal_like": true}}
            },
            "previous_card_same_turn": {
                "card_type": "Attack",
                "query_target": {
                    "before": {"block": {"gt": 0}},
                    "block_delta": {"lt": 0}
                }
            },
            "bypass_previous_card": {
                "status": "applied",
                "query_target_after": {"terminal_like": false}
            },
            "max_matches": 8
        }]
    }))
    .expect("typed query batch should decode");

    let value = serde_json::to_value(execute_query_batch(&batch, &[record]).unwrap()).unwrap();
    assert_eq!(value["results"][0]["matched_action_count"], 1);
    assert_eq!(value["results"][0]["independent_root_count"], 1);
    assert_eq!(value["results"][0]["matches"][0]["action_index"], 1);
}

#[test]
fn trace_suffix_discovery_includes_plan_trace_names() {
    assert!(is_trace_artifact_name("combined.plan-trace.json"));
    assert!(is_trace_artifact_name("witness.trace.json"));
    assert!(!is_trace_artifact_name("summary.json"));
}

#[test]
fn tracked_slime_boss_pair_replays_through_typed_timeline() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture_root = repository.join("fixtures/oracle_witnesses");
    let candidate = PairCandidate {
        case_path: fixture_root.join("seed20260713009_a0_slime_boss.combat-case.json"),
        action_paths: vec![
            fixture_root.join("seed20260713009_a0_slime_boss.local-turn-graph.actions.json")
        ],
        provenance: BTreeSet::from(["test_fixture".to_string()]),
        source_paths: BTreeSet::new(),
        expectations: super::ReplayExpectations::default(),
    };

    let record = replay_pair(&candidate, 250).expect("tracked exact witness should replay");

    assert_eq!(record.final_terminal, CombatTerminal::Win);
    assert_eq!(record.consumed_action_count, record.supplied_action_count);
    assert!(!record.actions.is_empty());
}
