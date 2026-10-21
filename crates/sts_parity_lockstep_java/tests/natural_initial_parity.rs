use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};
use sts_oracle_run_control::{
    content::cards::CardId,
    content::events::neow::NeowEligibilityV1,
    content::potions::{Potion, PotionId},
    eval::run_control::{AtomicCombatSearchOptionsV2, RunControlConfig, RunControlSession},
    state::{
        core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState},
        events::{EventId, EventState},
        map::RoomType,
        selection::DomainEventSource,
    },
};
use sts_parity_lockstep::{
    compare_natural_run_rng_calls_v1, compare_snapshots_v1, natural_card_reward_choice_v1,
    natural_reward_choice_v1, AdapterFailureKindV1, AdapterFailureV1, ComparisonStatusV1,
    DecisionSurfaceV1, DomainEvidenceV1, LockstepAdapterV1, LockstepModeV1, LockstepRunnerV1,
    NaturalRunRewardItemV1, NaturalRunRewardsV2, ParityDomainV1, ParitySnapshotV1,
    ProducerIdentityV1, SemanticActionV1, SemanticScalarV1, NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1,
    NATURAL_RUN_SNAPSHOT_DOMAINS_V3,
};
use sts_parity_lockstep_java::{
    JavaLockstepAdapterV1, JavaLockstepReceiptV1, JavaLockstepTransportV1,
    JavaNativeReloadExecutionV1, JavaRuntimeActionV1, JavaRuntimeObservationV1,
    RecordedJavaActionPhaseV1, RecordedJavaEvidenceStoreV1, RecordedJavaEvidenceV1,
};
use sts_parity_lockstep_rust::{natural_run_bootstrap_v1, RustLockstepAdapterV1};

struct FixtureTransport {
    observations: Vec<JavaRuntimeObservationV1>,
    cursor: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct FixtureTransportCalls {
    methods: Vec<&'static str>,
    hand_selections: Vec<Vec<String>>,
}

struct RecordingFixtureTransport {
    inner: FixtureTransport,
    calls: Rc<RefCell<FixtureTransportCalls>>,
}

impl JavaLockstepTransportV1 for FixtureTransport {
    fn status(&mut self) -> Result<JavaRuntimeObservationV1, AdapterFailureV1> {
        Ok(self.observations[self.cursor].clone())
    }

    fn act(&mut self, runtime_handle: &str) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let before = &self.observations[self.cursor];
        if !before
            .actions
            .iter()
            .any(|action| action.runtime_handle == runtime_handle)
        {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Evidence,
                "fixture received an action outside its current frame",
            ));
        }
        let action_id = before
            .actions
            .iter()
            .find(|action| action.runtime_handle == runtime_handle)
            .unwrap()
            .action_id
            .clone();
        let before_frame_id = before.frame_id;
        self.cursor += 1;
        let after = &mut self.observations[self.cursor];
        let calls = after
            .frame
            .as_object_mut()
            .and_then(|frame| frame.remove("fixture_rng_calls"))
            .unwrap_or_else(|| json!([]));
        let receipt = JavaLockstepReceiptV1 {
            status: "completed".to_owned(),
            request_id: format!("fixture-request-{before_frame_id}"),
            based_on_frame_id: before_frame_id,
            action_id,
            successor_frame_id: Some(after.frame_id),
        };
        if after.frame["rng_call_witness"].is_null() {
            after.frame["rng_call_witness"] = json!({
                "kind": "rng_call_witness_v1",
                "schema": NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1,
                "from_frame_id": receipt.based_on_frame_id,
                "to_frame_id": after.frame_id,
                "request_id": receipt.request_id,
                "action_id": receipt.action_id,
                "complete": true,
                "calls": calls,
                "unsupported_reason": null,
            });
        }
        Ok(receipt)
    }

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let grid = &self.observations[self.cursor].frame["public_base"]["decision"]["grid_select"];
        let permitted = grid["allowed_counts"]
            .as_array()
            .is_some_and(|counts| counts.contains(&json!(selected_card_uuids.len())));
        if !permitted {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Evidence,
                "fixture grid submission violates its declared count",
            ));
        }
        self.act(runtime_handle)
    }

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        if !self.observations[self.cursor].frame["public_base"]["decision"]["hand_select"]
            ["allowed_counts"]
            .as_array()
            .is_some_and(|counts| counts.contains(&json!(selected_card_uuids.len())))
        {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Evidence,
                "fixture hand submission violates its declared count",
            ));
        }
        self.act(runtime_handle)
    }
}

impl JavaLockstepTransportV1 for RecordingFixtureTransport {
    fn status(&mut self) -> Result<JavaRuntimeObservationV1, AdapterFailureV1> {
        self.inner.status()
    }

    fn act(&mut self, runtime_handle: &str) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        self.calls.borrow_mut().methods.push("act");
        self.inner.act(runtime_handle)
    }

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        self.calls.borrow_mut().methods.push("grid");
        self.inner
            .submit_grid_select(runtime_handle, selected_card_uuids)
    }

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let mut calls = self.calls.borrow_mut();
        calls.methods.push("hand");
        calls.hand_selections.push(selected_card_uuids.to_vec());
        drop(calls);
        self.inner
            .submit_hand_select(runtime_handle, selected_card_uuids)
    }
}

#[test]
fn natural_neow_common_projection_is_exact_across_both_adapters() {
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
    let mut rust =
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture-rust", "rust-epoch")
            .unwrap();
    let rust_snapshot = rust.observe().unwrap();
    let java_observation = equivalent_java_observation(&rust_snapshot, 1);
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![java_observation],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let java_snapshot = java.observe().unwrap();

    let comparison = compare_snapshots_v1(&java_snapshot, &rust_snapshot).unwrap();
    let stops = comparison
        .domains
        .iter()
        .filter(|domain| domain.status != ComparisonStatusV1::Exact)
        .map(|domain| {
            format!(
                "{:?}: {}",
                domain.domain,
                domain.reason.as_deref().unwrap_or("values differ")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        comparison.status,
        ComparisonStatusV1::Exact,
        "initial common projection stopped: {}",
        stops.join(", ")
    );
    assert_eq!(
        comparison.domains.len(),
        NATURAL_RUN_SNAPSHOT_DOMAINS_V3.len()
    );
    assert!(comparison
        .domains
        .iter()
        .all(|domain| domain.status == ComparisonStatusV1::Exact));
}

#[test]
fn java_projection_accepts_a_typed_neow_curse_binding_after_bootstrap() {
    let config = RunControlConfig {
        seed: 2_026_090_302,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "fixture-rust",
        "java-neow-curse-fixture",
    )
    .unwrap();
    let introduction = rust.observe().unwrap();
    let reward = execute(&mut rust, &introduction, &only_action(&introduction));
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
    let rust_after = execute(&mut rust, &reward, &curse);
    let java_observation = equivalent_java_observation(&rust_after, 2);
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![java_observation],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let java_after = java.observe().unwrap();

    let binding = "natural_run/neow_drawback/one_rare_relic/curse/0";
    let deck = observed(&java_after, ParityDomainV1::MasterDeck);
    let injury = deck
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["binding"] == binding)
        .expect("Java projection should retain the Neow curse binding");
    assert_eq!(injury["card_id"], "injury");
    let relations = observed(&java_after, ParityDomainV1::CardBindings);
    assert!(relations
        .as_array()
        .unwrap()
        .iter()
        .any(|relation| relation["binding"] == binding
            && relation["source"]["kind"] == "neow_drawback"));

    let comparison = compare_snapshots_v1(&java_after, &rust_after).unwrap();
    assert_eq!(comparison.status, ComparisonStatusV1::Exact);
}

#[test]
fn neow_transform_transition_is_exact_with_a_source_less_java_replacement() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "fixture-rust",
        "neow-transform-rust-epoch",
    )
    .unwrap();
    let introduction = rust.observe().unwrap();
    let reward = execute(&mut rust, &introduction, &only_action(&introduction));
    let transform = action_space(&reward)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select { choice, .. } if choice.id == "transform_card"
            )
        })
        .expect("seed 2026090301 should offer one-card transform");
    let rust_before = execute(&mut rust, &reward, &transform);
    let shared = action_space(&rust_before)
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
    let selected_binding = match &shared {
        SemanticActionV1::SubmitSelection { choices, .. } => choices[0].id.clone(),
        _ => unreachable!("selected a grid action"),
    };
    let rust_prepared = rust.prepare(&shared, &rust_before).unwrap();
    let rust_after = rust.execute(rust_prepared).unwrap();

    let java_before_raw = equivalent_java_observation(&rust_before, 60);
    let mut java_after_raw = equivalent_java_successor(&rust, &rust_after, 61);
    let prior_uuid_by_binding = observed(&rust_before, ParityDomainV1::MasterDeck)
        .as_array()
        .unwrap()
        .iter()
        .map(|card| {
            (
                required_text(card, "binding").to_owned(),
                format!("fixture-card-{}", card["ordinal"].as_u64().unwrap()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (ordinal, card) in observed(&rust_after, ParityDomainV1::MasterDeck)
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let binding = required_text(card, "binding");
        let raw = &mut java_after_raw.frame["parity_internal"]["master_deck"][ordinal];
        if binding == selected_binding {
            raw["card_uuid"] = json!("fixture-transformed-card");
            raw["source"] = Value::Null;
        } else {
            raw["card_uuid"] = json!(prior_uuid_by_binding[binding]);
        }
    }
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![java_before_raw, java_after_raw],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let java_before = java.observe().unwrap();
    assert_eq!(
        compare_snapshots_v1(&java_before, &rust_before)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
    let java_prepared = java.prepare(&shared, &java_before).unwrap();
    let java_after = java.execute(java_prepared).unwrap();
    assert_eq!(
        compare_snapshots_v1(&java_after, &rust_after)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
    assert!(compare_natural_run_rng_calls_v1(
        java.last_rng_call_ledger_v1().unwrap(),
        rust.last_rng_call_ledger_v1().unwrap(),
    )
    .unwrap()
    .stop_reason()
    .is_none());
}

#[test]
fn astrolabe_ordered_grid_clicks_and_witnessed_products_are_exact() {
    use sts_oracle_run_control::content::relics::{astrolabe, RelicId, RelicState};
    let config = RunControlConfig {
        seed: 2026090709,
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.event_state = None;
    session.run_state.floor_num = 34;
    session.run_state.act_num = 2;
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 0;
    session.run_state.map.graph[0][0].class = Some(RoomType::TreasureRoom);
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::Astrolabe));
    session.run_state.pending_boss_act_transition = true;
    session.engine_state =
        astrolabe::on_equip(&mut session.run_state, EngineState::MapNavigation).unwrap();
    let mut rust =
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture", "astrolabe").unwrap();
    let mut before = rust.observe().unwrap();
    let initial = before.clone();
    let mut raw = vec![equivalent_java_observation(&before, 80)];
    let mut actions = Vec::new();
    let mut snapshots = vec![before.clone()];
    for step in 0..3 {
        let action = action_space(&before)
            .into_iter()
            .filter(|a| {
                matches!(
                    a,
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::GridSelect,
                        ..
                    }
                )
            })
            .nth(if step == 0 { 2 } else { 0 })
            .unwrap();
        let after = execute(&mut rust, &before, &action);
        if step < 2 {
            assert_eq!(
                observed(&initial, ParityDomainV1::MasterDeck),
                observed(&after, ParityDomainV1::MasterDeck)
            );
            assert!(rust.last_rng_call_ledger_v1().unwrap().0.is_empty());
            assert_eq!(
                observed(&after, ParityDomainV1::Room)["selected"]
                    .as_array()
                    .unwrap()
                    .len(),
                step + 1
            );
            assert!(!action_space(&after).contains(&action));
        }
        raw.push(equivalent_java_successor(&rust, &after, 81 + step as u64));
        actions.push(action);
        snapshots.push(after.clone());
        before = after;
    }
    assert_eq!(before.boundary_kind, "natural_map");
    assert_eq!(rust.session().run_state.act_num, 3);
    let parents = actions
        .iter()
        .map(|a| match a {
            SemanticActionV1::Select { choice, .. } => {
                (choice.id.clone(), fixture_card_uuid(&initial, &choice.id))
            }
            _ => unreachable!(),
        })
        .collect::<BTreeMap<_, _>>();
    for (i, card) in observed(&before, ParityDomainV1::MasterDeck)
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let binding = card["binding"].as_str().unwrap();
        let last = &mut raw[3].frame["parity_internal"]["master_deck"][i];
        if let Some(parent) = parents.get(binding) {
            let uuid = format!("product-of-{parent}");
            last["card_uuid"] = json!(uuid);
            last["source"] = Value::Null;
            last["transform_origin"] = json!({"schema":"master_deck_transform_origin_v1","owner":"astrolabe","parent_uuid":parent,"product_uuid":uuid});
        } else {
            last["card_uuid"] = json!(fixture_card_uuid(&initial, binding));
        }
    }
    // Every failure must stop before installing any partial product bindings.
    for malformed in [
        None,
        Some("missing"),
        Some("parent"),
        Some("product"),
        Some("duplicate"),
    ] {
        let mut observations = raw.clone();
        let products = observations[3].frame["parity_internal"]["master_deck"]
            .as_array_mut()
            .unwrap();
        let ids = products
            .iter()
            .enumerate()
            .filter(|(_, c)| !c["transform_origin"].is_null())
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        match malformed {
            Some("missing") => products[ids[0]]["transform_origin"] = Value::Null,
            Some("parent") => {
                products[ids[0]]["transform_origin"]["parent_uuid"] = json!("foreign")
            }
            Some("product") => {
                products[ids[0]]["transform_origin"]["product_uuid"] = json!("foreign")
            }
            Some("duplicate") => {
                products[ids[0]]["transform_origin"]["parent_uuid"] =
                    products[ids[1]]["transform_origin"]["parent_uuid"].clone()
            }
            _ => {}
        }
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations,
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let mut java_before = java.observe().unwrap();
        for (step, action) in actions.iter().enumerate() {
            assert_eq!(
                compare_snapshots_v1(&java_before, &snapshots[step])
                    .unwrap()
                    .status,
                ComparisonStatusV1::Exact
            );
            let prepared = java.prepare(action, &java_before).unwrap();
            let result = java.execute(prepared);
            if step == 2 && malformed.is_some() {
                assert!(
                    result.is_err(),
                    "accepted malformed Astrolabe {malformed:?}"
                );
                break;
            }
            java_before = result.unwrap();
            assert_eq!(
                compare_snapshots_v1(&java_before, &snapshots[step + 1])
                    .unwrap()
                    .status,
                ComparisonStatusV1::Exact
            );
        }
    }
}

#[test]
fn drug_dealer_two_transforms_require_exact_native_parent_witnesses() {
    let config = RunControlConfig {
        seed: 2026101803,
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.potions[0] = Some(Potion::new(PotionId::SmokeBomb, 100));
    session.run_state.potions[1] = Some(Potion::new(PotionId::LiquidBronze, 101));
    session.run_state.act_num = 2;
    session.run_state.floor_num = 20;
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 0;
    session.run_state.map.graph[0][0].class = Some(RoomType::EventRoom);
    let mut event = EventState::new(EventId::DrugDealer);
    event.current_screen = 1;
    session.run_state.event_state = Some(event);
    session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
        selected: vec![],
        min_choices: 2,
        max_choices: 2,
        reason: RunPendingChoiceReason::Transform,
        source: DomainEventSource::Event(EventId::DrugDealer),
        return_state: Box::new(EngineState::EventRoom),
    });
    let mut rust =
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture", "two-transforms")
            .unwrap();
    let before = rust.observe().unwrap();
    let action = action_space(&before).into_iter().find(|action| matches!(action,
        SemanticActionV1::SubmitSelection { surface:DecisionSurfaceV1::GridSelect, choices } if choices.len()==2)).unwrap();
    let after = execute(&mut rust, &before, &action);
    let SemanticActionV1::SubmitSelection { choices, .. } = &action else {
        unreachable!()
    };
    let parents = choices
        .iter()
        .map(|choice| (choice.id.clone(), fixture_card_uuid(&before, &choice.id)))
        .collect::<BTreeMap<_, _>>();
    let raw_before = equivalent_java_observation(&before, 80);
    let mut raw_after = equivalent_java_successor(&rust, &after, 81);
    for (i, card) in observed(&after, ParityDomainV1::MasterDeck)
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let binding = card["binding"].as_str().unwrap();
        let native = &mut raw_after.frame["parity_internal"]["master_deck"][i];
        if let Some(parent) = parents.get(binding) {
            let product = format!("product-of-{parent}");
            native["card_uuid"] = json!(product);
            native["source"] = Value::Null;
            native["transform_origin"] = json!({"schema":"master_deck_transform_origin_v1","owner":"drugdealer","parent_uuid":parent,"product_uuid":product});
        } else {
            native["card_uuid"] = json!(fixture_card_uuid(&before, binding));
        }
    }
    for malformed in [
        None,
        Some("missing"),
        Some("owner"),
        Some("parent"),
        Some("product"),
        Some("duplicate"),
    ] {
        let mut successor = raw_after.clone();
        let products = successor.frame["parity_internal"]["master_deck"]
            .as_array_mut()
            .unwrap();
        let ids = products
            .iter()
            .enumerate()
            .filter(|(_, c)| !c["transform_origin"].is_null())
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        match malformed {
            Some("missing") => products[ids[0]]["transform_origin"] = Value::Null,
            Some("owner") => products[ids[0]]["transform_origin"]["owner"] = json!("astrolabe"),
            Some("parent") => {
                products[ids[0]]["transform_origin"]["parent_uuid"] = json!("foreign")
            }
            Some("product") => {
                products[ids[0]]["transform_origin"]["product_uuid"] = json!("foreign")
            }
            Some("duplicate") => {
                products[ids[0]]["transform_origin"]["parent_uuid"] =
                    products[ids[1]]["transform_origin"]["parent_uuid"].clone()
            }
            _ => {}
        }
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![raw_before.clone(), successor],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let java_before = java.observe().unwrap();
        let comparison = compare_snapshots_v1(&java_before, &before).unwrap();
        assert_eq!(
            comparison.status,
            ComparisonStatusV1::Exact,
            "{comparison:#?}"
        );
        let prepared = java.prepare(&action, &java_before).unwrap();
        let result = java.execute(prepared);
        if malformed.is_some() {
            assert!(result.is_err(), "accepted {malformed:?}");
        } else {
            assert_eq!(
                compare_snapshots_v1(&result.unwrap(), &after)
                    .unwrap()
                    .status,
                ComparisonStatusV1::Exact
            );
        }
    }
}

#[test]
fn bonfire_grid_selection_and_its_successor_are_exact_across_both_adapters() {
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
    session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason: RunPendingChoiceReason::PurgeNonBottled,
        source: DomainEventSource::Event(EventId::BonfireElementals),
        return_state: Box::new(EngineState::EventRoom),
    });
    let mut rust = RustLockstepAdapterV1::new(
        session,
        bootstrap.clone(),
        "fixture-rust",
        "bonfire-rust-epoch",
    )
    .unwrap();
    let rust_before = rust.observe().unwrap();
    let shared = action_space(&rust_before)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::SubmitSelection { choices, .. }
                    if choices[0].attributes.get("card_id")
                        == Some(&SemanticScalarV1::Text("strike".to_owned()))
            )
        })
        .unwrap();
    let rust_prepared = rust.prepare(&shared, &rust_before).unwrap();
    let rust_after = rust.execute(rust_prepared).unwrap();

    let java_before_raw = equivalent_java_observation(&rust_before, 70);
    let java_after_raw = equivalent_java_observation(&rust_after, 71);
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![java_before_raw, java_after_raw],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let java_before = java.observe().unwrap();
    assert_eq!(
        compare_snapshots_v1(&java_before, &rust_before)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
    let java_prepared = java.prepare(&shared, &java_before).unwrap();
    let java_after = java.execute(java_prepared).unwrap();
    assert_eq!(
        compare_snapshots_v1(&java_after, &rust_after)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
}

#[test]
fn campfire_smith_choices_and_upgraded_successor_are_exact_across_both_adapters() {
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
    let mut rust = RustLockstepAdapterV1::new(
        session,
        bootstrap.clone(),
        "fixture-rust",
        "campfire-rust-epoch",
    )
    .unwrap();

    let rust_campfire = rust.observe().unwrap();
    let smith = action_space(&rust_campfire)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select { choice, .. }
                    if choice.kind == "campfire_option" && choice.id == "smith"
            )
        })
        .unwrap();
    let rust_smith = execute(&mut rust, &rust_campfire, &smith);
    let strike = action_space(&rust_smith)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select { choice, .. }
                    if choice.kind == "deck_card"
                        && choice.id == "natural_run/starter/0"
            )
        })
        .unwrap();
    let rust_map = execute(&mut rust, &rust_smith, &strike);

    let java_campfire_raw = equivalent_java_observation(&rust_campfire, 80);
    let java_smith_raw = equivalent_java_observation(&rust_smith, 81);
    let java_confirm_raw =
        smith_confirmation_observation(&java_smith_raw, &rust_smith, &strike, 82);
    let java_complete_raw = campfire_completion_observation(&rust_map, 83);
    let java_map_raw = equivalent_java_observation(&rust_map, 84);
    let evidence_root = std::env::temp_dir().join(format!(
        "sts-campfire-settlement-evidence-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let evidence_directory = evidence_root.join("java-evidence");
    let evidence_store =
        RecordedJavaEvidenceStoreV1::create_new(&evidence_directory, bootstrap.clone()).unwrap();
    let mut java = JavaLockstepAdapterV1::with_evidence_store(
        FixtureTransport {
            observations: vec![
                java_campfire_raw,
                java_smith_raw,
                java_confirm_raw,
                java_complete_raw,
                java_map_raw,
            ],
            cursor: 0,
        },
        bootstrap,
        evidence_store,
    )
    .unwrap();

    let java_campfire = java.observe().unwrap();
    assert_exact(&java_campfire, &rust_campfire);
    let prepared = java.prepare(&smith, &java_campfire).unwrap();
    let java_smith = java.execute(prepared).unwrap();
    assert_exact(&java_smith, &rust_smith);
    let prepared = java.prepare(&strike, &java_smith).unwrap();
    let java_map = java.execute(prepared).unwrap();
    assert_exact(&java_map, &rust_map);
    drop(java);

    let evidence = RecordedJavaEvidenceV1::load_complete(&evidence_directory).unwrap();
    assert_eq!(evidence.steps.len(), 2);
    assert_eq!(evidence.java_action_count, 4);
    assert_eq!(evidence.steps[1].action, strike);
    assert_eq!(
        evidence.steps[1]
            .java_actions
            .iter()
            .map(|action| action.phase)
            .collect::<Vec<_>>(),
        [
            RecordedJavaActionPhaseV1::Committed,
            RecordedJavaActionPhaseV1::InterstitialSettlement,
            RecordedJavaActionPhaseV1::InterstitialSettlement,
        ]
    );
    assert!(matches!(
        evidence.steps[1].java_actions[2].action,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::Campfire
        }
    ));
    fs::remove_dir_all(evidence_root).unwrap();
}

#[test]
fn shovel_dig_exposes_a_claimable_relic_before_returning_to_map() {
    use sts_oracle_run_control::content::relics::{RelicId, RelicState};
    let config = RunControlConfig {
        seed: 42,
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
        .find(|node| node.class.is_some() && !node.edges.is_empty())
        .map(|node| (node.x, node.y))
        .unwrap();
    session.run_state.map.current_x = x;
    session.run_state.map.current_y = y;
    session.run_state.map.graph[y as usize][x as usize].class = Some(RoomType::RestRoom);
    session.run_state.event_state = None;
    session.engine_state = EngineState::Campfire;
    let mut without =
        RustLockstepAdapterV1::new(session.clone(), bootstrap.clone(), "fixture", "no-shovel")
            .unwrap();
    assert!(!action_space(&without.observe().unwrap()).iter().any(
        |action| matches!(action, SemanticActionV1::Select { choice, .. } if choice.id == "dig")
    ));
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::Shovel));
    let mut rust =
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture", "shovel").unwrap();
    let initial = rust.observe().unwrap();
    let dig = action_space(&initial).into_iter().find(|action|
        matches!(action, SemanticActionV1::Select { surface: DecisionSurfaceV1::Campfire, choice } if choice.id == "dig")).unwrap();
    let reward = execute(&mut rust, &initial, &dig);
    assert_eq!(reward.boundary_kind, "natural_combat_reward");
    assert_eq!(
        initial.domains[&ParityDomainV1::Relics],
        reward.domains[&ParityDomainV1::Relics]
    );
    assert_eq!(
        initial.domains[&ParityDomainV1::MasterDeck],
        reward.domains[&ParityDomainV1::MasterDeck]
    );
    assert_eq!(
        rust.session().run_state.current_hp,
        without.session().run_state.current_hp
    );
    let claim = action_space(&reward).into_iter().find(|action|
        matches!(action, SemanticActionV1::Select { surface: DecisionSurfaceV1::CombatReward, choice } if choice.id == "relic")).unwrap();
    let claimed = execute(&mut rust, &reward, &claim);
    assert_eq!(
        rust.session().run_state.relics.len(),
        without.session().run_state.relics.len() + 2
    );
    let proceed = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::CombatReward,
    };
    let map = execute(&mut rust, &claimed, &proceed);
    assert_eq!(map.boundary_kind, "natural_map");

    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: [&initial, &reward, &claimed, &map]
                .into_iter()
                .enumerate()
                .map(|(index, snapshot)| equivalent_java_observation(snapshot, 80 + index as u64))
                .collect(),
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let mut java_state = java.observe().unwrap();
    assert_exact(&java_state, &initial);
    for (action, expected) in [(dig, reward), (claim, claimed), (proceed, map)] {
        let prepared = java.prepare(&action, &java_state).unwrap();
        java_state = java.execute(prepared).unwrap();
        assert_exact(&java_state, &expected);
    }
}

#[test]
fn final_campfire_publishes_the_shared_boss_anchor_without_a_raw_java_node() {
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
    let x = session.run_state.map.graph[14]
        .iter()
        .position(|node| node.class.is_some())
        .unwrap();
    session.run_state.map.current_x = x as i32;
    session.run_state.map.current_y = 14;
    session.run_state.map.graph[14][x].class = Some(RoomType::RestRoom);
    session.run_state.event_state = None;
    session.engine_state = EngineState::Campfire;
    let mut rust = RustLockstepAdapterV1::new(
        session,
        bootstrap.clone(),
        "fixture-rust",
        "final-campfire-rust-epoch",
    )
    .unwrap();
    let rust_snapshot = rust.observe().unwrap();
    let mut java_observation = equivalent_java_observation(&rust_snapshot, 90);
    java_observation.frame["public_base"]["map"]["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|node| node["y"] != 15);
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![java_observation],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let java_snapshot = java.observe().unwrap();

    assert_exact(&java_snapshot, &rust_snapshot);
    let map = observed(&java_snapshot, ParityDomainV1::Map);
    assert!(map["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["id"] == "F16X00" && node["room"] == "boss"));
}

#[test]
fn surrounded_requires_geometry_and_compares_facing_independently_of_the_marker() {
    let (bootstrap, combat) = natural_first_combat_fixture();
    let mut observation = equivalent_java_observation(&combat, 10);
    observation.frame["parity_internal"]["player_powers"] = json!([{
        "index": 0, "id": "Surrounded", "amount": -1, "type": "BUFF",
        "class_name": "com.megacrit.cardcrawl.powers.SurroundedPower",
    }]);
    observation.frame["parity_internal"]["monsters"][0]["powers"] = json!([{
        "index": 0, "id": "BackAttack", "amount": -1, "type": "BUFF",
        "class_name": "com.megacrit.cardcrawl.powers.BackAttackPower",
    }]);
    let project = |raw| {
        JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![raw],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap()
        .observe()
    };
    assert!(project(observation.clone()).is_err());
    observation.frame["public_base"]["combat"]["player"]["facing_left"] = json!(false);
    assert!(project(observation.clone()).is_err());
    for monster in observation.frame["public_base"]["combat"]["monsters"]
        .as_array_mut()
        .unwrap()
    {
        monster["left_of_player"] = json!(true);
    }
    let right_facing = project(observation.clone()).unwrap();
    let value = observed(&right_facing, ParityDomainV1::Combat);
    assert_eq!(value["state"]["player"]["facing_left"], false);
    assert_eq!(value["state"]["monsters"][0]["left_of_player"], true);

    // AbstractMonster applies the multiplier using real geometry. An unchanged
    // (potentially stale) BackAttack icon must not conceal a facing mismatch.
    observation.frame["public_base"]["combat"]["player"]["facing_left"] = json!(true);
    let left_facing = project(observation.clone()).unwrap();
    assert_ne!(
        compare_snapshots_v1(&right_facing, &left_facing)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
    observation.frame["public_base"]["combat"]["monsters"][0]["left_of_player"] = json!(false);
    let other_side = project(observation).unwrap();
    assert_ne!(
        compare_snapshots_v1(&left_facing, &other_side)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
}

#[test]
fn natural_map_to_combat_entry_is_exact_across_both_adapters() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "fixture-rust",
        "rust-combat-epoch",
    )
    .unwrap();
    let map = advance_to_map(&mut rust);
    let route = action_space(&map).into_iter().next().unwrap();
    let combat = execute(&mut rust, &map, &route);
    assert_eq!(combat.boundary_kind, "natural_combat");

    let mut java_observation = equivalent_java_observation(&combat, 10);
    for (index, kind) in [
        "run_reset_to_main_menu_preserving_save",
        "run_discard_to_main_menu",
    ]
    .into_iter()
    .enumerate()
    {
        java_observation.frame["action_space"]["actions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"action_id": kind, "kind": kind}));
        java_observation.actions.push(JavaRuntimeActionV1 {
            runtime_handle: format!("fixture-lifecycle-{index}"),
            action_id: kind.to_owned(),
            kind: kind.to_owned(),
            event_id: None,
            option_index: None,
            neow_reward_type: None,
            neow_drawback: None,
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: None,
            reward_index: None,
            reward_id: None,
            reward_gold: None,
            potion_slot: None,
            potion_id: None,
            parameters_required: false,
        });
    }
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![java_observation],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let java_combat = java.observe().unwrap();

    let comparison = compare_snapshots_v1(&java_combat, &combat).unwrap();
    let stops = comparison
        .domains
        .iter()
        .filter(|domain| domain.status != ComparisonStatusV1::Exact)
        .map(|domain| {
            format!(
                "{:?}: {}",
                domain.domain,
                domain.reason.as_deref().unwrap_or("values differ")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        comparison.status,
        ComparisonStatusV1::Exact,
        "combat-entry common projection stopped: {}",
        stops.join(", ")
    );
    assert_eq!(
        comparison.domains.len(),
        NATURAL_RUN_SNAPSHOT_DOMAINS_V3.len()
    );
    assert!(comparison
        .domains
        .iter()
        .all(|domain| domain.status == ComparisonStatusV1::Exact));
}

#[test]
fn upgraded_true_grit_selection_is_exact_and_uses_the_bound_hand_card() {
    single_exhaust_selection_fixture(CardId::TrueGrit, 1, 0, false);
}

#[test]
fn distilled_chaos_true_grit_selection_uses_the_bound_auto_played_card() {
    single_exhaust_selection_fixture(CardId::TrueGrit, 1, 0, true);
}

#[test]
fn burning_pact_exhausts_the_bound_card_then_draws_its_upgrade_dependent_amount() {
    for upgrades in [0, 1] {
        single_exhaust_selection_fixture(CardId::BurningPact, upgrades, 0, false);
    }
}

#[test]
fn distilled_chaos_burning_pact_requires_the_bound_auto_played_card() {
    for upgrades in [0, 1] {
        single_exhaust_selection_fixture(CardId::BurningPact, upgrades, 0, true);
    }
}

#[test]
fn hex_births_after_hand_selection_require_adjacent_generation_evidence() {
    for amount in [1, 2] {
        single_exhaust_selection_fixture(CardId::TrueGrit, 1, amount, false);
    }
}

#[test]
fn armaments_selection_preserves_held_cards_and_upgrades_only_the_bound_target() {
    for autoplay in [false, true] {
        let config = RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            player_class: "Ironclad",
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let mut bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut setup = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap.clone(),
            "fixture-rust",
            "armaments-setup",
        )
        .unwrap();
        let map = advance_to_map(&mut setup);
        execute(&mut setup, &map, &action_space(&map)[0]);
        let mut session = setup.into_session();
        for (slot, id, upgrades) in [(0, CardId::Armaments, 0), (1, CardId::Strike, 1)] {
            let card = session
                .run_state
                .master_deck
                .iter_mut()
                .find(|card| {
                    card.lineage
                        == sts_oracle_run_control::runtime::combat::CardLineageId::master_deck(
                            slot + 1,
                        )
                })
                .unwrap();
            card.id = id;
            card.upgrades = upgrades;
            let source = bootstrap
                .cards
                .iter_mut()
                .find(|card| card.binding == format!("natural_run/starter/{slot}"))
                .unwrap();
            source.card_id = sts_java_parity_protocol::canonical_game_id(id.stable_id());
            source.upgrades = upgrades;
        }
        let deck = &session.run_state.master_deck;
        let starter = |slot: u32| {
            deck.iter()
                .find(|card| {
                    card.lineage
                        == sts_oracle_run_control::runtime::combat::CardLineageId::master_deck(
                            slot + 1,
                        )
                })
                .unwrap()
                .clone()
        };
        let armaments = starter(0);
        let held_strike = starter(1);
        let first_strike = starter(2);
        let second_strike = starter(3);
        let bane = deck
            .iter()
            .find(|card| card.id == CardId::AscendersBane)
            .unwrap()
            .clone();
        let active = session.active_combat.as_mut().unwrap();
        active.combat_state.zones.hand = vec![held_strike, first_strike, bane, second_strike];
        active.combat_state.zones.draw_pile.clear();
        active.combat_state.zones.discard_pile.clear();
        active.combat_state.zones.exhaust_pile.clear();
        active.combat_state.zones.limbo.clear();
        active.combat_state.zones.queued_cards.clear();
        active.combat_state.entities.potions.clear();
        if autoplay {
            active.combat_state.zones.draw_pile = vec![armaments].into();
            active
                .combat_state
                .entities
                .potions
                .push(Some(Potion::new(PotionId::DistilledChaosPotion, 301)));
        } else {
            active.combat_state.zones.hand.insert(0, armaments);
        }
        active.engine_state = EngineState::CombatPlayerTurn;
        session.engine_state = EngineState::CombatPlayerTurn;
        let mut rust = RustLockstepAdapterV1::new(
            session,
            bootstrap.clone(),
            "fixture-rust",
            "armaments-rust",
        )
        .unwrap();
        let before = rust.observe().unwrap();
        let play = action_space(&before)
            .into_iter()
            .find(|action| match action {
                SemanticActionV1::PlayCard { card_id, .. } => !autoplay && card_id == "armaments",
                SemanticActionV1::UsePotion { potion_id, .. } => {
                    autoplay && potion_id == "distilledchaos"
                }
                _ => false,
            })
            .unwrap();
        let pending = execute(&mut rust, &before, &play);
        assert_eq!(pending.boundary_kind, "natural_hand_select");
        let pending_combat = observed(&pending, ParityDomainV1::Combat);
        assert_eq!(pending_combat["state"]["player"]["block"], 5);
        let hand = pending_combat["state"]["zones"]["hand"].as_array().unwrap();
        assert_eq!(
            hand.iter()
                .map(|card| card["card_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["strike", "strike", "strike", "ascendersbane"]
        );
        assert_eq!(hand[2]["upgrades"], 1);
        let choices = action_space(&pending);
        assert_eq!(choices.len(), 2);
        let select = choices.into_iter().find(|action| matches!(action,
            SemanticActionV1::SubmitSelection { choices, .. } if choices[0].id == "natural_run/starter/2"
        )).unwrap();
        let raw_before = equivalent_java_observation(&before, 300);
        let mut raw_pending = equivalent_java_successor(&rust, &pending, 301);
        attach_fixture_card_birth_witness(&mut raw_pending, &before, &pending, &play, 300, 0);
        let after = execute(&mut rust, &pending, &select);
        let after_combat = observed(&after, ParityDomainV1::Combat);
        let hand = after_combat["state"]["zones"]["hand"].as_array().unwrap();
        assert_eq!(
            hand.iter()
                .map(|card| (
                    card["binding"].as_str().unwrap(),
                    card["upgrades"].as_u64().unwrap()
                ))
                .collect::<Vec<_>>(),
            [
                ("natural_run/starter/3", 0),
                ("natural_run/starter/2", 1),
                ("natural_run/starter/1", 1),
                ("natural_run/ascension_10/ascenders_bane", 0)
            ]
        );
        assert_eq!(
            observed(&after, ParityDomainV1::MasterDeck),
            observed(&before, ParityDomainV1::MasterDeck)
        );
        let mut raw_after = equivalent_java_successor(&rust, &after, 302);
        attach_fixture_card_birth_witness(&mut raw_after, &pending, &after, &select, 301, 1);
        for fault in ["missing_held", "duplicate_held", "missing_in_use"] {
            let mut invalid = raw_pending.clone();
            match fault {
                "missing_held" => {
                    invalid.frame["parity_internal"]["zones"]["armaments_held"] = Value::Null
                }
                "duplicate_held" => {
                    invalid.frame["parity_internal"]["zones"]["armaments_held"]["cards"][0] =
                        invalid.frame["parity_internal"]["zones"]["hand"]["cards"][0].clone()
                }
                "missing_in_use" => {
                    invalid.frame["parity_internal"]["zones"]["card_in_use"] = Value::Null
                }
                _ => unreachable!(),
            }
            let mut java = JavaLockstepAdapterV1::new(
                FixtureTransport {
                    observations: vec![raw_before.clone(), invalid],
                    cursor: 0,
                },
                bootstrap.clone(),
            )
            .unwrap();
            let entry = java.observe().unwrap();
            let prepared = java.prepare(&play, &entry).unwrap();
            assert!(java.execute(prepared).is_err(), "{fault} must fail closed");
        }
        let calls = Rc::new(RefCell::new(FixtureTransportCalls::default()));
        let mut java = JavaLockstepAdapterV1::new(
            RecordingFixtureTransport {
                inner: FixtureTransport {
                    observations: vec![raw_before, raw_pending, raw_after],
                    cursor: 0,
                },
                calls: calls.clone(),
            },
            bootstrap,
        )
        .unwrap();
        let entry = java.observe().unwrap();
        assert_exact(&entry, &before);
        let prepared = java.prepare(&play, &entry).unwrap();
        let paused = java.execute(prepared).unwrap();
        assert_exact(&paused, &pending);
        let prepared = java.prepare(&select, &paused).unwrap();
        assert_exact(&java.execute(prepared).unwrap(), &after);
        assert_eq!(
            calls.borrow().hand_selections,
            vec![vec!["fixture-card-3".to_owned()]]
        );
    }
}

#[test]
fn dual_wield_selection_preserves_held_objects_and_requires_fresh_copy_births() {
    for (upgrades, autoplay, corruption) in [
        (0, false, false),
        (1, false, false),
        (0, true, false),
        (1, true, false),
        (0, false, true),
        (1, false, true),
        (0, true, true),
        (1, true, true),
    ] {
        let config = RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            player_class: "Ironclad",
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let mut bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut setup = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap.clone(),
            "fixture-rust",
            "dual-wield-setup",
        )
        .unwrap();
        let map = advance_to_map(&mut setup);
        execute(&mut setup, &map, &action_space(&map)[0]);
        let mut session = setup.into_session();
        let mut hand = Vec::new();
        for (slot, id, level) in [
            (0, CardId::DualWield, upgrades),
            (1, CardId::Defend, 0),
            (2, CardId::Rampage, 0),
            (3, CardId::Strike, 0),
        ] {
            let card = session
                .run_state
                .master_deck
                .iter_mut()
                .find(|card| {
                    card.lineage
                        == sts_oracle_run_control::runtime::combat::CardLineageId::master_deck(
                            slot + 1,
                        )
                })
                .unwrap();
            card.id = id;
            card.upgrades = level;
            let source = bootstrap
                .cards
                .iter_mut()
                .find(|card| card.binding == format!("natural_run/starter/{slot}"))
                .unwrap();
            source.card_id = sts_java_parity_protocol::canonical_game_id(id.stable_id());
            source.upgrades = level;
            hand.push(card.clone());
        }
        // The chosen Rampage has grown in combat; copying must retain that
        // persistent damage while replacing the selected original identity.
        hand[2].base_damage_override = Some(13);
        let active = session.active_combat.as_mut().unwrap();
        active.combat_state.zones.hand = hand;
        active.combat_state.zones.draw_pile.clear();
        active.combat_state.zones.discard_pile.clear();
        active.combat_state.zones.exhaust_pile.clear();
        active.combat_state.zones.limbo.clear();
        active.combat_state.zones.queued_cards.clear();
        active.combat_state.entities.potions.clear();
        if corruption {
            sts_oracle_run_control::content::powers::store::set_powers_for(
                &mut active.combat_state,
                0,
                vec![sts_oracle_run_control::runtime::combat::Power {
                    power_type: sts_oracle_run_control::content::powers::PowerId::Corruption,
                    amount: -1,
                    instance_id: None,
                    extra_data: 0,
                    payload: sts_oracle_run_control::runtime::combat::PowerPayload::None,
                    just_applied: false,
                }],
            );
            for card in &mut active.combat_state.zones.hand {
                if matches!(card.id, CardId::DualWield | CardId::Defend) {
                    card.set_cost_for_turn_java(-9);
                }
            }
        }
        if autoplay {
            let dual_wield = active.combat_state.zones.hand.remove(0);
            active.combat_state.zones.draw_pile = vec![dual_wield].into();
            active
                .combat_state
                .entities
                .potions
                .push(Some(Potion::new(PotionId::DistilledChaosPotion, 301)));
        }
        active.engine_state = EngineState::CombatPlayerTurn;
        session.engine_state = EngineState::CombatPlayerTurn;
        let mut rust = RustLockstepAdapterV1::new(
            session,
            bootstrap.clone(),
            "fixture-rust",
            "dual-wield-rust",
        )
        .unwrap();
        let before = rust.observe().unwrap();
        let play = action_space(&before)
            .into_iter()
            .find(|action| match action {
                SemanticActionV1::PlayCard { card_id, .. } => !autoplay && card_id == "dualwield",
                SemanticActionV1::UsePotion { potion_id, .. } => {
                    autoplay && potion_id == "distilledchaos"
                }
                _ => false,
            })
            .unwrap();
        let pending = execute(&mut rust, &before, &play);
        assert_eq!(pending.boundary_kind, "natural_hand_select");
        let hand = observed(&pending, ParityDomainV1::Combat)["state"]["zones"]["hand"]
            .as_array()
            .unwrap();
        assert_eq!(
            hand.iter()
                .map(|card| card["card_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["rampage", "strike", "defend"]
        );
        let choices = action_space(&pending);
        assert_eq!(choices.len(), 2);
        let select = choices.into_iter().find(|action| matches!(action,
            SemanticActionV1::SubmitSelection { choices, .. } if choices[0].id == "natural_run/starter/2"
        )).unwrap();
        let raw_before = equivalent_java_observation(&before, 300);
        let mut raw_pending = equivalent_java_successor(&rust, &pending, 301);
        attach_fixture_card_birth_witness(&mut raw_pending, &before, &pending, &play, 300, 0);
        let after = execute(&mut rust, &pending, &select);
        let hand = observed(&after, ParityDomainV1::Combat)["state"]["zones"]["hand"]
            .as_array()
            .unwrap();
        assert_eq!(hand.len(), 4 + usize::from(upgrades));
        let zones = &observed(&after, ParityDomainV1::Combat)["state"]["zones"];
        let disposal = if corruption {
            "exhaust_by_binding"
        } else {
            "discard_bottom_first"
        };
        assert_eq!(zones[disposal][0]["card_id"], "dualwield");
        assert_eq!(hand[0]["binding"], "natural_run/starter/3");
        assert_eq!(hand[1]["binding"], "natural_run/starter/1");
        for copy in &hand[2..] {
            assert_eq!(copy["card_id"], "rampage");
            assert_eq!(copy["base_damage"], 13);
            assert_ne!(copy["binding"], "natural_run/starter/2");
        }
        assert_eq!(
            observed(&after, ParityDomainV1::MasterDeck),
            observed(&before, ParityDomainV1::MasterDeck)
        );
        let mut raw_after = equivalent_java_successor(&rust, &after, 302);
        attach_fixture_card_birth_witness(&mut raw_after, &pending, &after, &select, 301, 1);
        assert_eq!(
            raw_after.frame["card_birth_witness"]["products"]
                .as_array()
                .unwrap()
                .len(),
            2 + usize::from(upgrades)
        );
        for fault in [
            "missing_held",
            "duplicate_held",
            "missing_in_use",
            "wrong_producer",
            "missing_birth",
            "old_object_birth",
        ] {
            let mut invalid_pending = raw_pending.clone();
            let mut invalid_after = raw_after.clone();
            match fault {
                "missing_held" => {
                    invalid_pending.frame["parity_internal"]["zones"]["dual_wield_held"] =
                        Value::Null
                }
                "duplicate_held" => {
                    invalid_pending.frame["parity_internal"]["zones"]["dual_wield_held"]["cards"]
                        [0] = invalid_pending.frame["parity_internal"]["zones"]["hand"]["cards"][0]
                        .clone()
                }
                "missing_in_use" => {
                    invalid_pending.frame["parity_internal"]["zones"]["card_in_use"] = Value::Null
                }
                "wrong_producer" => {
                    invalid_pending.frame["parity_internal"]["zones"]["card_in_use"]["card_id"] =
                        json!("Armaments")
                }
                "missing_birth" => invalid_after.frame["card_birth_witness"] = Value::Null,
                "old_object_birth" => {
                    invalid_after.frame["card_birth_witness"]["products"][0]["java_uuid"] =
                        json!(fixture_card_uuid(&before, "natural_run/starter/1"))
                }
                _ => unreachable!(),
            }
            let mut java = JavaLockstepAdapterV1::new(
                FixtureTransport {
                    observations: vec![raw_before.clone(), invalid_pending, invalid_after],
                    cursor: 0,
                },
                bootstrap.clone(),
            )
            .unwrap();
            let entry = java.observe().unwrap();
            let prepared = java.prepare(&play, &entry).unwrap();
            let result = java.execute(prepared).and_then(|paused| {
                let prepared = java.prepare(&select, &paused)?;
                java.execute(prepared)
            });
            assert!(
                result.is_err(),
                "{fault} must fail closed (autoplay={autoplay})"
            );
        }
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![raw_before, raw_pending, raw_after],
                cursor: 0,
            },
            bootstrap,
        )
        .unwrap();
        let entry = java.observe().unwrap();
        assert_exact(&entry, &before);
        let prepared = java.prepare(&play, &entry).unwrap();
        let paused = java.execute(prepared).unwrap();
        assert_exact(&paused, &pending);
        let prepared = java.prepare(&select, &paused).unwrap();
        assert_exact(&java.execute(prepared).unwrap(), &after);
    }
}

#[test]
fn well_laid_plans_retains_bound_cards_before_the_enemy_turn() {
    use sts_oracle_run_control::content::powers::{store, PowerId};
    for (amount, pick_count, tools) in [
        (1, 0, false),
        (1, 1, false),
        (2, 2, false),
        (9, 0, false),
        (1, 1, true),
    ] {
        let config = RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            player_class: "Silent",
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut setup = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap.clone(),
            "fixture-rust",
            "retain-setup",
        )
        .unwrap();
        let map = advance_to_map(&mut setup);
        let route = action_space(&map).remove(0);
        execute(&mut setup, &map, &route);
        let mut session = setup.into_session();
        let mut powers = vec![sts_oracle_run_control::runtime::combat::Power {
            power_type: PowerId::RetainCards,
            amount,
            extra_data: 0,
            instance_id: None,
            payload: sts_oracle_run_control::runtime::combat::PowerPayload::None,
            just_applied: false,
        }];
        if tools {
            powers.push(sts_oracle_run_control::runtime::combat::Power {
                power_type: PowerId::ToolsOfTheTrade,
                amount: 1,
                ..powers[0].clone()
            });
        }
        store::set_powers_for(
            &mut session.active_combat.as_mut().unwrap().combat_state,
            0,
            powers,
        );
        let mut rust =
            RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture-rust", "retain-rust")
                .unwrap();
        let before = rust.observe().unwrap();
        let pending = execute(&mut rust, &before, &SemanticActionV1::EndTurn);
        assert_eq!(pending.boundary_kind, "natural_hand_select");
        assert_eq!(
            observed(&pending, ParityDomainV1::Combat)["state"]["turn"],
            observed(&before, ParityDomainV1::Combat)["state"]["turn"]
        );
        let select = action_space(&pending)
            .into_iter()
            .find(|action| {
                matches!(action,
            SemanticActionV1::SubmitSelection { choices, .. } if choices.len() == pick_count
                && choices.iter().all(|card| card.attributes.get("card_id")
                    != Some(&SemanticScalarV1::Text("ascendersbane".into()))))
            })
            .unwrap();
        let mut raw_pending = equivalent_java_successor(&rust, &pending, 201);
        attach_fixture_card_birth_witness(
            &mut raw_pending,
            &before,
            &pending,
            &SemanticActionV1::EndTurn,
            200,
            0,
        );
        let after = execute(&mut rust, &pending, &select);
        assert_eq!(
            after.boundary_kind,
            if tools {
                "natural_hand_select"
            } else {
                "natural_combat"
            }
        );
        assert_eq!(
            observed(&after, ParityDomainV1::Combat)["state"]["turn"].as_u64(),
            Some(
                observed(&before, ParityDomainV1::Combat)["state"]["turn"]
                    .as_u64()
                    .unwrap()
                    + 1
            )
        );
        if let SemanticActionV1::SubmitSelection { choices, .. } = &select {
            let hand = observed(&after, ParityDomainV1::Combat)["state"]["zones"]["hand"]
                .as_array()
                .unwrap();
            assert!(choices
                .iter()
                .all(|choice| hand.iter().any(|card| card["binding"] == choice.id)));
        }
        let mut raw_after = equivalent_java_successor(&rust, &after, 202);
        attach_fixture_card_birth_witness(&mut raw_after, &pending, &after, &select, 201, 1);
        let trade = if tools {
            let discard = action_space(&after)
                .into_iter()
                .find(|action| {
                    matches!(action,
                SemanticActionV1::SubmitSelection { choices, .. } if choices.len() == 1)
                })
                .unwrap();
            let final_state = execute(&mut rust, &after, &discard);
            let mut raw_final = equivalent_java_successor(&rust, &final_state, 203);
            attach_fixture_card_birth_witness(
                &mut raw_final,
                &after,
                &final_state,
                &discard,
                202,
                2,
            );
            Some((discard, final_state, raw_final))
        } else {
            None
        };
        for fault in [
            "missing_power",
            "next_turn",
            "wrong_count",
            "wrong_flags",
            "card_in_use",
        ] {
            let mut entry = equivalent_java_observation(&before, 200);
            let mut next = raw_pending.clone();
            match fault {
                "missing_power" => entry.frame["parity_internal"]["player_powers"] = json!([]),
                "next_turn" => {
                    next.frame["public_base"]["combat"]["turn"] = json!(
                        observed(&before, ParityDomainV1::Combat)["state"]["turn"]
                            .as_u64()
                            .unwrap()
                            + 1
                    )
                }
                "wrong_count" => {
                    next.frame["public_base"]["decision"]["hand_select"]["requested_cards"] =
                        json!(amount + 1)
                }
                "wrong_flags" => {
                    next.frame["public_base"]["decision"]["hand_select"]["up_to"] = json!(false)
                }
                _ => next.frame["parity_internal"]["zones"]["card_in_use"] = json!({}),
            }
            let mut java = JavaLockstepAdapterV1::new(
                FixtureTransport {
                    observations: vec![entry, next],
                    cursor: 0,
                },
                bootstrap.clone(),
            )
            .unwrap();
            let entry = java.observe().unwrap();
            let prepared = java.prepare(&SemanticActionV1::EndTurn, &entry).unwrap();
            assert!(java.execute(prepared).is_err(), "{fault}");
        }
        let mut observations = vec![
            equivalent_java_observation(&before, 200),
            raw_pending,
            raw_after,
        ];
        if let Some((_, _, raw_final)) = &trade {
            observations.push(raw_final.clone());
        }
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations,
                cursor: 0,
            },
            bootstrap,
        )
        .unwrap();
        let entry = java.observe().unwrap();
        assert_exact(&entry, &before);
        let prepared = java.prepare(&SemanticActionV1::EndTurn, &entry).unwrap();
        let paused = java.execute(prepared).unwrap();
        assert_exact(&paused, &pending);
        let prepared = java.prepare(&select, &paused).unwrap();
        let next = java.execute(prepared).unwrap();
        assert_exact(&next, &after);
        if let Some((discard, final_state, _)) = trade {
            let prepared = java.prepare(&discard, &next).unwrap();
            assert_exact(&java.execute(prepared).unwrap(), &final_state);
        }
    }
}

#[test]
fn tools_of_the_trade_draws_then_discards_bound_cards_and_triggers_kite_once() {
    use sts_oracle_run_control::content::{
        powers::{store, PowerId},
        relics::{RelicId, RelicState},
    };
    for amount in [1, 2] {
        let config = RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            player_class: "Silent",
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut setup = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap.clone(),
            "fixture-rust",
            "tools-setup",
        )
        .unwrap();
        let map = advance_to_map(&mut setup);
        let route = action_space(&map).remove(0);
        execute(&mut setup, &map, &route);
        let mut session = setup.into_session();
        let mut kite = RelicState::new(RelicId::HoveringKite);
        kite.used_up = true;
        session.run_state.relics.push(kite.clone());
        let active = session.active_combat.as_mut().unwrap();
        active.combat_state.entities.player.add_relic(kite);
        store::set_powers_for(
            &mut active.combat_state,
            0,
            vec![sts_oracle_run_control::runtime::combat::Power {
                power_type: PowerId::ToolsOfTheTrade,
                amount,
                extra_data: 0,
                instance_id: None,
                payload: sts_oracle_run_control::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut rust =
            RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture-rust", "tools-rust")
                .unwrap();
        let before = rust.observe().unwrap();
        let pending = execute(&mut rust, &before, &SemanticActionV1::EndTurn);
        assert_eq!(pending.boundary_kind, "natural_hand_select");
        let pending_combat = observed(&pending, ParityDomainV1::Combat);
        assert_eq!(
            pending_combat["state"]["zones"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            5 + amount as usize
        );
        assert_eq!(pending_combat["state"]["player"]["energy"], 3);
        let pending_relics = observed(&pending, ParityDomainV1::Relics);
        assert_eq!(
            pending_relics[1]["combat_state"]["triggered_this_turn"],
            false
        );
        let select = action_space(&pending).into_iter().find(|action| matches!(
            action, SemanticActionV1::SubmitSelection { choices, .. } if choices.len() == amount as usize
        )).unwrap();
        let mut raw_pending = equivalent_java_successor(&rust, &pending, 201);
        attach_fixture_card_birth_witness(
            &mut raw_pending,
            &before,
            &pending,
            &SemanticActionV1::EndTurn,
            200,
            0,
        );
        let after = execute(&mut rust, &pending, &select);
        assert_eq!(after.boundary_kind, "natural_combat");
        assert_eq!(
            observed(&after, ParityDomainV1::Combat)["state"]["player"]["energy"],
            4
        );
        assert_eq!(
            observed(&after, ParityDomainV1::Combat)["state"]["player"]
                ["cards_discarded_this_turn"],
            amount
        );
        assert_eq!(
            observed(&after, ParityDomainV1::Relics)[1]["combat_state"]["triggered_this_turn"],
            true
        );
        let mut raw_after = equivalent_java_successor(&rust, &after, 202);
        attach_fixture_card_birth_witness(&mut raw_after, &pending, &after, &select, 201, 1);

        for invalid in [
            "missing_power",
            "same_turn",
            "wrong_count",
            "missing_card_in_use",
            "unbound",
        ] {
            let mut entry = equivalent_java_observation(&before, 200);
            let mut next = raw_pending.clone();
            match invalid {
                "missing_power" => entry.frame["parity_internal"]["player_powers"] = json!([]),
                "same_turn" => {
                    next.frame["public_base"]["combat"]["turn"] =
                        entry.frame["public_base"]["combat"]["turn"].clone()
                }
                "wrong_count" => {
                    next.frame["public_base"]["decision"]["hand_select"]["requested_cards"] =
                        json!(amount + 1)
                }
                "missing_card_in_use" => {
                    next.frame["parity_internal"]["zones"]
                        .as_object_mut()
                        .unwrap()
                        .remove("card_in_use");
                }
                _ => {}
            }
            let observations = if invalid == "unbound" {
                vec![next]
            } else {
                vec![entry, next]
            };
            let mut java = JavaLockstepAdapterV1::new(
                FixtureTransport {
                    observations,
                    cursor: 0,
                },
                bootstrap.clone(),
            )
            .unwrap();
            if invalid == "unbound" {
                assert!(java.observe().is_err());
            } else {
                let entry = java.observe().unwrap();
                let prepared = java.prepare(&SemanticActionV1::EndTurn, &entry).unwrap();
                assert!(java.execute(prepared).is_err(), "{invalid}");
            }
        }
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![
                    equivalent_java_observation(&before, 200),
                    raw_pending,
                    raw_after,
                ],
                cursor: 0,
            },
            bootstrap,
        )
        .unwrap();
        let entry = java.observe().unwrap();
        assert_exact(&entry, &before);
        let prepared = java.prepare(&SemanticActionV1::EndTurn, &entry).unwrap();
        let paused = java.execute(prepared).unwrap();
        assert_exact(&paused, &pending);
        let prepared = java.prepare(&select, &paused).unwrap();
        assert_exact(&java.execute(prepared).unwrap(), &after);
    }
}

#[test]
fn setup_binds_the_selected_card_and_spends_its_waiver_after_drawing() {
    for upgrades in [0, 1] {
        let config = RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            player_class: "Silent",
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let mut bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut setup = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap.clone(),
            "fixture-rust",
            "setup-entry",
        )
        .unwrap();
        let map = advance_to_map(&mut setup);
        let route = action_space(&map).remove(0);
        execute(&mut setup, &map, &route);
        let mut session = setup.into_session();
        let source_slot = session
            .run_state
            .master_deck
            .iter()
            .position(|card| card.id == CardId::StrikeG)
            .unwrap();
        let deck = &mut session.run_state.master_deck;
        deck[source_slot].id = CardId::Setup;
        deck[source_slot].upgrades = upgrades;
        bootstrap.cards[0].card_id = "setup".into();
        bootstrap.cards[0].upgrades = upgrades;
        let played = deck[source_slot].clone();
        let defends = deck
            .iter()
            .filter(|card| card.id == CardId::DefendG)
            .cloned()
            .collect::<Vec<_>>();
        let other = deck
            .iter()
            .filter(|card| {
                card.lineage != played.lineage
                    && card.lineage != defends[0].lineage
                    && card.lineage != defends[1].lineage
            })
            .cloned()
            .collect::<Vec<_>>();
        let active = session.active_combat.as_mut().unwrap();
        active.combat_state.zones.hand = vec![played, defends[0].clone(), defends[1].clone()];
        active.combat_state.zones.draw_pile = other.into();
        active.combat_state.zones.discard_pile.clear();
        active.combat_state.zones.exhaust_pile.clear();
        active.combat_state.zones.limbo.clear();
        active.combat_state.turn.energy = 3;
        active.combat_state.entities.monsters[0].current_hp = 500;
        active.combat_state.entities.monsters[0].max_hp = 500;
        let mut rust =
            RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture-rust", "setup-rust")
                .unwrap();
        let mut current = rust.observe().unwrap();
        let mut frames = vec![equivalent_java_observation(&current, 200)];
        let mut snapshots = vec![current.clone()];
        let mut adopted = Vec::new();
        let play = action_space(&current).into_iter().find(|action| matches!(action, SemanticActionV1::PlayCard { card_id, .. } if card_id == "setup")).unwrap();
        current = execute(&mut rust, &current, &play);
        assert_eq!(current.boundary_kind, "natural_hand_select");
        frames.push(equivalent_java_successor(&rust, &current, 201));
        snapshots.push(current.clone());
        adopted.push(play);
        let select = action_space(&current).into_iter().find(|action| matches!(action, SemanticActionV1::SubmitSelection { choices, .. } if choices.len() == 1)).unwrap();
        let SemanticActionV1::SubmitSelection { choices, .. } = &select else {
            unreachable!()
        };
        let selected_binding = choices[0].id.clone();
        current = execute(&mut rust, &current, &select);
        let next =
            &observed(&current, ParityDomainV1::Combat)["state"]["zones"]["draw_next_first"][0];
        assert_eq!(next["binding"], selected_binding);
        assert_eq!(next["combat_cost"], 1);
        assert_eq!(next["cost_for_turn"], 1);
        assert_eq!(next["free_to_play_once"], true);
        frames.push(equivalent_java_successor(&rust, &current, 202));
        snapshots.push(current.clone());
        adopted.push(select);
        current = execute(&mut rust, &current, &SemanticActionV1::EndTurn);
        frames.push(equivalent_java_successor(&rust, &current, 203));
        snapshots.push(current.clone());
        adopted.push(SemanticActionV1::EndTurn);
        for _ in 0..3 {
            let paid = action_space(&current).into_iter().find(|action| {
                matches!(action, SemanticActionV1::PlayCard { card_binding, .. } if card_binding != &selected_binding)
            }).expect("fixture retains three ordinary one-cost attacks");
            current = execute(&mut rust, &current, &paid);
            frames.push(equivalent_java_successor(
                &rust,
                &current,
                200 + frames.len() as u64,
            ));
            snapshots.push(current.clone());
            adopted.push(paid);
        }
        let energy =
            observed(&current, ParityDomainV1::Combat)["state"]["player"]["energy"].clone();
        assert_eq!(
            energy, 0,
            "the free target must remain legal without enough energy for its printed cost"
        );
        let free_play = action_space(&current).into_iter().find(|action| matches!(action, SemanticActionV1::PlayCard { card_binding, .. } if card_binding == &selected_binding)).unwrap();
        current = execute(&mut rust, &current, &free_play);
        assert_eq!(
            observed(&current, ParityDomainV1::Combat)["state"]["player"]["energy"],
            energy
        );
        let spent = observed(&current, ParityDomainV1::Combat)["state"]["zones"]
            ["discard_bottom_first"]
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["binding"] == selected_binding)
            .unwrap();
        assert_ne!(spent["free_to_play_once"], true);
        frames.push(equivalent_java_successor(
            &rust,
            &current,
            200 + frames.len() as u64,
        ));
        snapshots.push(current.clone());
        adopted.push(free_play);
        let mut ordinal = 0;
        for (index, action) in adopted.iter().enumerate() {
            attach_fixture_card_birth_witness(
                &mut frames[index + 1],
                &snapshots[index],
                &snapshots[index + 1],
                action,
                200 + index as u64,
                ordinal,
            );
            if matches!(
                action,
                SemanticActionV1::PlayCard { .. } | SemanticActionV1::EndTurn
            ) {
                ordinal += 1;
            }
        }
        let mut missing_flags = frames.clone();
        missing_flags[1].frame["parity_internal"]["zones"]["hand"]["cards"][0]
            .as_object_mut()
            .unwrap()
            .remove("free_to_play_once");
        let mut missing = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: missing_flags,
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let entry = missing.observe().unwrap();
        let prepared = missing.prepare(&adopted[0], &entry).unwrap();
        assert!(missing
            .execute(prepared)
            .unwrap_err()
            .message
            .contains("observed free-to-play-once"));
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: frames,
                cursor: 0,
            },
            bootstrap,
        )
        .unwrap();
        let mut java_current = java.observe().unwrap();
        for (action, expected) in adopted.into_iter().zip(&snapshots[1..]) {
            let prepared = java.prepare(&action, &java_current).unwrap();
            java_current = java.execute(prepared).unwrap();
            assert_exact(&java_current, expected);
        }
    }
}

#[test]
fn discard_cards_draw_before_source_bound_choice_at_both_upgrades() {
    for (card_id, shared_id, upgrades, draw_count, discard_count) in [
        (CardId::DaggerThrow, "daggerthrow", 0, 1, 1),
        (CardId::DaggerThrow, "daggerthrow", 1, 1, 1),
        (CardId::Acrobatics, "acrobatics", 0, 3, 1),
        (CardId::Acrobatics, "acrobatics", 1, 4, 1),
        (CardId::Prepared, "prepared", 0, 1, 1),
        (CardId::Prepared, "prepared", 1, 2, 2),
    ] {
        let config = RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            final_act: true,
            player_class: "Silent",
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let mut bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut setup = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap.clone(),
            "fixture-rust",
            "discard-setup",
        )
        .unwrap();
        let map = advance_to_map(&mut setup);
        let route = action_space(&map).remove(0);
        execute(&mut setup, &map, &route);
        let mut session = setup.into_session();
        let strike_index = session
            .run_state
            .master_deck
            .iter()
            .position(|card| card.id == CardId::StrikeG)
            .unwrap();
        session.run_state.master_deck[strike_index].id = card_id;
        session.run_state.master_deck[strike_index].upgrades = upgrades;
        bootstrap.cards[0].card_id = shared_id.into();
        bootstrap.cards[0].upgrades = upgrades;
        let deck = &session.run_state.master_deck;
        let played = deck[strike_index].clone();
        let drawn = deck
            .iter()
            .filter(|card| card.id == CardId::StrikeG)
            .take(draw_count)
            .cloned()
            .collect::<Vec<_>>();
        let defends = deck
            .iter()
            .filter(|card| card.id == CardId::DefendG)
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        let active = session.active_combat.as_mut().unwrap();
        active.combat_state.zones.draw_pile = drawn.into();
        active.combat_state.zones.discard_pile.clear();
        active.combat_state.zones.exhaust_pile.clear();
        active.combat_state.zones.limbo.clear();
        active.combat_state.zones.queued_cards.clear();
        active.combat_state.zones.hand = vec![played, defends[0].clone(), defends[1].clone()];
        let mut target = active.combat_state.entities.monsters[0].clone();
        target.current_hp = 30;
        target.max_hp = 30;
        let target_id = target.id;
        active.combat_state.entities.monsters = vec![target];
        sts_oracle_run_control::content::powers::store::set_powers_for(
            &mut active.combat_state,
            target_id,
            vec![sts_oracle_run_control::runtime::combat::Power {
                power_type: sts_oracle_run_control::content::powers::PowerId::CurlUp,
                amount: 11,
                extra_data: 0,
                instance_id: None,
                payload: sts_oracle_run_control::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        active.engine_state = EngineState::CombatPlayerTurn;
        session.engine_state = EngineState::CombatPlayerTurn;
        let mut rust =
            RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture-rust", "discard-rust")
                .unwrap();
        let before = rust.observe().unwrap();
        let play = action_space(&before).into_iter().find(|action| {
            matches!(action, SemanticActionV1::PlayCard { card_id, .. } if card_id == shared_id)
        }).unwrap();
        let pending = execute(&mut rust, &before, &play);
        assert_eq!(pending.boundary_kind, "natural_hand_select");
        assert_eq!(
            observed(&pending, ParityDomainV1::Combat)["state"]["monsters"][0]["block"],
            0
        );
        assert_eq!(
            observed(&pending, ParityDomainV1::Combat)["state"]["monsters"][0]["powers"][0]
                ["state"]["kind"],
            if card_id == CardId::DaggerThrow {
                "curl_up_triggered"
            } else {
                "stateless"
            }
        );
        assert_eq!(
            observed(&pending, ParityDomainV1::Combat)["state"]["zones"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            2 + draw_count
        );
        let mut raw_pending = equivalent_java_successor(&rust, &pending, 201);
        attach_fixture_card_birth_witness(&mut raw_pending, &before, &pending, &play, 200, 0);
        let play_rng = rust.last_rng_call_ledger_v1().unwrap().clone();
        let discard = action_space(&pending).into_iter().find(|action| {
            matches!(action, SemanticActionV1::SubmitSelection { surface: DecisionSurfaceV1::HandSelect, choices }
                if choices.len() == discard_count && choices.iter().any(|c| c.id == "natural_run/starter/1"))
        }).expect("the newly drawn Strike must already be a discard candidate");
        let after = execute(&mut rust, &pending, &discard);
        assert_eq!(after.boundary_kind, "natural_combat");
        assert_eq!(
            observed(&after, ParityDomainV1::Combat)["state"]["monsters"][0]["block"],
            if card_id == CardId::DaggerThrow {
                11
            } else {
                0
            }
        );
        assert_eq!(
            observed(&after, ParityDomainV1::Combat)["state"]["monsters"][0]["powers"]
                .as_array()
                .unwrap()
                .is_empty(),
            card_id == CardId::DaggerThrow
        );
        assert_eq!(
            observed(&after, ParityDomainV1::Combat)["state"]["zones"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            2 + draw_count - discard_count
        );
        let mut raw_after = equivalent_java_successor(&rust, &after, 202);
        attach_fixture_card_birth_witness(&mut raw_after, &pending, &after, &discard, 201, 1);
        let discard_rng = rust.last_rng_call_ledger_v1().unwrap().clone();

        let mut unbound = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![raw_pending.clone()],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        assert!(
            unbound.observe().is_err(),
            "a hand-selection page alone supplies no predecessor authorization"
        );

        let mut missing_flag = raw_pending.clone();
        missing_flag.frame["parity_internal"]["monsters"][0]["powers"][0]
            .as_object_mut()
            .unwrap()
            .remove("curl_up_triggered");
        let mut incomplete = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![equivalent_java_observation(&before, 200), missing_flag],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let entry = incomplete.observe().unwrap();
        let prepared = incomplete.prepare(&play, &entry).unwrap();
        assert!(incomplete
            .execute(prepared)
            .unwrap_err()
            .message
            .contains("observed triggered flag"));

        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![
                    equivalent_java_observation(&before, 200),
                    raw_pending,
                    raw_after,
                ],
                cursor: 0,
            },
            bootstrap,
        )
        .unwrap();
        let entry = java.observe().unwrap();
        assert_exact(&entry, &before);
        let prepared = java.prepare(&play, &entry).unwrap();
        let paused = java.execute(prepared).unwrap();
        assert_exact(&paused, &pending);
        assert!(compare_natural_run_rng_calls_v1(
            java.last_rng_call_ledger_v1().unwrap(),
            &play_rng
        )
        .unwrap()
        .stop_reason()
        .is_none());
        let prepared = java.prepare(&discard, &paused).unwrap();
        assert_exact(&java.execute(prepared).unwrap(), &after);
        assert!(compare_natural_run_rng_calls_v1(
            java.last_rng_call_ledger_v1().unwrap(),
            &discard_rng
        )
        .unwrap()
        .stop_reason()
        .is_none());
    }
}

fn single_exhaust_selection_fixture(
    card_id: CardId,
    upgrades: u8,
    hex_amount: i32,
    autoplay: bool,
) {
    let shared_id = match card_id {
        CardId::TrueGrit => "truegrit",
        CardId::BurningPact => "burningpact",
        _ => panic!("fixture requires a supported single-exhaust card"),
    };
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let mut bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut setup = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "fixture-rust",
        "single-exhaust-setup-epoch",
    )
    .unwrap();
    let map = advance_to_map(&mut setup);
    let route = action_space(&map).into_iter().next().unwrap();
    execute(&mut setup, &map, &route);

    let mut session = setup.into_session();
    let strike_index = session
        .run_state
        .master_deck
        .iter()
        .position(|card| card.id == CardId::Strike)
        .unwrap();
    let defend_indices = session
        .run_state
        .master_deck
        .iter()
        .enumerate()
        .filter_map(|(index, card)| (card.id == CardId::Defend).then_some(index))
        .take(2)
        .collect::<Vec<_>>();
    let [first_defend_index, second_defend_index] = defend_indices.as_slice() else {
        panic!("Ironclad fixture omitted two Defends")
    };
    session.run_state.master_deck[strike_index].id = card_id;
    session.run_state.master_deck[strike_index].upgrades = upgrades;
    let exhaust_card = session.run_state.master_deck[strike_index].clone();
    let first_defend = session.run_state.master_deck[*first_defend_index].clone();
    let second_defend = session.run_state.master_deck[*second_defend_index].clone();
    let bootstrap_exhaust_card = bootstrap
        .cards
        .iter_mut()
        .find(|card| card.binding == "natural_run/starter/0")
        .unwrap();
    bootstrap_exhaust_card.card_id = shared_id.to_owned();
    bootstrap_exhaust_card.upgrades = upgrades;

    let pact_draws = session
        .run_state
        .master_deck
        .iter()
        .filter(|card| card.id == CardId::Strike)
        .take(3)
        .cloned()
        .collect::<Vec<_>>();

    let active = session.active_combat.as_mut().unwrap();
    active.combat_state.zones.draw_pile.clear();
    active.combat_state.zones.discard_pile.clear();
    active.combat_state.zones.exhaust_pile.clear();
    active.combat_state.zones.limbo.clear();
    active.combat_state.zones.queued_cards.clear();
    let mut art_of_war = sts_oracle_run_control::content::relics::RelicState::new(
        sts_oracle_run_control::content::relics::RelicId::ArtOfWar,
    );
    art_of_war.counter = 0;
    active.combat_state.entities.player.relics.push(art_of_war);
    if autoplay {
        active.combat_state.zones.draw_pile = vec![exhaust_card].into();
        active.combat_state.zones.hand = vec![first_defend, second_defend];
    } else {
        active.combat_state.zones.hand = vec![exhaust_card, first_defend, second_defend];
        if card_id == CardId::BurningPact {
            active.combat_state.zones.draw_pile = pact_draws.clone().into();
        }
    }
    if hex_amount > 0 {
        let player_id = active.combat_state.entities.player.id;
        sts_oracle_run_control::content::powers::store::set_powers_for(
            &mut active.combat_state,
            player_id,
            vec![sts_oracle_run_control::runtime::combat::Power {
                power_type: sts_oracle_run_control::content::powers::PowerId::Hex,
                amount: hex_amount,
                instance_id: None,
                extra_data: 0,
                payload: sts_oracle_run_control::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
    }
    active.combat_state.entities.potions = vec![
        Some(sts_oracle_run_control::content::potions::Potion::new(
            sts_oracle_run_control::content::potions::PotionId::GamblersBrew,
            301,
        )),
        Some(sts_oracle_run_control::content::potions::Potion::new(
            sts_oracle_run_control::content::potions::PotionId::EnergyPotion,
            302,
        )),
    ];
    if autoplay {
        active.combat_state.entities.potions[0] =
            Some(sts_oracle_run_control::content::potions::Potion::new(
                sts_oracle_run_control::content::potions::PotionId::DistilledChaosPotion,
                301,
            ));
    }
    active.engine_state = EngineState::CombatPlayerTurn;
    session.engine_state = EngineState::CombatPlayerTurn;

    let mut rust = RustLockstepAdapterV1::new(
        session,
        bootstrap.clone(),
        "fixture-rust",
        "single-exhaust-rust-epoch",
    )
    .unwrap();
    let combat = rust.observe().unwrap();
    let exhaust_card = action_space(&combat)
        .into_iter()
        .find(|action| {
            (!autoplay
                && matches!(
                    action,
                    SemanticActionV1::PlayCard { card_id, .. } if card_id == shared_id
                ))
                || (autoplay
                    && matches!(action,
                SemanticActionV1::UsePotion { potion_id, .. } if potion_id == "distilledchaos"))
        })
        .unwrap();
    let hand_select = execute(&mut rust, &combat, &exhaust_card);
    assert_eq!(hand_select.boundary_kind, "natural_hand_select");
    assert_eq!(
        observed(&hand_select, ParityDomainV1::Relics)[1]["combat_state"],
        json!({"kind":"art_of_war", "first_turn":false, "gain_energy_next":false})
    );
    let mut java_hand_select = equivalent_java_successor(&rust, &hand_select, 201);
    if autoplay {
        let entry = equivalent_java_observation(&combat, 200);
        java_hand_select.frame["parity_internal"]["zones"]["card_in_use"] =
            entry.frame["parity_internal"]["zones"]["draw"]["cards"][0].clone();
    }
    attach_fixture_card_birth_witness(
        &mut java_hand_select,
        &combat,
        &hand_select,
        &exhaust_card,
        200,
        0,
    );
    let exhaust_card_rng = rust.last_rng_call_ledger_v1().unwrap().clone();

    let hand_actions = action_space(&hand_select);
    assert_eq!(hand_actions.len(), if autoplay { 3 } else { 4 });
    assert_eq!(
        hand_actions
            .iter()
            .filter(|action| matches!(action, SemanticActionV1::DiscardPotion { .. }))
            .count(),
        if autoplay { 1 } else { 2 }
    );
    let selected_defend = hand_actions
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::HandSelect,
                    choices,
                } if choices[0].id == "natural_run/starter/6"
            )
        })
        .unwrap();
    let combat_after_select = execute(&mut rust, &hand_select, &selected_defend);
    assert_eq!(combat_after_select.boundary_kind, "natural_combat");
    if card_id == CardId::BurningPact && !autoplay {
        let state = &rust.session().active_combat.as_ref().unwrap().combat_state;
        let draw_count = 2 + usize::from(upgrades);
        assert_eq!(state.zones.hand.len(), 1 + draw_count);
        assert_eq!(state.zones.draw_pile.len(), 3 - draw_count);
        for drawn in &pact_draws[..draw_count] {
            assert!(state
                .zones
                .hand
                .iter()
                .any(|card| card.lineage == drawn.lineage));
        }
    }
    let mut java_combat_after_select = equivalent_java_successor(&rust, &combat_after_select, 202);
    if hex_amount > 0 {
        attach_fixture_card_birth_witness(
            &mut java_combat_after_select,
            &hand_select,
            &combat_after_select,
            &selected_defend,
            201,
            1,
        );
        assert_eq!(
            java_combat_after_select.frame["card_birth_witness"]["products"]
                .as_array()
                .unwrap()
                .len(),
            hex_amount as usize
        );
        for bad in ["missing", "cause", "frame", "index"] {
            let mut invalid = java_combat_after_select.clone();
            match bad {
                "missing" => invalid.frame["card_birth_witness"] = Value::Null,
                "cause" => {
                    invalid.frame["card_birth_witness"]["cause"] =
                        json!({"kind":"discovery_resolution"})
                }
                "frame" => invalid.frame["card_birth_witness"]["from_frame_id"] = json!(200),
                "index" => {
                    invalid.frame["card_birth_witness"]["products"][0]["generation_index"] =
                        json!(2)
                }
                _ => unreachable!(),
            }
            let mut java = JavaLockstepAdapterV1::new(
                FixtureTransport {
                    observations: vec![
                        equivalent_java_observation(&combat, 200),
                        java_hand_select.clone(),
                        invalid,
                    ],
                    cursor: 0,
                },
                bootstrap.clone(),
            )
            .unwrap();
            let entry = java.observe().unwrap();
            let prepared = java.prepare(&exhaust_card, &entry).unwrap();
            let paused = java.execute(prepared).unwrap();
            let prepared = java.prepare(&selected_defend, &paused).unwrap();
            assert!(java.execute(prepared).is_err(), "{bad} witness must fail");
        }
    }
    let selection_rng = rust.last_rng_call_ledger_v1().unwrap().clone();
    assert!(
        observed(&combat_after_select, ParityDomainV1::Combat)["state"]["zones"]
            ["exhaust_by_binding"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["binding"] == "natural_run/starter/6")
    );

    let calls = Rc::new(RefCell::new(FixtureTransportCalls::default()));
    if autoplay {
        for fault in ["missing", "unbound", "hand_source", "card_id", "upgrades"] {
            let mut invalid = java_hand_select.clone();
            let card = &mut invalid.frame["parity_internal"]["zones"]["card_in_use"];
            match fault {
                "missing" => *card = Value::Null,
                "unbound" => card["instance_uuid"] = json!("unobserved-card"),
                "hand_source" => card["instance_uuid"] = json!("fixture-card-7"),
                "card_id" => card["card_id"] = json!("Warcry"),
                "upgrades" => card["upgrades"] = json!(upgrades + 1),
                _ => unreachable!(),
            }
            let mut java = JavaLockstepAdapterV1::new(
                FixtureTransport {
                    observations: vec![equivalent_java_observation(&combat, 200), invalid],
                    cursor: 0,
                },
                bootstrap.clone(),
            )
            .unwrap();
            let entry = java.observe().unwrap();
            let prepared = java.prepare(&exhaust_card, &entry).unwrap();
            assert!(
                java.execute(prepared).is_err(),
                "{fault} autoplay source must fail"
            );
        }
    }
    let java_transport = RecordingFixtureTransport {
        inner: FixtureTransport {
            observations: vec![
                equivalent_java_observation(&combat, 200),
                java_hand_select,
                java_combat_after_select,
            ],
            cursor: 0,
        },
        calls: calls.clone(),
    };
    let mut java = JavaLockstepAdapterV1::new(java_transport, bootstrap).unwrap();
    let mut java_current = java.observe().unwrap();
    assert_exact(&java_current, &combat);

    let prepared = java.prepare(&exhaust_card, &java_current).unwrap();
    java_current = java.execute(prepared).unwrap();
    assert_exact(&java_current, &hand_select);
    assert!(compare_natural_run_rng_calls_v1(
        java.last_rng_call_ledger_v1().unwrap(),
        &exhaust_card_rng,
    )
    .unwrap()
    .stop_reason()
    .is_none());

    let prepared = java.prepare(&selected_defend, &java_current).unwrap();
    java_current = java.execute(prepared).unwrap();
    assert_exact(&java_current, &combat_after_select);
    assert!(compare_natural_run_rng_calls_v1(
        java.last_rng_call_ledger_v1().unwrap(),
        &selection_rng,
    )
    .unwrap()
    .stop_reason()
    .is_none());
    assert_eq!(
        *calls.borrow(),
        FixtureTransportCalls {
            methods: vec!["act", "hand"],
            hand_selections: vec![vec!["fixture-card-7".to_owned()]],
        }
    );
}

#[test]
fn mini_neow_path_through_lab_rewards_runs_exact_at_every_player_boundary() {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();

    let mut oracle = RustLockstepAdapterV1::new(
        RunControlSession::new(config.clone()),
        bootstrap.clone(),
        "fixture-rust",
        "oracle-epoch",
    )
    .unwrap();
    let introduction = oracle.observe().unwrap();
    let mut java_states = vec![
        equivalent_java_observation(&introduction, 1),
        equivalent_java_observation(&introduction, 2),
    ];
    let proceed = only_action(&introduction);
    let reward = execute(&mut oracle, &introduction, &proceed);
    java_states.push(equivalent_java_successor(&oracle, &reward, 3));
    let choose = action_space(&reward)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::Select { .. }))
        .unwrap();
    let leave = execute(&mut oracle, &reward, &choose);
    java_states.push(equivalent_java_successor(&oracle, &leave, 4));
    let leave_action = only_action(&leave);
    let map = execute(&mut oracle, &leave, &leave_action);
    java_states.push(equivalent_java_successor(&oracle, &map, 5));
    let route = action_space(&map).into_iter().next().unwrap();
    let combat = execute(&mut oracle, &map, &route);
    java_states.push(equivalent_java_successor(&oracle, &combat, 6));
    let mut adopted_combat_actions = Vec::new();
    let mut expected = combat;
    let mut combat_action_ordinal = 0_u32;
    for frame_id in 7..=96 {
        if expected.boundary_kind == "natural_combat_reward" {
            break;
        }
        let actions = action_space(&expected);
        let adopted = actions
            .iter()
            .find(|action| {
                matches!(
                    action,
                    SemanticActionV1::PlayCard { card_id, .. }
                        if card_id == "strike" || card_id == "bash"
                )
            })
            .or_else(|| {
                actions
                    .iter()
                    .find(|action| matches!(action, SemanticActionV1::PlayCard { .. }))
            })
            .or_else(|| {
                actions
                    .iter()
                    .find(|action| matches!(action, SemanticActionV1::EndTurn))
            })
            .expect("combat fixture must expose a progress action")
            .clone();
        let before = expected.clone();
        expected = execute(&mut oracle, &expected, &adopted);
        let mut java_successor = equivalent_java_successor(&oracle, &expected, frame_id);
        attach_fixture_card_birth_witness(
            &mut java_successor,
            &before,
            &expected,
            &adopted,
            frame_id - 1,
            combat_action_ordinal,
        );
        if expected.boundary_kind == "natural_combat_reward" {
            // A transition may generate a card and then end combat (for example,
            // Sentries followed by Fire Breathing draw damage). The receipt
            // retains its birth even though no combat identity survives.
            java_successor.frame["boundary"]["evidence"]["room_phase"] = json!("COMPLETE");
            java_successor.frame["card_birth_witness"]["products"] = json!([{
                "generation_index": 0,
                "java_uuid": "retired-combat-product",
                "card_id": "Dazed",
                "upgrades": 0,
                "first_zone": "discard",
            }]);
            assert!(java_successor.frame["parity_internal"]["zones"].is_null());
        }
        if matches!(
            adopted,
            SemanticActionV1::PlayCard { .. }
                | SemanticActionV1::EndTurn
                | SemanticActionV1::UsePotion { .. }
        ) {
            combat_action_ordinal += 1;
        }
        adopted_combat_actions.push(adopted);
        java_states.push(java_successor);
    }
    assert_eq!(expected.boundary_kind, "natural_combat_reward");

    let gold_reward = action_space(&expected)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CombatReward,
                    choice,
                } if choice.kind == "run_reward" && choice.id == "gold"
            )
        })
        .expect("initial combat reward should expose one gold claim");
    let before_gold = observed(&expected, ParityDomainV1::Run)["gold"]
        .as_i64()
        .unwrap();
    expected = execute(&mut oracle, &expected, &gold_reward);
    assert_eq!(expected.boundary_kind, "natural_combat_reward");
    assert!(observed(&expected, ParityDomainV1::Run)["gold"]
        .as_i64()
        .is_some_and(|gold| gold > before_gold));
    let reward_after_gold_frame = java_states.last().unwrap().frame_id + 1;
    java_states.push(equivalent_java_successor(
        &oracle,
        &expected,
        reward_after_gold_frame,
    ));

    let close_reward = action_space(&expected)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::CombatReward
                }
            )
        })
        .expect("combat reward should expose one close action");
    expected = execute(&mut oracle, &expected, &close_reward);
    assert_eq!(expected.boundary_kind, "natural_map");
    java_states.push(equivalent_java_successor(
        &oracle,
        &expected,
        reward_after_gold_frame + 1,
    ));

    let second_combat_route = action_space(&expected)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    choice,
                } if matches!(
                    choice.attributes.get("room"),
                    Some(SemanticScalarV1::Text(room)) if room == "monster"
                )
            )
        })
        .expect("reward map preview should expose a second-floor combat route");
    expected = execute(&mut oracle, &expected, &second_combat_route);
    assert_eq!(expected.boundary_kind, "natural_combat");
    java_states.push(equivalent_java_successor(
        &oracle,
        &expected,
        reward_after_gold_frame + 2,
    ));

    let second_combat_actions = oracle
        .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(2_000),
            wall_ms: Some(5_000),
            ..Default::default()
        })
        .expect("second combat fixture should have a winning trajectory");
    let mut second_combat_action_ordinal = 0_u32;
    for adopted in &second_combat_actions {
        let before = expected.clone();
        expected = execute(&mut oracle, &expected, adopted);
        let frame_id = java_states.last().unwrap().frame_id + 1;
        let mut java_successor = equivalent_java_successor(&oracle, &expected, frame_id);
        attach_fixture_card_birth_witness(
            &mut java_successor,
            &before,
            &expected,
            adopted,
            frame_id - 1,
            second_combat_action_ordinal,
        );
        if matches!(
            adopted,
            SemanticActionV1::PlayCard { .. }
                | SemanticActionV1::EndTurn
                | SemanticActionV1::UsePotion { .. }
        ) {
            second_combat_action_ordinal += 1;
        }
        java_states.push(java_successor);
    }
    assert_eq!(expected.boundary_kind, "natural_combat_reward");

    let second_gold_reward = action_space(&expected)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CombatReward,
                    choice,
                } if choice.kind == "run_reward" && choice.id == "gold"
            )
        })
        .expect("second combat reward should expose one gold claim");
    expected = execute(&mut oracle, &expected, &second_gold_reward);
    java_states.push(equivalent_java_successor(
        &oracle,
        &expected,
        java_states.last().unwrap().frame_id + 1,
    ));

    let close_second_reward = action_space(&expected)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::CombatReward
                }
            )
        })
        .expect("second combat reward should expose one close action");
    expected = execute(&mut oracle, &expected, &close_second_reward);
    assert_eq!(expected.boundary_kind, "natural_map");
    java_states.push(equivalent_java_successor(
        &oracle,
        &expected,
        java_states.last().unwrap().frame_id + 1,
    ));

    let question_mark_route = action_space(&expected)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    choice,
                } if matches!(
                    choice.attributes.get("room"),
                    Some(SemanticScalarV1::Text(room)) if room == "question_mark"
                )
            )
        })
        .expect("second reward map should expose a question-mark route");
    expected = execute(&mut oracle, &expected, &question_mark_route);
    assert_eq!(expected.boundary_kind, "natural_event");
    java_states.push(equivalent_java_successor(
        &oracle,
        &expected,
        java_states.last().unwrap().frame_id + 1,
    ));

    let lab_take = only_action(&expected);
    assert!(matches!(
        &lab_take,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Event,
            choice,
        } if choice.id == "lab"
    ));
    expected = execute(&mut oracle, &expected, &lab_take);
    assert_eq!(expected.boundary_kind, "natural_combat_reward");
    let lab_rewards: NaturalRunRewardsV2 =
        serde_json::from_value(observed(&expected, ParityDomainV1::Rewards).clone()).unwrap();
    let NaturalRunRewardsV2::Combat {
        items: lab_items, ..
    } = lab_rewards
    else {
        panic!("Lab should open the original combat reward screen")
    };
    assert_eq!(lab_items.len(), 2, "A20 Lab should offer two potions");
    assert!(lab_items
        .iter()
        .all(|item| matches!(item, NaturalRunRewardItemV1::Potion { .. })));
    java_states.push(equivalent_java_successor(
        &oracle,
        &expected,
        java_states.last().unwrap().frame_id + 1,
    ));

    let mut lab_potion_claims = Vec::new();
    for _ in 0..2 {
        let claim = action_space(&expected)
            .into_iter()
            .find(|action| {
                matches!(
                    action,
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::CombatReward,
                        choice,
                    } if choice.kind == "run_reward" && choice.id == "potion"
                )
            })
            .expect("Lab reward should retain one potion claim");
        expected = execute(&mut oracle, &expected, &claim);
        assert_eq!(expected.boundary_kind, "natural_combat_reward");
        lab_potion_claims.push(claim);
        java_states.push(equivalent_java_successor(
            &oracle,
            &expected,
            java_states.last().unwrap().frame_id + 1,
        ));
    }
    assert_eq!(
        observed(&expected, ParityDomainV1::Potions)
            .as_array()
            .unwrap()
            .iter()
            .filter(|slot| !slot["id"].is_null())
            .count(),
        2,
        "claiming both Lab rewards should occupy two potion slots"
    );

    let close_lab_reward = action_space(&expected)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::CombatReward
                }
            )
        })
        .expect("empty Lab reward should expose one close action");
    expected = execute(&mut oracle, &expected, &close_lab_reward);
    assert_eq!(
        expected.boundary_kind, "natural_map",
        "original Lab closes its reward screen directly to the map"
    );
    java_states.push(equivalent_java_successor(
        &oracle,
        &expected,
        java_states.last().unwrap().frame_id + 1,
    ));

    let java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: java_states,
            cursor: 0,
        },
        bootstrap.clone(),
    )
    .unwrap();
    let rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        "fixture-rust",
        "runner-epoch",
    )
    .unwrap();
    let journal = std::env::temp_dir().join(format!(
        "sts-lockstep-neow-prefix-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut runner = LockstepRunnerV1::start_new(
        java,
        rust,
        LockstepModeV1::Certification,
        &journal,
        "natural-neow-to-map",
    )
    .unwrap();

    for expected_boundary in ["natural_neow", "natural_neow", "natural_map"] {
        let handle = runner.current_action_space().unwrap().actions[0]
            .handle
            .clone();
        let entry = runner.step_handle(&handle).unwrap();
        assert!(matches!(
            entry.comparison,
            Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
        ));
        assert_eq!(
            entry.java_after.as_ref().unwrap().boundary_kind,
            expected_boundary
        );
        assert!(!runner.is_stopped());
    }
    assert_eq!(runner.current_snapshots().0.boundary_kind, "natural_map");

    let route_handle = runner.current_action_space().unwrap().actions[0]
        .handle
        .clone();
    let combat_entry = runner.step_handle(&route_handle).unwrap();
    assert!(matches!(
        combat_entry.comparison,
        Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
    ));
    assert_eq!(runner.current_snapshots().0.boundary_kind, "natural_combat");
    assert!(!runner.is_stopped());

    for adopted in &adopted_combat_actions {
        let handle = runner
            .current_action_space()
            .unwrap()
            .actions
            .iter()
            .find(|candidate| candidate.action == *adopted)
            .expect("runner action space must retain the adopted combat action")
            .handle
            .clone();
        let entry = runner.step_handle(&handle).unwrap();
        assert!(matches!(
            entry.comparison,
            Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
        ));
        assert!(!runner.is_stopped());
    }
    assert_eq!(
        runner.current_snapshots().0.boundary_kind,
        "natural_combat_reward"
    );

    let gold_handle = runner
        .current_action_space()
        .unwrap()
        .actions
        .iter()
        .find(|candidate| candidate.action == gold_reward)
        .expect("runner should retain the shared gold action")
        .handle
        .clone();
    let gold_entry = runner.step_handle(&gold_handle).unwrap();
    assert!(matches!(
        gold_entry.comparison,
        Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
    ));
    assert_eq!(
        runner.current_snapshots().0.boundary_kind,
        "natural_combat_reward"
    );
    assert!(!runner.is_stopped());

    let close_handle = runner
        .current_action_space()
        .unwrap()
        .actions
        .iter()
        .find(|candidate| candidate.action == close_reward)
        .expect("runner should retain the shared reward close action")
        .handle
        .clone();
    let close_entry = runner.step_handle(&close_handle).unwrap();
    assert!(matches!(
        close_entry.comparison,
        Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
    ));
    assert_eq!(runner.current_snapshots().0.boundary_kind, "natural_map");
    assert!(!runner.is_stopped());

    let second_route_handle = runner
        .current_action_space()
        .unwrap()
        .actions
        .iter()
        .find(|candidate| candidate.action == second_combat_route)
        .expect("runner should retain the shared second-floor combat route")
        .handle
        .clone();
    let second_combat_entry = runner.step_handle(&second_route_handle).unwrap();
    assert!(matches!(
        second_combat_entry.comparison,
        Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
    ));
    assert_eq!(runner.current_snapshots().0.boundary_kind, "natural_combat");
    assert!(!runner.is_stopped());

    for adopted in &second_combat_actions {
        let handle = runner
            .current_action_space()
            .unwrap()
            .actions
            .iter()
            .find(|candidate| candidate.action == *adopted)
            .expect("runner action space must retain the second combat action")
            .handle
            .clone();
        let entry = runner.step_handle(&handle).unwrap();
        assert!(matches!(
            entry.comparison,
            Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
        ));
        assert!(!runner.is_stopped());
    }
    assert_eq!(
        runner.current_snapshots().0.boundary_kind,
        "natural_combat_reward"
    );

    for expected_action in [
        &second_gold_reward,
        &close_second_reward,
        &question_mark_route,
    ] {
        let handle = runner
            .current_action_space()
            .unwrap()
            .actions
            .iter()
            .find(|candidate| candidate.action == *expected_action)
            .expect("runner should retain the shared post-second-combat action")
            .handle
            .clone();
        let entry = runner.step_handle(&handle).unwrap();
        assert!(matches!(
            entry.comparison,
            Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
        ));
        assert!(!runner.is_stopped());
    }
    assert_eq!(runner.current_snapshots().0.boundary_kind, "natural_event");

    for expected_action in std::iter::once(&lab_take)
        .chain(lab_potion_claims.iter())
        .chain(std::iter::once(&close_lab_reward))
    {
        let handle = runner
            .current_action_space()
            .unwrap()
            .actions
            .iter()
            .find(|candidate| candidate.action == *expected_action)
            .expect("runner should retain the shared Lab action")
            .handle
            .clone();
        let entry = runner.step_handle(&handle).unwrap();
        assert!(matches!(
            entry.comparison,
            Some(ref comparison) if comparison.status == ComparisonStatusV1::Exact
        ));
        assert!(!runner.is_stopped());
    }
    assert_eq!(runner.current_snapshots().0.boundary_kind, "natural_map");
    assert_eq!(
        fs::read_dir(journal.join("steps")).unwrap().count(),
        14 + adopted_combat_actions.len() + second_combat_actions.len()
    );
    fs::remove_dir_all(journal).unwrap();
}

#[test]
fn end_turn_slimed_birth_is_exact_in_the_stateful_java_adapter() {
    let config = RunControlConfig {
        seed: 2_026_090_302,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "fixture-rust",
        "slimed-oracle-epoch",
    )
    .unwrap();
    let introduction = rust.observe().unwrap();
    let reward = execute(&mut rust, &introduction, &only_action(&introduction));
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
    let leave = execute(&mut rust, &reward, &curse);
    let map = execute(&mut rust, &leave, &only_action(&leave));
    let route = action_space(&map).into_iter().next().unwrap();
    let combat = execute(&mut rust, &map, &route);
    let trajectory = rust
        .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(2_000),
            wall_ms: Some(5_000),
            ..Default::default()
        })
        .unwrap();

    let first_frame_id = 100_u64;
    let mut rust_snapshots = vec![combat.clone()];
    let mut java_states = vec![equivalent_java_observation(&combat, first_frame_id)];
    let mut before = combat;
    for (action_ordinal, action) in trajectory.iter().enumerate() {
        let after = execute(&mut rust, &before, action);
        let frame_id = first_frame_id + u64::try_from(action_ordinal).unwrap() + 1;
        let mut java_after = equivalent_java_observation(&after, frame_id);
        attach_fixture_card_birth_witness(
            &mut java_after,
            &before,
            &after,
            action,
            frame_id - 1,
            u32::try_from(action_ordinal).unwrap(),
        );
        rust_snapshots.push(after.clone());
        java_states.push(java_after);
        before = after;
    }

    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: java_states,
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let mut java_snapshot = java.observe().unwrap();
    assert_eq!(
        compare_snapshots_v1(&java_snapshot, &rust_snapshots[0])
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
    let mut saw_slimed_birth = false;
    for (index, action) in trajectory.iter().enumerate() {
        let prepared = java.prepare(action, &java_snapshot).unwrap();
        java_snapshot = java.execute(prepared).unwrap();
        let comparison = compare_snapshots_v1(&java_snapshot, &rust_snapshots[index + 1]).unwrap();
        assert_eq!(comparison.status, ComparisonStatusV1::Exact);
        if java_snapshot.boundary_kind != "natural_combat" {
            continue;
        }
        let relations = observed(&java_snapshot, ParityDomainV1::CardBindings);
        saw_slimed_birth |= relations.as_array().unwrap().iter().any(|relation| {
            relation["source"]["kind"] == "combat_birth"
                && relation["source"]["cause"]["kind"] == "end_turn_resolution"
                && fixture_combat_card(&java_snapshot, required_text(relation, "binding")).0
                    ["card_id"]
                    == "slimed"
        });
    }
    assert!(saw_slimed_birth);
    assert_eq!(java_snapshot.boundary_kind, "natural_combat_reward");
}

#[test]
fn java_power_identity_and_private_state_are_strict_contracts() {
    let (bootstrap, combat) = natural_first_combat_fixture();
    let base = equivalent_java_observation(&combat, 100);
    assert!(base.frame["parity_internal"]["monsters"][0]["powers"]
        .as_array()
        .is_some());

    let mut weakened = base.clone();
    weakened.frame["parity_internal"]["monsters"][0]["powers"] = json!([{
        "index": 0,
        "id": "Weakened",
        "class_name": "com.megacrit.cardcrawl.powers.WeakPower",
        "amount": 2,
        "type": "DEBUFF",
        "round_duration_just_applied": false,
        "ritual_skip_first": null,
        "ritual_player_controlled": null,
    }]);
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![weakened],
            cursor: 0,
        },
        bootstrap.clone(),
    )
    .unwrap();
    let weakened = java.observe().unwrap();
    let power = &observed(&weakened, ParityDomainV1::Combat)["state"]["monsters"][0]["powers"][0];
    assert_eq!(power["power_id"], "weak");
    assert_eq!(
        power["state"],
        json!({"kind": "round_duration", "just_applied": false})
    );

    let cases = [
        (
            json!({
                "index": 0,
                "id": "Strength",
                "class_name": "example.mod.FakeStrengthPower",
                "amount": 1,
                "type": "BUFF",
                "round_duration_just_applied": null,
                "ritual_skip_first": null,
                "ritual_player_controlled": null,
            }),
            "expected original-game class and type",
        ),
        (
            json!({
                "index": 0,
                "id": "Weakened",
                "class_name": "com.megacrit.cardcrawl.powers.WeakPower",
                "amount": 1,
                "type": "DEBUFF",
                "round_duration_just_applied": null,
                "ritual_skip_first": null,
                "ritual_player_controlled": null,
            }),
            "omitted boolean round_duration_just_applied",
        ),
        (
            json!({
                "index": 0,
                "id": "Ritual",
                "class_name": "com.megacrit.cardcrawl.powers.RitualPower",
                "amount": 3,
                "type": "BUFF",
                "round_duration_just_applied": true,
                "ritual_skip_first": false,
                "ritual_player_controlled": false,
            }),
            "Ritual carries round-duration state",
        ),
    ];

    for (power, expected_message) in cases {
        let mut observation = base.clone();
        observation.frame["parity_internal"]["monsters"][0]["powers"] = json!([power]);
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![observation],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();

        let error = java.observe().unwrap_err();
        assert_eq!(error.kind, AdapterFailureKindV1::Evidence);
        assert!(
            error.message.contains(expected_message),
            "unexpected power contract failure: {}",
            error.message
        );
    }
}

#[test]
fn java_dead_monster_powers_and_intent_are_excluded_from_stable_projection() {
    let (bootstrap, combat) = natural_first_combat_fixture();
    let mut observation = equivalent_java_observation(&combat, 109);
    observation.frame["public_base"]["combat"]["monsters"][0]["current_hp"] = json!(0);
    observation.frame["public_base"]["combat"]["monsters"][0]["dead_or_escaped"] = json!(true);
    observation.frame["public_base"]["combat"]["monsters"][0]["public_intent"] = json!({
        "evidence": "visible_exact",
        "visible_intent": "ATTACK",
        "damage_per_hit": 999,
        "hits": 7,
        "hidden_reason": null,
    });
    observation.frame["parity_internal"]["monsters"][0]["powers"] = json!([{
        "index": 0,
        "id": "Curl Up",
        "class_name": "com.megacrit.cardcrawl.powers.CurlUpPower",
        "amount": 9,
        "type": "BUFF",
        "round_duration_just_applied": null,
        "ritual_skip_first": null,
        "ritual_player_controlled": null,
    }]);
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![observation],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();

    let snapshot = java.observe().unwrap();
    assert_eq!(
        observed(&snapshot, ParityDomainV1::Combat)["state"]["monsters"][0]["powers"],
        json!([])
    );
    assert_eq!(
        observed(&snapshot, ParityDomainV1::Combat)["state"]["monsters"][0]["public_intent"],
        json!({
            "evidence": "hidden",
            "kind": null,
            "damage_per_hit": null,
            "hits": null,
            "hidden_reason": "terminal_monster",
        })
    );
}

#[test]
fn java_act1_stateless_powers_keep_original_identity_and_amount_contracts() {
    let (bootstrap, combat) = natural_first_combat_fixture();
    let template = equivalent_java_observation(&combat, 110);
    for (raw_id, class_name, amount, power_type, shared_id) in [
        (
            "Curl Up",
            "com.megacrit.cardcrawl.powers.CurlUpPower",
            9,
            "BUFF",
            "curlup",
        ),
        (
            "Anger",
            "com.megacrit.cardcrawl.powers.AngerPower",
            3,
            "BUFF",
            "anger",
        ),
        (
            "Angry",
            "com.megacrit.cardcrawl.powers.AngryPower",
            2,
            "BUFF",
            "angry",
        ),
        (
            "Metallicize",
            "com.megacrit.cardcrawl.powers.MetallicizePower",
            8,
            "BUFF",
            "metallicize",
        ),
        (
            "Spore Cloud",
            "com.megacrit.cardcrawl.powers.SporeCloudPower",
            2,
            "BUFF",
            "sporecloud",
        ),
        (
            "Split",
            "com.megacrit.cardcrawl.powers.SplitPower",
            -1,
            "BUFF",
            "split",
        ),
        (
            "Artifact",
            "com.megacrit.cardcrawl.powers.ArtifactPower",
            1,
            "BUFF",
            "artifact",
        ),
        (
            "Mode Shift",
            "com.megacrit.cardcrawl.powers.ModeShiftPower",
            30,
            "BUFF",
            "modeshift",
        ),
        (
            "Sharp Hide",
            "com.megacrit.cardcrawl.powers.SharpHidePower",
            4,
            "BUFF",
            "sharphide",
        ),
        (
            "Thievery",
            "com.megacrit.cardcrawl.powers.ThieveryPower",
            15,
            "BUFF",
            "thievery",
        ),
        (
            "Feel No Pain",
            "com.megacrit.cardcrawl.powers.FeelNoPainPower",
            3,
            "BUFF",
            "feelnopain",
        ),
        (
            "Brutality",
            "com.megacrit.cardcrawl.powers.BrutalityPower",
            1,
            "BUFF",
            "brutality",
        ),
        (
            "DuplicationPower",
            "com.megacrit.cardcrawl.powers.DuplicationPower",
            2,
            "BUFF",
            "duplicationpower",
        ),
        (
            "Double Tap",
            "com.megacrit.cardcrawl.powers.DoubleTapPower",
            1,
            "BUFF",
            "doubletap",
        ),
        (
            "Double Tap",
            "com.megacrit.cardcrawl.powers.DoubleTapPower",
            2,
            "BUFF",
            "doubletap",
        ),
        (
            "Corruption",
            "com.megacrit.cardcrawl.powers.CorruptionPower",
            -1,
            "BUFF",
            "corruption",
        ),
        (
            "Dexterity",
            "com.megacrit.cardcrawl.powers.DexterityPower",
            -2,
            "DEBUFF",
            "dexterity",
        ),
        (
            "Entangled",
            "com.megacrit.cardcrawl.powers.EntanglePower",
            1,
            "DEBUFF",
            "entangle",
        ),
    ] {
        let mut observation = template.clone();
        observation.frame["parity_internal"]["monsters"][0]["powers"] = json!([{
            "index": 0,
            "id": raw_id,
            "class_name": class_name,
            "amount": amount,
            "type": power_type,
            "round_duration_just_applied": null,
            "ritual_skip_first": null,
            "ritual_player_controlled": null,
        }]);
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![observation],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let snapshot = java.observe().unwrap();
        let power =
            &observed(&snapshot, ParityDomainV1::Combat)["state"]["monsters"][0]["powers"][0];
        assert_eq!(power["power_id"], shared_id, "{raw_id}");
        assert_eq!(power["amount"], amount, "{raw_id}");
        assert_eq!(power["state"], json!({"kind": "stateless"}), "{raw_id}");
    }

    let mut base = template;
    let feel_no_pain = json!({
        "index": 0,
        "id": "Feel No Pain",
        "class_name": "com.megacrit.cardcrawl.powers.FeelNoPainPower",
        "amount": 3,
        "type": "BUFF",
        "round_duration_just_applied": null,
        "ritual_skip_first": null,
        "ritual_player_controlled": null,
    });
    base.frame["parity_internal"]["monsters"][0]["powers"] = json!([feel_no_pain]);

    for (field, value, expected) in [
        (
            "class_name",
            json!("example.mod.FakeFeelNoPainPower"),
            "expected original-game class and type",
        ),
        (
            "round_duration_just_applied",
            json!(false),
            "stateless power carries unexpected private state",
        ),
        ("amount", json!(0), "not positive at a stable boundary"),
    ] {
        let mut invalid = base.clone();
        invalid.frame["parity_internal"]["monsters"][0]["powers"][0][field] = value;
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![invalid],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let error = java.observe().unwrap_err();
        assert!(error.message.contains(expected), "{}", error.message);
    }
}

#[test]
fn java_combat_reward_shape_indices_and_action_identity_are_strict_contracts() {
    let (bootstrap, reward) = natural_first_combat_reward_fixture();
    let base = equivalent_java_observation(&reward, 200);

    let mut non_contiguous = base.clone();
    non_contiguous.frame["parity_internal"]["combat_rewards"][0]["index"] = json!(1);

    let mut malformed_variant = base.clone();
    malformed_variant.frame["parity_internal"]["combat_rewards"][0]["potion_id"] =
        json!("Fire Potion");

    let mut mismatched_action = base;
    let claim = mismatched_action
        .actions
        .iter_mut()
        .find(|action| action.kind == "claim_combat_reward")
        .expect("initial reward fixture should expose a claim action");
    claim.reward_type = Some("NOT_THE_OBSERVED_TYPE".to_owned());

    for (observation, expected_message) in [
        (non_contiguous, "indices are not contiguous"),
        (malformed_variant, "fields outside its typed variant"),
        (mismatched_action, "disagrees with its typed item"),
    ] {
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![observation],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();

        let error = java.observe().unwrap_err();
        assert_eq!(error.kind, AdapterFailureKindV1::Evidence);
        assert!(
            error.message.contains(expected_message),
            "unexpected reward contract failure: {}",
            error.message
        );
    }
}

#[test]
fn singing_bowl_and_skip_keep_distinct_reward_and_health_effects_across_adapters() {
    use sts_oracle_run_control::{
        content::{
            cards::CardId,
            relics::{RelicId, RelicState},
        },
        state::{
            core::EngineState,
            rewards::{RewardCard, RewardItem, RewardState},
        },
    };
    let config = RunControlConfig::default();
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 0;
    session.run_state.event_state = None;
    session.run_state.current_hp = 60;
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::SingingBowl));
    let cards = vec![RewardCard::new(CardId::ShrugItOff, 0)];
    let mut reward = RewardState::new();
    reward.items.push(RewardItem::Card {
        cards: cards.clone(),
    });
    reward.pending_card_choice = Some(cards);
    reward.pending_card_reward_index = Some(0);
    session.engine_state = EngineState::RewardScreen(reward);
    let root = RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture", "bowl").unwrap();

    for take_bowl in [false, true] {
        let mut rust = root.clone();
        let before = rust.observe().unwrap();
        let choices = action_space(&before);
        let bowl = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CardReward,
            choice: sts_parity_lockstep::natural_singing_bowl_choice_v1(),
        };
        let skip = SemanticActionV1::Cancel {
            surface: DecisionSurfaceV1::CardReward,
        };
        assert!(choices.contains(&bowl) && choices.contains(&skip));
        assert!(choices.iter().any(|action| matches!(action,
            SemanticActionV1::Select { choice, .. } if choice.kind == "reward_card")));
        let action = if take_bowl { bowl } else { skip };
        let after = execute(&mut rust, &before, &action);
        assert_eq!(
            rust.session().run_state.current_hp,
            60 + if take_bowl { 2 } else { 0 }
        );
        assert_eq!(
            rust.session().run_state.max_hp,
            root.session().run_state.max_hp + if take_bowl { 2 } else { 0 }
        );
        assert_eq!(
            rust.session().run_state.master_deck,
            root.session().run_state.master_deck
        );
        let EngineState::RewardScreen(reward) = &rust.session().engine_state else {
            panic!("return to rewards")
        };
        assert_eq!(reward.has_card_reward_item(), !take_bowl);
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![
                    equivalent_java_observation(&before, 240),
                    equivalent_java_observation(&after, 241),
                ],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let java_before = java.observe().unwrap();
        assert_exact(&java_before, &before);
        let prepared = java.prepare(&action, &java_before).unwrap();
        assert_exact(&java.execute(prepared).unwrap(), &after);
    }
}

#[test]
fn watcher_second_boss_entry_requires_adjacent_complete_miracle_births() {
    use sts_oracle_run_control::content::{
        monsters::factory::EncounterId,
        relics::{RelicId, RelicState},
    };
    let config = RunControlConfig {
        seed: 2026101803,
        ascension_level: 20,
        final_act: true,
        player_class: "Watcher",
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.act_num = 3;
    session.run_state.floor_num = 50;
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 15;
    session.run_state.boss_key = Some(EncounterId::DonuAndDeca);
    session.run_state.boss_list = vec![EncounterId::TimeEater, EncounterId::AwakenedOne];
    session.run_state.relics = vec![RelicState::new(RelicId::HolyWater)];
    session.engine_state = EngineState::DoubleBossComplete;
    let mut rust =
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture", "double-boss").unwrap();
    let before = rust.observe().unwrap();
    let proceed = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::DoubleBoss,
    };
    assert!(action_space(&before).contains(&proceed));
    let prepared = rust.prepare(&proceed, &before).unwrap();
    assert_eq!(
        rust.observe().unwrap(),
        before,
        "preparing entry must not mutate the first boss boundary"
    );
    let after = rust.execute(prepared).unwrap();
    assert_eq!(after.boundary_kind, "natural_combat");
    assert_eq!(rust.session().run_state.floor_num, 51);
    let java_before = equivalent_java_observation(&before, 40);
    let mut java_after = equivalent_java_observation(&after, 41);
    attach_fixture_card_birth_witness(&mut java_after, &before, &after, &proceed, 40, 0);
    assert_eq!(
        java_after.frame["card_birth_witness"]["products"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    for fault in [
        "none",
        "missing",
        "nonadjacent",
        "wrong_cause",
        "omitted_product",
    ] {
        let mut successor = java_after.clone();
        match fault {
            "missing" => {
                successor
                    .frame
                    .as_object_mut()
                    .unwrap()
                    .remove("card_birth_witness");
            }
            "nonadjacent" => successor.frame["card_birth_witness"]["from_frame_id"] = json!(39),
            "wrong_cause" => {
                successor.frame["card_birth_witness"]["cause"] =
                    json!({"kind":"end_turn_resolution"})
            }
            "omitted_product" => {
                successor.frame["card_birth_witness"]["products"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            _ => {}
        }
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![java_before.clone(), successor],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let start = java.observe().unwrap();
        assert_exact(&start, &before);
        let prepared = java.prepare(&proceed, &start).unwrap();
        let actual = java.execute(prepared);
        if fault == "none" {
            assert_exact(&actual.unwrap(), &after);
        } else {
            assert!(actual.is_err(), "accepted {fault} birth evidence");
        }
    }
}

#[test]
fn holy_water_native_reload_requires_the_actual_continue_execution() {
    use sts_oracle_run_control::content::relics::{RelicId, RelicState};
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Watcher",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut session = RunControlSession::new(config);
    session.run_state.relics = vec![RelicState::new(RelicId::HolyWater)];
    let mut rust =
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture", "holy-water").unwrap();
    let map = advance_to_map(&mut rust);
    let route = action_space(&map).into_iter().next().unwrap();
    let combat = execute(&mut rust, &map, &route);
    let before = equivalent_java_observation(&map, 30);
    let mut after = equivalent_java_observation(&combat, 31);
    attach_fixture_card_birth_witness(&mut after, &map, &combat, &route, 30, 0);
    assert_eq!(
        after.frame["card_birth_witness"]["products"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![before.clone(), after.clone()],
            cursor: 0,
        },
        bootstrap.clone(),
    )
    .unwrap();
    let start = java.observe().unwrap();
    let prepared = java.prepare(&route, &start).unwrap();
    assert_exact(&java.execute(prepared).unwrap(), &combat);
    let resume = java.resume_state_v1().unwrap();

    // Continue creates new UUIDs for both the saved deck and the three
    // independent Miracles. No old lease may bind a fresh runtime object.
    let mut reloaded: JavaRuntimeObservationV1 = serde_json::from_str(
        &serde_json::to_string(&after)
            .unwrap()
            .replace("fixture-", "reloaded-"),
    )
    .unwrap();
    reloaded.frame_id = 90;
    reloaded.frame["frame_id"] = json!(90);
    for card in reloaded.frame["parity_internal"]["master_deck"]
        .as_array_mut()
        .unwrap()
    {
        card["source"] = Value::Null;
    }
    reloaded.frame["card_birth_witness"]["from_frame_id"] = json!(89);
    reloaded.frame["card_birth_witness"]["to_frame_id"] = json!(90);
    reloaded.frame["card_birth_witness"]["request_id"] = json!("actual-continue-request");
    reloaded.frame["card_birth_witness"]["action_id"] = json!("continue_run");
    let execution = fixture_native_reload_execution(&reloaded);
    let root = std::env::temp_dir().join(format!(
        "holy-water-reload-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = RecordedJavaEvidenceStoreV1::create_new(&root, bootstrap.clone()).unwrap();
    let mut java = JavaLockstepAdapterV1::reattach_after_native_reload_with_evidence_store(
        FixtureTransport {
            observations: vec![reloaded.clone()],
            cursor: 0,
        },
        bootstrap.clone(),
        store,
        resume.clone(),
        Some(execution.clone()),
    )
    .unwrap();
    assert_exact(&java.observe().unwrap(), &combat);
    let evidence = RecordedJavaEvidenceV1::load_complete(&root).unwrap();
    assert_eq!(evidence.java_action_count, 0); // Continue is preparation, not gameplay.
    let mut offline = JavaLockstepAdapterV1::reattach_after_native_reload(
        FixtureTransport {
            observations: vec![evidence.initial.observation],
            cursor: 0,
        },
        bootstrap.clone(),
        resume.clone(),
        evidence.native_reload,
    )
    .unwrap();
    assert_exact(&offline.observe().unwrap(), &combat);

    for defect in [
        "no_execution",
        "request",
        "missing_witness",
        "generation",
        "successor",
        "master_product",
    ] {
        let mut raw = reloaded.clone();
        let mut proof = execution.clone();
        match defect {
            "request" => proof.receipt.request_id = "unrelated-request".into(),
            "missing_witness" => raw.frame["card_birth_witness"] = Value::Null,
            "generation" => {
                raw.frame["card_birth_witness"]["products"][0]["generation_index"] = json!(1)
            }
            "successor" => proof.after.frame["unexpected"] = json!(true),
            "master_product" => {
                raw.frame["card_birth_witness"]["products"][0]["java_uuid"] =
                    raw.frame["parity_internal"]["master_deck"][0]["card_uuid"].clone();
            }
            _ => (),
        }
        if defect != "successor" {
            proof.after = raw.clone();
        }
        let mut invalid = JavaLockstepAdapterV1::reattach_after_native_reload(
            FixtureTransport {
                observations: vec![raw],
                cursor: 0,
            },
            bootstrap.clone(),
            resume.clone(),
            (defect != "no_execution").then_some(proof),
        )
        .unwrap();
        assert!(invalid.observe().is_err(), "accepted {defect}");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_continue_without_entry_births_preserves_the_combat_action_ordinal() {
    let (bootstrap, combat) = natural_first_combat_fixture();
    let raw = equivalent_java_observation(&combat, 30);
    let mut source = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![raw],
            cursor: 0,
        },
        bootstrap.clone(),
    )
    .unwrap();
    source.observe().unwrap();
    let resume = source.resume_state_v1().unwrap();
    let mut reloaded = equivalent_java_observation(&combat, 90);
    reloaded.frame["card_birth_witness"] = json!({
        "kind":"card_birth_witness_v1", "from_frame_id":89, "to_frame_id":90,
        "request_id":"actual-continue-request", "action_id":"continue_run",
        "cause":{"kind":"combat_entry_resolution"}, "complete":true,
        "products":[], "unsupported_reason":null,
    });
    let mut states = Vec::new();
    for execution in [None, Some(fixture_native_reload_execution(&reloaded))] {
        let mut java = JavaLockstepAdapterV1::reattach_after_native_reload(
            FixtureTransport {
                observations: vec![reloaded.clone()],
                cursor: 0,
            },
            bootstrap.clone(),
            resume.clone(),
            execution,
        )
        .unwrap();
        assert_exact(&java.observe().unwrap(), &combat);
        states.push(java.resume_state_v1().unwrap());
    }
    assert_eq!(states[0], states[1]);
}

fn fixture_native_reload_execution(
    after: &JavaRuntimeObservationV1,
) -> JavaNativeReloadExecutionV1 {
    let mut menu = after.clone();
    menu.frame_id = after.frame_id - 1;
    menu.frame = json!({"frame_id":menu.frame_id, "boundary":{"kind":"main_menu_ready"},
        "public_base":{"decision":{"kind":"main_menu_ready"},"run":null}});
    menu.actions.truncate(1);
    menu.actions[0].kind = "continue_run".into();
    menu.actions[0].action_id = "continue_run".into();
    menu.actions[0].parameters_required = true;
    JavaNativeReloadExecutionV1 {
        receipt: JavaLockstepReceiptV1 {
            status: "completed".into(),
            request_id: "actual-continue-request".into(),
            based_on_frame_id: menu.frame_id,
            action_id: "continue_run".into(),
            successor_frame_id: Some(after.frame_id),
        },
        before: menu,
        after: after.clone(),
    }
}

#[test]
fn ordinary_card_reward_and_its_acquisition_are_exact_across_both_adapters() {
    let (bootstrap, reward) = natural_first_combat_reward_fixture();
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let mut rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "fixture-rust",
        "card-reward-exact-epoch",
    )
    .unwrap();
    let map = advance_to_map(&mut rust);
    let route = action_space(&map).into_iter().next().unwrap();
    let mut current = execute(&mut rust, &map, &route);
    for action in rust
        .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(2_000),
            wall_ms: Some(5_000),
            ..Default::default()
        })
        .unwrap()
    {
        current = execute(&mut rust, &current, &action);
    }
    assert_exact(&current, &reward);
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
    current = execute(&mut rust, &current, &open);
    assert_eq!(current.boundary_kind, "natural_card_reward");
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
    let rust_before = current.clone();
    let rust_after = execute(&mut rust, &rust_before, &take);

    let before = equivalent_java_observation(&rust_before, 240);
    let mut after = equivalent_java_observation(&rust_after, 241);
    // Original ordinary rewards acquire the selected offer object itself.
    // A independently numbered deck fixture does not prove that acquisition.
    after.frame["parity_internal"]["master_deck"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["card_uuid"] =
        before.frame["public_base"]["decision"]["card_reward"]["cards"][0]["card_uuid"].clone();
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![before.clone(), after.clone()],
            cursor: 0,
        },
        bootstrap.clone(),
    )
    .unwrap();
    let java_before = java.observe().unwrap();
    assert_exact(&java_before, &rust_before);
    let prepared = java.prepare(&take, &java_before).unwrap();
    let java_after = java.execute(prepared).unwrap();
    assert_exact(&java_after, &rust_after);

    // Reload erases every SpireField. The next acquisition at this coordinate
    // (including another act) still owns slot 1, even though Java tags it 0.
    let resume = java.resume_state_v1().unwrap();
    let mut reloaded = before;
    reloaded.frame["parity_internal"]["master_deck"] =
        after.frame["parity_internal"]["master_deck"].clone();
    for (index, card) in reloaded.frame["parity_internal"]["master_deck"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        card["card_uuid"] = json!(format!("reload-card-{index}"));
        card.as_object_mut().unwrap().remove("source");
    }
    let mut acquired = reloaded.clone();
    acquired.frame_id = 241;
    acquired.frame["frame_id"] = json!(241);
    acquired.frame["public_base"]["decision"] = after.frame["public_base"]["decision"].clone();
    acquired.actions = after.actions;
    acquired.frame["parity_internal"]["combat_rewards"] =
        after.frame["parity_internal"]["combat_rewards"].clone();
    let mut product = after.frame["parity_internal"]["master_deck"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    let deck = acquired.frame["parity_internal"]["master_deck"]
        .as_array_mut()
        .unwrap();
    product["ordinal"] = json!(deck.len());
    deck.push(product);
    let mut java = JavaLockstepAdapterV1::reattach_after_native_reload(
        FixtureTransport {
            observations: vec![reloaded, acquired],
            cursor: 0,
        },
        bootstrap.clone(),
        resume,
        None,
    )
    .unwrap();
    let before = java.observe().unwrap();
    let prepared = java.prepare(&take, &before).unwrap();
    let after = java.execute(prepared).unwrap();
    let deck = observed(&after, ParityDomainV1::MasterDeck)
        .as_array()
        .unwrap();
    assert!(deck[deck.len() - 2]["binding"]
        .as_str()
        .unwrap()
        .ends_with("/0"));
    assert!(deck[deck.len() - 1]["binding"]
        .as_str()
        .unwrap()
        .ends_with("/1"));

    // A journal split at this very frame must preserve the proven slot 1
    // binding, even though its raw SpireField says slot 0. The complete raw
    // observation, not just its canonical snapshot or card names, authorizes it.
    let resume = java.resume_state_v1().unwrap();
    let raw = java.current_raw_observation_v1().unwrap().clone();
    for fault in ["none", "epoch", "frame", "source"] {
        let mut boundary = raw.clone();
        match fault {
            "epoch" => boundary.producer.session_epoch.push_str("-different"),
            "frame" => boundary.frame_id += 1,
            "source" => {
                boundary.frame["parity_internal"]["master_deck"]
                    .as_array_mut()
                    .unwrap()
                    .last_mut()
                    .unwrap()["source"]["product_slot"] = json!(9);
            }
            _ => {}
        }
        assert_eq!(resume.matches_observation(&boundary), fault == "none");
        let mut continued = JavaLockstepAdapterV1::resume(
            FixtureTransport {
                observations: vec![boundary],
                cursor: 0,
            },
            bootstrap.clone(),
            resume.clone(),
        )
        .unwrap();
        if fault == "none" {
            assert_eq!(continued.observe().unwrap(), after);
            assert_eq!(continued.resume_state_v1().unwrap(), resume);
        } else {
            assert!(continued
                .observe()
                .unwrap_err()
                .message
                .contains("differs from the replayed resume boundary"));
        }
    }
    let mut conflicting_reload = raw;
    conflicting_reload
        .producer
        .session_epoch
        .push_str("-reloaded");
    let mut reattached = JavaLockstepAdapterV1::reattach_after_native_reload(
        FixtureTransport {
            observations: vec![conflicting_reload],
            cursor: 0,
        },
        bootstrap,
        resume,
        None,
    )
    .unwrap();
    assert!(reattached
        .observe()
        .unwrap_err()
        .message
        .contains("native reload card source conflicts"));
}

#[test]
fn power_potion_discovery_keeps_combat_and_requires_typed_birth_evidence() {
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
    let mut rust =
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture-rust", "discovery")
            .unwrap();
    let map = advance_to_map(&mut rust);
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
    let combat = execute(&mut rust, &map, &route);
    let use_potion = action_space(&combat).into_iter().find(|action|
        matches!(action, SemanticActionV1::UsePotion { potion_id, .. } if potion_id == "powerpotion")
    ).unwrap();
    let discovery = execute(&mut rust, &combat, &use_potion);
    let mut offer = equivalent_java_successor(&rust, &discovery, 201);
    attach_fixture_card_birth_witness(&mut offer, &combat, &discovery, &use_potion, 200, 0);
    for choice in action_space(&discovery) {
        let discard = matches!(choice, SemanticActionV1::DiscardPotion { .. });
        let mut branch = rust.clone();
        let after = execute(&mut branch, &discovery, &choice);
        let mut resolved = equivalent_java_successor(&branch, &after, 202);
        if !discard {
            attach_fixture_card_birth_witness(&mut resolved, &discovery, &after, &choice, 201, 1);
        }
        for wrong_cause in [false, true] {
            if discard && wrong_cause {
                continue;
            }
            let mut successor = resolved.clone();
            if wrong_cause {
                successor.frame["card_birth_witness"]["cause"] =
                    json!({"kind": "end_turn_resolution"});
            }
            let mut java = JavaLockstepAdapterV1::new(
                FixtureTransport {
                    observations: vec![
                        equivalent_java_observation(&combat, 200),
                        offer.clone(),
                        successor,
                    ],
                    cursor: 0,
                },
                bootstrap.clone(),
            )
            .unwrap();
            let initial = java.observe().unwrap();
            assert_exact(&initial, &combat);
            let prepared = java.prepare(&use_potion, &initial).unwrap();
            let java_offer = java.execute(prepared).unwrap();
            assert_exact(&java_offer, &discovery);
            assert_eq!(
                observed(&java_offer, ParityDomainV1::Combat)["status"],
                "player_decision"
            );
            let prepared = java.prepare(&choice, &java_offer).unwrap();
            let result = java.execute(prepared);
            if wrong_cause {
                assert!(result.unwrap_err().message.contains("cause"));
            } else {
                assert_exact(&result.unwrap(), &after);
                assert_eq!(
                    observed(&after, ParityDomainV1::MasterDeck),
                    observed(&combat, ParityDomainV1::MasterDeck)
                );
            }
        }
    }
    let mut missing_context = offer;
    missing_context.frame["public_base"]["decision"]["card_reward"]["discovery_context"] =
        Value::Null;
    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![missing_context],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    assert!(java
        .observe()
        .unwrap_err()
        .message
        .contains("Discovery amount"));
}

#[test]
fn treasure_open_and_reward_are_exact_across_both_adapters() {
    let (bootstrap, mut oracle, treasure) = natural_treasure_fixture("treasure-oracle");
    let open = action_space(&treasure)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Treasure,
                    ..
                }
            )
        })
        .unwrap();
    assert!(matches!(
        &open,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Treasure,
            choice,
        } if choice.kind == "treasure_chest" && choice.id == "open"
    ));
    let leave = SemanticActionV1::Proceed {
        surface: DecisionSurfaceV1::Treasure,
    };
    let mut leave_rust = oracle.clone();
    let map = execute(&mut leave_rust, &treasure, &leave);
    let mut leave_java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![
                equivalent_java_observation(&treasure, 290),
                equivalent_java_observation(&map, 291),
            ],
            cursor: 0,
        },
        bootstrap.clone(),
    )
    .unwrap();
    let before = leave_java.observe().unwrap();
    assert_exact(&before, &treasure);
    let prepared = leave_java.prepare(&leave, &before).unwrap();
    assert_exact(&leave_java.execute(prepared).unwrap(), &map);
    assert!(leave_rust.last_rng_call_ledger_v1().unwrap().0.is_empty());
    let reward = execute(&mut oracle, &treasure, &open);
    assert_eq!(reward.boundary_kind, "natural_treasure_reward");
    let sapphire = action_space(&reward)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::TreasureReward,
                    choice,
                } if choice.id == "sapphire_key"
            )
        })
        .unwrap();
    let empty_reward = execute(&mut oracle, &reward, &sapphire);
    let close = action_space(&empty_reward)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::TreasureReward,
                }
            )
        })
        .unwrap();
    let map_preview = execute(&mut oracle, &empty_reward, &close);

    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![
                equivalent_java_observation(&treasure, 300),
                equivalent_java_observation(&reward, 301),
                equivalent_java_observation(&empty_reward, 302),
                equivalent_java_observation(&map_preview, 303),
            ],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let (_, mut rust, rust_before) = natural_treasure_fixture("treasure-under-test");
    let java_before = java.observe().unwrap();
    assert_eq!(
        compare_snapshots_v1(&java_before, &rust_before)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );

    let java_prepared = java.prepare(&open, &java_before).unwrap();
    let rust_prepared = rust.prepare(&open, &rust_before).unwrap();
    let java_after = java.execute(java_prepared).unwrap();
    let rust_after = rust.execute(rust_prepared).unwrap();

    assert_eq!(java_after.boundary_kind, "natural_treasure_reward");
    assert_eq!(
        observed(&java_after, ParityDomainV1::Rewards)["status"],
        "treasure"
    );
    assert!(action_space(&java_after).iter().all(|action| matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::TreasureReward,
            ..
        } | SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::TreasureReward,
        }
    )));
    assert_eq!(
        compare_snapshots_v1(&java_after, &rust_after)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );

    let sapphire = action_space(&rust_after)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::TreasureReward,
                    choice,
                } if choice.id == "sapphire_key"
            )
        })
        .unwrap();
    let java_prepared = java.prepare(&sapphire, &java_after).unwrap();
    let rust_prepared = rust.prepare(&sapphire, &rust_after).unwrap();
    let java_empty = java.execute(java_prepared).unwrap();
    let rust_empty = rust.execute(rust_prepared).unwrap();
    assert_eq!(
        compare_snapshots_v1(&java_empty, &rust_empty)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );

    let close = action_space(&rust_empty)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::TreasureReward,
                }
            )
        })
        .unwrap();
    let java_prepared = java.prepare(&close, &java_empty).unwrap();
    let rust_prepared = rust.prepare(&close, &rust_empty).unwrap();
    let java_map = java.execute(java_prepared).unwrap();
    let rust_map = rust.execute(rust_prepared).unwrap();
    assert_eq!(
        observed(&java_map, ParityDomainV1::Room)["kind"],
        "map_preview"
    );
    assert_eq!(
        observed(&java_map, ParityDomainV1::Rewards)["status"],
        "treasure"
    );
    assert_eq!(
        compare_snapshots_v1(&java_map, &rust_map).unwrap().status,
        ComparisonStatusV1::Exact
    );
}

#[test]
fn codex_choices_compare_combat_and_require_actual_selection_birth_witness() {
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
    let mut rust =
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture", "codex").unwrap();
    let map = advance_to_map(&mut rust);
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
    let combat = execute(&mut rust, &map, &route);
    let offers = execute(&mut rust, &combat, &SemanticActionV1::EndTurn);
    assert_eq!(offers.boundary_kind, "natural_combat_codex");
    let mut offer = equivalent_java_successor(&rust, &offers, 201);
    attach_fixture_card_birth_witness(
        &mut offer,
        &combat,
        &offers,
        &SemanticActionV1::EndTurn,
        200,
        0,
    );
    assert!(offer.frame["public_base"]["decision"]["card_reward"]["discovery_context"].is_null());
    for choice in action_space(&offers) {
        let discard = matches!(choice, SemanticActionV1::DiscardPotion { .. });
        let mut branch = rust.clone();
        let after = execute(&mut branch, &offers, &choice);
        let mut resolved = equivalent_java_successor(&branch, &after, 202);
        if !discard {
            attach_fixture_card_birth_witness(&mut resolved, &offers, &after, &choice, 201, 1);
        }
        for corruption in 0..=2 {
            if discard && corruption != 0 {
                continue;
            }
            if corruption == 2 && matches!(choice, SemanticActionV1::Cancel { .. }) {
                continue; // No new object needs binding when the entire resumed queue creates none.
            }
            let mut successor = resolved.clone();
            if corruption == 1 {
                successor.frame["card_birth_witness"]["cause"] =
                    json!({"kind":"discovery_resolution"});
            }
            if corruption == 2 {
                successor.frame["card_birth_witness"] = Value::Null;
            }
            let mut java = JavaLockstepAdapterV1::new(
                FixtureTransport {
                    observations: vec![
                        equivalent_java_observation(&combat, 200),
                        offer.clone(),
                        successor,
                    ],
                    cursor: 0,
                },
                bootstrap.clone(),
            )
            .unwrap();
            let initial = java.observe().unwrap();
            assert_exact(&initial, &combat);
            let prepared = java.prepare(&SemanticActionV1::EndTurn, &initial).unwrap();
            let java_offers = java.execute(prepared).unwrap();
            assert_exact(&java_offers, &offers);
            assert_eq!(
                observed(&java_offers, ParityDomainV1::Combat)["status"],
                "player_decision"
            );
            let prepared = java.prepare(&choice, &java_offers).unwrap();
            let result = java.execute(prepared);
            if corruption == 0 {
                assert_exact(&result.unwrap(), &after);
            } else {
                match result {
                    Err(_) => {}
                    Ok(snapshot) => assert_ne!(
                        compare_snapshots_v1(&snapshot, &after).unwrap().status,
                        ComparisonStatusV1::Exact,
                        "missing/wrong source cannot compare Exact"
                    ),
                }
            }
        }
    }
    for (field, bad) in [("skip_available", json!(false)), ("cards", json!([]))] {
        let mut invalid = offer.clone();
        invalid.frame["public_base"]["decision"]["card_reward"][field] = bad;
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![invalid],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        assert!(java.observe().is_err());
    }
}

#[test]
fn java_treasure_boundary_rejects_missing_or_malformed_chest_identity() {
    let (bootstrap, _, treasure) = natural_treasure_fixture("treasure-invalid-fixture");
    let base = equivalent_java_observation(&treasure, 310);

    let mut missing = base.clone();
    missing.frame["parity_internal"]["treasure_chest"] = Value::Null;
    let mut malformed = base.clone();
    malformed.frame["parity_internal"]["treasure_chest"]["size"] = json!("tiny");
    let mut explicit_null_gold = base;
    explicit_null_gold.frame["parity_internal"]["treasure_chest"]["gold_base_amount"] = Value::Null;

    for (observation, expected_message) in [
        (missing, "omitted its pending chest"),
        (malformed, "invalid typed specification"),
        (
            explicit_null_gold,
            "must be omitted when no gold is present",
        ),
    ] {
        let mut java = JavaLockstepAdapterV1::new(
            FixtureTransport {
                observations: vec![observation],
                cursor: 0,
            },
            bootstrap.clone(),
        )
        .unwrap();
        let error = java.observe().unwrap_err();
        assert_eq!(error.kind, AdapterFailureKindV1::Evidence);
        assert!(
            error.message.contains(expected_message),
            "unexpected treasure contract failure: {}",
            error.message
        );
    }
}

#[test]
fn java_treasure_reward_rejects_a_sapphire_key_link_to_another_relic() {
    let (bootstrap, mut oracle, treasure) = natural_treasure_fixture("treasure-link-fixture");
    let open = action_space(&treasure)
        .into_iter()
        .find(|action| {
            matches!(
                action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Treasure,
                    ..
                }
            )
        })
        .unwrap();
    let reward = execute(&mut oracle, &treasure, &open);
    let mut observation = equivalent_java_observation(&reward, 320);
    let sapphire = observation
        .actions
        .iter_mut()
        .find(|action| action.reward_type.as_deref() == Some("SAPPHIRE_KEY"))
        .expect("ordinary A20H treasure should expose one Sapphire Key action");
    sapphire.reward_id = Some("Burning Blood".to_owned());

    let mut java = JavaLockstepAdapterV1::new(
        FixtureTransport {
            observations: vec![observation],
            cursor: 0,
        },
        bootstrap,
    )
    .unwrap();
    let error = java.observe().unwrap_err();
    assert_eq!(error.kind, AdapterFailureKindV1::Evidence);
    assert!(
        error.message.contains("reward action at index")
            && error.message.contains("disagrees with its typed item"),
        "unexpected Sapphire Key link failure: {}",
        error.message
    );
}

fn natural_treasure_fixture(
    epoch: &str,
) -> (
    sts_parity_lockstep::NaturalRunBootstrapV1,
    RustLockstepAdapterV1,
    ParitySnapshotV1,
) {
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
        RustLockstepAdapterV1::new(session, bootstrap.clone(), "fixture-rust", epoch).unwrap();
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
    let treasure = execute(&mut adapter, &map, &route);
    (bootstrap, adapter, treasure)
}

fn natural_first_combat_fixture() -> (sts_parity_lockstep::NaturalRunBootstrapV1, ParitySnapshotV1)
{
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "fixture-rust",
        "negative-contract-epoch",
    )
    .unwrap();
    let map = advance_to_map(&mut rust);
    let route = action_space(&map).into_iter().next().unwrap();
    let combat = execute(&mut rust, &map, &route);
    (bootstrap, combat)
}

fn natural_first_combat_reward_fixture(
) -> (sts_parity_lockstep::NaturalRunBootstrapV1, ParitySnapshotV1) {
    let config = RunControlConfig {
        seed: 2_026_090_301,
        ascension_level: 20,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Mini,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "fixture-rust",
        "negative-reward-epoch",
    )
    .unwrap();
    let map = advance_to_map(&mut rust);
    let route = action_space(&map).into_iter().next().unwrap();
    let mut current = execute(&mut rust, &map, &route);
    let actions = rust
        .plan_winning_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
            max_nodes: Some(2_000),
            wall_ms: Some(5_000),
            ..Default::default()
        })
        .unwrap();
    for action in actions {
        current = execute(&mut rust, &current, &action);
    }
    assert_eq!(current.boundary_kind, "natural_combat_reward");
    (bootstrap, current)
}

// This fixture is an interoperability contract, not Java parity evidence. It
// translates one shared snapshot back into the raw shape that CommunicationMod
// publishes, then requires the Java adapter to recover the same shared state.
fn equivalent_java_successor(
    adapter: &RustLockstepAdapterV1,
    snapshot: &ParitySnapshotV1,
    frame_id: u64,
) -> JavaRuntimeObservationV1 {
    let mut observation = equivalent_java_observation(snapshot, frame_id);
    observation.frame["fixture_rng_calls"] =
        serde_json::to_value(adapter.last_rng_call_ledger_v1().unwrap()).unwrap();
    observation
}

fn equivalent_java_observation(
    snapshot: &ParitySnapshotV1,
    frame_id: u64,
) -> JavaRuntimeObservationV1 {
    let run = observed(snapshot, ParityDomainV1::Run);
    let map = observed(snapshot, ParityDomainV1::Map);
    let room = observed(snapshot, ParityDomainV1::Room);
    let deck = observed(snapshot, ParityDomainV1::MasterDeck);
    let relations = observed(snapshot, ParityDomainV1::CardBindings);
    let relics = observed(snapshot, ParityDomainV1::Relics);
    let potions = observed(snapshot, ParityDomainV1::Potions);
    let pools = observed(snapshot, ParityDomainV1::CardPools);
    let rng = observed(snapshot, ParityDomainV1::RngStreams);

    let sources = relations
        .as_array()
        .unwrap()
        .iter()
        .map(|relation| {
            (
                required_text(relation, "binding").to_owned(),
                relation["source"].clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut binding_uuids = BTreeMap::new();
    let raw_deck = deck
        .as_array()
        .unwrap()
        .iter()
        .map(|card| {
            let ordinal = card["ordinal"].as_u64().unwrap();
            let binding = required_text(card, "binding");
            let uuid = format!("fixture-card-{ordinal}");
            binding_uuids.insert(binding.to_owned(), uuid.clone());
            json!({
                "ordinal": ordinal,
                "card_uuid": uuid,
                "card_id": card["card_id"].clone(),
                "upgrades": card["upgrades"].clone(),
                "source": sources[binding].clone(),
            })
        })
        .collect::<Vec<_>>();
    for relation in relations.as_array().unwrap() {
        let binding = required_text(relation, "binding");
        binding_uuids
            .entry(binding.to_owned())
            .or_insert_with(|| fixture_dynamic_card_uuid(&relation["source"]));
    }
    let raw_relics = relics
        .as_array()
        .unwrap()
        .iter()
        .map(|relic| {
            let bound_card_uuid = relic["bound_card"]
                .as_str()
                .map(|binding| binding_uuids[binding].clone());
            let art_of_war = if relic["combat_state"]["kind"] == "art_of_war" {
                json!({
                    "first_turn": relic["combat_state"]["first_turn"],
                    "gain_energy_next": relic["combat_state"]["gain_energy_next"],
                })
            } else {
                Value::Null
            };
            json!({
                "index": relic["ordinal"].clone(),
                "relic_id": relic["id"].clone(),
                "counter": relic["counter"].clone(),
                "used_up": relic["used_up"].clone(),
                "bound_card_uuid": bound_card_uuid,
                "art_of_war": art_of_war,
                "hovering_kite": if relic["combat_state"]["kind"] == "hovering_kite" {
                    json!({"triggered_this_turn":relic["combat_state"]["triggered_this_turn"]})
                } else { Value::Null },
            })
        })
        .collect::<Vec<_>>();
    let raw_potions = potions
        .as_array()
        .unwrap()
        .iter()
        .map(|potion| {
            let id = potion["id"].clone();
            json!({
                "slot_index": potion["slot"].clone(),
                "empty": id.is_null(),
                "potion_id": id,
            })
        })
        .collect::<Vec<_>>();
    let (current_x, current_y) = map["current"]
        .as_str()
        .map(node_coordinate)
        .unwrap_or((-1, -1));
    let raw_nodes = map["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            let edges = node["links"]
                .as_array()
                .unwrap()
                .iter()
                .map(|link| {
                    let (destination_x, destination_y) = node_coordinate(link.as_str().unwrap());
                    json!({
                        "destination_x": destination_x,
                        "destination_y": destination_y,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "x": node["x"].clone(),
                "y": node["y"].clone(),
                "room_type": java_room_type(required_text(node, "room")),
                "has_emerald_key": node["emerald_key"].clone(),
                "edges": edges,
            })
        })
        .collect::<Vec<_>>();
    let (
        boundary_kind,
        decision,
        public_combat,
        internal_combat,
        combat_rewards,
        mut raw_action_space,
        mut actions,
    ) = match snapshot.boundary_kind.as_str() {
        "natural_neow" => {
            let screen = match required_text(room, "stage") {
                "introduction" => 0,
                "choose_reward" => 3,
                "leave" => 99,
                stage => panic!("unsupported fixture Neow stage {stage:?}"),
            };
            (
                "run_decision",
                json!({
                    "kind": "event",
                    "event_id": "Neow",
                    "event_screen_index": screen,
                }),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                java_actions(snapshot, frame_id, Some(screen)),
            )
        }
        "natural_map" => {
            let rewards: NaturalRunRewardsV2 =
                serde_json::from_value(observed(snapshot, ParityDomainV1::Rewards).clone())
                    .unwrap();
            let internal_rewards = match rewards {
                NaturalRunRewardsV2::Inactive => Value::Null,
                NaturalRunRewardsV2::Combat { items, .. }
                | NaturalRunRewardsV2::Treasure { items, .. } => Value::Array(
                    items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| java_reward_internal(index, item))
                        .collect(),
                ),
            };
            (
                "run_decision",
                json!({"kind": "map"}),
                Value::Null,
                Value::Null,
                internal_rewards,
                Value::Null,
                java_actions(snapshot, frame_id, None),
            )
        }
        "natural_double_boss_complete" => (
            "run_decision",
            json!({"kind":"double_boss_complete"}),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            java_actions(snapshot, frame_id, None),
        ),
        "natural_event" => {
            let event_id = required_text(room, "event_id");
            let screen = room["screen"].as_i64().unwrap();
            (
                "run_decision",
                json!({
                    "kind": "event",
                    "event_id": event_id,
                    "event_screen_index": screen,
                    "event_identity": {
                        "status": "supported",
                        "event_id": event_id,
                        "runtime_class": format!("fixture.events.{event_id}"),
                        "unsupported_reason": null,
                    },
                }),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                java_actions(snapshot, frame_id, None),
            )
        }
        "natural_campfire" => (
            "run_decision",
            json!({"kind": "campfire"}),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            java_campfire_option_actions(snapshot, frame_id),
        ),
        "natural_treasure" => (
            "run_decision",
            json!({"kind": "treasure"}),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            java_actions(snapshot, frame_id, None),
        ),
        "natural_campfire_smith" => {
            let (raw_actions, actions) = java_smith_card_actions(
                snapshot,
                deck,
                &binding_uuids,
                frame_id,
                "smith_card",
                "choose_smith_card",
            );
            (
                "run_decision",
                json!({"kind": "smith_card"}),
                Value::Null,
                Value::Null,
                Value::Null,
                json!({"actions": raw_actions}),
                actions,
            )
        }
        "natural_grid_select" | "natural_neow_grid_select" => {
            let neow = snapshot.boundary_kind == "natural_neow_grid_select";
            let astrolabe = room["relic_id"].as_str() == Some("astrolabe");
            let event_id = if neow {
                "Neow"
            } else if astrolabe {
                "none"
            } else {
                required_text(room, "event_id")
            };
            let event_id = if event_id == "drugdealer" {
                "DrugDealer"
            } else {
                event_id
            };
            let screen = if neow || astrolabe {
                99
            } else {
                room["screen"].as_i64().unwrap()
            };
            let selection_reason = match required_text(room, "selection_kind") {
                "deck_remove" => "DeckRemove",
                "deck_upgrade" => "DeckUpgrade",
                "deck_transform" => "DeckTransform",
                "deck_transform_upgraded" => "Astrolabe",
                kind => panic!("unsupported fixture grid selection {kind:?}"),
            };
            let mut unique_choices = std::collections::BTreeSet::new();
            let cards = action_space(snapshot).into_iter().flat_map(|action| match action {
                SemanticActionV1::SubmitSelection { surface: DecisionSurfaceV1::GridSelect, choices } => choices,
                SemanticActionV1::Select { surface: DecisionSurfaceV1::GridSelect, choice } if astrolabe => vec![choice],
                SemanticActionV1::DiscardPotion { .. } => vec![],
                _ => panic!("fixture grid boundary exposed a non-grid action"),
            }).filter(|choice| unique_choices.insert(choice.id.clone()))
                .enumerate().map(|(index, choice)| {
                    let card = deck.as_array().unwrap().iter()
                        .find(|card| card["binding"].as_str() == Some(choice.id.as_str())).unwrap();
                    json!({"index":index, "card_uuid":binding_uuids[&choice.id],
                        "card_id":card["card_id"], "name":card["card_id"], "upgrades":card["upgrades"],
                        "combat_cost":-1, "cost_for_turn":-1})
                }).collect::<Vec<_>>();
            let candidate_uuids = cards
                .iter()
                .map(|card| card["card_uuid"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let selected_cards = room["selected"].as_array().into_iter().flatten().enumerate().map(|(i, b)| {
                let binding = b.as_str().unwrap();
                let card = deck.as_array().unwrap().iter().find(|c| c["binding"] == b.clone()).unwrap();
                json!({"index":i,"card_uuid":binding_uuids[binding],"card_id":card["card_id"],"upgrades":card["upgrades"]})
            }).collect::<Vec<_>>();
            let requested = if astrolabe {
                3
            } else if event_id == "DrugDealer" {
                2
            } else {
                1
            };
            let allowed_counts = if event_id == "DrugDealer" {
                vec![2]
            } else {
                vec![1]
            };
            let grid_id = format!("grid_select:{event_id}:MasterDeck:{selection_reason}:{requested}:false:false:{}:selected:{}",
                candidate_uuids.join(","), selected_cards.iter().map(|c| c["card_uuid"].as_str().unwrap()).collect::<Vec<_>>().join(","));
            let event_value = if astrolabe {
                Value::Null
            } else {
                json!(event_id)
            };
            let raw_action = json!({
                "action_id": format!("submit_grid_select:{grid_id}"),
                "kind": "submit_grid_select",
                "grid_select_id": grid_id,
                "source_pile": "MasterDeck",
                "selection_reason": selection_reason,
                "event_id": event_value,
                "parameter_schema": {
                    "field": "selected_card_uuids",
                    "item_format": "uuid",
                    "ordered": true,
                    "unique_items": true,
                    "allowed_card_uuids": candidate_uuids,
                    "allowed_counts": allowed_counts,
                },
            });
            (
                "run_decision",
                json!({
                    "kind": "grid_select",
                    "event_id": event_value,
                    "event_screen_index": screen,
                    "event_identity": {
                        "status": "supported",
                        "event_id": event_id,
                        "runtime_class": format!("fixture.events.{event_id}"),
                        "unsupported_reason": null,
                    },
                    "grid_select": {
                        "grid_select_id": grid_id,
                        "source_pile": "MasterDeck",
                        "selection_reason": selection_reason,
                        "requested_cards": requested,
                        "any_number": false,
                        "can_cancel": false,
                        "allowed_counts": allowed_counts,
                        "confirm_supported": false,
                        "cancel_supported": false,
                        "candidate_cards": cards,
                        "selected_cards": selected_cards,
                    },
                }),
                Value::Null,
                Value::Null,
                Value::Null,
                json!({"actions": [raw_action]}),
                vec![JavaRuntimeActionV1 {
                    runtime_handle: format!("fixture-f{frame_id}-grid"),
                    action_id: format!("submit_grid_select:{grid_id}"),
                    kind: "submit_grid_select".to_owned(),
                    event_id: if astrolabe {
                        None
                    } else {
                        Some(event_id.to_owned())
                    },
                    option_index: None,
                    neow_reward_type: None,
                    neow_drawback: None,
                    map_x: None,
                    map_y: None,
                    map_room_type: None,
                    reward_type: None,
                    reward_index: None,
                    reward_id: None,
                    reward_gold: None,
                    potion_slot: None,
                    potion_id: None,
                    parameters_required: true,
                }],
            )
        }
        "natural_combat" | "natural_combat_discovery" | "natural_combat_codex" => {
            let combat = observed(snapshot, ParityDomainV1::Combat);
            let fixture =
                java_combat_fixture(snapshot, combat, &raw_potions, &binding_uuids, frame_id);
            let discovery = matches!(
                snapshot.boundary_kind.as_str(),
                "natural_combat_discovery" | "natural_combat_codex"
            )
            .then(|| java_card_reward_fixture(snapshot, frame_id));
            (
                if discovery.is_some() {
                    "run_decision"
                } else {
                    "combat_player_decision"
                },
                discovery
                    .as_ref()
                    .map(|offer| offer.decision.clone())
                    .unwrap_or(Value::Null),
                fixture.public_combat,
                fixture.internal_combat,
                Value::Null,
                fixture.action_space,
                discovery
                    .map(|offer| offer.actions)
                    .unwrap_or(fixture.actions),
            )
        }
        "natural_hand_select" => {
            let shared_actions = action_space(snapshot);
            let allowed_counts = shared_actions
                .iter()
                .filter_map(|action| match action {
                    SemanticActionV1::SubmitSelection { choices, .. } => Some(choices.len()),
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let retain = allowed_counts.first() == Some(&0);
            let retain_amount = observed(snapshot, ParityDomainV1::Combat)["state"]
                ["player_powers"]
                .as_array()
                .and_then(|powers| {
                    powers
                        .iter()
                        .find(|power| power["power_id"] == "retaincards")
                })
                .and_then(|power| power["amount"].as_u64());
            let requested = if retain {
                retain_amount.expect("optional fixture selection requires its retain power")
                    as usize
            } else {
                shared_actions
                    .iter()
                    .find_map(|action| match action {
                        SemanticActionV1::SubmitSelection { choices, .. } => Some(choices.len()),
                        _ => None,
                    })
                    .unwrap()
            };
            let mut unique_cards = std::collections::BTreeSet::new();
            let mut cards = shared_actions
                .iter()
                .flat_map(|action| match action {
                    SemanticActionV1::SubmitSelection {
                        surface: DecisionSurfaceV1::HandSelect,
                        choices,
                    } => {
                        assert!(allowed_counts.contains(&choices.len()));
                        choices.as_slice()
                    }
                    _ => &[],
                })
                .filter(|choice| unique_cards.insert(choice.id.clone()))
                .enumerate()
                .map(|(index, choice)| {
                    let SemanticScalarV1::Text(card_id) = &choice.attributes["card_id"] else {
                        panic!("fixture hand card omitted its stable ID")
                    };
                    let SemanticScalarV1::Integer(upgrades) = choice.attributes["upgrades"] else {
                        panic!("fixture hand card omitted its upgrade count")
                    };
                    json!({
                        "index": index,
                        "card_uuid": binding_uuids[&choice.id],
                        "card_id": card_id,
                        "name": card_id,
                        "upgrades": upgrades,
                        "combat_cost": 1,
                        "cost_for_turn": 1,
                    })
                })
                .collect::<Vec<_>>();
            if let Some(hand) =
                observed(snapshot, ParityDomainV1::Combat)["state"]["zones"]["hand"].as_array()
            {
                cards.sort_by_key(|card| {
                    hand.iter()
                        .position(|source| {
                            binding_uuids[source["binding"].as_str().unwrap()] == card["card_uuid"]
                        })
                        .unwrap()
                });
                for (index, card) in cards.iter_mut().enumerate() {
                    card["index"] = json!(index);
                }
            }
            let candidate_uuids = cards
                .iter()
                .map(|card| card["card_uuid"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let raw_hand = cards
                .iter()
                .map(|card| {
                    json!({
                        "instance_uuid": card["card_uuid"].clone(),
                        "card_id": card["card_id"].clone(),
                        "upgrades": card["upgrades"].clone(),
                        "combat_cost": card["combat_cost"].clone(),
                        "cost_for_turn": card["cost_for_turn"].clone(),
                        "free_to_play_once": card["free_to_play_once"].as_bool().unwrap_or(false),
                    })
                })
                .collect::<Vec<_>>();
            let hand_select_id = format!(
                "hand_select:{requested}:false:{retain}:{retain}:{}:selected:",
                candidate_uuids.join(",")
            );
            let submit_raw_action = json!({
                "action_id": format!("submit_hand_select:{hand_select_id}"),
                "kind": "submit_hand_select",
                "hand_select_id": hand_select_id,
                "parameter_schema": {
                    "field": "selected_card_uuids",
                    "item_format": "uuid",
                    "ordered": true,
                    "unique_items": true,
                    "allowed_card_uuids": candidate_uuids,
                    "allowed_counts": allowed_counts,
                },
            });
            let mut raw_actions = vec![submit_raw_action];
            let mut runtime_actions = vec![JavaRuntimeActionV1 {
                runtime_handle: format!("fixture-f{frame_id}-hand"),
                action_id: format!("submit_hand_select:{hand_select_id}"),
                kind: "submit_hand_select".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: None,
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: None,
                potion_id: None,
                parameters_required: true,
            }];
            for (index, action) in shared_actions.into_iter().enumerate() {
                let SemanticActionV1::DiscardPotion { slot, potion_id } = action else {
                    continue;
                };
                let action_id = format!("discard_potion:{slot}:{potion_id}");
                raw_actions.push(json!({
                    "action_id": action_id,
                    "kind": "discard_potion",
                    "potion_slot": slot,
                    "potion_id": potion_id,
                    "parameters_required": false,
                }));
                runtime_actions.push(JavaRuntimeActionV1 {
                    runtime_handle: format!("fixture-f{frame_id}-hand-discard-{index}"),
                    action_id,
                    kind: "discard_potion".to_owned(),
                    event_id: None,
                    option_index: None,
                    neow_reward_type: None,
                    neow_drawback: None,
                    map_x: None,
                    map_y: None,
                    map_room_type: None,
                    reward_type: None,
                    reward_index: None,
                    reward_id: None,
                    reward_gold: None,
                    potion_slot: Some(usize::from(slot)),
                    potion_id: Some(potion_id),
                    parameters_required: false,
                });
            }
            let empty_zone = json!({
                "ordering": "java_group_index_ascending",
                "top": "last",
                "cards": [],
            });
            let combat = observed(snapshot, ParityDomainV1::Combat);
            let (public_combat, internal_combat) = if combat["status"] == "player_decision" {
                let mut fixture =
                    java_combat_fixture(snapshot, combat, &raw_potions, &binding_uuids, frame_id);
                let zones = &mut fixture.internal_combat["zones"];
                let all_hand = zones["hand"]["cards"].as_array().unwrap().clone();
                for card in &mut cards {
                    let raw = all_hand
                        .iter()
                        .find(|raw| raw["instance_uuid"] == card["card_uuid"])
                        .unwrap();
                    card["combat_cost"] = raw["combat_cost"].clone();
                    card["cost_for_turn"] = raw["cost_for_turn"].clone();
                }
                zones["hand"] = java_ordered_zone(
                    all_hand
                        .iter()
                        .filter(|card| {
                            candidate_uuids
                                .contains(&card["instance_uuid"].as_str().unwrap().to_owned())
                        })
                        .cloned()
                        .collect(),
                );
                let held_zone = if zones["limbo"]["cards"][0]["card_id"] == "dualwield" {
                    "dual_wield_held"
                } else {
                    "armaments_held"
                };
                if !zones["limbo"]["cards"].as_array().unwrap().is_empty() {
                    zones[held_zone] = java_ordered_zone(
                        all_hand
                            .into_iter()
                            .filter(|card| {
                                !candidate_uuids
                                    .contains(&card["instance_uuid"].as_str().unwrap().to_owned())
                            })
                            .collect(),
                    );
                }
                let limbo = zones["limbo"]["cards"].as_array_mut().unwrap();
                assert!(limbo.len() <= 1);
                let in_use = limbo.pop().unwrap_or(Value::Null);
                zones["card_in_use"] = in_use;
                (fixture.public_combat, fixture.internal_combat)
            } else {
                (
                    Value::Null,
                    json!({
                        "zones": {
                            "draw": empty_zone, "discard": empty_zone, "exhaust": empty_zone,
                            "hand": java_ordered_zone(raw_hand), "limbo": empty_zone,
                        },
                        "monsters": [], "player_powers": [],
                    }),
                )
            };
            (
                "run_decision",
                json!({
                    "kind": "hand_select",
                    "hand_select": {
                        "hand_select_id": hand_select_id,
                        "selection_reason": "localized fixture text is not identity",
                        "requested_cards": requested,
                        "any_number": false,
                        "can_pick_zero": retain,
                        "up_to": retain,
                        "allowed_counts": allowed_counts,
                        "confirm_supported": true,
                        "cancel_supported": false,
                        "candidate_cards": cards,
                        "selected_cards": [],
                    },
                }),
                public_combat,
                internal_combat,
                Value::Null,
                json!({"actions": raw_actions}),
                runtime_actions,
            )
        }
        "natural_combat_reward" | "natural_treasure_reward" => {
            let fixture = java_reward_fixture(snapshot, frame_id);
            (
                "run_decision",
                json!({"kind": "combat_reward"}),
                Value::Null,
                Value::Null,
                fixture.internal_rewards,
                Value::Null,
                fixture.actions,
            )
        }
        "natural_card_reward" => {
            let fixture = java_card_reward_fixture(snapshot, frame_id);
            (
                "run_decision",
                fixture.decision,
                Value::Null,
                Value::Null,
                fixture.internal_rewards,
                Value::Null,
                fixture.actions,
            )
        }
        boundary => panic!("unsupported fixture boundary {boundary:?}"),
    };
    if matches!(
        snapshot.boundary_kind.as_str(),
        "natural_grid_select" | "natural_neow_grid_select"
    ) {
        for semantic in action_space(snapshot) {
            let SemanticActionV1::DiscardPotion { slot, potion_id } = semantic else {
                continue;
            };
            let action_id = format!("discard_potion:{slot}:{potion_id}");
            let mut action = parameter_free_java_action(
                frame_id,
                actions.len(),
                action_id.clone(),
                "discard_potion",
            );
            action.potion_slot = Some(usize::from(slot));
            action.potion_id = Some(potion_id.clone());
            actions.push(action);
            raw_action_space["actions"].as_array_mut().unwrap().push(json!({"action_id":action_id,"kind":"discard_potion","potion_slot":slot,"potion_id":potion_id,"parameters_required":false}));
        }
    }
    let treasure_chest = if snapshot.boundary_kind == "natural_treasure" {
        let mut chest = room["chest"].clone();
        if chest["gold_base_amount"].is_null() {
            chest.as_object_mut().unwrap().remove("gold_base_amount");
        }
        chest
    } else {
        Value::Null
    };

    JavaRuntimeObservationV1 {
        producer: ProducerIdentityV1 {
            implementation: "communicationmod-fixture".to_owned(),
            version: "fixture-java".to_owned(),
            session_epoch: "java-epoch".to_owned(),
        },
        frame_id,
        frame: json!({
            "kind": "state_frame",
            "profile": "parity_run_control",
            "frame_id": frame_id,
            "boundary": {"kind": boundary_kind, "stable": true},
            "scenario": null,
            "public_base": {
                "run": {
                    "ascension": run["ascension"].clone(),
                    "character": run["character"].clone(),
                    "act": run["act"].clone(),
                    "floor": run["floor"].clone(),
                    "current_hp": run["current_hp"].clone(),
                    "max_hp": run["max_hp"].clone(),
                    "gold": run["gold"].clone(),
                    "final_act_available": run["final_act_available"].clone(),
                    "has_ruby_key": run["keys"]["ruby"].clone(),
                    "has_sapphire_key": run["keys"]["sapphire"].clone(),
                    "has_emerald_key": run["keys"]["emerald"].clone(),
                },
                "decision": decision,
                "combat": public_combat,
                "map": {
                    "current_x": current_x,
                    "current_y": current_y,
                    "boss": map["boss"].clone(),
                    "nodes": raw_nodes,
                },
            },
            "parity_internal": {
                "run_seed": run["seed"].clone(),
                "content_profile": "all_unlocked",
                "run_state": {
                    "daily": run["daily"].clone(),
                    "shop_purge_base_cost": run["shop_purge_base_cost"].clone(),
                    "drop_state": run["drop_state"].clone(),
                    "room_flags": run["room_flags"].clone(),
                    "relic_pools": run["relic_pools"].clone(),
                    "encounter_schedule": run["encounter_schedule"].clone(),
                    "event_schedule": run["event_schedule"].clone(),
                },
                "card_pools": pools.clone(),
                "master_deck": raw_deck,
                "relics": raw_relics,
                "potions": raw_potions,
                "zones": internal_combat["zones"].clone(),
                "monsters": internal_combat["monsters"].clone(),
                "player_powers": internal_combat["player_powers"].clone(),
                "combat_rewards": combat_rewards,
                "treasure_chest": treasure_chest,
            },
            "rng_internal": raw_rng_capture(rng),
            "action_space": raw_action_space,
        }),
        actions,
    }
}

fn java_campfire_option_actions(
    snapshot: &ParitySnapshotV1,
    frame_id: u64,
) -> Vec<JavaRuntimeActionV1> {
    action_space(snapshot)
        .into_iter()
        .enumerate()
        .map(|(index, semantic)| {
            let SemanticActionV1::Select {
                surface: DecisionSurfaceV1::Campfire,
                choice,
            } = semantic
            else {
                panic!("fixture campfire exposed a non-campfire action")
            };
            assert_eq!(choice.kind, "campfire_option");
            let kind = match choice.id.as_str() {
                "rest" => "begin_campfire_rest",
                "smith" => "begin_campfire_smith",
                "recall" => "begin_campfire_recall",
                "dig" => "begin_campfire_dig",
                other => panic!("unsupported fixture campfire option {other:?}"),
            };
            parameter_free_java_action(frame_id, index, kind.to_owned(), kind)
        })
        .collect()
}

fn java_smith_card_actions(
    snapshot: &ParitySnapshotV1,
    deck: &Value,
    binding_uuids: &BTreeMap<String, String>,
    frame_id: u64,
    action_prefix: &str,
    kind: &str,
) -> (Vec<Value>, Vec<JavaRuntimeActionV1>) {
    let entries = action_space(snapshot)
        .into_iter()
        .enumerate()
        .map(|(index, semantic)| {
            let SemanticActionV1::Select {
                surface: DecisionSurfaceV1::Campfire,
                choice,
            } = semantic
            else {
                panic!("fixture Smith boundary exposed a non-campfire action")
            };
            assert_eq!(choice.kind, "deck_card");
            let card = deck
                .as_array()
                .unwrap()
                .iter()
                .find(|card| card["binding"].as_str() == Some(choice.id.as_str()))
                .unwrap();
            let ordinal = card["ordinal"].as_u64().unwrap();
            let uuid = &binding_uuids[&choice.id];
            let card_id = required_text(card, "card_id");
            let upgrades = card["upgrades"].as_u64().unwrap();
            let action_id = format!("{action_prefix}:{ordinal}:{uuid}:{card_id}:{upgrades}");
            (
                json!({
                    "action_id": action_id,
                    "kind": kind,
                    "card_ordinal": ordinal,
                    "card_uuid": uuid,
                    "card_id": card_id,
                    "card_upgrades": upgrades,
                }),
                parameter_free_java_action(frame_id, index, action_id, kind),
            )
        })
        .collect::<Vec<_>>();
    entries.into_iter().unzip()
}

fn smith_confirmation_observation(
    selection: &JavaRuntimeObservationV1,
    snapshot: &ParitySnapshotV1,
    selected: &SemanticActionV1,
    frame_id: u64,
) -> JavaRuntimeObservationV1 {
    let index = action_space(snapshot)
        .iter()
        .position(|candidate| candidate == selected)
        .unwrap();
    let mut raw = selection.frame["action_space"]["actions"][index].clone();
    let suffix = required_text(&raw, "action_id")
        .strip_prefix("smith_card:")
        .unwrap();
    let action_id = format!("confirm_smith_card:{suffix}");
    raw["action_id"] = json!(action_id);
    raw["kind"] = json!("confirm_smith_card");

    let mut observation = selection.clone();
    observation.frame_id = frame_id;
    observation.frame["frame_id"] = json!(frame_id);
    observation.frame["public_base"]["decision"] = json!({"kind": "smith_confirm"});
    observation.frame["action_space"] = json!({"actions": [raw]});
    observation.actions = vec![parameter_free_java_action(
        frame_id,
        0,
        action_id,
        "confirm_smith_card",
    )];
    observation
}

fn campfire_completion_observation(
    map: &ParitySnapshotV1,
    frame_id: u64,
) -> JavaRuntimeObservationV1 {
    let mut observation = equivalent_java_observation(map, frame_id);
    observation.frame["public_base"]["decision"] = json!({"kind": "campfire_complete"});
    observation.frame["action_space"] = Value::Null;
    observation.actions = vec![parameter_free_java_action(
        frame_id,
        0,
        "proceed_rest_room".to_owned(),
        "proceed_rest_room",
    )];
    observation
}

fn parameter_free_java_action(
    frame_id: u64,
    index: usize,
    action_id: String,
    kind: &str,
) -> JavaRuntimeActionV1 {
    JavaRuntimeActionV1 {
        runtime_handle: format!("fixture-f{frame_id}-a{index}"),
        action_id,
        kind: kind.to_owned(),
        event_id: None,
        option_index: None,
        neow_reward_type: None,
        neow_drawback: None,
        map_x: None,
        map_y: None,
        map_room_type: None,
        reward_type: None,
        reward_index: None,
        reward_id: None,
        reward_gold: None,
        potion_slot: None,
        potion_id: None,
        parameters_required: false,
    }
}

fn attach_fixture_card_birth_witness(
    successor: &mut JavaRuntimeObservationV1,
    before: &ParitySnapshotV1,
    after: &ParitySnapshotV1,
    action: &SemanticActionV1,
    from_frame_id: u64,
    action_ordinal: u32,
) {
    let cause = match action {
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            ..
        }
        | SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::DoubleBoss,
        } if observed(before, ParityDomainV1::Run)["character"] == "WATCHER" => {
            json!({"kind":"combat_entry_resolution"})
        }
        SemanticActionV1::PlayCard { card_binding, .. } => json!({
            "kind": "card_play",
            "producer_java_uuid": fixture_card_uuid(before, card_binding),
        }),
        SemanticActionV1::EndTurn => json!({"kind": "end_turn_resolution"}),
        SemanticActionV1::UsePotion {
            slot, potion_id, ..
        } => json!({
            "kind": "potion_use",
            "potion_id": potion_id,
            "slot": slot,
        }),
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatDiscovery,
            ..
        }
        | SemanticActionV1::Cancel {
            surface: DecisionSurfaceV1::CombatDiscovery,
        } => json!({"kind": "discovery_resolution"}),
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatCodex,
            ..
        }
        | SemanticActionV1::Cancel {
            surface: DecisionSurfaceV1::CombatCodex,
        } => json!({"kind": "combat_selection_resolution"}),
        SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::HandSelect | DecisionSurfaceV1::GridSelect,
            ..
        } if matches!(
            before.boundary_kind.as_str(),
            "natural_hand_select" | "natural_combat_grid_select"
        ) =>
        {
            json!({"kind":"combat_selection_resolution"})
        }
        _ => return,
    };
    let observation = equivalent_java_observation(before, from_frame_id);
    let action_id = if let SemanticActionV1::SubmitSelection { surface, .. } = action {
        let kind = match surface {
            DecisionSurfaceV1::HandSelect => "submit_hand_select",
            DecisionSurfaceV1::GridSelect => "submit_grid_select",
            _ => unreachable!(),
        };
        observation
            .actions
            .iter()
            .find(|candidate| candidate.kind == kind)
            .unwrap()
            .action_id
            .clone()
    } else {
        let action_index = action_space(before)
            .iter()
            .position(|candidate| candidate == action)
            .unwrap();
        observation.actions[action_index].action_id.clone()
    };
    let mut products = observed(after, ParityDomainV1::CardBindings)
        .as_array()
        .unwrap()
        .iter()
        .filter(|relation| {
            relation["source"]["kind"] == "combat_birth"
                && relation["source"]["action_ordinal"].as_u64() == Some(u64::from(action_ordinal))
        })
        .map(|relation| {
            let binding = required_text(relation, "binding");
            let (card, first_zone) = fixture_combat_card(after, binding);
            json!({
                "generation_index": relation["source"]["product_slot"].clone(),
                "java_uuid": fixture_card_uuid(after, binding),
                "card_id": card["card_id"].clone(),
                "upgrades": card["upgrades"].clone(),
                "first_zone": first_zone,
            })
        })
        .collect::<Vec<_>>();
    products.sort_by_key(|product| product["generation_index"].as_u64().unwrap());
    successor.frame["card_birth_witness"] = json!({
        "kind": "card_birth_witness_v1",
        "from_frame_id": from_frame_id,
        "to_frame_id": successor.frame_id,
        "request_id": format!("fixture-request-{from_frame_id}"),
        "action_id": action_id,
        "cause": cause,
        "complete": true,
        "products": products,
        "unsupported_reason": null,
    });
}

fn fixture_card_uuid(snapshot: &ParitySnapshotV1, binding: &str) -> String {
    if let Some(card) = observed(snapshot, ParityDomainV1::MasterDeck)
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["binding"].as_str() == Some(binding))
    {
        return format!("fixture-card-{}", card["ordinal"].as_u64().unwrap());
    }
    let source = &observed(snapshot, ParityDomainV1::CardBindings)
        .as_array()
        .unwrap()
        .iter()
        .find(|relation| relation["binding"].as_str() == Some(binding))
        .unwrap()["source"];
    fixture_dynamic_card_uuid(source)
}

fn fixture_dynamic_card_uuid(source: &Value) -> String {
    assert_eq!(source["kind"], "combat_birth");
    format!(
        "fixture-combat-{}-{}-{}",
        required_text(source, "room_id"),
        source["action_ordinal"].as_u64().unwrap(),
        source["product_slot"].as_u64().unwrap(),
    )
}

fn fixture_combat_card<'a>(
    snapshot: &'a ParitySnapshotV1,
    binding: &str,
) -> (&'a Value, &'static str) {
    let combat = observed(snapshot, ParityDomainV1::Combat);
    let zones = &combat["state"]["zones"];
    for (zone, first_zone) in [
        ("draw_next_first", "draw"),
        ("hand", "hand"),
        ("discard_bottom_first", "discard"),
        ("exhaust_by_binding", "exhaust"),
        ("limbo", "limbo"),
    ] {
        if let Some(card) = zones[zone]
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["binding"].as_str() == Some(binding))
        {
            return (card, first_zone);
        }
    }
    panic!("combat birth binding is absent from the fixture successor")
}

struct JavaCombatFixture {
    public_combat: Value,
    internal_combat: Value,
    action_space: Value,
    actions: Vec<JavaRuntimeActionV1>,
}

struct JavaRewardFixture {
    internal_rewards: Value,
    actions: Vec<JavaRuntimeActionV1>,
}

struct JavaCardRewardFixture {
    decision: Value,
    internal_rewards: Value,
    actions: Vec<JavaRuntimeActionV1>,
}

fn java_card_reward_fixture(snapshot: &ParitySnapshotV1, frame_id: u64) -> JavaCardRewardFixture {
    let discovery = snapshot.boundary_kind == "natural_combat_discovery";
    let codex = snapshot.boundary_kind == "natural_combat_codex";
    let rewards: NaturalRunRewardsV2 =
        serde_json::from_value(observed(snapshot, ParityDomainV1::Rewards).clone()).unwrap();
    let items = match rewards {
        NaturalRunRewardsV2::Inactive if discovery || codex => Vec::new(),
        NaturalRunRewardsV2::Combat {
            items,
            skippable: false,
        } => items,
        _ => panic!("card reward fixture requires its supported reward context"),
    };
    let cards = if discovery || codex {
        serde_json::from_value::<Vec<sts_parity_lockstep::NaturalRewardCardV1>>(
            observed(snapshot, ParityDomainV1::Room)["selection"]["cards"].clone(),
        )
        .unwrap()
    } else {
        items
            .iter()
            .find_map(|item| match item {
                NaturalRunRewardItemV1::Card { cards } => Some(cards),
                _ => None,
            })
            .expect("ordinary card reward fixture requires one card item")
            .clone()
    };
    let mut occurrences = BTreeMap::<(String, u8), u32>::new();
    let choices = cards
        .iter()
        .map(|card| {
            let key = (card.card_id.clone(), card.upgrades);
            let occurrence = *occurrences.entry(key.clone()).or_default();
            *occurrences.get_mut(&key).unwrap() += 1;
            if discovery {
                sts_parity_lockstep::natural_discovery_choice_v1(card, occurrence)
            } else if codex {
                sts_parity_lockstep::natural_codex_choice_v1(card, occurrence)
            } else {
                natural_card_reward_choice_v1(card, occurrence)
            }
        })
        .collect::<Vec<_>>();
    let raw_cards = cards
        .iter()
        .enumerate()
        .map(|(index, card)| {
            json!({
                "reward_index": index,
                "card_uuid": format!("fixture-reward-card-{index}"),
                "card_id": card.card_id,
                "name": card.card_id,
                "upgrades": card.upgrades,
            })
        })
        .collect::<Vec<_>>();
    let actions = action_space(snapshot)
        .into_iter()
        .enumerate()
        .map(|(action_index, semantic)| match semantic {
            SemanticActionV1::Select {
                surface:
                    DecisionSurfaceV1::CardReward
                    | DecisionSurfaceV1::CombatDiscovery
                    | DecisionSurfaceV1::CombatCodex,
                choice,
            } if choice.kind == "card_reward_option" && choice.id == "singing_bowl" => {
                serde_json::from_value(json!({
                    "runtime_handle": format!("fixture-f{frame_id}-a{action_index}"),
                    "action_id": "take_singing_bowl", "kind": "take_singing_bowl",
                    "reward_type": "CARD", "parameters_required": false,
                }))
                .unwrap()
            }
            SemanticActionV1::Select {
                surface:
                    DecisionSurfaceV1::CardReward
                    | DecisionSurfaceV1::CombatDiscovery
                    | DecisionSurfaceV1::CombatCodex,
                choice,
            } => {
                let index = choices
                    .iter()
                    .position(|candidate| *candidate == choice)
                    .expect("card reward action should name one offered card");
                let card = &cards[index];
                JavaRuntimeActionV1 {
                    runtime_handle: format!("fixture-f{frame_id}-a{action_index}"),
                    action_id: format!(
                        "card_reward:{index}:fixture-reward-card-{index}:{}:{}",
                        card.card_id, card.upgrades
                    ),
                    kind: "choose_card_reward".to_owned(),
                    event_id: None,
                    option_index: None,
                    neow_reward_type: None,
                    neow_drawback: None,
                    map_x: None,
                    map_y: None,
                    map_room_type: None,
                    reward_type: Some("CARD".to_owned()),
                    reward_index: Some(index),
                    reward_id: None,
                    reward_gold: None,
                    potion_slot: None,
                    potion_id: None,
                    parameters_required: false,
                }
            }
            SemanticActionV1::Cancel {
                surface:
                    DecisionSurfaceV1::CardReward
                    | DecisionSurfaceV1::CombatDiscovery
                    | DecisionSurfaceV1::CombatCodex,
            } => JavaRuntimeActionV1 {
                runtime_handle: format!("fixture-f{frame_id}-a{action_index}"),
                action_id: "skip_card_reward".to_owned(),
                kind: "skip_card_reward".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: Some("CARD".to_owned()),
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: None,
                potion_id: None,
                parameters_required: false,
            },
            SemanticActionV1::DiscardPotion { slot, potion_id } => JavaRuntimeActionV1 {
                runtime_handle: format!("fixture-f{frame_id}-a{action_index}"),
                action_id: format!("discard_potion:{slot}:{potion_id}"),
                kind: "discard_potion".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: None,
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: Some(usize::from(slot)),
                potion_id: Some(potion_id),
                parameters_required: false,
            },
            other => panic!("unsupported fixture card reward action {other:?}"),
        })
        .collect::<Vec<_>>();
    let skip_available = actions
        .iter()
        .any(|action| action.kind == "skip_card_reward");
    let bowl_available = actions
        .iter()
        .any(|action| action.kind == "take_singing_bowl");
    JavaCardRewardFixture {
        decision: json!({
            "kind": "card_reward",
            "card_reward": {
                "source_kind": if discovery { "combat_discovery" } else if codex { "codex" } else { "ordinary_reward" },
                "discovery_context": if discovery { observed(snapshot, ParityDomainV1::Room)["selection"].clone() } else { Value::Null },
                "cards": raw_cards,
                "skip_available": skip_available,
                "singing_bowl_available": bowl_available,
                "singing_bowl_control_supported": true,
            },
        }),
        internal_rewards: Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| java_reward_internal(index, item))
                .collect(),
        ),
        actions,
    }
}

fn java_reward_fixture(snapshot: &ParitySnapshotV1, frame_id: u64) -> JavaRewardFixture {
    let rewards: NaturalRunRewardsV2 =
        serde_json::from_value(observed(snapshot, ParityDomainV1::Rewards).clone()).unwrap();
    let (items, skippable, surface) = match rewards {
        NaturalRunRewardsV2::Combat { items, skippable } => {
            (items, skippable, DecisionSurfaceV1::CombatReward)
        }
        NaturalRunRewardsV2::Treasure { items, skippable } => {
            (items, skippable, DecisionSurfaceV1::TreasureReward)
        }
        NaturalRunRewardsV2::Inactive => panic!("reward fixture requires an active reward"),
    };
    let internal_rewards = items
        .iter()
        .enumerate()
        .map(|(index, item)| java_reward_internal(index, item))
        .collect::<Vec<_>>();
    let mut occurrences = BTreeMap::<String, u32>::new();
    let choices = items
        .iter()
        .map(|item| {
            let key = serde_json::to_string(item).unwrap();
            let occurrence = *occurrences.entry(key.clone()).or_default();
            *occurrences.get_mut(&key).unwrap() += 1;
            natural_reward_choice_v1(item, occurrence)
        })
        .collect::<Vec<_>>();
    let actions = action_space(snapshot)
        .into_iter()
        .enumerate()
        .map(|(action_index, semantic)| match semantic {
            SemanticActionV1::Select {
                surface: action_surface,
                choice,
            } if action_surface == surface => {
                let reward_index = choices
                    .iter()
                    .position(|candidate| *candidate == choice)
                    .unwrap();
                let (reward_type, reward_id, reward_gold) =
                    java_reward_action_identity_for_action(&items, reward_index);
                JavaRuntimeActionV1 {
                    runtime_handle: format!("fixture-f{frame_id}-a{action_index}"),
                    action_id: format!("combat_reward:{reward_index}:{reward_type}:{reward_id}"),
                    kind: "claim_combat_reward".to_owned(),
                    event_id: None,
                    option_index: None,
                    neow_reward_type: None,
                    neow_drawback: None,
                    map_x: None,
                    map_y: None,
                    map_room_type: None,
                    reward_type: Some(reward_type.to_owned()),
                    reward_index: Some(reward_index),
                    reward_id: Some(reward_id),
                    reward_gold,
                    potion_slot: None,
                    potion_id: None,
                    parameters_required: false,
                }
            }
            SemanticActionV1::Proceed {
                surface: action_surface,
            } if action_surface == surface => JavaRuntimeActionV1 {
                runtime_handle: format!("fixture-f{frame_id}-a{action_index}"),
                action_id: "close_combat_reward".to_owned(),
                kind: "close_combat_reward".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: None,
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: None,
                potion_id: None,
                parameters_required: false,
            },
            SemanticActionV1::DiscardPotion { slot, potion_id } => JavaRuntimeActionV1 {
                runtime_handle: format!("fixture-f{frame_id}-a{action_index}"),
                action_id: format!("discard_potion:{slot}:{potion_id}"),
                kind: "discard_potion".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: None,
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: Some(usize::from(slot)),
                potion_id: Some(potion_id),
                parameters_required: false,
            },
            other => panic!("unsupported fixture reward action {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .any(|action| action.kind == "close_combat_reward"),
        skippable
    );
    JavaRewardFixture {
        internal_rewards: Value::Array(internal_rewards),
        actions,
    }
}

fn java_reward_internal(index: usize, item: &NaturalRunRewardItemV1) -> Value {
    let (reward_type, reward_id, reward_gold) = java_reward_action_identity(item);
    let (relic_id, potion_id, cards) = match item {
        NaturalRunRewardItemV1::Relic { relic_id } => (json!(relic_id), Value::Null, Value::Null),
        NaturalRunRewardItemV1::Potion { potion_id } => {
            (Value::Null, json!(potion_id), Value::Null)
        }
        NaturalRunRewardItemV1::Card { cards } => (
            Value::Null,
            Value::Null,
            json!(cards
                .iter()
                .map(|card| json!({
                    "card_id": card.card_id,
                    "upgrades": card.upgrades,
                }))
                .collect::<Vec<_>>()),
        ),
        _ => (Value::Null, Value::Null, Value::Null),
    };
    let _ = reward_id;
    json!({
        "index": index,
        "type": reward_type,
        "gold": reward_gold,
        "relic_id": relic_id,
        "potion_id": potion_id,
        "cards": cards,
    })
}

fn java_reward_action_identity(
    item: &NaturalRunRewardItemV1,
) -> (&'static str, String, Option<i64>) {
    match item {
        NaturalRunRewardItemV1::Gold { amount } => {
            ("GOLD", amount.to_string(), Some(i64::from(*amount)))
        }
        NaturalRunRewardItemV1::StolenGold { amount } => {
            ("STOLEN_GOLD", amount.to_string(), Some(i64::from(*amount)))
        }
        NaturalRunRewardItemV1::Card { cards } => (
            "CARD",
            cards
                .iter()
                .enumerate()
                .map(|(index, _)| format!("fixture-reward-card-{index}"))
                .collect::<Vec<_>>()
                .join(","),
            None,
        ),
        NaturalRunRewardItemV1::Relic { relic_id } => ("RELIC", relic_id.clone(), None),
        NaturalRunRewardItemV1::Potion { potion_id } => ("POTION", potion_id.clone(), None),
        NaturalRunRewardItemV1::EmeraldKey => ("EMERALD_KEY", "EMERALD_KEY".to_owned(), None),
        NaturalRunRewardItemV1::SapphireKey => ("SAPPHIRE_KEY", "SAPPHIRE_KEY".to_owned(), None),
    }
}

fn java_reward_action_identity_for_action(
    items: &[NaturalRunRewardItemV1],
    index: usize,
) -> (&'static str, String, Option<i64>) {
    let item = &items[index];
    if matches!(item, NaturalRunRewardItemV1::SapphireKey) {
        let NaturalRunRewardItemV1::Relic { relic_id } = &items[index - 1] else {
            panic!("fixture Sapphire Key must immediately follow its linked relic")
        };
        return ("SAPPHIRE_KEY", relic_id.clone(), None);
    }
    java_reward_action_identity(item)
}

fn java_powers(powers: &Value) -> Vec<Value> {
    powers
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, power)| {
            let amount = power["amount"].as_i64().unwrap();
            let (id, class_name, power_type) = match required_text(power, "power_id") {
                "hex" => ("Hex", "com.megacrit.cardcrawl.powers.HexPower", "DEBUFF"),
                "strength" => (
                    "Strength",
                    "com.megacrit.cardcrawl.powers.StrengthPower",
                    if amount > 0 { "BUFF" } else { "DEBUFF" },
                ),
                "vulnerable" => (
                    "Vulnerable",
                    "com.megacrit.cardcrawl.powers.VulnerablePower",
                    "DEBUFF",
                ),
                "weak" => (
                    "Weakened",
                    "com.megacrit.cardcrawl.powers.WeakPower",
                    "DEBUFF",
                ),
                "frail" => (
                    "Frail",
                    "com.megacrit.cardcrawl.powers.FrailPower",
                    "DEBUFF",
                ),
                "dexterity" => (
                    "Dexterity",
                    "com.megacrit.cardcrawl.powers.DexterityPower",
                    if amount > 0 { "BUFF" } else { "DEBUFF" },
                ),
                "curlup" => (
                    "Curl Up",
                    "com.megacrit.cardcrawl.powers.CurlUpPower",
                    "BUFF",
                ),
                "anger" => ("Anger", "com.megacrit.cardcrawl.powers.AngerPower", "BUFF"),
                "angry" => ("Angry", "com.megacrit.cardcrawl.powers.AngryPower", "BUFF"),
                "metallicize" => (
                    "Metallicize",
                    "com.megacrit.cardcrawl.powers.MetallicizePower",
                    "BUFF",
                ),
                "sporecloud" => (
                    "Spore Cloud",
                    "com.megacrit.cardcrawl.powers.SporeCloudPower",
                    "BUFF",
                ),
                "split" => ("Split", "com.megacrit.cardcrawl.powers.SplitPower", "BUFF"),
                "artifact" => (
                    "Artifact",
                    "com.megacrit.cardcrawl.powers.ArtifactPower",
                    "BUFF",
                ),
                "modeshift" => (
                    "Mode Shift",
                    "com.megacrit.cardcrawl.powers.ModeShiftPower",
                    "BUFF",
                ),
                "sharphide" => (
                    "Sharp Hide",
                    "com.megacrit.cardcrawl.powers.SharpHidePower",
                    "BUFF",
                ),
                "thievery" => (
                    "Thievery",
                    "com.megacrit.cardcrawl.powers.ThieveryPower",
                    "BUFF",
                ),
                "entangle" => (
                    "Entangled",
                    "com.megacrit.cardcrawl.powers.EntanglePower",
                    "DEBUFF",
                ),
                "ritual" => (
                    "Ritual",
                    "com.megacrit.cardcrawl.powers.RitualPower",
                    "BUFF",
                ),
                "corruption" => (
                    "Corruption",
                    "com.megacrit.cardcrawl.powers.CorruptionPower",
                    "BUFF",
                ),
                "toolsofthetrade" => (
                    "Tools Of The Trade",
                    "com.megacrit.cardcrawl.powers.ToolsOfTheTradePower",
                    "BUFF",
                ),
                "retaincards" => (
                    "Retain Cards",
                    "com.megacrit.cardcrawl.powers.RetainCardPower",
                    "BUFF",
                ),
                "timewarp" => (
                    "Time Warp",
                    "com.megacrit.cardcrawl.powers.TimeWarpPower",
                    "BUFF",
                ),
                other => panic!("unsupported fixture power {other:?}"),
            };
            let (round_duration_just_applied, ritual_skip_first, ritual_player_controlled) =
                match required_text(&power["state"], "kind") {
                    "stateless" | "curl_up_triggered" => (Value::Null, Value::Null, Value::Null),
                    "round_duration" => (
                        power["state"]["just_applied"].clone(),
                        Value::Null,
                        Value::Null,
                    ),
                    "ritual" => (
                        Value::Null,
                        power["state"]["skip_first"].clone(),
                        power["state"]["player_controlled"].clone(),
                    ),
                    other => panic!("unsupported fixture power state {other:?}"),
                };
            json!({
                "index": index,
                "id": id,
                "class_name": class_name,
                "amount": amount,
                "type": power_type,
                "round_duration_just_applied": round_duration_just_applied,
                "ritual_skip_first": ritual_skip_first,
                "ritual_player_controlled": ritual_player_controlled,
                "curl_up_triggered": if id == "Curl Up" { json!(power["state"]["kind"] == "curl_up_triggered") } else { Value::Null },
            })
        })
        .collect()
}

fn java_combat_fixture(
    snapshot: &ParitySnapshotV1,
    combat: &Value,
    raw_potions: &[Value],
    binding_uuids: &BTreeMap<String, String>,
    frame_id: u64,
) -> JavaCombatFixture {
    assert_eq!(required_text(combat, "status"), "player_decision");
    let state = &combat["state"];
    let zones = &state["zones"];
    let raw_hand = java_combat_cards(&zones["hand"], binding_uuids);
    let mut raw_draw = java_combat_cards(&zones["draw_next_first"], binding_uuids);
    raw_draw.reverse();
    let raw_discard = java_combat_cards(&zones["discard_bottom_first"], binding_uuids);
    let raw_exhaust = java_combat_cards(&zones["exhaust_by_binding"], binding_uuids);
    let raw_limbo = java_combat_cards(&zones["limbo"], binding_uuids);

    let shared_monsters = state["monsters"].as_array().unwrap();
    let public_monsters = shared_monsters
        .iter()
        .enumerate()
        .map(|(index, monster)| {
            json!({
                "index": index,
                "instance_id": fixture_monster_instance(index),
                "id": monster["enemy_id"].clone(),
                "current_hp": monster["current_hp"].clone(),
                "max_hp": monster["max_hp"].clone(),
                "block": monster["block"].clone(),
                "dead_or_escaped": monster["dead_or_escaped"].clone(),
                "public_intent": java_public_intent(&monster["public_intent"]),
            })
        })
        .collect::<Vec<_>>();
    let internal_monsters = shared_monsters
        .iter()
        .enumerate()
        .map(|(index, monster)| {
            json!({
                "instance_id": fixture_monster_instance(index),
                "next_move_id": java_signed_byte(monster["next_move_id"].as_u64().unwrap()),
                "intent": monster["public_intent"]["kind"]
                    .as_str()
                    .map(java_intent_kind),
                "move_history": monster["move_history"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| java_signed_byte(value.as_u64().unwrap()))
                    .collect::<Vec<_>>(),
                "powers": java_powers(&monster["powers"]),
            })
        })
        .collect::<Vec<_>>();
    let raw_actions = if matches!(
        snapshot.boundary_kind.as_str(),
        "natural_combat_discovery" | "natural_combat_codex" | "natural_hand_select"
    ) {
        Vec::new()
    } else {
        raw_combat_actions(snapshot, state, binding_uuids, frame_id)
    };
    let live_actions = raw_actions
        .iter()
        .enumerate()
        .map(|(index, raw)| JavaRuntimeActionV1 {
            runtime_handle: format!("fixture-f{frame_id}-a{index}"),
            action_id: required_text(raw, "action_id").to_owned(),
            kind: required_text(raw, "kind").to_owned(),
            event_id: None,
            option_index: None,
            neow_reward_type: None,
            neow_drawback: None,
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: None,
            reward_index: None,
            reward_id: None,
            reward_gold: None,
            potion_slot: raw["potion_slot"]
                .as_u64()
                .map(|slot| usize::try_from(slot).unwrap()),
            potion_id: raw["potion_id"].as_str().map(str::to_owned),
            parameters_required: false,
        })
        .collect();

    JavaCombatFixture {
        public_combat: json!({
            "turn": state["turn"].clone(),
            "player": state["player"].clone(),
            "hand": raw_hand,
            "draw_count": raw_draw.len(),
            "discard": {"ordering": "unspecified", "cards": raw_discard.clone()},
            "exhaust": {"ordering": "unspecified", "cards": raw_exhaust.clone()},
            "monsters": public_monsters,
            "potions": raw_potions,
        }),
        internal_combat: json!({
            "zones": {
                "draw": java_ordered_zone(raw_draw),
                "discard": java_ordered_zone(raw_discard),
                "exhaust": java_ordered_zone(raw_exhaust),
                "hand": java_ordered_zone(java_combat_cards(&zones["hand"], binding_uuids)),
                "limbo": java_ordered_zone(raw_limbo),
            },
            "monsters": internal_monsters,
            "player_powers": java_powers(&state["player_powers"]),
        }),
        action_space: json!({"kind": "combat_action_space", "actions": raw_actions}),
        actions: live_actions,
    }
}

fn java_combat_cards(cards: &Value, binding_uuids: &BTreeMap<String, String>) -> Vec<Value> {
    cards
        .as_array()
        .unwrap()
        .iter()
        .map(|card| {
            json!({
                "instance_uuid": binding_uuids[required_text(card, "binding")],
                "card_id": card["card_id"].clone(),
                "upgrades": card["upgrades"].clone(),
                "combat_cost": card["combat_cost"].clone(),
                "cost_for_turn": card["cost_for_turn"].clone(),
                "free_to_play_once": card["free_to_play_once"].as_bool().unwrap_or(false),
                "base_damage": card["base_damage"].clone(),
            })
        })
        .collect()
}

fn java_ordered_zone(cards: Vec<Value>) -> Value {
    json!({
        "ordering": "java_group_index_ascending",
        "top": "last",
        "cards": cards,
    })
}

fn java_public_intent(intent: &Value) -> Value {
    match required_text(intent, "evidence") {
        "visible_exact" => json!({
            "evidence": "visible_exact",
            "visible_intent": java_intent_kind(required_text(intent, "kind")),
            "damage_per_hit": intent["damage_per_hit"].clone(),
            "hits": intent["hits"].clone(),
            "hidden_reason": null,
        }),
        "hidden" => json!({
            "evidence": "hidden",
            "visible_intent": null,
            "damage_per_hit": null,
            "hits": null,
            "hidden_reason": intent["hidden_reason"].clone(),
        }),
        evidence => panic!("unsupported fixture intent evidence {evidence:?}"),
    }
}

fn java_intent_kind(kind: &str) -> &'static str {
    match kind {
        "attack" => "ATTACK",
        "attack_buff" => "ATTACK_BUFF",
        "attack_debuff" => "ATTACK_DEBUFF",
        "attack_defend" => "ATTACK_DEFEND",
        "buff" => "BUFF",
        "debuff" => "DEBUFF",
        "strong_debuff" => "STRONG_DEBUFF",
        "debug" => "DEBUG",
        "defend" => "DEFEND",
        "defend_debuff" => "DEFEND_DEBUFF",
        "defend_buff" => "DEFEND_BUFF",
        "escape" => "ESCAPE",
        "magic" => "MAGIC",
        "none" => "NONE",
        "sleep" => "SLEEP",
        "stun" => "STUN",
        "unknown" => "UNKNOWN",
        other => panic!("unsupported fixture intent {other:?}"),
    }
}

fn raw_combat_actions(
    snapshot: &ParitySnapshotV1,
    combat_state: &Value,
    binding_uuids: &BTreeMap<String, String>,
    frame_id: u64,
) -> Vec<Value> {
    action_space(snapshot)
        .into_iter()
        .enumerate()
        .map(|(index, action)| {
            let action_id = format!("fixture-combat-f{frame_id}-a{index}");
            match action {
                SemanticActionV1::PlayCard {
                    card_binding,
                    card_id,
                    target,
                } => {
                    let hand = combat_state["zones"]["hand"].as_array().unwrap();
                    let hand_index = hand
                        .iter()
                        .position(|card| card["binding"].as_str() == Some(&card_binding))
                        .unwrap();
                    let (target_index, target_instance, target_id) =
                        java_combat_target(combat_state, target.as_ref());
                    json!({
                        "action_id": action_id,
                        "kind": "play_card",
                        "card_uuid": binding_uuids[&card_binding],
                        "card_id": card_id,
                        "hand_index": hand_index,
                        "target_monster_instance_id": target_instance,
                        "target_monster_index": target_index,
                        "target_monster_id": target_id,
                        "potion_slot": null,
                        "potion_id": null,
                        "potion_potency": null,
                    })
                }
                SemanticActionV1::UsePotion {
                    slot,
                    potion_id,
                    target,
                } => {
                    let (target_index, target_instance, target_id) =
                        java_combat_target(combat_state, target.as_ref());
                    json!({
                        "action_id": action_id,
                        "kind": "use_potion",
                        "card_uuid": null,
                        "card_id": null,
                        "hand_index": null,
                        "target_monster_instance_id": target_instance,
                        "target_monster_index": target_index,
                        "target_monster_id": target_id,
                        "potion_slot": slot,
                        "potion_id": potion_id,
                        "potion_potency": 0,
                    })
                }
                SemanticActionV1::DiscardPotion { slot, potion_id } => json!({
                    "action_id": action_id,
                    "kind": "discard_potion",
                    "card_uuid": null,
                    "card_id": null,
                    "hand_index": null,
                    "target_monster_instance_id": null,
                    "target_monster_index": null,
                    "target_monster_id": null,
                    "potion_slot": slot,
                    "potion_id": potion_id,
                    "potion_potency": 0,
                }),
                SemanticActionV1::EndTurn => json!({
                    "action_id": action_id,
                    "kind": "end_turn",
                    "card_uuid": null,
                    "card_id": null,
                    "hand_index": null,
                    "target_monster_instance_id": null,
                    "target_monster_index": null,
                    "target_monster_id": null,
                    "potion_slot": null,
                    "potion_id": null,
                    "potion_potency": null,
                }),
                other => panic!("unsupported fixture combat action {other:?}"),
            }
        })
        .collect()
}

fn java_combat_target(
    combat_state: &Value,
    target: Option<&sts_parity_lockstep::SemanticChoiceV1>,
) -> (Value, Value, Value) {
    let Some(target) = target else {
        return (Value::Null, Value::Null, Value::Null);
    };
    assert_eq!(target.kind, "monster");
    let index = combat_state["monsters"]
        .as_array()
        .unwrap()
        .iter()
        .position(|monster| {
            monster["enemy_id"].as_str() == Some(&target.id)
                && monster["occurrence"].as_u64() == Some(u64::from(target.occurrence))
        })
        .unwrap();
    (
        json!(index),
        json!(fixture_monster_instance(index)),
        json!(target.id),
    )
}

fn fixture_monster_instance(index: usize) -> u64 {
    10_000 + index as u64
}

fn java_signed_byte(value: u64) -> i64 {
    i64::from(value as u8 as i8)
}

fn java_actions(
    snapshot: &ParitySnapshotV1,
    frame_id: u64,
    neow_screen: Option<i64>,
) -> Vec<JavaRuntimeActionV1> {
    action_space(snapshot)
        .into_iter()
        .enumerate()
        .map(|(index, semantic)| match semantic {
            SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::DoubleBoss,
            } => parameter_free_java_action(
                frame_id,
                index,
                "proceed_double_boss".to_owned(),
                "proceed_double_boss",
            ),
            SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::Treasure,
            } => parameter_free_java_action(
                frame_id,
                index,
                "proceed_treasure".to_owned(),
                "proceed_treasure",
            ),
            SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::Neow,
            } => {
                let screen = neow_screen.expect("fixture Proceed must belong to Neow");
                JavaRuntimeActionV1 {
                    runtime_handle: format!("fixture-f{frame_id}-a{index}"),
                    action_id: format!("event_option:Neow:index:{screen}:0"),
                    kind: "choose_event_option".to_owned(),
                    event_id: Some("Neow".to_owned()),
                    option_index: Some(0),
                    neow_reward_type: None,
                    neow_drawback: None,
                    map_x: None,
                    map_y: None,
                    map_room_type: None,
                    reward_type: None,
                    reward_index: None,
                    reward_id: None,
                    reward_gold: None,
                    potion_slot: None,
                    potion_id: None,
                    parameters_required: false,
                }
            }
            SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Neow,
                choice,
            } => {
                let Some(SemanticScalarV1::Text(drawback)) = choice.attributes.get("drawback")
                else {
                    panic!("fixture Neow choice omitted its drawback")
                };
                JavaRuntimeActionV1 {
                    runtime_handle: format!("fixture-f{frame_id}-a{index}"),
                    action_id: format!(
                        "event_option:Neow:index:{}:{index}",
                        neow_screen.expect("fixture Neow choice omitted its screen")
                    ),
                    kind: "choose_event_option".to_owned(),
                    event_id: Some("Neow".to_owned()),
                    option_index: Some(index),
                    neow_reward_type: Some(choice.id.to_ascii_uppercase()),
                    neow_drawback: Some(drawback.to_ascii_uppercase()),
                    map_x: None,
                    map_y: None,
                    map_room_type: None,
                    reward_type: None,
                    reward_index: None,
                    reward_id: None,
                    reward_gold: None,
                    potion_slot: None,
                    potion_id: None,
                    parameters_required: false,
                }
            }
            SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Event,
                choice,
            } => {
                let Some(SemanticScalarV1::Integer(screen)) = choice.attributes.get("screen")
                else {
                    panic!("fixture event choice omitted its screen")
                };
                let option_index = usize::try_from(choice.occurrence).unwrap();
                JavaRuntimeActionV1 {
                    runtime_handle: format!("fixture-f{frame_id}-a{index}"),
                    action_id: format!("event_option:{}:index:{screen}:{option_index}", choice.id),
                    kind: "choose_event_option".to_owned(),
                    event_id: Some(choice.id),
                    option_index: Some(option_index),
                    neow_reward_type: None,
                    neow_drawback: None,
                    map_x: None,
                    map_y: None,
                    map_room_type: None,
                    reward_type: None,
                    reward_index: None,
                    reward_id: None,
                    reward_gold: None,
                    potion_slot: None,
                    potion_id: None,
                    parameters_required: false,
                }
            }
            SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Map,
                choice,
            } => {
                let Some(SemanticScalarV1::Integer(x)) = choice.attributes.get("x") else {
                    panic!("fixture map choice omitted x")
                };
                let Some(SemanticScalarV1::Integer(y)) = choice.attributes.get("y") else {
                    panic!("fixture map choice omitted y")
                };
                let Some(SemanticScalarV1::Text(room)) = choice.attributes.get("room") else {
                    panic!("fixture map choice omitted room")
                };
                JavaRuntimeActionV1 {
                    runtime_handle: format!("fixture-f{frame_id}-a{index}"),
                    action_id: format!("map_node:{x}:{y}"),
                    kind: "choose_map_node".to_owned(),
                    event_id: None,
                    option_index: None,
                    neow_reward_type: None,
                    neow_drawback: None,
                    map_x: Some(*x),
                    map_y: Some(*y),
                    map_room_type: Some(java_room_type(room).to_owned()),
                    reward_type: None,
                    reward_index: None,
                    reward_id: None,
                    reward_gold: None,
                    potion_slot: None,
                    potion_id: None,
                    parameters_required: false,
                }
            }
            SemanticActionV1::Select {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Treasure,
                choice,
            } => {
                assert_eq!(choice.kind, "treasure_chest");
                assert_eq!(choice.id, "open");
                assert_eq!(choice.occurrence, 0);
                parameter_free_java_action(frame_id, index, "open_chest".to_owned(), "open_chest")
            }
            SemanticActionV1::Cancel {
                surface: sts_parity_lockstep::DecisionSurfaceV1::Map,
            } => JavaRuntimeActionV1 {
                runtime_handle: format!("fixture-f{frame_id}-a{index}"),
                action_id: "close_map".to_owned(),
                kind: "close_map".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: None,
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: None,
                potion_id: None,
                parameters_required: false,
            },
            SemanticActionV1::DiscardPotion { slot, potion_id } => JavaRuntimeActionV1 {
                runtime_handle: format!("fixture-f{frame_id}-a{index}"),
                action_id: format!("discard_potion:{slot}:{potion_id}"),
                kind: "discard_potion".to_owned(),
                event_id: None,
                option_index: None,
                neow_reward_type: None,
                neow_drawback: None,
                map_x: None,
                map_y: None,
                map_room_type: None,
                reward_type: None,
                reward_index: None,
                reward_id: None,
                reward_gold: None,
                potion_slot: Some(usize::from(slot)),
                potion_id: Some(potion_id),
                parameters_required: false,
            },
            other => panic!("unsupported fixture action {other:?}"),
        })
        .collect()
}

fn execute(
    adapter: &mut RustLockstepAdapterV1,
    before: &ParitySnapshotV1,
    action: &SemanticActionV1,
) -> ParitySnapshotV1 {
    let prepared = adapter.prepare(action, before).unwrap();
    adapter.execute(prepared).unwrap()
}

fn assert_exact(java: &ParitySnapshotV1, rust: &ParitySnapshotV1) {
    let comparison = compare_snapshots_v1(java, rust).unwrap();
    let stops = comparison
        .domains
        .iter()
        .filter(|domain| domain.status != ComparisonStatusV1::Exact)
        .map(|domain| {
            format!(
                "{:?}: {}",
                domain.domain,
                domain.reason.as_deref().unwrap_or("values differ")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        comparison.status,
        ComparisonStatusV1::Exact,
        "fixture projection stopped: {}",
        stops.join(", ")
    );
}

fn advance_to_map(adapter: &mut RustLockstepAdapterV1) -> ParitySnapshotV1 {
    let introduction = adapter.observe().unwrap();
    let proceed = action_space(&introduction)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::Proceed { .. }))
        .unwrap();
    let reward = execute(adapter, &introduction, &proceed);
    let choose = action_space(&reward)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::Select { .. }))
        .unwrap();
    let leave = execute(adapter, &reward, &choose);
    let leave_action = action_space(&leave)
        .into_iter()
        .find(|action| matches!(action, SemanticActionV1::Proceed { .. }))
        .unwrap();
    execute(adapter, &leave, &leave_action)
}

fn only_action(snapshot: &ParitySnapshotV1) -> SemanticActionV1 {
    let actions = action_space(snapshot);
    assert_eq!(actions.len(), 1);
    actions.into_iter().next().unwrap()
}

fn action_space(snapshot: &ParitySnapshotV1) -> Vec<SemanticActionV1> {
    serde_json::from_value(observed(snapshot, ParityDomainV1::ActionSpace).clone()).unwrap()
}

fn raw_rng_capture(shared: &Value) -> Value {
    let mut streams = Vec::new();
    let mut run_streams = Vec::new();
    for stream in shared.as_array().unwrap() {
        let owner = required_text(stream, "owner");
        let raw = json!({
            "owner": java_rng_owner(owner),
            "algorithm": "com.badlogic.gdx.math.RandomXS128",
            "scope": stream["scope"].clone(),
            "state_hex": stream["state_hex"].clone(),
            "counter": stream["counter"].as_u64().unwrap_or(0),
        });
        if owner.starts_with("floor.") || owner == "run.potion" {
            streams.push(raw);
        } else {
            run_streams.push(raw);
        }
    }
    if !shared
        .as_array()
        .unwrap()
        .iter()
        .any(|stream| stream["owner"].as_str() == Some("floor.misc"))
    {
        streams.push(json!({
            "owner": java_rng_owner("floor.misc"),
            "algorithm": "com.badlogic.gdx.math.RandomXS128",
            "scope": "floor",
            "state_hex": ["0000000000000000", "0000000000000000"],
            "counter": 0,
        }));
    }
    if !shared
        .as_array()
        .unwrap()
        .iter()
        .any(|stream| stream["owner"].as_str() == Some("run.neow"))
    {
        run_streams.push(json!({
            "owner": java_rng_owner("run.neow"),
            "algorithm": "com.badlogic.gdx.math.RandomXS128",
            "scope": "run",
            "state_hex": ["0000000000000000", "0000000000000000"],
            "counter": 0,
        }));
    }
    json!({"streams": streams, "run_streams": run_streams})
}

fn java_rng_owner(owner: &str) -> &'static str {
    match owner {
        "run.monster" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterRng",
        "run.event" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.eventRng",
        "run.merchant" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.merchantRng",
        "run.card" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRng",
        "run.treasure" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.treasureRng",
        "run.relic" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.relicRng",
        "run.potion" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.potionRng",
        "run.map" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng",
        "run.neow" => "com.megacrit.cardcrawl.neow.NeowEvent.rng",
        "floor.monster_hp" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterHpRng",
        "floor.ai" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.aiRng",
        "floor.shuffle" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.shuffleRng",
        "floor.card_random" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRandomRng",
        "floor.misc" => "com.megacrit.cardcrawl.dungeons.AbstractDungeon.miscRng",
        other => panic!("unsupported shared RNG owner {other:?}"),
    }
}

fn java_room_type(room: &str) -> &'static str {
    match room {
        "question_mark" => "com.megacrit.cardcrawl.rooms.EventRoom",
        "monster" => "com.megacrit.cardcrawl.rooms.MonsterRoom",
        "elite" => "com.megacrit.cardcrawl.rooms.MonsterRoomElite",
        "boss" => "com.megacrit.cardcrawl.rooms.MonsterRoomBoss",
        "rest" => "com.megacrit.cardcrawl.rooms.RestRoom",
        "shop" => "com.megacrit.cardcrawl.rooms.ShopRoom",
        "treasure" => "com.megacrit.cardcrawl.rooms.TreasureRoom",
        other => panic!("unsupported shared map room {other:?}"),
    }
}

fn node_coordinate(id: &str) -> (i64, i64) {
    let (floor, x) = id
        .strip_prefix('F')
        .and_then(|value| value.split_once('X'))
        .unwrap_or_else(|| panic!("invalid shared map node {id:?}"));
    (x.parse().unwrap(), floor.parse::<i64>().unwrap() - 1)
}

fn observed(snapshot: &ParitySnapshotV1, domain: ParityDomainV1) -> &Value {
    let DomainEvidenceV1::Observed { value, .. } = &snapshot.domains[&domain].evidence else {
        panic!("Rust fixture did not observe {domain:?}")
    };
    value
}

fn required_text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("fixture value omitted text {key:?}"))
}
