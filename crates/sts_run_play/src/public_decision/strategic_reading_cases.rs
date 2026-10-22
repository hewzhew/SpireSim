//! Constructed public-format cases, not executable sessions or strategy labels.
use super::*;
use sts_oracle_learning::agent::task::TaskDecisionSiteV1;
use sts_oracle_learning::ai::planner_core::*;
use sts_oracle_learning::content::{cards::CardId, potions::PotionId, relics::RelicId};
use sts_oracle_learning::state::events::{EventActionKind, EventId};
use sts_oracle_learning::state::selection::{SelectionReason, SelectionScope};

fn cases() -> Vec<(&'static str, TaskPublicDecisionV1)> {
    use PlannerDecisionContext as C;
    use PlannerDecisionSite as S;
    use TaskStrategicActionV1 as A;
    let shop = PlannerShopInventoryV1 {
        cards: vec![
            PlannerShopCardOfferV1 {
                slot: 0,
                card: CardId::DarkShackles,
                upgrades: 0,
                price: 50,
                affordable: true,
                purchase_status: PlannerShopPurchaseStatusV1::Purchasable,
            },
            PlannerShopCardOfferV1 {
                slot: 1,
                card: CardId::Corruption,
                upgrades: 0,
                price: 200,
                affordable: false,
                purchase_status: PlannerShopPurchaseStatusV1::Blocked {
                    reason: PlannerShopPurchaseBlockReasonV1::NotEnoughGold {
                        gold: 131,
                        price: 200,
                    },
                },
            },
        ],
        relics: vec![PlannerShopRelicOfferV1 {
            slot: 0,
            relic: RelicId::Anchor,
            price: 99,
            affordable: true,
            purchase_status: PlannerShopPurchaseStatusV1::Purchasable,
        }],
        potions: vec![PlannerShopPotionOfferV1 {
            slot: 0,
            potion: PotionId::FirePotion,
            price: 50,
            affordable: true,
            purchase_status: PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot,
            },
        }],
        purge: Some(PlannerShopPurgeOfferV1 {
            price: 75,
            affordable: true,
            purchase_status: PlannerShopPurchaseStatusV1::Purchasable,
        }),
    };
    let rewards = vec![
        PlannerRewardDescriptor::Gold { amount: 25 },
        PlannerRewardDescriptor::CardReward {
            cards: vec![PlannerOfferedCard {
                card: CardId::Clash,
                upgrades: 0,
            }],
        },
        PlannerRewardDescriptor::Potion {
            potion: PotionId::FirePotion,
        },
        PlannerRewardDescriptor::Relic {
            relic: RelicId::Anchor,
        },
        PlannerRewardDescriptor::SapphireKey,
    ];
    let event = |event, label: Option<&str>| A::ChooseEventOption {
        event,
        screen: 0,
        option_index: 0,
        action: EventActionKind::Continue,
        public_semantics: PlannerEventOptionPublicSemanticsV1 {
            public_label: label.map(str::to_owned),
            ..Default::default()
        },
    };
    let specs = vec![
        (
            "reward",
            S::Reward,
            C::Reward {
                pending_rewards: rewards.clone(),
            },
            vec![
                A::ClaimReward {
                    reward_item_index: 0,
                    reward: rewards[0].clone(),
                },
                A::OpenCardReward {
                    reward_item_index: 1,
                },
                A::DiscardRunPotion {
                    slot: 0,
                    potion: PotionId::FirePotion,
                },
                A::ClaimReward {
                    reward_item_index: 3,
                    reward: rewards[3].clone(),
                },
                A::ClaimReward {
                    reward_item_index: 4,
                    reward: rewards[4].clone(),
                },
                A::Proceed { site: S::Reward },
            ],
        ),
        (
            "empty_reward",
            S::Reward,
            C::Reward {
                pending_rewards: vec![],
            },
            vec![A::Proceed { site: S::Reward }],
        ),
        (
            "shop",
            S::Shop,
            C::Shop {
                purge_cost: 75,
                purge_available: true,
                inventory: shop,
            },
            vec![
                A::BuyCard {
                    shop_slot: 0,
                    card: CardId::DarkShackles,
                    upgrades: 0,
                    price: 50,
                },
                A::BuyRelic {
                    shop_slot: 0,
                    relic: RelicId::Anchor,
                    price: 99,
                },
                A::RemoveCard {
                    card_ordinal: 0,
                    card: CardId::Strike,
                    upgrades: 0,
                    price: 75,
                },
                A::DiscardRunPotion {
                    slot: 1,
                    potion: PotionId::FirePotion,
                },
                A::OpenPendingRewards,
                A::LeaveShop,
            ],
        ),
        (
            "campfire",
            S::Campfire,
            C::Campfire,
            vec![
                A::Rest,
                A::Smith {
                    card_ordinal: 0,
                    card: CardId::Strike,
                    upgrades: 0,
                },
                A::Recall,
            ],
        ),
        (
            "map",
            S::Map,
            C::Map { overlay: false },
            vec![
                A::ChooseRouteNode {
                    x: 0,
                    y: 6,
                    flight: false,
                    question_mark_outcomes: vec![],
                },
                A::ChooseRouteNode {
                    x: 2,
                    y: 6,
                    flight: false,
                    question_mark_outcomes: vec![
                        PlannerQuestionMarkOutcome::Event,
                        PlannerQuestionMarkOutcome::Monster,
                    ],
                },
            ],
        ),
        (
            "neow",
            S::Neow,
            C::Event {
                event: EventId::Neow,
                screen: 0,
                face_trader_result: None,
            },
            vec![event(EventId::Neow, Some("[Continue]"))],
        ),
        (
            "event",
            S::Event,
            C::Event {
                event: EventId::LivingWall,
                screen: 0,
                face_trader_result: None,
            },
            vec![event(EventId::LivingWall, None)],
        ),
        (
            "boss_relic",
            S::BossRelic,
            C::BossRelic,
            vec![
                A::TakeBossRelic {
                    option_index: 0,
                    relic: RelicId::SneckoEye,
                },
                A::SkipBossRelic,
            ],
        ),
        (
            "run_choice",
            S::RunChoice,
            C::RunChoice {
                scope: SelectionScope::Deck,
                reason: SelectionReason::Purge,
                source: None,
                min_choices: 1,
                max_choices: 1,
            },
            vec![A::BeginRunCardSelection {
                scope: SelectionScope::Deck,
                reason: SelectionReason::Purge,
                min_choices: 1,
                max_choices: 1,
                selectable_card_ordinals: vec![0, 1],
            }],
        ),
        ("treasure", S::Treasure, C::Treasure, vec![A::OpenChest]),
    ];
    specs
        .into_iter()
        .map(|(name, site, context, actions)| {
            let TaskPublicObservationV1::Strategic { mut observation } =
                super::tests::fixture_decision().observation
            else {
                unreachable!()
            };
            observation.decision_site = site;
            observation.context = context;
            if matches!(site, S::Shop | S::Reward) {
                // Full slots: seeing an offered potion must not invent a claim/buy action.
                observation.potions[1].potion = observation.potions[0].potion;
            }
            let decision = TaskPublicDecisionV1::new(
                TaskDecisionSiteV1::from(site),
                TaskPublicObservationV1::Strategic { observation },
                actions
                    .into_iter()
                    .map(|action| TaskPublicCandidateV1::Strategic { action })
                    .collect(),
            )
            .unwrap();
            (name, decision)
        })
        .collect()
}

fn exports() -> Value {
    let cases = cases()
        .into_iter()
        .map(|(name, decision)| {
            let export = public_decision_export_v3(
                &decision,
                PublicDecisionSourceV1::ConstructedContractCase {
                    case_id: format!("strategic-reading-{name}"),
                },
                sts_presentation::Locale::En,
            )
            .unwrap();
            (name.to_owned(), export)
        })
        .collect::<serde_json::Map<_, _>>();
    json!({"schema": "sts-strategic-reading-contract-cases-v1", "cases": cases})
}

#[test]
fn strategic_reading_preserves_every_public_candidate_and_its_typed_binding() {
    for (name, decision) in cases() {
        let export = public_decision_export_v3(
            &decision,
            PublicDecisionSourceV1::ConstructedContractCase {
                case_id: name.into(),
            },
            sts_presentation::Locale::En,
        )
        .unwrap();
        let surface = &export["payload"]["strategic_reading"];
        assert_eq!(
            surface["public_snapshot_id"],
            decision.information.snapshot_id
        );
        assert_eq!(
            surface["candidates"].as_array().unwrap().len(),
            decision.candidates.len()
        );
        for ((row, candidate), id) in surface["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&decision.candidates)
            .zip(&decision.information.candidate_surface.ordered_candidate_ids)
        {
            assert_eq!(row["candidate_id"], *id);
            assert_eq!(
                row["candidate_sha256"],
                format!(
                    "{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&serde_json::to_value(candidate).unwrap()).unwrap()
                    )
                )
            );
        }
    }
}

#[test]
fn open_card_reading_retains_the_other_pending_rewards() {
    let (_, reward) = cases()
        .into_iter()
        .find(|(name, _)| *name == "reward")
        .unwrap();
    let TaskPublicObservationV1::Strategic { mut observation } = reward.observation else {
        unreachable!()
    };
    let PlannerDecisionContext::Reward { pending_rewards } = observation.context else {
        unreachable!()
    };
    observation.context = PlannerDecisionContext::CardReward {
        reward_item_index: Some(1),
        pending_rewards,
    };
    observation.decision_site = PlannerDecisionSite::CardReward;
    let decision = TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::CardReward,
        TaskPublicObservationV1::Strategic { observation },
        vec![TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::SkipCardReward {
                reward_item_index: 1,
            },
        }],
    )
    .unwrap();
    let reading =
        super::strategic_reading::project(&decision, sts_presentation::Locale::En).unwrap();
    let detail = reading["context_detail"].as_str().unwrap();
    assert!(detail.contains("Reward item 0: 25 gold"));
    assert!(detail.contains("Reward item 2: potion"));
}

#[test]
#[ignore = "regenerates the constructed run-choice fixture; run explicitly"]
fn regenerate_constructed_run_choice_reading() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/expert/tests/fixtures/public-strategic-sites.json");
    let mut fixture: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    fixture["cases"]["run_choice"] = exports()["cases"]["run_choice"].clone();
    std::fs::write(path, serde_json::to_string_pretty(&fixture).unwrap() + "\n").unwrap();
}

#[test]
fn checked_in_strategic_readings_match_the_typed_projection() {
    let captured: Value = serde_json::from_str(include_str!(
        "../../../../tools/expert/tests/fixtures/public-strategic-sites.json"
    ))
    .unwrap();
    let current = exports();
    for (name, export) in current["cases"].as_object().unwrap() {
        let retained = &captured["cases"][name];
        assert_eq!(export["schema"], retained["schema"]);
        // Producer provenance remains the original capture; never relabel it.
        for key in ["decision", "source", "card_mechanics"] {
            assert_eq!(
                export["payload"][key], retained["payload"][key],
                "{name}/{key}"
            );
        }
        // Wording is presentation, not a frozen protocol. Check the identity
        // join and shape without making old prose a compatibility requirement.
        for key in ["schema", "public_snapshot_id", "decision_site"] {
            assert_eq!(
                export["payload"]["strategic_reading"][key],
                retained["payload"]["strategic_reading"][key]
            );
        }
        let current_rows = export["payload"]["strategic_reading"]["candidates"]
            .as_array()
            .unwrap();
        let retained_rows = retained["payload"]["strategic_reading"]["candidates"]
            .as_array()
            .unwrap();
        assert_eq!(current_rows.len(), retained_rows.len());
        for (current, old) in current_rows.iter().zip(retained_rows) {
            for key in ["candidate_id", "candidate_sha256"] {
                assert_eq!(current[key], old[key]);
            }
        }
        assert_eq!(
            retained["payload_sha256"],
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&retained["payload"]).unwrap())
            )
        );
    }
}
