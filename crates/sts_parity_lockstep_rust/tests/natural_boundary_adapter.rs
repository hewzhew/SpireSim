use sts_oracle_run_control::{
    content::events::neow::NeowEligibilityV1,
    content::potions::{Potion, PotionId},
    eval::run_control::{
        AtomicCombatSearchOptionsV2, RunCombatResolutionKindV1, RunControlCombatSegmentMode,
        RunControlConfig, RunControlSession,
    },
    runtime::rng::StsRng,
    state::{
        core::{ClientInput, EngineState, RunPendingChoiceReason, RunPendingChoiceState},
        events::{EventId, EventState},
        map::RoomType,
        selection::DomainEventSource,
    },
};
use sts_parity_lockstep::{
    DecisionSurfaceV1, DomainEvidenceV1, LockstepAdapterV1, NaturalRunRngOperationV1,
    ParityDomainV1, SemanticActionV1, SemanticScalarV1, NATURAL_RUN_RELICS_SCHEMA_V3,
    NATURAL_RUN_STATE_SCHEMA_V2,
};
use sts_parity_lockstep_rust::{
    natural_run_bootstrap_v1, run_control_config_from_bootstrap_v1, RustLockstepAdapterV1,
    NATURAL_RUN_SNAPSHOT_DOMAINS_V3,
};

fn mini_neow_adapter() -> RustLockstepAdapterV1 {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let session = RunControlSession::new(config);
    RustLockstepAdapterV1::new(session, bootstrap, "test-build", "rust-test-epoch").unwrap()
}

fn act_three_event_input_adapter() -> RustLockstepAdapterV1 {
    use sts_oracle_run_control::state::events::generator::EventGenerator;
    let config = RunControlConfig {
        seed: 2026090607,
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.engine_state = EngineState::MapNavigation;
    session.run_state.act_num = 3;
    session.run_state.floor_num = 34;
    session.run_state.event_state = None;
    session.run_state.event_generator = EventGenerator::new_with_note_for_yourself(3, false);
    let generator = &mut session.run_state.event_generator;
    generator.monster_chance = 0.0;
    generator.shop_chance = 0.0;
    generator.treasure_chance = 0.0;
    generator.shrine_chance = 1.0;
    generator.shrine_pool = vec![EventId::GremlinWheelGame];
    generator.one_time_event_pool = vec![EventId::SecretPortal];
    for node in &mut session.run_state.map.graph[0] {
        if !node.edges.is_empty() {
            node.class = Some(RoomType::EventRoom);
        }
    }
    RustLockstepAdapterV1::new(session, bootstrap, "test", "external-time").unwrap()
}

#[test]
fn wheel_confirmation_opens_bound_purge_at_leave_screen_then_returns_to_event() {
    let config = RunControlConfig {
        seed: 2026090607,
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.floor_num = 1;
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 0;
    session.run_state.map.graph[0][0].class = Some(RoomType::EventRoom);
    let mut event = EventState::new(EventId::GremlinWheelGame);
    event.current_screen = 1;
    event.internal_state = 4;
    session.run_state.event_state = Some(event);
    session.engine_state = EngineState::EventRoom;
    let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "fixture", "wheel").unwrap();
    let before = adapter.observe().unwrap();
    let confirm = action_space(&before)
        .into_iter()
        .find(|a| {
            matches!(
                a,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Event,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&confirm, &before).unwrap();
    let grid = adapter.execute(prepared).unwrap();
    assert_eq!(grid.boundary_kind, "natural_grid_select");
    let DomainEvidenceV1::Observed { value, .. } = &grid.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("room must be observed");
    };
    assert_eq!(value["screen"], 2);
    assert_eq!(value["wheel_result"], 4);
    let purge = action_space(&grid)
        .into_iter()
        .find(|a| {
            matches!(
                a,
                SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::GridSelect,
                    ..
                }
            )
        })
        .unwrap();
    let count = adapter.session().run_state.master_deck.len();
    let prepared = adapter.prepare(&purge, &grid).unwrap();
    let leave = adapter.execute(prepared).unwrap();
    assert_eq!(leave.boundary_kind, "natural_event");
    assert_eq!(adapter.session().run_state.master_deck.len(), count - 1);
    assert_eq!(
        adapter
            .session()
            .run_state
            .event_state
            .as_ref()
            .unwrap()
            .current_screen,
        2
    );
}

#[test]
fn execution_inputs_defer_event_successor_and_fail_atomically_on_missing_or_extra_time() {
    use sts_parity_lockstep::{AdapterFailureKindV1, ExecutionInputsV1};
    let mut adapter = act_three_event_input_adapter();
    let before = adapter.observe().unwrap();
    let action = action_space(&before)
        .into_iter()
        .find(|a| {
            matches!(
                a,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&action, &before).unwrap();
    assert_eq!(adapter.observe().unwrap(), before);
    assert!(adapter.last_rng_call_ledger_v1().is_none());
    assert_eq!(
        adapter.execute(prepared.clone()).unwrap_err().kind,
        AdapterFailureKindV1::Unobserved
    );
    assert_eq!(adapter.observe().unwrap(), before);
    assert!(adapter.last_rng_call_ledger_v1().is_none());
    for samples in [
        vec![],
        vec![800.0f32.to_bits(), 801.0f32.to_bits()],
        vec![f32::NAN.to_bits()],
    ] {
        let inputs = ExecutionInputsV1 {
            secret_portal_playtime_bits: Some(samples),
        };
        assert_eq!(
            adapter
                .execute_with_inputs_v1(prepared.clone(), &inputs)
                .unwrap_err()
                .kind,
            AdapterFailureKindV1::Evidence
        );
        assert_eq!(adapter.observe().unwrap(), before);
    }
    let mut cold = adapter.clone();
    let mut hot = adapter.clone();
    for (endpoint, time, expected_range) in [(&mut cold, 799.0f32, 0), (&mut hot, 800.0f32, 1)] {
        endpoint
            .execute_with_inputs_v1(
                prepared.clone(),
                &ExecutionInputsV1 {
                    secret_portal_playtime_bits: Some(vec![time.to_bits()]),
                },
            )
            .unwrap();
        assert!(endpoint.last_rng_call_ledger_v1().unwrap().0.iter().any(|call| {
            matches!(call.operation, NaturalRunRngOperationV1::RandomInt { range, .. } if range == expected_range)
        }));
        assert_eq!(endpoint.session().run_state.playtime_seconds, 0.0);
    }
    assert_eq!(
        cold.session().run_state.event_state.as_ref().unwrap().id,
        EventId::GremlinWheelGame
    );
    assert_ne!(
        cold.last_rng_call_ledger_v1(),
        hot.last_rng_call_ledger_v1()
    );
    assert!(hot
        .execute_with_inputs_v1(prepared, &ExecutionInputsV1::observed_empty())
        .is_err());
}

#[test]
fn run_potions_remain_usable_across_reward_map_and_shop_boundaries() {
    use sts_oracle_run_control::state::{
        rewards::{RewardItem, RewardState},
        shop::ShopState,
    };
    for room in ["reward", "map", "shop"] {
        for potion in [
            PotionId::BloodPotion,
            PotionId::FruitJuice,
            PotionId::EntropicBrew,
        ] {
            let config = RunControlConfig {
                seed: 2_026_090_607,
                ascension_level: 20,
                final_act: true,
                ..Default::default()
            };
            let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
            let mut session = RunControlSession::new(config);
            session.run_state.floor_num = 1;
            session.run_state.map.current_x = 1;
            session.run_state.map.current_y = 0;
            session.run_state.event_state = None;
            session.run_state.current_hp = 40;
            session.run_state.max_hp = 75;
            session.run_state.potions = vec![
                Some(Potion::new(PotionId::LiquidBronze, 40)),
                (room != "reward").then(|| Potion::new(potion, 41)),
            ];
            session.engine_state = match room {
                "reward" => EngineState::RewardScreen(RewardState {
                    items: vec![RewardItem::Potion { potion_id: potion }],
                    ..RewardState::new()
                }),
                "map" => EngineState::MapNavigation,
                "shop" => EngineState::Shop(ShopState::new()),
                _ => unreachable!(),
            };
            let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "test", room).unwrap();
            if room == "reward" {
                let reward = adapter.observe().unwrap();
                let collect = action_space(&reward)
                    .into_iter()
                    .find(|action| {
                        matches!(
                            action,
                            SemanticActionV1::Select {
                                surface: DecisionSurfaceV1::CombatReward,
                                ..
                            }
                        )
                    })
                    .unwrap();
                // The original failure occurred while projecting this prepared successor:
                // collecting a usable potion adds a run-level UsePotion candidate.
                let prepared = adapter.prepare(&collect, &reward).unwrap();
                assert_eq!(adapter.observe().unwrap(), reward);
                adapter.execute(prepared).unwrap();
                assert_eq!(
                    adapter.session().run_state.potions[1].as_ref().unwrap().id,
                    potion
                );
            }
            let before = adapter.observe().unwrap();
            let actions = action_space(&before);
            let action = actions
                .iter()
                .find(|action| {
                    matches!(
                        action,
                        SemanticActionV1::UsePotion {
                            slot: 1,
                            target: None,
                            ..
                        }
                    )
                })
                .unwrap();
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action, SemanticActionV1::UsePotion { .. }))
                    .count(),
                1
            );
            let prepared = adapter.prepare(action, &before).unwrap();
            assert_eq!(adapter.observe().unwrap(), before);
            let after = adapter.execute(prepared).unwrap();
            assert_eq!(after.boundary_kind, before.boundary_kind);
            assert_eq!(
                adapter.session().run_state.potions[0]
                    .as_ref()
                    .unwrap()
                    .uuid,
                40
            );
            match potion {
                PotionId::BloodPotion => {
                    assert_eq!(adapter.session().run_state.current_hp, 55);
                    assert_eq!(adapter.session().run_state.max_hp, 75);
                    assert!(adapter.session().run_state.potions[1].is_none());
                    assert!(adapter.last_rng_call_ledger_v1().unwrap().0.is_empty());
                }
                PotionId::FruitJuice => {
                    assert_eq!(adapter.session().run_state.current_hp, 45);
                    assert_eq!(adapter.session().run_state.max_hp, 80);
                    assert!(adapter.session().run_state.potions[1].is_none());
                    assert!(adapter.last_rng_call_ledger_v1().unwrap().0.is_empty());
                }
                PotionId::EntropicBrew => {
                    assert!(adapter.session().run_state.potions[1].is_some());
                    assert!(adapter
                        .last_rng_call_ledger_v1()
                        .unwrap()
                        .0
                        .iter()
                        .all(|call| call.owner == "run.potion"));
                    assert!(!adapter.last_rng_call_ledger_v1().unwrap().0.is_empty());
                }
                _ => unreachable!(),
            }
        }
    }
}

fn late_boss_adapter(final_boss: bool) -> RustLockstepAdapterV1 {
    boss_adapter(final_boss, false)
}

fn boss_adapter(final_boss: bool, heart: bool) -> RustLockstepAdapterV1 {
    use sts_oracle_run_control::{
        content::{cards::CardId, monsters::factory::EncounterId},
        engine::run_loop::tick_run_active_with_observer,
        state::core::CombatStartRequest,
    };
    let config = RunControlConfig {
        seed: 1,
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.act_num = 3;
    session.run_state.floor_num = 50;
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 15;
    session.run_state.event_state = None;
    session.run_state.current_hp = 40;
    session.run_state.boss_key = Some(EncounterId::DonuAndDeca);
    session.run_state.boss_list = vec![
        EncounterId::DonuAndDeca,
        EncounterId::AwakenedOne,
        EncounterId::TimeEater,
    ];
    if final_boss {
        session.run_state.keys = [true; 3];
        session.run_state.boss_list = vec![EncounterId::DonuAndDeca, EncounterId::TimeEater];
    }
    if heart {
        session.run_state.act_num = 4;
        session.run_state.floor_num = 56;
        session.run_state.map = sts_oracle_run_control::state::map::state::MapState::new(
            sts_oracle_run_control::state::map::generator::generate_ending_map(),
        );
        session.run_state.map.current_x = 3;
        session.run_state.map.current_y = 3;
        session.run_state.boss_key = Some(EncounterId::TheHeart);
        session.run_state.boss_list = vec![EncounterId::TheHeart];
    }
    let encounter = session.run_state.next_boss().unwrap();
    session.engine_state = EngineState::CombatStart(CombatStartRequest::room(
        encounter,
        RoomType::MonsterRoomBoss,
    ));
    assert!(
        tick_run_active_with_observer(
            &mut session.engine_state,
            &mut session.run_state,
            &mut session.active_combat,
            None
        )
        .keep_running
    );
    let active = session.active_combat.as_mut().unwrap();
    active.combat_state.entities.monsters[0].current_hp = 1;
    for monster in active.combat_state.entities.monsters.iter_mut().skip(1) {
        monster.current_hp = 0;
        monster.is_dying = true;
    }
    let strike = session
        .run_state
        .master_deck
        .iter()
        .find(|card| card.id == CardId::Strike)
        .unwrap()
        .clone();
    active.combat_state.zones.hand.clear();
    active.combat_state.zones.draw_pile = session
        .run_state
        .master_deck
        .iter()
        .filter(|card| card.lineage != strike.lineage)
        .cloned()
        .collect();
    active.combat_state.zones.hand.push(strike);
    RustLockstepAdapterV1::new(session, bootstrap, "fixture", "late-boss").unwrap()
}

#[test]
fn surrounded_projection_tracks_target_facing_and_both_monster_sides() {
    use sts_oracle_run_control::{
        content::{cards::CardId, monsters::factory::EncounterId},
        engine::run_loop::tick_run_active_with_observer,
        state::core::CombatStartRequest,
    };
    let config = RunControlConfig {
        seed: 1,
        ascension_level: 20,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.event_state = None;
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 0;
    session.run_state.map.graph[0][0].class = Some(RoomType::MonsterRoomElite);
    session.engine_state = EngineState::CombatStart(CombatStartRequest::room(
        EncounterId::ShieldAndSpear,
        RoomType::MonsterRoomElite,
    ));
    tick_run_active_with_observer(
        &mut session.engine_state,
        &mut session.run_state,
        &mut session.active_combat,
        None,
    );
    let strike = session
        .run_state
        .master_deck
        .iter()
        .find(|card| card.id == CardId::Strike)
        .unwrap()
        .clone();
    let active = session.active_combat.as_mut().unwrap();
    active.combat_state.zones.hand.clear();
    active.combat_state.zones.draw_pile = session
        .run_state
        .master_deck
        .iter()
        .filter(|card| card.lineage != strike.lineage)
        .cloned()
        .collect();
    active.combat_state.zones.hand.push(strike);
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "fixture", "surrounded").unwrap();
    let before = adapter.observe().unwrap();
    let combat_value = |snapshot: &sts_parity_lockstep::ParitySnapshotV1| {
        let DomainEvidenceV1::Observed { value, .. } =
            &snapshot.domains[&ParityDomainV1::Combat].evidence
        else {
            panic!("combat must be observed");
        };
        value.clone()
    };
    let before_value = combat_value(&before);
    assert_eq!(before_value["state"]["player"]["facing_left"], false);
    assert_eq!(
        before_value["state"]["monsters"][0]["enemy_id"],
        "spireshield"
    );
    assert_eq!(before_value["state"]["monsters"][0]["left_of_player"], true);
    assert_eq!(
        before_value["state"]["monsters"][1]["left_of_player"],
        false
    );
    let action = action_space(&before).into_iter().find(|action| matches!(
        action, SemanticActionV1::PlayCard { target: Some(target), .. } if target.id == "spireshield"
    )).unwrap();
    let prepared = adapter.prepare(&action, &before).unwrap();
    let after = adapter.execute(prepared).unwrap();
    let after_value = combat_value(&after);
    assert_eq!(after_value["state"]["player"]["facing_left"], true);
    assert_eq!(after_value["state"]["monsters"][0]["left_of_player"], true);
    assert_eq!(after_value["state"]["monsters"][1]["left_of_player"], false);
}

#[test]
fn late_boss_victory_closes_the_draft_before_the_explicit_room_transition() {
    for final_boss in [false, true] {
        let mut adapter = late_boss_adapter(final_boss);
        let before = adapter.observe().unwrap();
        let attack = action_space(&before)
            .into_iter()
            .find(|action| matches!(action, SemanticActionV1::PlayCard { .. }))
            .unwrap();
        let victory = adapter
            .verify_semantic_combat_victory_v1(std::slice::from_ref(&attack))
            .unwrap();
        assert_eq!(
            victory.boundary_kind,
            if final_boss {
                "natural_act3_victory_complete"
            } else {
                "natural_double_boss_complete"
            }
        );
        assert_eq!(adapter.session().run_state.current_hp, 40);
        let proceed = SemanticActionV1::Proceed {
            surface: if final_boss {
                DecisionSurfaceV1::Act3Victory
            } else {
                DecisionSurfaceV1::DoubleBoss
            },
        };
        assert!(adapter
            .verify_semantic_combat_victory_v1(&[attack.clone(), proceed.clone()])
            .is_err());
        let prepared = adapter.prepare(&attack, &before).unwrap();
        let after = adapter.execute(prepared).unwrap();
        assert_eq!(after, victory);
        assert!(adapter.session().active_combat.is_none());
        assert_eq!(adapter.session().run_state.current_hp, 46);
        assert_eq!(adapter.session().run_state.floor_num, 50);
        assert_eq!(action_space(&after), vec![proceed.clone()]);
        for domain in NATURAL_RUN_SNAPSHOT_DOMAINS_V3 {
            assert!(
                matches!(
                    after.domains[&domain].evidence,
                    DomainEvidenceV1::Observed { .. }
                ),
                "{domain:?}"
            );
        }
        let prepared = adapter.prepare(&proceed, &after).unwrap();
        let mut next = adapter.execute(prepared).unwrap();
        assert_eq!(adapter.session().run_state.floor_num, 51);
        if !final_boss {
            assert_eq!(next.boundary_kind, "natural_combat");
            continue;
        }
        for screen in [0, 1, 2, 4] {
            assert_eq!(next.boundary_kind, "natural_event");
            assert_eq!(adapter.session().run_state.current_hp, 46);
            assert_eq!(
                adapter
                    .session()
                    .run_state
                    .event_state
                    .as_ref()
                    .unwrap()
                    .current_screen,
                screen
            );
            let choices = action_space(&next);
            assert_eq!(choices.len(), 1);
            let prepared = adapter.prepare(&choices[0], &next).unwrap();
            next = adapter.execute(prepared).unwrap();
        }
        assert_eq!(next.boundary_kind, "natural_map");
        assert_eq!(adapter.session().run_state.act_num, 4);
        assert!(adapter.session().run_state.event_state.is_none());
    }
}

#[test]
fn heart_draft_closes_at_proceed_and_terminal_victory_has_no_actions() {
    let mut adapter = boss_adapter(true, true);
    let before = adapter.observe().unwrap();
    let attack = action_space(&before)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::PlayCard { .. }))
        .unwrap();
    let victory = adapter
        .verify_semantic_combat_victory_v1(std::slice::from_ref(&attack))
        .unwrap();
    assert_eq!(victory.boundary_kind, "natural_true_victory_complete");
    let proceed = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::TrueVictory,
    };
    assert!(adapter
        .verify_semantic_combat_victory_v1(&[attack.clone(), proceed.clone()])
        .is_err());
    let prepared = adapter.prepare(&attack, &before).unwrap();
    let after = adapter.execute(prepared).unwrap();
    assert_eq!(after, victory);
    assert_eq!(action_space(&after), vec![proceed.clone()]);
    assert_eq!(adapter.session().run_state.floor_num, 56);
    let prepared = adapter.prepare(&proceed, &after).unwrap();
    let terminal = adapter.execute(prepared).unwrap();
    assert_eq!(terminal.boundary_kind, "natural_heart_victory");
    assert!(action_space(&terminal).is_empty());
    assert_eq!(adapter.session().run_state.floor_num, 57);
    for domain in NATURAL_RUN_SNAPSHOT_DOMAINS_V3 {
        assert!(matches!(
            terminal.domains[&domain].evidence,
            DomainEvidenceV1::Observed { .. }
        ));
    }
}

#[test]
fn orrery_rewards_preserve_shop_inventory_pending_choices_and_card_sources() {
    use sts_oracle_run_control::{
        content::relics::RelicId,
        state::shop::{ShopRelic, ShopState},
    };
    let config = RunControlConfig::default();
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.gold = 300;
    session.run_state.floor_num = 5;
    session.run_state.map.current_x = 1;
    session.run_state.map.current_y = 4;
    session.engine_state = EngineState::Shop(ShopState {
        relics: vec![ShopRelic {
            relic_id: RelicId::Orrery,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        }],
        ..Default::default()
    });
    let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "fixture", "orrery").unwrap();
    fn step(
        adapter: &mut RustLockstepAdapterV1,
        action: SemanticActionV1,
    ) -> sts_parity_lockstep::ParitySnapshotV1 {
        let before = adapter.observe().unwrap();
        let prepared = adapter.prepare(&action, &before).unwrap();
        adapter.execute(prepared).unwrap()
    }
    fn domain(
        snapshot: &sts_parity_lockstep::ParitySnapshotV1,
        domain: ParityDomainV1,
    ) -> &serde_json::Value {
        let DomainEvidenceV1::Observed { value, .. } = &snapshot.domains[&domain].evidence else {
            panic!("missing domain")
        };
        value
    }
    let buy = action_space(&adapter.observe().unwrap()).into_iter().find(|a| matches!(a,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::Shop, choice } if choice.id == "orrery"
    )).unwrap();
    let mut current = step(&mut adapter, buy);
    assert_eq!(current.boundary_kind, "natural_combat_reward");
    assert_eq!(adapter.session().run_state.gold, 150);
    let stock = domain(&current, ParityDomainV1::Room)["return_to"]["stock"].clone();
    assert_eq!(stock["relics"], serde_json::json!([]));
    assert_eq!(
        domain(&current, ParityDomainV1::Rewards)["items"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    let back = SemanticActionV1::Cancel {
        surface: DecisionSurfaceV1::CombatReward,
    };
    assert!(action_space(&current).contains(&back));
    assert!(
        !action_space(&current).contains(&SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward
        })
    );
    current = step(&mut adapter, back.clone());
    assert_eq!(current.boundary_kind, "natural_shop");
    assert_eq!(domain(&current, ParityDomainV1::Room)["stock"], stock);
    assert_eq!(
        domain(&current, ParityDomainV1::Rewards)["items"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    let reopen = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::CombatReward,
    };
    current = step(&mut adapter, reopen);
    for ordinal in 0..5 {
        let open = action_space(&current)
            .into_iter()
            .find(|a| {
                matches!(
                    a,
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::CombatReward,
                        ..
                    }
                )
            })
            .unwrap();
        current = step(&mut adapter, open);
        assert_eq!(current.boundary_kind, "natural_card_reward");
        assert_eq!(
            domain(&current, ParityDomainV1::Room)["return_to"]["stock"],
            stock
        );
        assert!(
            action_space(&current).contains(&adapter.preferred_card_reward_action_v1().unwrap())
        );
        if ordinal == 2 {
            let deck_len = adapter.session().run_state.master_deck.len();
            current = step(
                &mut adapter,
                SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::CardReward,
                },
            );
            assert_eq!(adapter.session().run_state.master_deck.len(), deck_len);
            assert_eq!(
                domain(&current, ParityDomainV1::Rewards)["items"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
            let reopen_card = action_space(&current)
                .into_iter()
                .find(|a| {
                    matches!(
                        a,
                        SemanticActionV1::Select {
                            surface: DecisionSurfaceV1::CombatReward,
                            ..
                        }
                    )
                })
                .unwrap();
            current = step(&mut adapter, reopen_card);
        }
        let take = action_space(&current)
            .into_iter()
            .find(|a| {
                matches!(
                    a,
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::CardReward,
                        ..
                    }
                )
            })
            .unwrap();
        current = step(&mut adapter, take);
        let binding = format!("natural_run/card_reward/F05X01/{ordinal}");
        assert!(domain(&current, ParityDomainV1::MasterDeck)
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["binding"] == binding));
    }
    current = step(&mut adapter, back);
    assert_eq!(current.boundary_kind, "natural_shop");
    assert_eq!(domain(&current, ParityDomainV1::Room)["stock"], stock);
    assert_eq!(
        domain(&current, ParityDomainV1::Rewards)["status"],
        "inactive"
    );
    assert!(
        !action_space(&current).contains(&SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward
        })
    );
}

#[test]
fn shop_purchases_and_purge_keep_distinct_monotonic_card_sources() {
    use sts_oracle_run_control::{
        content::cards::CardId,
        state::shop::{ShopCard, ShopState},
    };
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.gold = 300;
    session.run_state.map.current_x = 1;
    session.run_state.map.current_y = 4;
    let item = ShopCard {
        card_id: CardId::Exhume,
        upgrades: 0,
        price: 40,
        can_buy: true,
        blocked_reason: None,
    };
    session.engine_state = EngineState::Shop(ShopState {
        cards: vec![item.clone(), item],
        ..Default::default()
    });
    let mut rust = RustLockstepAdapterV1::new(session, bootstrap, "fixture", "shop").unwrap();
    let mut current = rust.observe().unwrap();
    let select = |snapshot: &sts_parity_lockstep::ParitySnapshotV1, kind: &str, id: &str| {
        action_space(snapshot).into_iter().find(|action|matches!(action,SemanticActionV1::Select {surface:DecisionSurfaceV1::Shop,choice} if choice.kind==kind && choice.id==id && choice.occurrence==0)).unwrap()
    };
    let buy = select(&current, "shop_card", "exhume");
    let prepared = rust.prepare(&buy, &current).unwrap();
    current = rust.execute(prepared).unwrap();
    let first = "natural_run/shop_purchase/F05X01/0";
    let purge = select(&current, "deck_card", first);
    let prepared = rust.prepare(&purge, &current).unwrap();
    current = rust.execute(prepared).unwrap();
    let buy = select(&current, "shop_card", "exhume");
    let prepared = rust.prepare(&buy, &current).unwrap();
    current = rust.execute(prepared).unwrap();
    assert_eq!(rust.session().run_state.gold, 145);
    let DomainEvidenceV1::Observed { value, .. } =
        &current.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("missing deck");
    };
    assert!(value
        .as_array()
        .unwrap()
        .iter()
        .any(|card| card["binding"] == "natural_run/shop_purchase/F05X01/1"));
    assert!(!value
        .as_array()
        .unwrap()
        .iter()
        .any(|card| card["binding"] == first));
    assert!(!action_space(&current).iter().any(|action|matches!(action,SemanticActionV1::Select {surface:DecisionSurfaceV1::Shop,choice} if choice.kind=="deck_card")));
}

#[test]
fn natural_neow_snapshot_declares_and_observes_every_required_domain() {
    let mut adapter = mini_neow_adapter();
    let snapshot = adapter.observe().unwrap();

    assert_eq!(snapshot.boundary_kind, "natural_neow");
    assert_eq!(
        snapshot.domains.len(),
        NATURAL_RUN_SNAPSHOT_DOMAINS_V3.len()
    );
    for domain in NATURAL_RUN_SNAPSHOT_DOMAINS_V3 {
        assert_eq!(snapshot.contract.critical(domain), Some(true));
        assert!(matches!(
            snapshot.domains[&domain].evidence,
            DomainEvidenceV1::Observed { .. }
        ));
    }
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("natural Neow RNG streams should be observed")
    };
    let streams = value.as_array().unwrap();
    assert_eq!(streams.len(), 8);
    assert!(streams.iter().all(|stream| stream["owner"] != "run.neow"));
    assert!(streams.iter().all(|stream| stream["scope"] != "floor"));

    let DomainEvidenceV1::Observed { schema, value } =
        &snapshot.domains[&ParityDomainV1::Run].evidence
    else {
        panic!("natural run state should be observed")
    };
    assert_eq!(schema, NATURAL_RUN_STATE_SCHEMA_V2);
    assert_eq!(value["shop_purge_base_cost"], 75);
    assert!(value["keys"].is_object());
    assert!(value.get("highest_unlocked_ascension").is_none());
    assert!(value["room_flags"].get("pending_boss_reward").is_none());

    let DomainEvidenceV1::Observed { schema, value } =
        &snapshot.domains[&ParityDomainV1::Relics].evidence
    else {
        panic!("natural relic state should be observed")
    };
    assert_eq!(schema, NATURAL_RUN_RELICS_SCHEMA_V3);
    assert_eq!(value[0]["id"], "burningblood");
    assert_eq!(value[0]["bound_card"], serde_json::Value::Null);
    assert!(value[0].get("amount").is_none());
}

#[test]
fn living_wall_grow_keeps_result_screen_while_upgrading_a_bound_card() {
    event_upgrade_keeps_result_screen(EventId::LivingWall, 2, 1);
}

#[test]
fn accursed_blacksmith_forge_keeps_result_screen_while_upgrading_a_bound_card() {
    event_upgrade_keeps_result_screen(EventId::AccursedBlacksmith, 0, 2);
}

#[test]
fn upgrade_shrine_pray_keeps_complete_screen_while_upgrading_a_bound_card() {
    event_upgrade_keeps_result_screen(EventId::UpgradeShrine, 0, 1);
}

#[test]
fn transmogrifier_pray_keeps_complete_screen_through_bound_card_transformation() {
    let config = RunControlConfig {
        seed: 2_026_101_901,
        player_class: "Defect",
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    let (x, y) = session
        .run_state
        .map
        .graph
        .iter()
        .flatten()
        .find(|node| node.class.is_some() && !node.edges.is_empty())
        .map(|node| (node.x, node.y))
        .unwrap();
    session.run_state.map.current_x = x;
    session.run_state.map.current_y = y;
    session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::EventRoom);
    session.run_state.event_state = Some(EventState::new(EventId::Transmorgrifier));
    session.engine_state = EngineState::EventRoom;
    let deck_before = session.run_state.master_deck.clone();
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test", "event-transform").unwrap();
    let initial = adapter.observe().unwrap();
    let pray = action_space(&initial)
        .into_iter()
        .find(|action| {
            matches!(action,
            SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, choice }
                if choice.occurrence == 0)
        })
        .unwrap();
    let prepared = adapter.prepare(&pray, &initial).unwrap();
    let grid = adapter.execute(prepared).unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &grid.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("transform grid must be observed")
    };
    assert_eq!(room["screen"], 1);
    assert_eq!(room["selection_kind"], "deck_transform");
    let transform = action_space(&grid)
        .into_iter()
        .find(|action| {
            matches!(action,
            SemanticActionV1::SubmitSelection { choices, .. }
                if choices.len() == 1 && choices[0].attributes.get("card_id")
                    == Some(&SemanticScalarV1::Text("strikeb".to_owned())))
        })
        .unwrap();
    let prepared = adapter.prepare(&transform, &grid).unwrap();
    let after = adapter.execute(prepared).unwrap();
    assert_eq!(after.boundary_kind, "natural_event");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &after.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("transform result must be observed")
    };
    assert_eq!(room["screen"], 1);
    let deck_after = &adapter.session().run_state.master_deck;
    assert_eq!(deck_before.len(), deck_after.len());
    assert_eq!(
        deck_before
            .iter()
            .filter(|before| !deck_after
                .iter()
                .any(|after| after.lineage == before.lineage))
            .count(),
        1
    );
    assert_eq!(
        deck_after
            .iter()
            .filter(|after| !deck_before
                .iter()
                .any(|before| before.lineage == after.lineage))
            .count(),
        1
    );
}

fn event_upgrade_keeps_result_screen(event: EventId, occurrence: u32, screen: i64) {
    let config = RunControlConfig {
        seed: 42,
        player_class: "Ironclad",
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    let (x, y) = session
        .run_state
        .map
        .graph
        .iter()
        .flatten()
        .find(|node| node.class.is_some() && !node.edges.is_empty())
        .map(|node| (node.x, node.y))
        .unwrap();
    session.run_state.map.current_x = x;
    session.run_state.map.current_y = y;
    session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::EventRoom);
    session.run_state.event_state = Some(EventState::new(event));
    session.engine_state = EngineState::EventRoom;
    let deck_before = session.run_state.master_deck.clone();
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test", "event-upgrade").unwrap();
    let initial = adapter.observe().unwrap();
    let grow = action_space(&initial)
        .into_iter()
        .find(|action| {
            matches!(action,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, choice }
            if choice.occurrence == occurrence)
        })
        .unwrap();
    let prepared = adapter.prepare(&grow, &initial).unwrap();
    let grid = adapter.execute(prepared).unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &grid.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("grid must be observed")
    };
    assert_eq!(room["screen"], screen);
    assert_eq!(room["selection_kind"], "deck_upgrade");
    let upgrade = action_space(&grid)
        .into_iter()
        .find(|action| {
            matches!(action,
        SemanticActionV1::SubmitSelection { choices, .. } if choices[0].attributes.get("card_id")
            == Some(&SemanticScalarV1::Text("bash".to_owned())))
        })
        .unwrap();
    let prepared = adapter.prepare(&upgrade, &grid).unwrap();
    let after = adapter.execute(prepared).unwrap();
    assert_eq!(after.boundary_kind, "natural_event");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &after.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("result event must be observed")
    };
    assert_eq!(room["screen"], screen);
    assert_eq!(
        initial.domains[&ParityDomainV1::Run],
        after.domains[&ParityDomainV1::Run]
    );
    assert_eq!(
        initial.domains[&ParityDomainV1::RngStreams],
        after.domains[&ParityDomainV1::RngStreams]
    );
    assert_eq!(
        adapter
            .session()
            .run_state
            .event_state
            .as_ref()
            .unwrap()
            .current_screen,
        1
    );
    assert!(adapter
        .session()
        .run_state
        .master_deck
        .iter()
        .any(|card| card.id.stable_id() == "Bash" && card.upgrades == 1));
    let deck_after = &adapter.session().run_state.master_deck;
    assert_eq!(deck_before.len(), deck_after.len());
    for (before, after) in deck_before.iter().zip(deck_after) {
        assert_eq!(before.id, after.id);
        assert_eq!(before.lineage, after.lineage);
        assert_eq!(
            after.upgrades,
            before.upgrades + u8::from(before.id.stable_id() == "Bash")
        );
    }
    let leave = action_space(&after)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Event,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&leave, &after).unwrap();
    adapter.execute(prepared).unwrap();
    assert!(matches!(
        adapter.session().engine_state,
        EngineState::MapNavigation
    ));
}

#[test]
fn cleric_purify_uses_proceed_screen_during_and_after_bound_card_removal() {
    event_purge_keeps_result_screen(EventId::Cleric, 1, 99, 50);
}

#[test]
fn purifier_pray_keeps_complete_screen_during_and_after_bound_card_removal() {
    event_purge_keeps_result_screen(EventId::Purifier, 0, 1, 0);
}

fn event_purge_keeps_result_screen(event: EventId, occurrence: u32, screen: i64, price: i32) {
    let config = RunControlConfig {
        seed: 42,
        player_class: "Ironclad",
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    let (x, y) = session
        .run_state
        .map
        .graph
        .iter()
        .flatten()
        .find(|node| node.class.is_some() && !node.edges.is_empty())
        .map(|node| (node.x, node.y))
        .unwrap();
    session.run_state.map.current_x = x;
    session.run_state.map.current_y = y;
    session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::EventRoom);
    session.run_state.event_state = Some(EventState::new(event));
    session.engine_state = EngineState::EventRoom;
    let gold = session.run_state.gold;
    let deck_len = session.run_state.master_deck.len();
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test", "event-purge").unwrap();
    let initial = adapter.observe().unwrap();
    let purify = action_space(&initial).into_iter().find(|action| matches!(action,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, choice } if choice.occurrence == occurrence)).unwrap();
    let prepared = adapter.prepare(&purify, &initial).unwrap();
    let grid = adapter.execute(prepared).unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &grid.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("grid must be observed")
    };
    assert_eq!(room["screen"], screen);
    assert_eq!(room["selection_kind"], "deck_remove");
    assert_eq!(adapter.session().run_state.gold, gold - price);
    let remove = action_space(&grid).into_iter().find(|action| matches!(action,
        SemanticActionV1::SubmitSelection { choices, .. } if choices[0].attributes.get("card_id") == Some(&SemanticScalarV1::Text("bash".to_owned())))).unwrap();
    let prepared = adapter.prepare(&remove, &grid).unwrap();
    let after = adapter.execute(prepared).unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &after.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("event must be observed")
    };
    assert_eq!(room["screen"], screen);
    assert_eq!(adapter.session().run_state.master_deck.len(), deck_len - 1);
    assert!(!adapter
        .session()
        .run_state
        .master_deck
        .iter()
        .any(|card| card.id.stable_id() == "Bash"));
    assert_eq!(
        initial.domains[&ParityDomainV1::RngStreams],
        after.domains[&ParityDomainV1::RngStreams]
    );
    let leave = action_space(&after)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Event,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&leave, &after).unwrap();
    adapter.execute(prepared).unwrap();
    assert!(matches!(
        adapter.session().engine_state,
        EngineState::MapNavigation
    ));
}

#[test]
fn golden_wing_removal_preserves_damage_then_purge_then_map_boundaries() {
    let config = RunControlConfig {
        seed: 42,
        player_class: "Ironclad",
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    let (x, y) = session
        .run_state
        .map
        .graph
        .iter()
        .flatten()
        .find(|node| node.class.is_some() && !node.edges.is_empty())
        .map(|node| (node.x, node.y))
        .unwrap();
    session.run_state.map.current_x = x;
    session.run_state.map.current_y = y;
    session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::EventRoom);
    session.run_state.event_state = Some(EventState::new(EventId::GoldenWing));
    session.engine_state = EngineState::EventRoom;
    let hp = session.run_state.current_hp;
    let deck_len = session.run_state.master_deck.len();
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test", "golden-wing").unwrap();
    let mut snapshot = adapter.observe().unwrap();
    for screen in [1, 2] {
        let proceed = action_space(&snapshot)
            .into_iter()
            .find(|action| {
                matches!(action,
                SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, choice }
                    if choice.occurrence == 0)
            })
            .unwrap();
        let prepared = adapter.prepare(&proceed, &snapshot).unwrap();
        snapshot = adapter.execute(prepared).unwrap();
        let DomainEvidenceV1::Observed { value: room, .. } =
            &snapshot.domains[&ParityDomainV1::Room].evidence
        else {
            panic!("event boundary must be observed")
        };
        assert_eq!(room["screen"], screen);
        assert_eq!(adapter.session().run_state.current_hp, hp - 7);
        assert_eq!(adapter.session().run_state.master_deck.len(), deck_len);
        if screen == 2 {
            assert_eq!(room["selection_kind"], "deck_remove");
        }
    }
    let remove = action_space(&snapshot)
        .into_iter()
        .find(|action| matches!(action,
            SemanticActionV1::SubmitSelection { choices, .. }
                if choices[0].attributes.get("card_id") == Some(&SemanticScalarV1::Text("bash".to_owned()))))
        .unwrap();
    let prepared = adapter.prepare(&remove, &snapshot).unwrap();
    snapshot = adapter.execute(prepared).unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &snapshot.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("event boundary must be observed")
    };
    assert_eq!(room["screen"], 2);
    assert_eq!(adapter.session().run_state.current_hp, hp - 7);
    assert_eq!(adapter.session().run_state.master_deck.len(), deck_len - 1);
    assert!(!adapter
        .session()
        .run_state
        .master_deck
        .iter()
        .any(|card| card.id.stable_id() == "Bash"));
    let leave = action_space(&snapshot)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Event,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&leave, &snapshot).unwrap();
    adapter.execute(prepared).unwrap();
    assert!(matches!(
        adapter.session().engine_state,
        EngineState::MapNavigation
    ));
}

#[test]
fn rich_neow_rng_becomes_canonical_only_on_the_reward_page() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut adapter = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        "test-build",
        "rust-rich-neow-epoch",
    )
    .unwrap();
    let introduction = adapter.observe().unwrap();
    let DomainEvidenceV1::Observed { value, .. } =
        &introduction.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("Rich Neow introduction RNG streams should be observed")
    };
    assert!(value
        .as_array()
        .unwrap()
        .iter()
        .all(|stream| stream["owner"] != "run.neow"));

    let proceed = action_space(&introduction).remove(0);
    let prepared = adapter.prepare(&proceed, &introduction).unwrap();
    let reward = adapter.execute(prepared).unwrap();
    let DomainEvidenceV1::Observed { value, .. } =
        &reward.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("Rich Neow reward RNG streams should be observed")
    };
    assert!(value
        .as_array()
        .unwrap()
        .iter()
        .any(|stream| stream["owner"] == "run.neow"));
}

#[test]
fn neow_colorless_choice_opens_directly_and_returns_to_neow_on_pick_or_skip() {
    for skip in [false, true] {
        let config = RunControlConfig {
            seed: 2_910_982_430_734_805_206,
            ascension_level: 20,
            final_act: true,
            neow_eligibility: NeowEligibilityV1::Rich,
            ..Default::default()
        };
        let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut adapter = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap,
            "fixture",
            "neow-card-choice",
        )
        .unwrap();
        let mut snapshot = adapter.observe().unwrap();
        let proceed = action_space(&snapshot).remove(0);
        let prepared = adapter.prepare(&proceed, &snapshot).unwrap();
        snapshot = adapter.execute(prepared).unwrap();
        let reward = action_space(&snapshot).into_iter().find(|action| {
            matches!(action, SemanticActionV1::Select { choice, .. } if choice.id == "random_colorless_2")
        }).unwrap();
        let prepared = adapter.prepare(&reward, &snapshot).unwrap();
        snapshot = adapter.execute(prepared).unwrap();
        assert_eq!(snapshot.boundary_kind, "natural_neow_card_reward");
        assert_eq!(adapter.session().run_state.gold, 0);
        let choices = action_space(&snapshot);
        let offered = choices
            .iter()
            .filter_map(|action| match action {
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CardReward,
                    choice,
                } => Some(choice.id.as_str()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        // Captured Java offers at this natural seed, before any card pick.
        assert_eq!(
            offered,
            ["secrettechnique", "secretweapon", "transmutation"]
                .into_iter()
                .collect()
        );
        assert_eq!(
            choices
                .iter()
                .filter(|action| matches!(
                    action,
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::CardReward,
                        ..
                    }
                ))
                .count(),
            3
        );
        let choice = choices
            .into_iter()
            .find(|action| {
                if skip {
                    matches!(
                        action,
                        SemanticActionV1::Cancel {
                            surface: DecisionSurfaceV1::CardReward
                        }
                    )
                } else {
                    matches!(
                        action,
                        SemanticActionV1::Select {
                            surface: DecisionSurfaceV1::CardReward,
                            ..
                        }
                    )
                }
            })
            .unwrap();
        let prepared = adapter.prepare(&choice, &snapshot).unwrap();
        snapshot = adapter.execute(prepared).unwrap();
        assert_eq!(snapshot.boundary_kind, "natural_neow");
        assert_eq!(
            adapter.session().run_state.master_deck.len(),
            if skip { 11 } else { 12 }
        );
        if !skip {
            let DomainEvidenceV1::Observed { value, .. } =
                &snapshot.domains[&ParityDomainV1::CardBindings].evidence
            else {
                panic!("missing bindings")
            };
            assert!(value
                .as_array()
                .unwrap()
                .iter()
                .any(|binding| binding["source"]["kind"] == "neow_card_reward"));
        }
        let leave = action_space(&snapshot).remove(0);
        let prepared = adapter.prepare(&leave, &snapshot).unwrap();
        snapshot = adapter.execute(prepared).unwrap();
        assert_eq!(snapshot.boundary_kind, "natural_map");
        if !skip {
            let enter = action_space(&snapshot)
                .into_iter()
                .find(|action| {
                    matches!(action,
                SemanticActionV1::Select { surface: DecisionSurfaceV1::Map, choice }
                if choice.attributes.get("x") == Some(&SemanticScalarV1::Integer(2)))
                })
                .unwrap();
            let prepared = adapter.prepare(&enter, &snapshot).unwrap();
            snapshot = adapter.execute(prepared).unwrap();
            let play = action_space(&snapshot).into_iter().find(|action| matches!(action,
                SemanticActionV1::PlayCard {card_binding, ..} if card_binding == "natural_run/neow_card_reward/0")).unwrap();
            let prepared = adapter.prepare(&play, &snapshot).unwrap();
            snapshot = adapter.execute(prepared).unwrap();
            assert_eq!(snapshot.boundary_kind, "natural_combat_grid_select");
            let choices = action_space(&snapshot);
            assert_eq!(choices.len(), 2);
            assert!(choices.iter().all(|action| matches!(action,
                SemanticActionV1::SubmitSelection {choices, ..} if choices.len() == 1 && choices[0].kind == "draw_card")));
            let prepared = adapter.prepare(&choices[0], &snapshot).unwrap();
            snapshot = adapter.execute(prepared).unwrap();
            assert_eq!(snapshot.boundary_kind, "natural_combat");
            let combat = &adapter
                .session()
                .active_combat
                .as_ref()
                .unwrap()
                .combat_state;
            assert_eq!(combat.zones.hand.len(), 5);
            assert_eq!(combat.zones.draw_pile.len(), 6);
            assert_eq!(combat.zones.exhaust_pile.len(), 1);
        }
    }
}

#[test]
fn rich_neow_transform_preserves_the_selected_logical_card_binding() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut adapter = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        "test-build",
        "rust-neow-transform-epoch",
    )
    .unwrap();
    let introduction = adapter.observe().unwrap();
    let reward = {
        let action = action_space(&introduction).remove(0);
        let prepared = adapter.prepare(&action, &introduction).unwrap();
        adapter.execute(prepared).unwrap()
    };
    let transform = action_space(&reward)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select { choice, .. } if choice.id == "transform_card"
            )
        })
        .expect("seed 2026090301 should offer one-card transform");
    let selection = {
        let prepared = adapter.prepare(&transform, &reward).unwrap();
        adapter.execute(prepared).unwrap()
    };

    assert_eq!(selection.boundary_kind, "natural_neow_grid_select");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &selection.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("Neow grid room should be observed")
    };
    assert_eq!(room["kind"], "neow_selection");
    assert_eq!(room["current"], serde_json::Value::Null);
    assert_eq!(room["event_id"], "neow");
    assert_eq!(room["screen"], 2);
    assert_eq!(room["selection_kind"], "deck_transform");
    let DomainEvidenceV1::Observed { value: rng, .. } =
        &selection.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("Neow transform RNG should be observed")
    };
    assert!(rng
        .as_array()
        .unwrap()
        .iter()
        .any(|stream| stream["owner"] == "run.neow"));
    assert!(rng
        .as_array()
        .unwrap()
        .iter()
        .all(|stream| stream["scope"] != "floor"));

    let strike = action_space(&selection)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::SubmitSelection { choices, .. }
                    if choices[0].attributes.get("card_id")
                        == Some(&SemanticScalarV1::Text("strike".to_owned()))
            )
        })
        .expect("Neow transform should allow selecting a Strike");
    let selected_binding = match &strike {
        SemanticActionV1::SubmitSelection { choices, .. } => choices[0].id.clone(),
        _ => unreachable!("selected a grid action"),
    };
    let successor = {
        let prepared = adapter.prepare(&strike, &selection).unwrap();
        adapter.execute(prepared).unwrap()
    };
    let calls = &adapter
        .last_rng_call_ledger_v1()
        .expect("Neow transform should publish a transition RNG ledger")
        .0;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].owner, "run.neow");
    assert!(matches!(
        calls[0].operation,
        NaturalRunRngOperationV1::RandomInt { .. }
    ));

    let DomainEvidenceV1::Observed { value: deck, .. } =
        &successor.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("Neow transform successor deck should be observed")
    };
    let transformed = deck
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["binding"] == selected_binding)
        .expect("transformed card should retain the selected stable binding");
    assert_eq!(transformed["card_id"], "brutality");
    let DomainEvidenceV1::Observed {
        value: relations, ..
    } = &successor.domains[&ParityDomainV1::CardBindings].evidence
    else {
        panic!("Neow transform successor bindings should be observed")
    };
    assert!(relations.as_array().unwrap().iter().any(|relation| {
        relation["binding"] == selected_binding && relation["source"]["kind"] == "starter_deck"
    }));
}

#[test]
fn event_grants_retain_acquisition_witnesses_through_the_real_adapter() {
    for (event_id, key, screen, option, count) in [
        (EventId::Ghosts, "ghosts", 0, 0, 3),
        (EventId::DrugDealer, "drugdealer", 0, 0, 1),
        (EventId::Nest, "nest", 1, 1, 1),
    ] {
        let config = RunControlConfig {
            ascension_level: 20,
            ..Default::default()
        };
        let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut session = RunControlSession::new(config);
        let (x, y) = session
            .run_state
            .map
            .graph
            .iter()
            .flatten()
            .find(|node| node.class.is_some())
            .map(|node| (node.x, node.y))
            .unwrap();
        session.run_state.map.current_x = x;
        session.run_state.map.current_y = y;
        session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::EventRoom);
        let mut event = EventState::new(event_id);
        event.current_screen = screen;
        session.run_state.event_state = Some(event);
        session.engine_state = EngineState::EventRoom;
        let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "fixture", key).unwrap();
        let before = adapter.observe().unwrap();
        let accept=action_space(&before).into_iter().find(|action| matches!(action,SemanticActionV1::Select {choice,..} if choice.id==key && choice.occurrence==option)).unwrap();
        let prepared = adapter.prepare(&accept, &before).unwrap();
        let after = adapter.execute(prepared).unwrap();
        let DomainEvidenceV1::Observed { value, .. } =
            &after.domains[&ParityDomainV1::CardBindings].evidence
        else {
            panic!()
        };
        let sources = value
            .as_array()
            .unwrap()
            .iter()
            .filter(|bound| bound["source"]["kind"] == "event_grant")
            .collect::<Vec<_>>();
        assert_eq!(sources.len(), count);
        for slot in 0..count {
            assert!(sources
                .iter()
                .any(|bound| bound["source"]["event_id"] == key
                    && bound["source"]["product_slot"] == slot));
        }
    }
}

#[test]
fn library_offers_are_compared_and_selected_acquisitions_get_event_sources() {
    let config = RunControlConfig {
        seed: 1234,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    let (x, y) = session
        .run_state
        .map
        .graph
        .iter()
        .flatten()
        .find(|node| node.class.is_some())
        .map(|node| (node.x, node.y))
        .unwrap();
    session.run_state.map.current_x = x;
    session.run_state.map.current_y = y;
    session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::EventRoom);
    session.run_state.event_state = Some(EventState::new(EventId::TheLibrary));
    session.engine_state = EngineState::EventRoom;
    session.run_state.potions[0] = Some(Potion::new(PotionId::AncientPotion, 1));
    for option in 0..20 {
        let mut adapter =
            RustLockstepAdapterV1::new(session.clone(), bootstrap.clone(), "fixture", "library")
                .unwrap();
        let before = adapter.observe().unwrap();
        let read=action_space(&before).into_iter().find(|action| matches!(action,SemanticActionV1::Select {choice,..} if choice.id=="thelibrary" && choice.occurrence==0)).unwrap();
        let prepared = adapter.prepare(&read, &before).unwrap();
        let selection = adapter.execute(prepared).unwrap();
        assert_eq!(selection.boundary_kind, "natural_grid_select");
        let calls = &adapter.last_rng_call_ledger_v1().unwrap().0;
        assert!(calls.len() >= 40 && calls.len().is_multiple_of(2));
        for pair in calls.chunks_exact(2) {
            assert_eq!(pair[0].owner, "run.card");
            assert_eq!(pair[1].owner, "run.card");
            assert!(matches!(
                pair[0].operation,
                NaturalRunRngOperationV1::RandomInt { range: 99, .. }
            ));
            assert!(matches!(
                pair[1].operation,
                NaturalRunRngOperationV1::RandomInt { .. }
            ));
        }
        let actions = action_space(&selection);
        assert_eq!(actions.len(), 21);
        assert!(actions
            .iter()
            .any(|action| matches!(action, SemanticActionV1::DiscardPotion { .. })));
        let selected=actions.into_iter().find(|action| matches!(action,SemanticActionV1::SubmitSelection {choices,..} if choices[0].occurrence==option)).unwrap();
        let SemanticActionV1::SubmitSelection { choices, .. } = &selected else {
            unreachable!()
        };
        let card_id = choices[0].attributes["card_id"].clone();
        let before_count = adapter.session().run_state.master_deck.len();
        let prepared = adapter.prepare(&selected, &selection).unwrap();
        let after = adapter.execute(prepared).unwrap();
        assert_eq!(after.boundary_kind, "natural_event");
        assert_eq!(
            adapter.session().run_state.master_deck.len(),
            before_count + 1
        );
        let DomainEvidenceV1::Observed { value: room, .. } =
            &after.domains[&ParityDomainV1::Room].evidence
        else {
            panic!()
        };
        assert_eq!(room["screen"], 1);
        let DomainEvidenceV1::Observed { value: deck, .. } =
            &after.domains[&ParityDomainV1::MasterDeck].evidence
        else {
            panic!()
        };
        let acquired = deck
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["binding"].as_str().unwrap().contains("/event_card/"))
            .unwrap();
        assert_eq!(
            Some(SemanticScalarV1::Text(
                acquired["card_id"].as_str().unwrap().into()
            )),
            Some(card_id)
        );
        let DomainEvidenceV1::Observed {
            value: bindings, ..
        } = &after.domains[&ParityDomainV1::CardBindings].evidence
        else {
            panic!()
        };
        assert!(bindings
            .as_array()
            .unwrap()
            .iter()
            .any(|bound| bound["source"]["kind"] == "event_card"
                && bound["source"]["offer_index"] == option));
    }
}

#[test]
fn bonfire_grid_selection_exposes_one_source_bound_action_per_legal_card() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    let (x, y) = session
        .run_state
        .map
        .graph
        .iter()
        .flatten()
        .find(|node| node.class.is_some())
        .map(|node| (node.x, node.y))
        .unwrap();
    session.run_state.map.current_x = x;
    session.run_state.map.current_y = y;
    session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::EventRoom);
    let mut event = EventState::new(EventId::BonfireElementals);
    event.current_screen = 2;
    session.run_state.event_state = Some(event);
    session.run_state.potions[0] = Some(Potion::new(PotionId::PowerPotion, 41));
    session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason: RunPendingChoiceReason::PurgeNonBottled,
        source: DomainEventSource::Event(EventId::BonfireElementals),
        return_state: Box::new(EngineState::EventRoom),
    });
    let removable_cards = match &session.engine_state {
        EngineState::RunPendingChoice(choice) => {
            choice.selection_request(&session.run_state).targets.len()
        }
        _ => unreachable!("test constructed a run pending choice"),
    };
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test-build", "bonfire-grid-epoch").unwrap();

    let selection = adapter.observe().unwrap();
    assert_eq!(selection.boundary_kind, "natural_grid_select");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &selection.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("Bonfire grid room should be observed")
    };
    assert_eq!(room["event_id"], "bonfireelementals");
    assert_eq!(room["screen"], 1);
    assert_eq!(room["selection_kind"], "deck_remove");
    let actions = action_space(&selection);
    assert!(!actions.is_empty());
    assert!(actions.iter().any(|action| matches!(
        action,
        SemanticActionV1::DiscardPotion {
            slot: 0,
            potion_id,
        } if potion_id == "powerpotion"
    )));
    assert!(
        actions
            .iter()
            .filter(|action| matches!(
                action,
                SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::GridSelect,
                    choices,
                } if matches!(choices.as_slice(), [choice]
                    if choice.kind == "deck_card"
                        && choice.id.starts_with("natural_run/")
                        && choice.attributes.contains_key("card_id")
                        && choice.attributes.contains_key("upgrades"))
            ))
            .count()
            == removable_cards
    );

    let strike = actions
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::SubmitSelection { choices, .. }
                    if choices[0].attributes.get("card_id")
                        == Some(&SemanticScalarV1::Text("strike".to_owned()))
            )
        })
        .expect("starter deck should expose a removable Strike");
    let removed_binding = match &strike {
        SemanticActionV1::SubmitSelection { choices, .. } => choices[0].id.clone(),
        _ => unreachable!(),
    };
    let prepared = adapter.prepare(&strike, &selection).unwrap();
    let complete = adapter.execute(prepared).unwrap();
    assert_eq!(complete.boundary_kind, "natural_event");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &complete.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("Bonfire complete room should be observed")
    };
    assert_eq!(room["screen"], 2);
    let DomainEvidenceV1::Observed { value: deck, .. } =
        &complete.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("Bonfire successor deck should be observed")
    };
    assert!(deck
        .as_array()
        .unwrap()
        .iter()
        .all(|card| card["binding"].as_str() != Some(removed_binding.as_str())));
}

#[test]
fn campfire_smith_is_two_shared_choices_and_upgrades_the_bound_card() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    let (x, y) = session
        .run_state
        .map
        .graph
        .iter()
        .flatten()
        .find(|node| node.class.is_some() && !node.edges.is_empty())
        .map(|node| (node.x, node.y))
        .unwrap();
    session.run_state.map.current_x = x;
    session.run_state.map.current_y = y;
    session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::RestRoom);
    session.run_state.event_state = None;
    session.engine_state = EngineState::Campfire;
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test-build", "campfire-smith-epoch")
            .unwrap();

    let campfire = adapter.observe().unwrap();
    assert_eq!(campfire.boundary_kind, "natural_campfire");
    let smith = action_space(&campfire)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Campfire,
                    choice,
                } if choice.kind == "campfire_option" && choice.id == "smith"
            )
        })
        .expect("starter deck should allow Smith");
    let prepared = adapter.prepare(&smith, &campfire).unwrap();
    let selection = adapter.execute(prepared).unwrap();

    assert_eq!(selection.boundary_kind, "natural_campfire_smith");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &selection.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("Smith selection room should be observed")
    };
    assert_eq!(room["kind"], "campfire_smith");
    let strike = action_space(&selection)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Campfire,
                    choice,
                } if choice.kind == "deck_card"
                    && choice.id == "natural_run/starter/0"
                    && choice.attributes.get("card_id")
                        == Some(&SemanticScalarV1::Text("strike".to_owned()))
            )
        })
        .expect("Smith should expose the source-bound first Strike");
    let prepared = adapter.prepare(&strike, &selection).unwrap();
    let map = adapter.execute(prepared).unwrap();

    assert_eq!(map.boundary_kind, "natural_map");
    let DomainEvidenceV1::Observed { value: deck, .. } =
        &map.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("Smith successor deck should be observed")
    };
    let upgraded = deck
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["binding"] == "natural_run/starter/0")
        .unwrap();
    assert_eq!(upgraded["upgrades"], 1);
}

#[test]
fn rich_neow_curse_drawback_adds_one_source_bound_injury() {
    let config = RunControlConfig {
        seed: 2_026_090_302,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut adapter = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        "test-build",
        "rust-neow-curse-epoch",
    )
    .unwrap();
    let introduction = adapter.observe().unwrap();
    let proceed = action_space(&introduction).remove(0);
    let prepared = adapter.prepare(&proceed, &introduction).unwrap();
    let reward = adapter.execute(prepared).unwrap();
    let curse = action_space(&reward)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select { choice, .. }
                    if choice.id == "one_rare_relic"
                        && choice.attributes.get("drawback")
                            == Some(&SemanticScalarV1::Text("curse".to_owned()))
            )
        })
        .expect("seed 2026090302 should offer one rare relic for a curse");
    let prepared = adapter.prepare(&curse, &reward).unwrap();
    let successor = adapter.execute(prepared).unwrap();

    let binding = "natural_run/neow_drawback/one_rare_relic/curse/0";
    let DomainEvidenceV1::Observed { value: deck, .. } =
        &successor.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("Neow successor master deck should remain observed")
    };
    let injury = deck
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["binding"] == binding)
        .expect("Neow curse should have a stable master-deck binding");
    assert_eq!(injury["card_id"], "injury");
    assert_eq!(injury["upgrades"], 0);

    let DomainEvidenceV1::Observed {
        value: relations, ..
    } = &successor.domains[&ParityDomainV1::CardBindings].evidence
    else {
        panic!("Neow successor card bindings should remain observed")
    };
    let relation = relations
        .as_array()
        .unwrap()
        .iter()
        .find(|relation| relation["binding"] == binding)
        .expect("Neow curse should have typed provenance");
    assert_eq!(relation["source"]["kind"], "neow_drawback");
    assert_eq!(relation["source"]["reward_type"], "one_rare_relic");
    assert_eq!(relation["source"]["drawback"], "curse");
    assert_eq!(relation["source"]["product_slot"], 0);
}

#[test]
fn rich_neow_introduction_rejects_a_preinitialized_rng() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.neow_rng = Some(StsRng::new(session.run_state.seed));
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test-build", "too-early-rng").unwrap();

    let error = adapter.observe().unwrap_err();
    assert!(error.message.contains("initialized its RNG too early"));
}

#[test]
fn rich_neow_reward_page_rejects_a_missing_rng() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    sts_oracle_run_control::content::events::neow::handle_choice(
        &mut session.engine_state,
        &mut session.run_state,
        0,
    );
    session.run_state.neow_rng = None;
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test-build", "missing-active-rng").unwrap();

    let error = adapter.observe().unwrap_err();
    assert!(error.message.contains("reward page omitted its RNG"));
}

#[test]
fn recorded_bootstrap_reconstructs_the_same_rust_run_recipe() {
    let original = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&original).unwrap();

    let reconstructed = run_control_config_from_bootstrap_v1(&bootstrap).unwrap();

    assert_eq!(natural_run_bootstrap_v1(&reconstructed).unwrap(), bootstrap);
    assert_eq!(reconstructed.neow_eligibility, NeowEligibilityV1::Mini);
}

#[test]
fn repeated_starter_cards_have_distinct_source_bindings() {
    let mut adapter = mini_neow_adapter();
    let snapshot = adapter.observe().unwrap();
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("starter deck should be observed")
    };
    let deck = value.as_array().unwrap();
    let defend_bindings = deck
        .iter()
        .filter(|card| card["card_id"] == "defend")
        .map(|card| card["binding"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(defend_bindings.len(), 4);
    assert!(defend_bindings
        .iter()
        .all(|binding| binding.starts_with("natural_run/starter/")));
}

#[test]
fn bootstrap_binding_uses_source_lineage_instead_of_current_deck_position() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.master_deck.swap(1, 10);
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test-build", "reordered").unwrap();
    let snapshot = adapter.observe().unwrap();
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("reordered deck should remain source-bound")
    };

    assert_eq!(value[1]["card_id"], "bash");
    assert_eq!(value[1]["binding"], "natural_run/starter/9");
    assert_eq!(value[10]["card_id"], "strike");
    assert_eq!(value[10]["binding"], "natural_run/starter/0");
}

#[test]
fn bootstrap_rejects_duplicate_rust_lineage_even_for_identical_cards() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.master_deck[7].lineage = session.run_state.master_deck[6].lineage;

    let error = RustLockstepAdapterV1::new(session, bootstrap, "test-build", "duplicate")
        .err()
        .expect("duplicate lineage must be rejected");
    assert!(error.contains("repeats one lineage"));
}

#[test]
fn prepared_neow_action_rejects_a_different_observation() {
    let mut adapter = mini_neow_adapter();
    let snapshot = adapter.observe().unwrap();
    let action = action_space(&snapshot).into_iter().next().unwrap();
    let mut foreign = snapshot.clone();
    foreign.observation_id.push_str("-foreign");

    let error = adapter.prepare(&action, &foreign).unwrap_err();
    assert!(error.message.contains("stale or foreign"));
}

#[test]
fn prepared_successor_keeps_live_state_private_and_commits_without_more_rng_calls() {
    use sts_oracle_run_control::{
        eval::run_control::RunControlSessionCheckpointV1, runtime::rng::begin_sts_rng_trace,
    };

    let mut adapter = mini_neow_adapter();
    let before = advance_to_map(&mut adapter);
    let action = action_space(&before).into_iter().next().unwrap();
    let checkpoint = serde_json::to_value(RunControlSessionCheckpointV1::from_session(
        adapter.session(),
    ))
    .unwrap();
    let prior_ledger = adapter.last_rng_call_ledger_v1().cloned();
    let discarded = adapter.prepare(&action, &before).unwrap();
    let prepared = adapter.prepare(&action, &before).unwrap();
    drop(discarded);
    assert_eq!(adapter.observe().unwrap(), before);
    assert_eq!(adapter.last_rng_call_ledger_v1(), prior_ledger.as_ref());
    assert_eq!(
        serde_json::to_value(RunControlSessionCheckpointV1::from_session(
            adapter.session()
        ))
        .unwrap(),
        checkpoint
    );

    // A commit publishes the already verified transition. It must not draw
    // randomness again, even though this map-to-combat transition uses RNG.
    let trace = begin_sts_rng_trace().unwrap();
    let after = adapter.execute(prepared).unwrap();
    assert!(trace.finish().unwrap().is_empty());
    assert!(!adapter.last_rng_call_ledger_v1().unwrap().0.is_empty());
    assert_eq!(after.boundary_kind, "natural_combat");
    assert_eq!(adapter.observe().unwrap(), after);
}

#[test]
fn prepared_successor_is_bound_to_its_exact_predecessor_generation() {
    let mut adapter = mini_neow_adapter();
    let before = adapter.observe().unwrap();
    let action = action_space(&before).into_iter().next().unwrap();
    let prepared = adapter.prepare(&action, &before).unwrap();
    let mut foreign = mini_neow_adapter();
    assert_eq!(foreign.observe().unwrap(), before);
    assert!(foreign
        .execute(prepared.clone())
        .unwrap_err()
        .message
        .contains("stale"));
    assert_eq!(foreign.observe().unwrap(), before);
    assert!(foreign.last_rng_call_ledger_v1().is_none());

    // An unchanged in-memory fork shares the predecessor. Once either branch
    // commits, a prepared action from that predecessor becomes stale there.
    let mut fork = adapter.clone();
    let independent_prepared = adapter.prepare(&action, &before).unwrap();
    let after = fork.execute(prepared.clone()).unwrap();
    assert!(fork.execute(prepared.clone()).is_err());
    assert_eq!(fork.observe().unwrap(), after);
    assert_eq!(adapter.execute(independent_prepared).unwrap(), after);

    // Two branches can reach equal public observations without sharing the
    // same prepared transaction. Labels and action positions are insufficient.
    let next = action_space(&after).into_iter().next().unwrap();
    let foreign_prepared = adapter.prepare(&next, &after).unwrap();
    let prior_ledger = fork.last_rng_call_ledger_v1().cloned();
    assert!(fork.execute(foreign_prepared).is_err());
    assert_eq!(fork.observe().unwrap(), after);
    assert_eq!(fork.last_rng_call_ledger_v1(), prior_ledger.as_ref());
}

#[test]
fn mini_neow_can_advance_to_map_through_shared_semantic_actions() {
    let mut adapter = mini_neow_adapter();
    let map = advance_to_map(&mut adapter);

    assert_eq!(map.boundary_kind, "natural_map");
    assert!(action_space(&map).iter().all(|action| matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            ..
        }
    )));
    let DomainEvidenceV1::Observed { value, .. } = &map.domains[&ParityDomainV1::Map].evidence
    else {
        panic!("map should be observed")
    };
    let nodes = value["nodes"].as_array().unwrap();
    assert!(nodes.len() > 20);
    assert!(nodes.iter().all(|node| node["id"]
        .as_str()
        .is_some_and(|id| id.starts_with('F') && id.contains('X'))));
    assert!(nodes
        .iter()
        .flat_map(|node| node["links"].as_array().unwrap())
        .all(|link| link.as_str().is_some_and(|id| id.starts_with('F'))));
    let DomainEvidenceV1::Observed { value, .. } =
        &map.domains[&ParityDomainV1::RngStreams].evidence
    else {
        panic!("natural map RNG streams should be observed")
    };
    let streams = value.as_array().unwrap();
    assert_eq!(streams.len(), 8);
    assert!(streams.iter().all(|stream| stream["scope"] != "floor"));
    assert!(streams.iter().all(|stream| stream["owner"] != "run.neow"));
}

#[test]
fn ordinary_treasure_has_a_typed_chest_and_distinct_reward_surface() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.engine_state = EngineState::MapNavigation;
    session.run_state.event_state = None;
    for node in &mut session.run_state.map.graph[0] {
        if !node.edges.is_empty() {
            node.class = Some(RoomType::TreasureRoom);
        }
    }
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test-build", "rust-treasure-epoch")
            .unwrap();

    let map = adapter.observe().unwrap();
    let route = action_space(&map)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select { choice, .. }
                    if choice.attributes.get("room")
                        == Some(&SemanticScalarV1::Text("treasure".to_owned()))
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let treasure = adapter.execute(prepared).unwrap();
    assert_eq!(treasure.boundary_kind, "natural_treasure");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &treasure.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("treasure room should be observed")
    };
    assert_eq!(room["kind"], "treasure");
    assert!(matches!(
        room["chest"]["size"].as_str(),
        Some("small" | "medium" | "large")
    ));
    assert!(matches!(
        room["chest"]["relic_tier"].as_str(),
        Some("common" | "uncommon" | "rare")
    ));
    let actions = action_space(&treasure);
    assert_eq!(actions.len(), 2);
    let open = actions.iter().find(|action| matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Treasure,
            choice,
        } if choice.kind == "treasure_chest" && choice.id == "open" && choice.occurrence == 0
    )).unwrap();
    let leave = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::Treasure,
    };
    assert!(actions.contains(&leave));
    let mut leave_unopened = adapter.clone();
    let prepared = leave_unopened.prepare(&leave, &treasure).unwrap();
    let map = leave_unopened.execute(prepared).unwrap();
    assert_eq!(map.boundary_kind, "natural_map");
    for domain in [
        ParityDomainV1::Run,
        ParityDomainV1::MasterDeck,
        ParityDomainV1::Relics,
        ParityDomainV1::Potions,
        ParityDomainV1::Rewards,
    ] {
        assert_eq!(
            map.domains[&domain], treasure.domains[&domain],
            "{domain:?}"
        );
    }
    // Map navigation retires floor-local RNG from the active projection.
    let run_streams = |snapshot: &sts_parity_lockstep::ParitySnapshotV1| {
        let DomainEvidenceV1::Observed { value, .. } =
            &snapshot.domains[&ParityDomainV1::RngStreams].evidence
        else {
            panic!("RNG streams should be observed")
        };
        value
            .as_array()
            .unwrap()
            .iter()
            .filter(|stream| stream["scope"] == "run")
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(run_streams(&map), run_streams(&treasure));
    assert!(leave_unopened
        .last_rng_call_ledger_v1()
        .unwrap()
        .0
        .is_empty());

    let prepared = adapter.prepare(open, &treasure).unwrap();
    let reward = adapter.execute(prepared).unwrap();
    assert_eq!(reward.boundary_kind, "natural_treasure_reward");
    let DomainEvidenceV1::Observed { value: rewards, .. } =
        &reward.domains[&ParityDomainV1::Rewards].evidence
    else {
        panic!("treasure rewards should be observed")
    };
    assert_eq!(rewards["status"], "treasure");
    assert!(action_space(&reward).iter().all(|action| matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::TreasureReward,
            ..
        } | SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::TreasureReward,
        }
    )));
}

#[test]
fn liquid_memories_retrieves_each_bound_discard_card_at_zero_cost() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.potions[0] = Some(Potion::new(PotionId::LiquidMemories, 41));
    session.run_state.potions[1] = Some(Potion::new(PotionId::LiquidBronze, 42));
    let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "test", "memories").unwrap();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let combat = adapter.execute(prepared).unwrap();
    let prepared = adapter
        .prepare(&SemanticActionV1::EndTurn, &combat)
        .unwrap();
    let combat = adapter.execute(prepared).unwrap();
    let discarded = adapter
        .session()
        .active_combat
        .as_ref()
        .unwrap()
        .combat_state
        .zones
        .discard_pile
        .len();
    assert!(discarded > 1);
    let potion = action_space(&combat).into_iter().find(|action|
        matches!(action, SemanticActionV1::UsePotion {potion_id, ..} if potion_id == "liquidmemories")).unwrap();
    let prepared = adapter.prepare(&potion, &combat).unwrap();
    let grid = adapter.execute(prepared).unwrap();
    assert_eq!(grid.boundary_kind, "natural_combat_grid_select");
    assert_eq!(
        grid.domains[&ParityDomainV1::CardBindings],
        combat.domains[&ParityDomainV1::CardBindings]
    );
    adapter.validate_combat_draft_root_v1().unwrap();
    let choices = action_space(&grid);
    assert_eq!(choices.len(), discarded + 1);
    assert!(!choices
        .iter()
        .any(|action| matches!(action, SemanticActionV1::UsePotion { .. })));
    for action in choices {
        let mut branch = adapter.clone();
        let prepared = branch.prepare(&action, &grid).unwrap();
        let after = branch.execute(prepared).unwrap();
        let SemanticActionV1::SubmitSelection { choices, .. } = action else {
            assert_eq!(after.boundary_kind, "natural_combat_grid_select");
            assert_eq!(
                after.domains[&ParityDomainV1::Room],
                grid.domains[&ParityDomainV1::Room]
            );
            continue;
        };
        assert_eq!(choices[0].kind, "discard_card");
        assert_eq!(after.boundary_kind, "natural_combat");
        assert_eq!(
            after.domains[&ParityDomainV1::CardBindings],
            grid.domains[&ParityDomainV1::CardBindings]
        );
        let state = &branch
            .session()
            .active_combat
            .as_ref()
            .unwrap()
            .combat_state;
        assert_eq!(state.zones.discard_pile.len(), discarded - 1);
        assert!(state.entities.potions[0].is_none());
        assert!(state.entities.potions[1].is_some());
        let DomainEvidenceV1::Observed { value, .. } =
            &after.domains[&ParityDomainV1::Combat].evidence
        else {
            panic!("combat missing")
        };
        assert_eq!(
            value["state"]["zones"]["hand"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["cost_for_turn"],
            0
        );
        assert!(action_space(&after).iter().any(|action|
            matches!(action, SemanticActionV1::PlayCard {card_binding, ..} if *card_binding == choices[0].id)));
    }
}

#[test]
fn power_potion_discovery_compares_the_choice_and_binds_each_generated_product() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.potions[0] = Some(Potion::new(PotionId::PowerPotion, 41));
    session.run_state.potions[1] = Some(Potion::new(PotionId::LiquidBronze, 42));
    let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "test", "discovery").unwrap();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let combat = adapter.execute(prepared).unwrap();
    let use_potion = action_space(&combat)
        .into_iter()
        .find(|action| {
            matches!(action,
        SemanticActionV1::UsePotion { potion_id, .. } if potion_id == "powerpotion")
        })
        .unwrap();
    let prepared = adapter.prepare(&use_potion, &combat).unwrap();
    let grid = adapter.execute(prepared).unwrap();
    assert_eq!(grid.boundary_kind, "natural_combat_discovery");
    assert!(action_space(&grid).iter().any(|action|
        matches!(action, SemanticActionV1::DiscardPotion { slot: 1, potion_id } if potion_id == "liquidbronze")));
    assert!(!action_space(&grid)
        .iter()
        .any(|action| matches!(action, SemanticActionV1::UsePotion { .. })));
    adapter.validate_combat_draft_root_v1().unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &grid.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("Discovery choice must be observed")
    };
    assert_eq!(room["selection"]["amount"], 1);
    assert_eq!(room["selection"]["card_type"], "power");
    assert_eq!(room["selection"]["cards"].as_array().unwrap().len(), 3);
    let DomainEvidenceV1::Observed {
        value: current_combat,
        ..
    } = &grid.domains[&ParityDomainV1::Combat].evidence
    else {
        panic!("suspended combat must be observed")
    };
    assert_eq!(current_combat["status"], "player_decision");
    let hand_len = adapter
        .session()
        .active_combat
        .as_ref()
        .unwrap()
        .combat_state
        .zones
        .hand
        .len();
    for action in action_space(&grid) {
        let mut branch = adapter.clone();
        let prepared = branch.prepare(&action, &grid).unwrap();
        let after = branch.execute(prepared).unwrap();
        if matches!(action, SemanticActionV1::DiscardPotion { .. }) {
            assert_eq!(after.boundary_kind, "natural_combat_discovery");
            assert_eq!(
                after.domains[&ParityDomainV1::Room],
                grid.domains[&ParityDomainV1::Room]
            );
            assert_eq!(
                after.domains[&ParityDomainV1::CardBindings],
                grid.domains[&ParityDomainV1::CardBindings]
            );
            assert!(branch
                .session()
                .active_combat
                .as_ref()
                .unwrap()
                .combat_state
                .entities
                .potions[1]
                .is_none());
            continue;
        }
        assert_eq!(after.boundary_kind, "natural_combat");
        assert!(branch
            .session()
            .active_combat
            .as_ref()
            .unwrap()
            .combat_state
            .entities
            .potions[1]
            .is_some());
        if matches!(action, SemanticActionV1::Cancel { .. }) {
            let mut line = vec![action.clone()];
            line.extend(
                branch
                    .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
                        max_nodes: Some(2_000),
                        wall_ms: Some(5_000),
                        ..Default::default()
                    })
                    .unwrap(),
            );
            let verified = adapter.verify_semantic_combat_victory_v1(&line).unwrap();
            assert_eq!(verified.boundary_kind, "natural_combat_reward");
            assert_eq!(
                adapter.observe().unwrap(),
                grid,
                "private choice draft must not mutate its root"
            );
        }
        let hand = &branch
            .session()
            .active_combat
            .as_ref()
            .unwrap()
            .combat_state
            .zones
            .hand;
        match action {
            SemanticActionV1::Select { choice, .. } => {
                assert_eq!(hand.len(), hand_len + 1);
                let card = hand.last().unwrap();
                assert_eq!(
                    sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                    choice.id
                );
                let DomainEvidenceV1::Observed {
                    value: relations, ..
                } = &after.domains[&ParityDomainV1::CardBindings].evidence
                else {
                    panic!("generated card must be source bound")
                };
                assert!(relations
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|relation| relation["source"]["cause"]["kind"] == "discovery_resolution"));
                let DomainEvidenceV1::Observed { value: combat, .. } =
                    &after.domains[&ParityDomainV1::Combat].evidence
                else {
                    unreachable!()
                };
                assert_eq!(
                    combat["state"]["zones"]["hand"]
                        .as_array()
                        .unwrap()
                        .last()
                        .unwrap()["cost_for_turn"],
                    0
                );
            }
            SemanticActionV1::Cancel { .. } => assert_eq!(hand.len(), hand_len),
            _ => panic!("unexpected Discovery action"),
        }
    }
}

#[test]
fn codex_selection_preserves_combat_and_binds_random_draw_copy_without_free_cost() {
    use sts_oracle_run_control::content::relics::{RelicId, RelicState};
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Silent",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::NilrysCodex));
    session.run_state.potions[0] = Some(Potion::new(PotionId::LiquidBronze, 42));
    let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "test", "codex").unwrap();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map)
        .into_iter()
        .find(|a| {
            matches!(
                a,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let combat = adapter.execute(prepared).unwrap();
    let prepared = adapter
        .prepare(&SemanticActionV1::EndTurn, &combat)
        .unwrap();
    let offers = adapter.execute(prepared).unwrap();
    assert_eq!(offers.boundary_kind, "natural_combat_codex");
    adapter.validate_combat_draft_root_v1().unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &offers.domains[&ParityDomainV1::Room].evidence
    else {
        panic!()
    };
    assert_eq!(room["kind"], "combat_codex");
    assert_eq!(room["selection"]["cards"].as_array().unwrap().len(), 3);
    assert!(room["selection"].get("cost_for_turn").is_none());
    let DomainEvidenceV1::Observed { value: before, .. } =
        &offers.domains[&ParityDomainV1::Combat].evidence
    else {
        panic!()
    };
    assert_eq!(before["status"], "player_decision");
    for action in action_space(&offers) {
        let mut branch = adapter.clone();
        let prepared = branch.prepare(&action, &offers).unwrap();
        let after = branch.execute(prepared).unwrap();
        if matches!(action, SemanticActionV1::DiscardPotion { .. }) {
            assert_eq!(after.boundary_kind, "natural_combat_codex");
            assert_eq!(
                after.domains[&ParityDomainV1::Room],
                offers.domains[&ParityDomainV1::Room]
            );
            assert_eq!(
                after.domains[&ParityDomainV1::CardBindings],
                offers.domains[&ParityDomainV1::CardBindings]
            );
            continue;
        }
        assert_eq!(after.boundary_kind, "natural_combat");
        assert_eq!(
            after.domains[&ParityDomainV1::MasterDeck],
            offers.domains[&ParityDomainV1::MasterDeck]
        );
        let DomainEvidenceV1::Observed {
            value: bindings, ..
        } = &after.domains[&ParityDomainV1::CardBindings].evidence
        else {
            panic!()
        };
        let products = bindings
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["source"]["cause"]["kind"] == "combat_selection_resolution")
            .collect::<Vec<_>>();
        match action {
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::CombatCodex,
                choice,
            } => {
                assert_eq!(choice.kind, "codex_card");
                assert_eq!(products.len(), 1);
                let DomainEvidenceV1::Observed { value: state, .. } =
                    &after.domains[&ParityDomainV1::Combat].evidence
                else {
                    panic!()
                };
                let card = [
                    "hand",
                    "draw_next_first",
                    "discard_bottom_first",
                    "exhaust_by_binding",
                    "limbo",
                ]
                .into_iter()
                .flat_map(|zone| state["state"]["zones"][zone].as_array().unwrap())
                .find(|c| c["binding"] == products[0]["binding"])
                .unwrap();
                assert_eq!(card["card_id"], choice.id);
                assert_eq!(card["cost_for_turn"], card["combat_cost"]);
            }
            SemanticActionV1::Cancel {
                surface: DecisionSurfaceV1::CombatCodex,
            } => assert!(products.is_empty()),
            _ => panic!("unexpected Codex action"),
        }
    }
}

#[test]
fn empty_cage_selects_two_source_bound_cards_before_entering_the_next_act() {
    use sts_oracle_run_control::content::relics::RelicId;
    use sts_oracle_run_control::state::rewards::BossRelicChoiceState;
    let config = RunControlConfig {
        seed: 2026102001,
        ascension_level: 20,
        final_act: true,
        player_class: "Silent",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 16;
    session.run_state.floor_num = 17;
    session.run_state.event_state = None;
    session.engine_state =
        EngineState::BossRelicSelect(BossRelicChoiceState::new(vec![RelicId::EmptyCage]));
    let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "test", "empty-cage").unwrap();
    let before = adapter.observe().unwrap();
    let take = action_space(&before)
        .into_iter()
        .find(|a| matches!(a, SemanticActionV1::Select {choice,..} if choice.id=="emptycage"))
        .unwrap();
    let prepared = adapter.prepare(&take, &before).unwrap();
    let selection = adapter.execute(prepared).unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &selection.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("missing room")
    };
    assert_eq!(room["relic_id"], "emptycage");
    assert_eq!(room["requested_cards"], 2);
    assert_eq!(adapter.session().run_state.act_num, 1);
    let actions = action_space(&selection);
    assert!(actions.iter().all(|a| matches!(a,SemanticActionV1::SubmitSelection {choices,..} if choices.len()==2 && choices[0].id!=choices[1].id)));
    let pair = actions.iter().find(|a| matches!(a,SemanticActionV1::SubmitSelection {choices,..} if choices.iter().all(|c| c.attributes["card_id"] == SemanticScalarV1::Text("strikeg".into())))).unwrap();
    let old_deck = adapter.session().run_state.master_deck.len();
    let prepared = adapter.prepare(pair, &selection).unwrap();
    let after = adapter.execute(prepared).unwrap();
    assert_eq!(after.boundary_kind, "natural_map");
    assert_eq!(adapter.session().run_state.act_num, 2);
    assert_eq!(adapter.session().run_state.master_deck.len(), old_deck - 2);
}

#[test]
fn boss_relic_choice_and_skip_reach_the_next_act_entrance() {
    use sts_oracle_run_control::content::potions::{Potion, PotionId};
    use sts_oracle_run_control::content::relics::RelicId;
    use sts_oracle_run_control::state::rewards::BossRelicChoiceState;
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.floor_num = 17;
    session.run_state.current_hp = 15;
    session.run_state.event_state = None;
    session.run_state.potions = vec![
        Some(Potion::new(PotionId::LiquidBronze, 81)),
        Some(Potion::new(PotionId::WeakenPotion, 82)),
    ];
    session.engine_state = EngineState::BossRelicSelect(BossRelicChoiceState::new(vec![
        RelicId::PandorasBox,
        RelicId::BlackBlood,
        RelicId::SacredBark,
    ]));
    let mut adapter = RustLockstepAdapterV1::new(session, bootstrap, "test", "boss-relic").unwrap();
    let before = adapter.observe().unwrap();
    assert_eq!(before.boundary_kind, "natural_boss_relic");
    let DomainEvidenceV1::Observed { value: run, .. } =
        &before.domains[&ParityDomainV1::Run].evidence
    else {
        panic!("boss relic choice must observe run state");
    };
    assert!(run["event_schedule"]["shrines"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(
        !adapter
            .session()
            .run_state
            .event_generator
            .shrine_pool
            .is_empty(),
        "projection must not mutate the simulator's pool"
    );
    let actions = action_space(&before);
    assert_eq!(actions.len(), 6);
    for slot in 0..2 {
        let discard = actions.iter().find(|action| {
            matches!(action, SemanticActionV1::DiscardPotion { slot: actual, .. } if *actual == slot)
        }).unwrap();
        let mut branch = adapter.clone();
        let prepared = branch.prepare(discard, &before).unwrap();
        let after = branch.execute(prepared).unwrap();
        assert_eq!(after.boundary_kind, "natural_boss_relic");
        assert_eq!(branch.session().run_state.act_num, 1);
        assert!(branch.session().run_state.potions[slot as usize].is_none());
        assert!(branch.session().run_state.potions[1 - slot as usize].is_some());
        assert_eq!(action_space(&after).len(), 5);
    }
    for action in actions.into_iter().filter(|action| matches!(action, SemanticActionV1::Cancel { .. })
        || matches!(action, SemanticActionV1::Select { choice, .. } if choice.id == "blackblood")) {
        let mut branch = adapter.clone();
        let prepared = branch.prepare(&action, &before).unwrap();
        let after = branch.execute(prepared).unwrap();
        assert_eq!(after.boundary_kind, "natural_map");
        assert_eq!(branch.session().run_state.act_num, 2);
        assert_eq!(branch.session().run_state.current_hp, 60);
        assert_eq!(branch.session().run_state.map.current_y, -1);
        let DomainEvidenceV1::Observed { value: run, .. } = &after.domains[&ParityDomainV1::Run].evidence else {
            panic!("next Act entrance must observe run state");
        };
        assert!(!run["event_schedule"]["shrines"].as_array().unwrap().is_empty(), "the new Act's pool remains active");
        assert!(branch.session().run_state.potions.iter().all(Option::is_some));
        let took_black_blood = matches!(action, SemanticActionV1::Select { .. });
        assert_eq!(branch.session().run_state.relics.iter().any(|r| r.id == RelicId::BlackBlood), took_black_blood);
        assert_eq!(branch.session().run_state.relics.iter().any(|r| r.id == RelicId::BurningBlood), !took_black_blood);
    }
}

#[test]
fn map_route_reaches_a_source_bound_combat_and_admits_the_next_player_action() {
    let mut adapter = mini_neow_adapter();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map).into_iter().next().unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let combat = adapter.execute(prepared).unwrap();

    assert_eq!(combat.boundary_kind, "natural_combat");
    let calls = &adapter
        .last_rng_call_ledger_v1()
        .expect("combat construction should publish a transition RNG ledger")
        .0;
    assert!(
        !calls.is_empty(),
        "combat construction consumed no named RNG"
    );
    assert!(
        calls.iter().any(|call| call.owner.starts_with("floor.")),
        "combat construction did not preserve a floor RNG owner: {calls:#?}"
    );
    let DomainEvidenceV1::Observed { value, .. } =
        &combat.domains[&ParityDomainV1::Combat].evidence
    else {
        panic!("combat entry should be observed")
    };
    assert_eq!(value["status"], "player_decision");
    assert_eq!(value["state"]["turn"], 1);
    let hand = value["state"]["zones"]["hand"].as_array().unwrap();
    let draw = value["state"]["zones"]["draw_next_first"]
        .as_array()
        .unwrap();
    assert_eq!(hand.len() + draw.len(), 11);
    let bindings = hand
        .iter()
        .chain(draw)
        .map(|card| card["binding"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(bindings.len(), 11);
    assert!(!action_space(&combat).is_empty());

    let combat_action = action_space(&combat).into_iter().next().unwrap();
    let decision_step = adapter.session().decision_step;
    let prepared = adapter.prepare(&combat_action, &combat).unwrap();
    let successor = adapter.execute(prepared).unwrap();
    assert_eq!(adapter.session().decision_step, decision_step + 1);
    assert_ne!(successor.observation_id, combat.observation_id);
    assert!(matches!(
        successor.boundary_kind.as_str(),
        "natural_combat" | "natural_combat_reward"
    ));
}

#[test]
fn private_search_returns_a_stepwise_semantic_win_without_mutating_the_adapter() {
    let mut adapter = mini_neow_adapter();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map).into_iter().next().unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let mut current = adapter.execute(prepared).unwrap();
    let decision_step = adapter.session().decision_step;

    let trajectory = adapter
        .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(2_000),
            wall_ms: Some(5_000),
            ..Default::default()
        })
        .unwrap();

    assert!(!trajectory.is_empty());
    assert_eq!(adapter.session().decision_step, decision_step);
    assert_eq!(current.boundary_kind, "natural_combat");
    for action in trajectory {
        assert!(action_space(&current).contains(&action));
        let prepared = adapter.prepare(&action, &current).unwrap();
        current = adapter.execute(prepared).unwrap();
    }
    assert_eq!(current.boundary_kind, "natural_combat_reward");
}

#[test]
fn ordinary_card_reward_adds_one_room_scoped_source_bound_master_deck_card() {
    let mut adapter = mini_neow_adapter();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map).into_iter().next().unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let mut current = adapter.execute(prepared).unwrap();
    for action in adapter
        .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(2_000),
            wall_ms: Some(5_000),
            ..Default::default()
        })
        .unwrap()
    {
        let prepared = adapter.prepare(&action, &current).unwrap();
        current = adapter.execute(prepared).unwrap();
    }
    assert_eq!(current.boundary_kind, "natural_combat_reward");
    let DomainEvidenceV1::Observed { value: before, .. } =
        &current.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("combat reward should expose the bound master deck")
    };
    let before_len = before.as_array().unwrap().len();
    let open = action_space(&current)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CombatReward,
                    choice,
                } if choice.kind == "run_reward" && choice.id == "card"
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&open, &current).unwrap();
    current = adapter.execute(prepared).unwrap();
    assert_eq!(current.boundary_kind, "natural_card_reward");
    assert!(action_space(&current).iter().any(|action| matches!(
        action,
        SemanticActionV1::Cancel {
            surface: DecisionSurfaceV1::CardReward
        }
    )));
    let preferred = adapter.preferred_card_reward_action_v1().unwrap();
    assert!(action_space(&current).contains(&preferred));
    let take = action_space(&current)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CardReward,
                    ..
                }
            )
        })
        .unwrap();
    let selected_card_id = match &take {
        SemanticActionV1::Select { choice, .. } => choice.id.clone(),
        _ => unreachable!(),
    };
    let prepared = adapter.prepare(&take, &current).unwrap();
    current = adapter.execute(prepared).unwrap();
    assert_eq!(current.boundary_kind, "natural_combat_reward");
    let DomainEvidenceV1::Observed { value: room, .. } =
        &current.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("card acquisition should retain the current room")
    };
    let room_id = room["current"].as_str().unwrap();

    let DomainEvidenceV1::Observed { value: deck, .. } =
        &current.domains[&ParityDomainV1::MasterDeck].evidence
    else {
        panic!("card acquisition should expose the bound master deck")
    };
    assert_eq!(deck.as_array().unwrap().len(), before_len + 1);
    let DomainEvidenceV1::Observed {
        value: relations, ..
    } = &current.domains[&ParityDomainV1::CardBindings].evidence
    else {
        panic!("card acquisition should expose its binding relation")
    };
    let acquired = relations
        .as_array()
        .unwrap()
        .iter()
        .find(|relation| relation["source"]["kind"] == "card_reward")
        .expect("selected reward should have one typed acquisition source");
    assert_eq!(acquired["source"]["room_id"], room_id);
    assert_eq!(acquired["source"]["product_slot"], 0);
    let acquired_binding = acquired["binding"].as_str().unwrap();
    let acquired_card = deck
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["binding"].as_str() == Some(acquired_binding))
        .expect("selected reward binding should name one master-deck card");
    assert_eq!(acquired_card["card_id"], selected_card_id);
    assert_eq!(
        acquired["binding"],
        format!("natural_run/card_reward/{room_id}/0")
    );
}

#[test]
fn private_search_failure_reports_its_bounded_evidence() {
    let mut adapter = mini_neow_adapter();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map).into_iter().next().unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let _combat = adapter.execute(prepared).unwrap();

    let error = adapter
        .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(1),
            max_actions_per_line: Some(1),
            wall_ms: Some(5_000),
            rollout_max_evaluations: Some(1),
            rollout_max_actions: Some(1),
            ..Default::default()
        })
        .unwrap_err();

    assert!(
        error.contains("rejection=NoCompleteWinningCandidate"),
        "{error}"
    );
    assert!(error.contains("coverage_status="), "{error}");
    assert!(error.contains("nodes_expanded="), "{error}");
    assert!(error.contains("coverage_reason="), "{error}");
}

#[test]
fn explicit_segment_mode_returns_a_stepwise_semantic_turn_without_mutation() {
    let mut adapter = mini_neow_adapter();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map).into_iter().next().unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let current = adapter.execute(prepared).unwrap();
    let decision_step = adapter.session().decision_step;

    let (kind, trajectory) = adapter
        .plan_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(1),
            wall_ms: Some(5_000),
            rollout_max_evaluations: Some(1),
            rollout_max_actions: Some(1),
            segment_mode: Some(RunControlCombatSegmentMode::TurnBoundary),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(kind, RunCombatResolutionKindV1::TurnSegment);
    assert!(!trajectory.is_empty());
    assert_eq!(adapter.session().decision_step, decision_step);
    assert_eq!(current.boundary_kind, "natural_combat");
}

#[test]
fn exact_input_bridge_translates_without_mutating_the_live_combat() {
    let mut adapter = mini_neow_adapter();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map).into_iter().next().unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let current = adapter.execute(prepared).unwrap();
    let decision_step = adapter.session().decision_step;

    let prefix = adapter
        .semantic_combat_prefix_from_inputs_v1(&[ClientInput::EndTurn])
        .unwrap();
    let winning_error = adapter
        .semantic_winning_combat_trajectory_from_inputs_v1(&[ClientInput::EndTurn])
        .unwrap_err();

    assert_eq!(prefix, vec![SemanticActionV1::EndTurn]);
    assert!(
        winning_error.contains("ended at an incompatible"),
        "unexpected bridge rejection: {winning_error}"
    );
    assert_eq!(adapter.session().decision_step, decision_step);
    assert_eq!(current.boundary_kind, "natural_combat");
}

#[test]
fn exact_input_bridge_keeps_grid_selection_lineage_and_replays_direct_inputs() {
    use sts_oracle_run_control::{
        content::cards::{get_card_definition, CardId, CardTarget},
        eval::run_control::RunDecisionAction,
        state::selection::{SelectionResolution, SelectionScope, SelectionTargetRef},
    };
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.potions[0] = Some(Potion::new(PotionId::LiquidMemories, 41));
    let mut adapter =
        RustLockstepAdapterV1::new(session, bootstrap, "test", "input-selection").unwrap();
    let map = advance_to_map(&mut adapter);
    let route = action_space(&map)
        .into_iter()
        .find(|a| {
            matches!(
                a,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    ..
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let original = adapter.execute(prepared).unwrap();
    let mut oracle = adapter.session().clone();
    let hand = &oracle
        .active_combat
        .as_ref()
        .unwrap()
        .combat_state
        .zones
        .hand;
    let duplicates = [CardId::Strike, CardId::Defend]
        .into_iter()
        .map(|id| {
            hand.iter()
                .filter(|card| card.id == id)
                .map(|card| card.lineage)
                .collect::<Vec<_>>()
        })
        .find(|cards| cards.len() >= 2)
        .unwrap();
    let mut inputs = Vec::new();
    for lineage in &duplicates[..2] {
        let hand = &oracle
            .active_combat
            .as_ref()
            .unwrap()
            .combat_state
            .zones
            .hand;
        let card_index = hand
            .iter()
            .position(|card| card.lineage == *lineage)
            .unwrap();
        let target = (get_card_definition(hand[card_index].id).target == CardTarget::Enemy)
            .then_some(
                oracle
                    .active_combat
                    .as_ref()
                    .unwrap()
                    .combat_state
                    .entities
                    .monsters[0]
                    .id,
            );
        let input = ClientInput::PlayCard { card_index, target };
        oracle
            .apply_decision_action(RunDecisionAction::Input(input.clone()))
            .unwrap();
        inputs.push(input);
    }
    for input in [
        ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
        ClientInput::SubmitSelection(SelectionResolution {
            scope: SelectionScope::Grid,
            selected: vec![SelectionTargetRef::CardLineage(duplicates[1])],
        }),
        ClientInput::EndTurn,
    ] {
        oracle
            .apply_decision_action(RunDecisionAction::Input(input.clone()))
            .unwrap();
        inputs.push(input);
    }
    let semantics = adapter
        .semantic_combat_prefix_from_inputs_v1(&inputs)
        .unwrap();
    assert_eq!(semantics.len(), inputs.len());
    let SemanticActionV1::SubmitSelection {
        surface: DecisionSurfaceV1::GridSelect,
        choices,
    } = &semantics[3]
    else {
        panic!("grid input must remain a bound selection");
    };
    assert_eq!(choices.len(), 1);
    let SemanticActionV1::PlayCard { card_binding, .. } = &semantics[1] else {
        unreachable!()
    };
    assert_eq!(&choices[0].id, card_binding);
    assert_eq!(adapter.observe().unwrap(), original);
    let mut invalid = inputs.clone();
    invalid[3] = ClientInput::SubmitSelection(SelectionResolution {
        scope: SelectionScope::Hand,
        selected: vec![SelectionTargetRef::CardLineage(duplicates[1])],
    });
    assert!(adapter
        .semantic_combat_prefix_from_inputs_v1(&invalid)
        .is_err());
    assert!(adapter
        .semantic_winning_combat_trajectory_from_inputs_v1(&inputs)
        .is_err());
    assert_eq!(adapter.observe().unwrap(), original);
    for action in semantics {
        let snapshot = adapter.observe().unwrap();
        let prepared = adapter.prepare(&action, &snapshot).unwrap();
        adapter.execute(prepared).unwrap();
    }
    assert_eq!(
        adapter
            .session()
            .active_combat
            .as_ref()
            .unwrap()
            .combat_state,
        oracle.active_combat.as_ref().unwrap().combat_state
    );
}

#[test]
fn end_turn_generated_slimed_keeps_typed_bindings_during_search_and_live_replay() {
    let config = RunControlConfig {
        seed: 2_026_090_302,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut adapter = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        "test-build",
        "rust-slimed-birth-epoch",
    )
    .unwrap();

    let introduction = adapter.observe().unwrap();
    let proceed = action_space(&introduction).remove(0);
    let prepared = adapter.prepare(&proceed, &introduction).unwrap();
    let reward = adapter.execute(prepared).unwrap();
    let curse = action_space(&reward)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select { choice, .. }
                    if choice.id == "one_rare_relic"
                        && choice.attributes.get("drawback")
                            == Some(&SemanticScalarV1::Text("curse".to_owned()))
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&curse, &reward).unwrap();
    let leave = adapter.execute(prepared).unwrap();
    let proceed = action_space(&leave)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::Proceed { .. }))
        .unwrap();
    let prepared = adapter.prepare(&proceed, &leave).unwrap();
    let map = adapter.execute(prepared).unwrap();
    let route = action_space(&map).into_iter().next().unwrap();
    let prepared = adapter.prepare(&route, &map).unwrap();
    let mut current = adapter.execute(prepared).unwrap();
    let DomainEvidenceV1::Observed { value: room, .. } =
        &current.domains[&ParityDomainV1::Room].evidence
    else {
        panic!("combat room should be observed")
    };
    let room_id = room["current"].as_str().unwrap().to_owned();

    let trajectory = adapter
        .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(2_000),
            wall_ms: Some(5_000),
            ..Default::default()
        })
        .unwrap();

    let mut saw_slimed_birth = false;
    for action in trajectory {
        let prepared = adapter.prepare(&action, &current).unwrap();
        current = adapter.execute(prepared).unwrap();
        if current.boundary_kind != "natural_combat" {
            continue;
        }
        let DomainEvidenceV1::Observed {
            value: relations, ..
        } = &current.domains[&ParityDomainV1::CardBindings].evidence
        else {
            panic!("combat card bindings should remain observed")
        };
        for relation in relations.as_array().unwrap() {
            let source = &relation["source"];
            if source["kind"] != "combat_birth" || source["cause"]["kind"] != "end_turn_resolution"
            {
                continue;
            }
            assert_eq!(source["room_id"], room_id);
            let binding = relation["binding"].as_str().unwrap();
            assert!(binding.starts_with(&format!("natural_run/combat_birth/{room_id}/")));
            let DomainEvidenceV1::Observed { value: combat, .. } =
                &current.domains[&ParityDomainV1::Combat].evidence
            else {
                panic!("combat state should remain observed")
            };
            let zones = &combat["state"]["zones"];
            saw_slimed_birth = [
                "draw_next_first",
                "hand",
                "discard_bottom_first",
                "exhaust_by_binding",
                "limbo",
            ]
            .into_iter()
            .flat_map(|zone| zones[zone].as_array().unwrap())
            .any(|card| card["binding"] == binding && card["card_id"] == "slimed");
        }
    }
    assert!(saw_slimed_birth);
    assert_eq!(current.boundary_kind, "natural_combat_reward");
    let DomainEvidenceV1::Observed {
        value: relations, ..
    } = &current.domains[&ParityDomainV1::CardBindings].evidence
    else {
        panic!("reward card bindings should remain observed")
    };
    assert!(relations
        .as_array()
        .unwrap()
        .iter()
        .all(|relation| relation["source"]["kind"] != "combat_birth"));
}

fn advance_to_map(adapter: &mut RustLockstepAdapterV1) -> sts_parity_lockstep::ParitySnapshotV1 {
    let introduction = adapter.observe().unwrap();
    let proceed = action_space(&introduction)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::Neow
                }
            )
        })
        .unwrap();
    assert!(matches!(
        proceed,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::Neow
        }
    ));
    let prepared = adapter.prepare(&proceed, &introduction).unwrap();
    let before = adapter.execute(prepared).unwrap();

    let choose = action_space(&before)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::Select { .. }))
        .unwrap();
    let prepared = adapter.prepare(&choose, &before).unwrap();
    let leave_boundary = adapter.execute(prepared).unwrap();
    assert_ne!(before.observation_id, leave_boundary.observation_id);

    let proceed = action_space(&leave_boundary)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::Neow
                }
            )
        })
        .unwrap();
    let prepared = adapter.prepare(&proceed, &leave_boundary).unwrap();
    adapter.execute(prepared).unwrap()
}

#[test]
fn bootstrap_wire_contract_contains_sources_but_no_rust_lineage_values() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let encoded = serde_json::to_string(&bootstrap).unwrap();

    assert!(encoded.contains("source_slot"));
    assert!(encoded.contains("ascenders_bane"));
    assert!(!encoded.contains("lineage"));
    assert!(!encoded.contains("uuid"));
}

fn action_space(snapshot: &sts_parity_lockstep::ParitySnapshotV1) -> Vec<SemanticActionV1> {
    let DomainEvidenceV1::Observed { value, .. } =
        &snapshot.domains[&ParityDomainV1::ActionSpace].evidence
    else {
        panic!("action space should be observed")
    };
    serde_json::from_value(value.clone()).unwrap()
}
