//! Constructed reading contracts motivated by a natural shop's public items.
//! The observations are test inputs, never restored sessions or optimal labels.
use super::*;
use sts_oracle_learning::agent::task::{TaskDecisionSiteV1, TaskStrategicPotionV1};
use sts_oracle_learning::ai::planner_core::*;
use sts_oracle_learning::content::{cards::CardId, potions::PotionId, relics::RelicId};

fn documents() -> Vec<Value> {
    use TaskStrategicActionV1 as A;
    let inventory = PlannerShopInventoryV1 {
        cards: vec![PlannerShopCardOfferV1 {
            slot: 0,
            card: CardId::Anger,
            upgrades: 0,
            price: 26,
            affordable: true,
            purchase_status: PlannerShopPurchaseStatusV1::Purchasable,
        }],
        relics: vec![PlannerShopRelicOfferV1 {
            slot: 0,
            relic: RelicId::BronzeScales,
            price: 165,
            affordable: true,
            purchase_status: PlannerShopPurchaseStatusV1::Purchasable,
        }],
        potions: [
            PotionId::BloodPotion,
            PotionId::Elixir,
            PotionId::WeakenPotion,
        ]
        .into_iter()
        .enumerate()
        .map(|(slot, potion)| PlannerShopPotionOfferV1 {
            slot,
            potion,
            price: if slot == 1 { 83 } else { 55 },
            affordable: true,
            purchase_status: PlannerShopPurchaseStatusV1::Purchasable,
        })
        .collect(),
        purge: None,
    };
    let mut shop_actions = vec![A::DiscardRunPotion {
        slot: 0,
        potion: PotionId::LiquidBronze,
    }];
    shop_actions.extend(inventory.potions.iter().map(|offer| A::BuyPotion {
        shop_slot: offer.slot,
        potion: offer.potion,
        price: offer.price,
    }));
    shop_actions.extend([
        A::BuyCard {
            shop_slot: 0,
            card: CardId::Anger,
            upgrades: 0,
            price: 26,
        },
        A::BuyRelic {
            shop_slot: 0,
            relic: RelicId::BronzeScales,
            price: 165,
        },
        A::LeaveShop,
    ]);
    let rewards: Vec<_> = [
        PotionId::PowerPotion,
        PotionId::ColorlessPotion,
        PotionId::FairyPotion,
    ]
    .into_iter()
    .map(|potion| PlannerRewardDescriptor::Potion { potion })
    .collect();
    let reward_actions = rewards
        .iter()
        .enumerate()
        .map(|(reward_item_index, reward)| A::ClaimReward {
            reward_item_index,
            reward: reward.clone(),
        })
        .chain(std::iter::once(A::Proceed {
            site: PlannerDecisionSite::Reward,
        }))
        .collect();
    [
        ("shop", PlannerDecisionSite::Shop, PlannerDecisionContext::Shop {
            purge_cost: 75, purge_available: false, inventory,
        }, shop_actions, PotionId::LiquidBronze),
        ("reward", PlannerDecisionSite::Reward, PlannerDecisionContext::Reward {
            pending_rewards: rewards,
        }, reward_actions, PotionId::LiquidBronze),
        ("owned", PlannerDecisionSite::Shop, PlannerDecisionContext::Shop {
            purge_cost: 75, purge_available: false,
            inventory: PlannerShopInventoryV1 { cards: vec![], relics: vec![], potions: vec![], purge: None },
        }, vec![A::UseRunPotion { slot: 0, potion: PotionId::BloodPotion },
                 A::DiscardRunPotion { slot: 0, potion: PotionId::BloodPotion }, A::LeaveShop], PotionId::BloodPotion),
    ].into_iter().map(|(case, site, context, actions, held)| {
        let TaskPublicObservationV1::Strategic { mut observation } = super::tests::fixture_decision().observation else { unreachable!() };
        observation.run.current_hp = 65;
        observation.run.max_hp = 75;
        observation.run.gold = 209;
        observation.decision_site = site;
        observation.context = context;
        observation.relics.retain(|relic| relic.relic != RelicId::SingingBowl);
        observation.potions[0].potion = Some(TaskStrategicPotionV1 {
            potion: held, can_use: held == PotionId::BloodPotion,
            can_discard: true, requires_target: false,
        });
        let decision = TaskPublicDecisionV1::new(
            TaskDecisionSiteV1::from(site), TaskPublicObservationV1::Strategic { observation },
            actions.into_iter().map(|action| TaskPublicCandidateV1::Strategic { action }).collect(),
        ).unwrap();
        json!({"case": case, "document": public_decision_export_v3(&decision,
            PublicDecisionSourceV1::ConstructedContractCase { case_id: format!("potion-reading-{case}") }, sts_presentation::Locale::En).unwrap()})
    }).collect()
}

#[test]
fn potion_reading_fixture_matches_typed_producer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-potion-readings.json");
    let recorded: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let current = documents();
    assert_eq!(recorded.len(), current.len());
    for (recorded, current) in recorded.iter().zip(current.iter()) {
        assert_eq!(recorded["case"], current["case"]);
        for field in [
            "source",
            "decision",
            "card_mechanics",
            "potion_mechanics",
            "strategic_reading",
        ] {
            if field == "strategic_reading" {
                super::tests::assert_reading_surface_eq(
                    &recorded["document"]["payload"][field],
                    &current["document"]["payload"][field],
                );
            } else {
                assert_eq!(
                    recorded["document"]["payload"][field], current["document"]["payload"][field],
                    "{field}"
                );
            }
        }
        assert_eq!(
            recorded["document"]["payload_sha256"],
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&recorded["document"]["payload"]).unwrap())
            )
        );
    }
}
