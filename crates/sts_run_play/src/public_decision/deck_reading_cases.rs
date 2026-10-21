//! Constructed campfire with persistent instance changes; no live run source.
use super::*;
use sts_oracle_learning::agent::task::{TaskDecisionSiteV1, TaskStrategicCardV1};
use sts_oracle_learning::ai::planner_core::{PlannerDecisionContext, PlannerDecisionSite};
use sts_oracle_learning::content::cards::{can_upgrade_card_once_for, CardId};

fn export() -> Value {
    let TaskPublicObservationV1::Strategic { mut observation } =
        super::tests::fixture_decision().observation
    else {
        unreachable!()
    };
    observation.decision_site = PlannerDecisionSite::Campfire;
    observation.context = PlannerDecisionContext::Campfire;
    observation.cards = [
        (CardId::Corruption, 0, 0, None, None, -3),
        (CardId::Corruption, 0, 0, None, None, 0),
        (CardId::Strike, 0, 0, Some(20), None, 0),
        (CardId::Defend, 0, 0, None, Some(17), 0),
        (CardId::SearingBlow, 5, 0, None, None, 0),
        (CardId::RitualDagger, 0, 42, None, None, 0),
        (CardId::GeneticAlgorithm, 0, 31, None, None, 0),
        (CardId::SecretTechnique, 0, 0, None, None, 0),
        (CardId::Whirlwind, 0, 0, None, None, 0),
        (CardId::Pain, 0, 0, None, None, 0),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(
            ordinal,
            (card, upgrades, misc_value, base_damage_override, base_block_override, cost_modifier),
        )| TaskStrategicCardV1 {
            ordinal,
            card,
            upgrades,
            misc_value,
            base_damage_override,
            base_block_override,
            cost_modifier,
            bottled_by: None,
        },
    )
    .collect();
    let candidates = observation
        .cards
        .iter()
        .filter(|card| can_upgrade_card_once_for(card.card, card.upgrades))
        .map(|card| TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Smith {
                card_ordinal: card.ordinal,
                card: card.card,
                upgrades: card.upgrades,
            },
        })
        .chain(std::iter::once(TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Rest,
        }))
        .collect();
    let decision = TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::Campfire,
        TaskPublicObservationV1::Strategic { observation },
        candidates,
    )
    .unwrap();
    public_decision_export_v3(
        &decision,
        PublicDecisionSourceV1::ConstructedContractCase {
            case_id: "campfire-persistent-card-instances".into(),
        },
    )
    .unwrap()
}

#[test]
fn owned_card_fixture_retains_instances_and_matches_current_projection() {
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/expert/tests/fixtures/public-owned-card-instances.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let current = export();
    let mut expected_payload = fixture["payload"].clone();
    let mut current_payload = current["payload"].clone();
    expected_payload.as_object_mut().unwrap().remove("producer");
    current_payload.as_object_mut().unwrap().remove("producer");
    assert_eq!(expected_payload, current_payload);
    assert_eq!(
        fixture["payload_sha256"],
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&fixture["payload"]).unwrap())
        )
    );
    let rows = &current["payload"]["deck_mechanics"]["cards"];
    assert_eq!(rows[0]["facts"]["current"]["persistent_cost"], 0);
    assert_eq!(rows[0]["facts"]["next_upgrade"]["persistent_cost"], 2);
    assert_eq!(rows[1]["facts"]["current"]["persistent_cost"], 3);
    assert_ne!(rows[0]["card_sha256"], rows[1]["card_sha256"]);
    assert_eq!(rows[2]["facts"]["next_upgrade"]["persistent_damage"], 23);
    assert_eq!(rows[3]["facts"]["next_upgrade"]["persistent_block"], 20);
}
