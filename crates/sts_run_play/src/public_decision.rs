//! Detached public reading artifact. No private support, checkpoint, or execution authority.

mod deck_mechanics;
#[cfg(test)]
mod deck_reading_cases;
#[cfg(test)]
mod energy_reading_cases;
#[cfg(test)]
mod follow_up_reading_cases;
#[cfg(test)]
mod map_reading_cases;
mod potion_mechanics;
#[cfg(test)]
mod potion_reading_cases;
mod relic_mechanics;
#[cfg(test)]
mod relic_reading_cases;
#[cfg(test)]
mod secondary_reading_cases;
mod strategic_reading;
#[cfg(test)]
mod strategic_reading_cases;
#[cfg(test)]
mod upgrade_reading_cases;

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1, TaskPublicDecisionV1, TaskPublicObservationV1, TaskStrategicActionV1,
};
use sts_oracle_learning::ai::card_semantics_v1::card_acquisition_reading_v1;
use sts_oracle_learning::state::rewards::RewardCard;
use sts_presentation::Locale;

use crate::model::{RunPlayEvidenceIdentityV1, RunPlayHeadV1};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PublicDecisionSourceV1 {
    RunPlayScratchHead {
        session_id: String,
        head_revision: u64,
        archive_node_id: String,
    },
    RunPlayHead {
        session_id: String,
        head_revision: u64,
        archive_node_id: String,
        evidence_identity: RunPlayEvidenceIdentityV1,
    },
    ConstructedContractCase {
        case_id: String,
    },
}

fn public_decision_export_v3(
    decision: &TaskPublicDecisionV1,
    source: PublicDecisionSourceV1,
    locale: Locale,
) -> Result<Value, String> {
    decision.validate()?;
    if !matches!(
        decision.observation,
        TaskPublicObservationV1::Strategic { .. }
    ) {
        return Err("public reading export currently supports strategic decisions only".into());
    }
    let card_mechanics: Vec<_> = decision
        .candidates
        .iter()
        .zip(&decision.information.candidate_surface.ordered_candidate_ids)
        .filter_map(|(candidate, candidate_id)| match candidate {
            TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::TakeCard { card, upgrades, .. }
                    | TaskStrategicActionV1::BuyCard { card, upgrades, .. },
            } => Some(json!({"candidate_id": candidate_id,
                    "facts": card_acquisition_reading_v1(&RewardCard::new(*card, *upgrades))})),
            _ => None,
        })
        .collect();
    let payload = json!({
        "source": source,
        "producer": {"component": "sts_run_play", "version": env!("CARGO_PKG_VERSION"),
            "evidence_identity": RunPlayEvidenceIdentityV1::current_v1(), "text_locale": locale},
        "decision": decision,
        "strategic_reading": strategic_reading::project(decision, locale)?,
        "potion_mechanics": potion_mechanics::project(decision),
        "relic_mechanics": relic_mechanics::project(decision),
        "deck_mechanics": deck_mechanics::project(decision)?,
        "card_mechanics": {
            "schema": "sts-card-acquisition-reading-v1",
            "public_snapshot_id": decision.information.snapshot_id,
            "basis": "simulator_definition_and_reviewed_effects",
            "coverage": "partial",
            "candidates": card_mechanics,
        },
    });
    // Value maps have sorted keys; the cross-language contract uses UTF-8 JSON,
    // compact separators, integer scalars and no ASCII escaping. Integrity is
    // not authenticity and does not assert that a detached head is still live.
    let bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    Ok(json!({
        "schema": "sts-public-decision-export-v3",
        "payload": payload,
        "payload_sha256": format!("{:x}", Sha256::digest(&bytes)),
    }))
}

pub(crate) fn export_head_public_decision_v1(
    head: &RunPlayHeadV1,
    output: &Path,
    locale: Locale,
) -> Result<(), String> {
    let source = if head.scratch {
        // This snapshot belongs to disposable play, not a retained trajectory.
        PublicDecisionSourceV1::RunPlayScratchHead {
            session_id: head.session_id.clone(),
            head_revision: head.head_revision,
            archive_node_id: head.position.archive_node_id().to_owned(),
        }
    } else {
        let identity = head
            .evidence_identity
            .as_ref()
            .ok_or("public export lacks source identity")?;
        RunPlayEvidenceIdentityV1::require_current_v1(Some(identity))?;
        PublicDecisionSourceV1::RunPlayHead {
            session_id: head.session_id.clone(),
            head_revision: head.head_revision,
            archive_node_id: head.position.archive_node_id().to_owned(),
            evidence_identity: identity.clone(),
        }
    };
    let decision = head
        .current_decision
        .as_ref()
        .ok_or("head has no public decision")?
        .public_decision_v1()?;
    let document = public_decision_export_v3(&decision, source, locale)?;
    let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
    crate::storage::write_new_synced(output, &bytes, "public decision reading export")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::agent::task::{
        TaskDecisionSiteV1, TaskPublicCandidateV1, TaskStrategicActionV1, TaskStrategicCardV1,
        TaskStrategicObservationV1, TaskStrategicPotionSlotV1, TaskStrategicPotionV1,
    };
    use sts_oracle_learning::ai::planner_core::*;
    use sts_oracle_learning::content::{cards::CardId, potions::PotionId, relics::RelicId};

    #[test]
    fn disposable_new_run_exports_a_snapshot_without_trajectory_identity() {
        let directory = crate::decision_tests::TestDirectory::new();
        let paths = crate::storage::RunPlayPathsV1::new(directory.0.join("fresh"));
        let workspace = crate::driver::create_scratch_natural_session_v1(
            &paths,
            Default::default(),
            sts_oracle_learning::agent::task::TaskBudgetV1 {
                max_world_transitions: 64,
                max_reloads: 0,
            },
            Default::default(),
        )
        .unwrap();
        assert!(workspace.head.scratch);
        assert!(workspace.head.evidence_identity.is_none());
        for locale in [Locale::En, Locale::ZhCn] {
            let output = directory.0.join(format!("{locale}.json"));
            export_head_public_decision_v1(&workspace.head, &output, locale).unwrap();
            let document: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
            let source = &document["payload"]["source"];
            assert_eq!(source["kind"], "run_play_scratch_head");
            assert!(source.get("evidence_identity").is_none());
            assert_eq!(source["head_revision"], workspace.head.head_revision);
            assert_eq!(paths.load().unwrap().head, workspace.head);
        }
    }

    /// Stored prose is evidence of its original renderer, not a wording contract.
    /// Check all typed facts exactly and keep the full display/identity join.
    pub(super) fn assert_reading_surface_eq(recorded: &Value, current: &Value) {
        fn surface(value: &Value) -> Value {
            let mut value = value.clone();
            let object = value.as_object_mut().expect("reading surface object");
            assert_eq!(object.len(), 7);
            for key in ["context_summary", "context_detail"] {
                assert!(object.remove(key).unwrap().is_string(), "{key}");
            }
            for row in object["candidates"].as_array_mut().unwrap() {
                let row = row.as_object_mut().unwrap();
                assert_eq!(row.len(), 4);
                for key in ["brief", "detail"] {
                    assert!(
                        !row.remove(key).unwrap().as_str().unwrap().trim().is_empty(),
                        "{key}"
                    );
                }
            }
            value
        }
        assert_eq!(surface(recorded), surface(current));
    }

    pub(super) fn assert_reading_payload_eq(recorded: &Value, current: &Value) {
        assert_reading_surface_eq(
            &recorded["strategic_reading"],
            &current["strategic_reading"],
        );
        // Some older exports predate the optional typed mechanics blocks.
        // Every block actually present in the capture must remain identical.
        for (key, value) in recorded.as_object().unwrap() {
            if key != "producer" && key != "strategic_reading" {
                assert_eq!(value, &current[key], "{key}");
            }
        }
    }

    #[test]
    fn localized_exports_keep_all_frozen_decision_facts_and_bindings() {
        fn check(value: &Value, count: &mut usize) {
            match value {
                Value::Object(object)
                    if object.get("schema").and_then(Value::as_str)
                        == Some("sts-public-decision-export-v3") =>
                {
                    let old = &value["payload"];
                    assert_eq!(
                        value["payload_sha256"],
                        format!("{:x}", Sha256::digest(serde_json::to_vec(old).unwrap()))
                    );
                    let decision = serde_json::from_value(old["decision"].clone()).unwrap();
                    let source: PublicDecisionSourceV1 =
                        serde_json::from_value(old["source"].clone()).unwrap();
                    let mut rendered = Vec::new();
                    for locale in [Locale::En, Locale::ZhCn] {
                        let current =
                            public_decision_export_v3(&decision, source.clone(), locale).unwrap();
                        assert_eq!(
                            current["payload"]["producer"]["text_locale"],
                            serde_json::to_value(locale).unwrap()
                        );
                        assert_reading_payload_eq(old, &current["payload"]);
                        rendered.push(current["payload"]["strategic_reading"].clone());
                    }
                    assert_ne!(rendered[0], rendered[1]);
                    *count += 1;
                }
                Value::Object(object) => {
                    for value in object.values() {
                        check(value, count);
                    }
                }
                Value::Array(array) => {
                    for value in array {
                        check(value, count);
                    }
                }
                _ => {}
            }
        }
        let directory =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/expert/tests/fixtures");
        let mut count = 0;
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
                check(&value, &mut count);
            }
        }
        assert!(
            count >= 30,
            "expected broad frozen decision coverage, found {count}"
        );
    }

    pub(super) fn fixture_decision() -> TaskPublicDecisionV1 {
        let card = |ordinal, upgrades| TaskStrategicCardV1 {
            ordinal,
            card: CardId::Strike,
            upgrades,
            misc_value: 0,
            base_damage_override: None,
            base_block_override: None,
            cost_modifier: 0,
            bottled_by: None,
        };
        TaskPublicDecisionV1::new(
            TaskDecisionSiteV1::CardReward,
            TaskPublicObservationV1::Strategic {
                observation: TaskStrategicObservationV1 {
                    mechanics: PlannerMechanicsManifest {
                        mechanics_id: "constructed-reading-contract".into(),
                        mechanics_version: 1,
                    },
                    run_goal: PlannerRunGoal::HeartVictory,
                    decision_site: PlannerDecisionSite::CardReward,
                    run: PlannerRunScalars {
                        player_class: PlannerPlayerClass::Ironclad,
                        ascension_level: 20,
                        act: 1,
                        floor: 7,
                        current_hp: 22,
                        max_hp: 80,
                        gold: 131,
                        keys: [false; 3],
                        potion_capacity: 2,
                        pending_act_entry: None,
                    },
                    cards: vec![card(0, 0), card(1, 1)],
                    relics: vec![
                        PlannerRelicObservation {
                            relic: RelicId::BurningBlood,
                            counter: -1,
                            used_up: false,
                            amount: 1,
                        },
                        PlannerRelicObservation {
                            relic: RelicId::SingingBowl,
                            counter: -1,
                            used_up: false,
                            amount: 1,
                        },
                    ],
                    potions: vec![
                        TaskStrategicPotionSlotV1 {
                            slot: 0,
                            potion: Some(TaskStrategicPotionV1 {
                                potion: PotionId::FirePotion,
                                can_use: false,
                                can_discard: true,
                                requires_target: true,
                            }),
                        },
                        TaskStrategicPotionSlotV1 {
                            slot: 1,
                            potion: None,
                        },
                    ],
                    public_map: PlannerPublicMap {
                        current_x: 1,
                        current_y: 5,
                        boss: None,
                        nodes: vec![
                            PlannerPublicMapNode {
                                x: 1,
                                y: 5,
                                room: PlannerMapRoom::Monster,
                                has_emerald_key: false,
                                edges: vec![
                                    PlannerPublicMapEdge {
                                        destination_x: 0,
                                        destination_y: 6,
                                    },
                                    PlannerPublicMapEdge {
                                        destination_x: 2,
                                        destination_y: 6,
                                    },
                                ],
                            },
                            PlannerPublicMapNode {
                                x: 0,
                                y: 6,
                                room: PlannerMapRoom::Elite,
                                has_emerald_key: true,
                                edges: vec![],
                            },
                            PlannerPublicMapNode {
                                x: 2,
                                y: 6,
                                room: PlannerMapRoom::QuestionMark,
                                has_emerald_key: false,
                                edges: vec![],
                            },
                        ],
                    },
                    context: PlannerDecisionContext::CardReward {
                        reward_item_index: Some(0),
                        pending_rewards: vec![],
                    },
                    public_history: PlannerPublicHistory {
                        shop_purge_count: 0,
                        recent_combat_hp_loss: None,
                        recent_combat_turns: None,
                        recent_combat_potions_used: None,
                    },
                },
            },
            vec![
                TaskStrategicActionV1::TakeCard {
                    reward_item_index: Some(0),
                    option_index: 0,
                    card: CardId::DarkShackles,
                    upgrades: 0,
                },
                TaskStrategicActionV1::TakeCard {
                    reward_item_index: Some(0),
                    option_index: 1,
                    card: CardId::DarkShackles,
                    upgrades: 1,
                },
                TaskStrategicActionV1::SkipCardReward {
                    reward_item_index: 0,
                },
                TaskStrategicActionV1::SingingBowl {
                    reward_item_index: Some(0),
                },
                TaskStrategicActionV1::DiscardRunPotion {
                    slot: 0,
                    potion: PotionId::FirePotion,
                },
                TaskStrategicActionV1::TakeCard {
                    reward_item_index: Some(0),
                    option_index: 2,
                    card: CardId::DeepBreath,
                    upgrades: 0,
                },
                TaskStrategicActionV1::TakeCard {
                    reward_item_index: Some(0),
                    option_index: 3,
                    card: CardId::EscapePlan,
                    upgrades: 0,
                },
            ]
            .into_iter()
            .map(|action| TaskPublicCandidateV1::Strategic { action })
            .collect(),
        )
        .unwrap()
    }

    fn fixture_export() -> Value {
        public_decision_export_v3(
            &fixture_decision(),
            PublicDecisionSourceV1::ConstructedContractCase {
                case_id: "card-reward-reading-中文".into(),
            },
            sts_presentation::Locale::En,
        )
        .unwrap()
    }

    fn damage_fixture_export() -> Value {
        let decision = TaskPublicDecisionV1::new(
            TaskDecisionSiteV1::CardReward,
            fixture_decision().observation,
            [
                CardId::Wish,
                CardId::Pummel,
                CardId::Ragnarok,
                CardId::DaggerSpray,
                CardId::FiendFire,
                CardId::Whirlwind,
                CardId::BodySlam,
                CardId::Bane,
                CardId::PerfectedStrike,
                CardId::Blizzard,
                CardId::SignatureMove,
            ]
            .into_iter()
            .enumerate()
            .map(|(option_index, card)| TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::TakeCard {
                    reward_item_index: Some(0),
                    option_index,
                    card,
                    upgrades: 0,
                },
            })
            .collect(),
        )
        .unwrap();
        public_decision_export_v3(
            &decision,
            PublicDecisionSourceV1::ConstructedContractCase {
                case_id: "damage-delivery-reading-contract".into(),
            },
            sts_presentation::Locale::En,
        )
        .unwrap()
    }

    fn strength_fixture_export() -> Value {
        let decision = TaskPublicDecisionV1::new(
            TaskDecisionSiteV1::CardReward,
            fixture_decision().observation,
            [
                CardId::ShrugItOff,
                CardId::SpotWeakness,
                CardId::Disarm,
                CardId::Inflame,
                CardId::Flex,
                CardId::DemonForm,
            ]
            .into_iter()
            .enumerate()
            .map(|(option_index, card)| TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::TakeCard {
                    reward_item_index: Some(0),
                    option_index,
                    card,
                    upgrades: 0,
                },
            })
            .collect(),
        )
        .unwrap();
        public_decision_export_v3(
            &decision,
            PublicDecisionSourceV1::ConstructedContractCase {
                case_id: "strength-delivery-reading-contract".into(),
            },
            sts_presentation::Locale::En,
        )
        .unwrap()
    }

    #[test]
    fn strength_reading_fixture_matches_typed_producer() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-strength.json"
        ))
        .unwrap();
        let current = strength_fixture_export();
        for field in ["source", "decision", "card_mechanics"] {
            assert_eq!(
                fixture["payload"][field], current["payload"][field],
                "{field}"
            );
        }
        assert_eq!(
            fixture["payload_sha256"],
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&fixture["payload"]).unwrap())
            )
        );
    }

    #[test]
    fn export_rejects_changed_payload_with_old_public_identity() {
        let mut decision = fixture_decision();
        decision.candidates.swap(0, 1);
        assert!(public_decision_export_v3(
            &decision,
            PublicDecisionSourceV1::ConstructedContractCase {
                case_id: "invalid".into()
            },
            sts_presentation::Locale::En
        )
        .is_err());
    }

    #[test]
    fn mechanics_are_bound_to_each_take_card_candidate_in_the_public_snapshot() {
        let decision = fixture_decision();
        let export = fixture_export();
        let mechanics = &export["payload"]["card_mechanics"];
        assert_eq!(mechanics["coverage"], "partial");
        assert_eq!(
            mechanics["public_snapshot_id"],
            decision.information.snapshot_id
        );
        let rows = mechanics["candidates"].as_array().unwrap();
        let ids = &decision.information.candidate_surface.ordered_candidate_ids;
        assert_eq!(rows.len(), 4);
        for (row, ordinal) in rows.iter().zip([0, 1, 5, 6]) {
            assert_eq!(row["candidate_id"], ids[ordinal]);
            let TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::TakeCard { card, upgrades, .. },
            } = decision.candidates[ordinal]
            else {
                panic!("take-card fixture")
            };
            assert_eq!(
                row["facts"]["current"]["card"],
                serde_json::to_value(card).unwrap()
            );
            assert_eq!(row["facts"]["current"]["upgrades"], upgrades);
        }
    }

    #[test]
    fn checked_in_reading_fixture_matches_typed_producer() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward.json"
        ))
        .unwrap();
        // The producer version is pinned at capture, not relabeled every time
        // an unrelated simulator semantics version changes.
        let current = fixture_export();
        assert_eq!(fixture["schema"], "sts-public-decision-export-v2");
        assert_eq!(current["schema"], "sts-public-decision-export-v3");
        assert_eq!(
            fixture["payload"]["decision"],
            current["payload"]["decision"]
        );
        assert_eq!(fixture["payload"]["source"], current["payload"]["source"]);
        assert_eq!(
            fixture["payload"]["card_mechanics"],
            current["payload"]["card_mechanics"]
        );
        let bytes = serde_json::to_vec(&fixture["payload"]).unwrap();
        assert_eq!(
            fixture["payload_sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
    }

    #[test]
    fn damage_reading_fixture_matches_typed_producer() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-card-reward-damage.json"
        ))
        .unwrap();
        let current = damage_fixture_export();
        for field in ["source", "decision", "card_mechanics"] {
            assert_eq!(
                fixture["payload"][field], current["payload"][field],
                "{field}"
            );
        }
        assert_eq!(
            fixture["payload_sha256"],
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&fixture["payload"]).unwrap())
            )
        );
    }
}
