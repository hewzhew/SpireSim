//! Constructed visible-relic contracts; not a captured or resumed natural run.
use super::*;
use sts_oracle_learning::agent::task::TaskDecisionSiteV1;
use sts_oracle_learning::ai::planner_core::*;
use sts_oracle_learning::content::relics::RelicId;

fn documents() -> Vec<Value> {
    use TaskStrategicActionV1 as A;
    let shop = PlannerShopInventoryV1 {
        cards: vec![],
        potions: vec![],
        purge: None,
        relics: [
            (RelicId::Sundial, 271),
            (RelicId::BronzeScales, 165),
            (RelicId::Sling, 161),
        ]
        .into_iter()
        .enumerate()
        .map(|(slot, (relic, price))| PlannerShopRelicOfferV1 {
            slot,
            relic,
            price,
            affordable: price <= 209,
            purchase_status: if price <= 209 {
                PlannerShopPurchaseStatusV1::Purchasable
            } else {
                PlannerShopPurchaseStatusV1::Blocked {
                    reason: PlannerShopPurchaseBlockReasonV1::NotEnoughGold { gold: 209, price },
                }
            },
        })
        .collect(),
    };
    let rewards = vec![
        PlannerRewardDescriptor::Relic {
            relic: RelicId::Sling,
        },
        PlannerRewardDescriptor::Relic {
            relic: RelicId::Anchor,
        },
    ];
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
            purge_cost: 75, purge_available: false, inventory: shop,
        }, vec![A::BuyRelic { shop_slot: 1, relic: RelicId::BronzeScales, price: 165 },
                 A::BuyRelic { shop_slot: 2, relic: RelicId::Sling, price: 161 }, A::LeaveShop]),
        ("owned", PlannerDecisionSite::Campfire, PlannerDecisionContext::Campfire, vec![A::Rest]),
        ("reward", PlannerDecisionSite::Reward, PlannerDecisionContext::Reward {
            pending_rewards: rewards,
        }, reward_actions),
        ("boss", PlannerDecisionSite::BossRelic, PlannerDecisionContext::BossRelic,
         vec![A::TakeBossRelic { option_index: 0, relic: RelicId::BlackBlood }]),
    ].into_iter().map(|(case, site, context, actions)| {
        let TaskPublicObservationV1::Strategic { mut observation } = super::tests::fixture_decision().observation else { unreachable!() };
        observation.run.gold = 209;
        observation.context = context;
        observation.decision_site = site;
        observation.relics.retain(|row| row.relic == RelicId::BurningBlood);
        if case == "owned" {
            observation.relics.push(PlannerRelicObservation {
                relic: RelicId::Sundial, counter: 2, amount: 0, used_up: false,
            });
        }
        let decision = TaskPublicDecisionV1::new(TaskDecisionSiteV1::from(site),
            TaskPublicObservationV1::Strategic { observation },
            actions.into_iter().map(|action| TaskPublicCandidateV1::Strategic { action }).collect()).unwrap();
        json!({"case": case, "document": public_decision_export_v3(&decision,
            PublicDecisionSourceV1::ConstructedContractCase { case_id: format!("visible-relic-reading-{case}") }).unwrap()})
    }).collect()
}

#[test]
fn relic_reading_fixture_matches_typed_producer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-relic-readings.json");
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
            "relic_mechanics",
            "strategic_reading",
        ] {
            assert_eq!(
                recorded["document"]["payload"][field], current["document"]["payload"][field],
                "{field}"
            );
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

#[test]
fn boss_relic_reading_keeps_energy_and_the_distinct_resource_restrictions() {
    use sts_oracle_learning::ai::card_semantics_v1::relic_acquisition_reading_v1;
    let read = |relic| serde_json::to_value(relic_acquisition_reading_v1(relic)).unwrap();
    for relic in [
        RelicId::CoffeeDripper,
        RelicId::FusionHammer,
        RelicId::Sozu,
        RelicId::VelvetChoker,
    ] {
        assert_eq!(read(relic)["energy_per_turn_delta"], 1);
    }
    assert_eq!(read(RelicId::CoffeeDripper)["campfire_restriction"], "rest");
    assert_eq!(read(RelicId::FusionHammer)["campfire_restriction"], "smith");
    let sozu = read(RelicId::Sozu);
    assert_eq!(
        sozu["potion_acquisition_rule"]["prevents_new_potions"],
        true
    );
    assert_eq!(
        sozu["potion_acquisition_rule"]["owned_potions_remain_usable"],
        true
    );
    assert_eq!(read(RelicId::VelvetChoker)["card_play_limit_per_turn"], 6);
    assert!(sozu.get("campfire_restriction").is_none());
    assert!(read(RelicId::Sundial)
        .get("energy_per_turn_delta")
        .is_none());
}

fn boss_resource_document() -> Value {
    let TaskPublicObservationV1::Strategic { mut observation } =
        super::tests::fixture_decision().observation
    else {
        unreachable!()
    };
    observation.decision_site = PlannerDecisionSite::BossRelic;
    observation.context = PlannerDecisionContext::BossRelic;
    observation
        .relics
        .retain(|row| row.relic == RelicId::BurningBlood);
    observation.relics.push(PlannerRelicObservation {
        relic: RelicId::FusionHammer,
        counter: -1,
        amount: 0,
        used_up: false,
    });
    let candidates = [RelicId::CoffeeDripper, RelicId::Sozu, RelicId::VelvetChoker]
        .into_iter()
        .enumerate()
        .map(
            |(option_index, relic)| TaskStrategicActionV1::TakeBossRelic {
                option_index,
                relic,
            },
        )
        .chain(std::iter::once(TaskStrategicActionV1::SkipBossRelic))
        .map(|action| TaskPublicCandidateV1::Strategic { action })
        .collect();
    let decision = TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::BossRelic,
        TaskPublicObservationV1::Strategic { observation },
        candidates,
    )
    .unwrap();
    public_decision_export_v3(
        &decision,
        PublicDecisionSourceV1::ConstructedContractCase {
            case_id: "boss-relic-resource-restrictions".into(),
        },
    )
    .unwrap()
}

#[test]
fn boss_resource_fixture_matches_typed_producer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-boss-relic-resources.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut recorded = fixture["payload"].clone();
    let mut current = boss_resource_document()["payload"].clone();
    recorded.as_object_mut().unwrap().remove("producer");
    current.as_object_mut().unwrap().remove("producer");
    assert_eq!(recorded, current);
    assert_eq!(
        fixture["payload_sha256"],
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&fixture["payload"]).unwrap())
        )
    );
}

fn snecko_document() -> Value {
    let TaskPublicObservationV1::Strategic { mut observation } =
        super::tests::fixture_decision().observation
    else {
        unreachable!()
    };
    observation.decision_site = PlannerDecisionSite::BossRelic;
    observation.context = PlannerDecisionContext::BossRelic;
    observation
        .relics
        .retain(|row| row.relic == RelicId::BurningBlood);
    let candidates = [RelicId::SneckoEye, RelicId::Sozu, RelicId::RunicPyramid]
        .into_iter()
        .enumerate()
        .map(
            |(option_index, relic)| TaskStrategicActionV1::TakeBossRelic {
                option_index,
                relic,
            },
        )
        .chain(std::iter::once(TaskStrategicActionV1::SkipBossRelic))
        .map(|action| TaskPublicCandidateV1::Strategic { action })
        .collect();
    let decision = TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::BossRelic,
        TaskPublicObservationV1::Strategic { observation },
        candidates,
    )
    .unwrap();
    public_decision_export_v3(
        &decision,
        PublicDecisionSourceV1::ConstructedContractCase {
            case_id: "boss-relic-snecko-draw-cost".into(),
        },
    )
    .unwrap()
}

#[test]
fn snecko_fixture_matches_typed_producer_and_scoped_confusion_rule() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-snecko-reading.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut recorded = fixture["payload"].clone();
    let mut current = snecko_document()["payload"].clone();
    recorded.as_object_mut().unwrap().remove("producer");
    current.as_object_mut().unwrap().remove("producer");
    assert_eq!(recorded, current);
    assert_eq!(
        fixture["payload_sha256"],
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&fixture["payload"]).unwrap())
        )
    );
    let definition = current["relic_mechanics"]["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["relic"] == "SneckoEye")
        .unwrap();
    assert_eq!(definition["additional_cards_drawn_each_turn"], 2);
    assert_eq!(
        definition["pre_battle_confusion"]["eligible_cost"],
        "nonnegative_combat_cost"
    );
    assert_eq!(
        definition["pre_battle_confusion"]["turn_cost_update"],
        "only_when_combat_cost_changes"
    );
    assert!(definition.get("energy_per_turn_delta").is_none());
    assert!(definition.get("end_turn_discard_rule").is_none());
    let pyramid = current["relic_mechanics"]["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["relic"] == "RunicPyramid")
        .unwrap();
    assert_eq!(pyramid["end_turn_discard_rule"], "skip_ordinary_discard");
    assert!(pyramid.get("additional_cards_drawn_each_turn").is_none());
    assert!(pyramid.get("pre_battle_confusion").is_none());
}

fn gold_gain_document() -> Value {
    let TaskPublicObservationV1::Strategic { mut observation } =
        super::tests::fixture_decision().observation
    else {
        unreachable!()
    };
    observation.decision_site = PlannerDecisionSite::BossRelic;
    observation.context = PlannerDecisionContext::BossRelic;
    observation.run.gold = 120;
    observation.relics = [
        RelicId::BloodyIdol,
        RelicId::QuestionCard,
        RelicId::PrayerWheel,
    ]
    .into_iter()
    .map(|relic| PlannerRelicObservation {
        relic,
        counter: -1,
        amount: 0,
        used_up: false,
    })
    .collect();
    let candidates = [RelicId::Ectoplasm, RelicId::BustedCrown, RelicId::CursedKey]
        .into_iter()
        .enumerate()
        .map(
            |(option_index, relic)| TaskStrategicActionV1::TakeBossRelic {
                option_index,
                relic,
            },
        )
        .chain(std::iter::once(TaskStrategicActionV1::SkipBossRelic))
        .map(|action| TaskPublicCandidateV1::Strategic { action })
        .collect();
    let decision = TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::BossRelic,
        TaskPublicObservationV1::Strategic { observation },
        candidates,
    )
    .unwrap();
    public_decision_export_v3(
        &decision,
        PublicDecisionSourceV1::ConstructedContractCase {
            case_id: "boss-relic-gold-gain-restriction".into(),
        },
    )
    .unwrap()
}

#[test]
fn gold_gain_fixture_matches_typed_producer_without_rewriting_owned_gold() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-gold-gain-reading.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut recorded = fixture["payload"].clone();
    let mut current = gold_gain_document()["payload"].clone();
    recorded.as_object_mut().unwrap().remove("producer");
    current.as_object_mut().unwrap().remove("producer");
    assert_eq!(recorded, current);
    assert_eq!(
        current["decision"]["observation"]["observation"]["run"]["gold"],
        120
    );
    assert_eq!(
        fixture["payload_sha256"],
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&fixture["payload"]).unwrap())
        )
    );
    let definitions = current["relic_mechanics"]["definitions"]
        .as_array()
        .unwrap();
    let ectoplasm = definitions
        .iter()
        .find(|row| row["relic"] == "Ectoplasm")
        .unwrap();
    assert_eq!(ectoplasm["gold_gain_rule"], "prevent_gold_gain");
    assert_eq!(ectoplasm["energy_per_turn_delta"], 1);
    let idol = definitions
        .iter()
        .find(|row| row["relic"] == "BloodyIdol")
        .unwrap();
    assert_eq!(idol["gold_gain_healing"], 5);
    assert!(definitions
        .iter()
        .filter(|row| row["relic"] != "BloodyIdol")
        .all(|row| row.get("gold_gain_healing").is_none()));
    assert!(definitions
        .iter()
        .filter(|row| row["relic"] != "Ectoplasm")
        .all(|row| row.get("gold_gain_rule").is_none()));
    let crown = definitions
        .iter()
        .find(|row| row["relic"] == "BustedCrown")
        .unwrap();
    assert_eq!(crown["card_reward_choice_delta"], -2);
    assert!(crown
        .get("additional_normal_monster_card_reward_rows")
        .is_none());
    let question = definitions
        .iter()
        .find(|row| row["relic"] == "QuestionCard")
        .unwrap();
    assert_eq!(question["card_reward_choice_delta"], 1);
    let prayer = definitions
        .iter()
        .find(|row| row["relic"] == "PrayerWheel")
        .unwrap();
    assert_eq!(prayer["additional_normal_monster_card_reward_rows"], 1);
    assert!(prayer.get("card_reward_choice_delta").is_none());
}
