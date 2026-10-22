//! Constructed bindings for the candidate set exposed by a natural reading
//! trial. This fixture is not that session, a restored head or an optimal pick.
use super::*;
use sts_oracle_learning::agent::task::TaskDecisionSiteV1;
use sts_oracle_learning::content::cards::CardId;

fn export() -> Value {
    let decision = TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::CardReward,
        super::tests::fixture_decision().observation,
        [CardId::Combust, CardId::Warcry, CardId::DarkEmbrace]
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
            .chain(std::iter::once(TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::SkipCardReward {
                    reward_item_index: 0,
                },
            }))
            .collect(),
    )
    .unwrap();
    public_decision_export_v3(
        &decision,
        PublicDecisionSourceV1::ConstructedContractCase {
            case_id: "delayed-effects-reward-reading-contract".into(),
        },
        sts_presentation::Locale::En,
    )
    .unwrap()
}

#[test]
fn follow_up_reading_fixture_matches_typed_producer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-card-reward-follow-up.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let current = export();
    for field in ["source", "decision", "card_mechanics", "strategic_reading"] {
        if field == "strategic_reading" {
            super::tests::assert_reading_surface_eq(
                &fixture["payload"][field],
                &current["payload"][field],
            );
        } else {
            assert_eq!(
                fixture["payload"][field], current["payload"][field],
                "{field}"
            );
        }
    }
    assert_eq!(
        fixture["payload_sha256"],
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&fixture["payload"]).unwrap())
        )
    );
}
