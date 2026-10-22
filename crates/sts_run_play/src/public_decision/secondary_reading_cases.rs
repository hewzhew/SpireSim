//! Constructed first-shop card comparison; not a resumed natural shop.
use super::*;
use sts_oracle_learning::agent::task::TaskDecisionSiteV1;
use sts_oracle_learning::ai::planner_core::*;
use sts_oracle_learning::content::cards::CardId;

fn export() -> Value {
    let offers = [
        (CardId::Anger, 26),
        (CardId::BodySlam, 55),
        (CardId::Shockwave, 83),
        (CardId::Flex, 58),
        (CardId::Rupture, 74),
        (CardId::FlashOfSteel, 107),
        (CardId::SecretTechnique, 194),
    ];
    let TaskPublicObservationV1::Strategic { mut observation } =
        super::tests::fixture_decision().observation
    else {
        unreachable!()
    };
    observation.run.gold = 209;
    observation.decision_site = PlannerDecisionSite::Shop;
    observation.context = PlannerDecisionContext::Shop {
        purge_cost: 75,
        purge_available: false,
        inventory: PlannerShopInventoryV1 {
            cards: offers
                .iter()
                .enumerate()
                .map(|(slot, &(card, price))| PlannerShopCardOfferV1 {
                    slot,
                    card,
                    upgrades: 0,
                    price,
                    affordable: true,
                    purchase_status: PlannerShopPurchaseStatusV1::Purchasable,
                })
                .collect(),
            relics: vec![],
            potions: vec![],
            purge: None,
        },
    };
    let candidates = offers
        .into_iter()
        .enumerate()
        .map(
            |(shop_slot, (card, price))| TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::BuyCard {
                    shop_slot,
                    card,
                    upgrades: 0,
                    price,
                },
            },
        )
        .chain(std::iter::once(TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::LeaveShop,
        }))
        .collect();
    let decision = TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::Shop,
        TaskPublicObservationV1::Strategic { observation },
        candidates,
    )
    .unwrap();
    public_decision_export_v3(
        &decision,
        PublicDecisionSourceV1::ConstructedContractCase {
            case_id: "ordered-and-secondary-shop-card-readings".into(),
        },
        sts_presentation::Locale::En,
    )
    .unwrap()
}

#[test]
fn secondary_reading_fixture_matches_typed_producer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-shop-card-secondary.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let current = export();
    for field in [
        "source",
        "decision",
        "card_mechanics",
        "potion_mechanics",
        "relic_mechanics",
        "strategic_reading",
    ] {
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
