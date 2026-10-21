use super::*;
use sts_oracle_learning::content::cards::CardId;

fn export() -> Value {
    use sts_oracle_learning::agent::task::{TaskDecisionSiteV1, TaskStrategicCardV1};
    use sts_oracle_learning::ai::planner_core::{PlannerDecisionContext, PlannerDecisionSite};
    let TaskPublicObservationV1::Strategic { mut observation } =
        super::tests::fixture_decision().observation
    else {
        unreachable!()
    };
    observation.decision_site = PlannerDecisionSite::Campfire;
    observation.context = PlannerDecisionContext::Campfire;
    observation.cards = [
        (CardId::Armaments, 0),
        (CardId::Armaments, 1),
        (CardId::Apotheosis, 0),
    ]
    .into_iter()
    .enumerate()
    .map(|(ordinal, (card, upgrades))| TaskStrategicCardV1 {
        ordinal,
        card,
        upgrades,
        misc_value: 0,
        base_damage_override: None,
        base_block_override: None,
        cost_modifier: 0,
        bottled_by: None,
    })
    .collect();
    let candidates = observation
        .cards
        .iter()
        .filter(|card| card.upgrades == 0)
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
            case_id: "campfire-combat-upgrade-scope".into(),
        },
    )
    .unwrap()
}

#[test]
fn upgrade_reading_fixture_matches_typed_producer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-combat-upgrades.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
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
}

#[test]
fn combat_upgrade_reading_exposes_scope_change_even_when_block_is_unchanged() {
    let armaments = serde_json::to_value(card_acquisition_reading_v1(&RewardCard::new(
        CardId::Armaments,
        0,
    )))
    .unwrap();
    assert_eq!(armaments["current"]["definition_numbers"]["block"], 5);
    assert_eq!(armaments["next_upgrade"]["definition_numbers"]["block"], 5);
    assert_eq!(
        armaments["current"]["reviewed_effects"]["combat_upgrade_scope"],
        "selected_card_in_hand"
    );
    assert_eq!(
        armaments["next_upgrade"]["reviewed_effects"]["combat_upgrade_scope"],
        "whole_hand"
    );
    for definition in ["current", "next_upgrade"] {
        assert_eq!(
            armaments[definition]["reviewed_effects"]["combat_upgrade_persistence"],
            "combat_only"
        );
    }
    let apotheosis = serde_json::to_value(card_acquisition_reading_v1(&RewardCard::new(
        CardId::Apotheosis,
        0,
    )))
    .unwrap();
    for definition in ["current", "next_upgrade"] {
        assert_eq!(apotheosis[definition]["play_target"], "no_explicit_target");
        assert_eq!(
            apotheosis[definition]["reviewed_effects"]["combat_upgrade_scope"],
            "all_combat_zones"
        );
        assert_eq!(
            apotheosis[definition]["reviewed_effects"]["combat_upgrade_persistence"],
            "combat_only"
        );
    }
    assert_eq!(apotheosis["current"]["base_cost"], 2);
    assert_eq!(apotheosis["next_upgrade"]["base_cost"], 1);
}
