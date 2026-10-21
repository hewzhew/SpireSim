use super::*;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use sts_java_parity_capture::{ArtifactReceipt, CollectorReceipt};

#[test]
fn feed_terminal_settlement_checks_meat_after_feed_growth() {
    let remains_bloodied = expected_feed_terminal_settlement(
        50,
        187,
        99,
        13,
        0,
        1,
        1,
        "unsupported_run_decision".to_owned(),
        "combat_reward".to_owned(),
    )
    .unwrap();
    assert_eq!(remains_bloodied.meat_on_the_bone_heal, 12);
    assert_eq!(
        (
            remains_bloodied.player_hp_after,
            remains_bloodied.player_max_hp_after
        ),
        (72, 191)
    );

    let crosses_threshold = expected_feed_terminal_settlement(
        49,
        100,
        99,
        13,
        0,
        1,
        1,
        "unsupported_run_decision".to_owned(),
        "combat_reward".to_owned(),
    )
    .unwrap();
    assert_eq!(crosses_threshold.meat_on_the_bone_heal, 0);
    assert_eq!(
        (
            crosses_threshold.player_hp_after,
            crosses_threshold.player_max_hp_after
        ),
        (59, 104)
    );
}

fn card(uuid: &str, card_id: &str) -> Value {
    card_with_cost(uuid, card_id, 1)
}

fn card_with_cost(uuid: &str, card_id: &str, cost_for_turn: i32) -> Value {
    json!({
        "instance_uuid": uuid,
        "card_id": card_id,
        "upgrades": 0,
        "cost_for_turn": cost_for_turn
    })
}

fn parity_zone(cards: Vec<CardProjection>) -> OrderedZoneProjection {
    OrderedZoneProjection {
        ordering: "java_group_index_ascending".to_owned(),
        top: "last".to_owned(),
        cards,
    }
}

fn parity_zones_with_hand(cards: Vec<CardProjection>) -> ParityZonesProjection {
    ParityZonesProjection {
        draw: parity_zone(Vec::new()),
        discard: parity_zone(Vec::new()),
        exhaust: parity_zone(Vec::new()),
        hand: parity_zone(cards),
        limbo: parity_zone(Vec::new()),
    }
}

#[test]
fn captured_same_instance_copies_share_one_cross_runtime_lineage() {
    let repeated = CardProjection {
        instance_uuid: "same-java-uuid".to_owned(),
        card_id: "Strike_R".to_owned(),
        upgrades: 0,
        cost_for_turn: 1,
    };
    let zones = parity_zones_with_hand(vec![repeated.clone(), repeated]);
    let bindings = compile_captured_scenario_bindings(&zones).unwrap();

    assert_eq!(bindings.java_to_rust.len(), 1);
    assert_eq!(bindings.rust_to_java.len(), 1);
    let cards = build_cards(&zones.hand.cards, &bindings).unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].lineage, cards[1].lineage);
    assert_eq!(cards[0].lineage, CardLineageId::constructed_scenario(1));

    let rust_cards = cards.iter().collect::<Vec<_>>();
    bindings
        .verify_successor_zone(
            "hand",
            &rust_cards,
            &zones.hand.cards,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
        )
        .unwrap();
}

#[test]
fn captured_same_uuid_rejects_conflicting_stable_card_semantics() {
    let zones = parity_zones_with_hand(vec![
        CardProjection {
            instance_uuid: "same-java-uuid".to_owned(),
            card_id: "Strike_R".to_owned(),
            upgrades: 0,
            cost_for_turn: 1,
        },
        CardProjection {
            instance_uuid: "same-java-uuid".to_owned(),
            card_id: "Defend_R".to_owned(),
            upgrades: 0,
            cost_for_turn: 1,
        },
    ]);

    let error = match compile_captured_scenario_bindings(&zones) {
        Ok(_) => panic!("conflicting same-UUID card semantics must be rejected"),
        Err(error) => error,
    };
    assert!(
        error.contains("conflicting stable card semantics"),
        "{error}"
    );
}

fn rng_stream(owner: &str, scope: &str, seed0: &str, seed1: &str, counter: i64) -> Value {
    json!({
        "owner": owner,
        "algorithm": "com.badlogic.gdx.math.RandomXS128",
        "scope": scope,
        "state_hex": [seed0, seed1],
        "counter": counter
    })
}

fn frame(frame_id: u64, previous_frame_id: Option<u64>, played: bool) -> Value {
    let strike = card("strike-uuid", "Strike_R");
    let defend = card("defend-uuid", "Defend_R");
    let hand = if played {
        vec![strike.clone()]
    } else {
        vec![strike.clone(), defend.clone()]
    };
    let discard = if played { vec![defend] } else { vec![] };
    json!({
        "kind": "state_frame",
        "capture_id": "11111111-1111-4111-8111-111111111111",
        "frame_id": frame_id,
        "previous_frame_id": previous_frame_id,
        "profile": "parity_snapshot",
        "grants": {"observation": ["public_base", "parity_internal", "rng_internal"], "control": []},
        "boundary": {"kind": "combat_player_decision", "stable": true},
        "public_base": {
            "run": {"ascension": 0, "character": "IRONCLAD", "act": 1, "floor": 1},
            "combat": {
                "turn": 1,
                "player": {
                    "current_hp": 80,
                    "max_hp": 80,
                    "block": if played { 5 } else { 0 },
                    "energy": if played { 2 } else { 3 }
                },
                "hand": hand,
                "draw_count": 1,
                "discard": {"ordering": "unspecified", "cards": discard},
                "exhaust": {"ordering": "unspecified", "cards": []},
                "monsters": [{
                    "index": 0,
                    "instance_id": 1,
                    "id": "JawWorm",
                    "current_hp": 30,
                    "max_hp": 30,
                    "block": 0,
                    "dead_or_escaped": false
                }]
            }
        },
        "parity_internal": {
            "run_seed": "42",
            "zones": {
                "draw": {"ordering": "java_group_index_ascending", "top": "last", "cards": [card("draw-uuid", "Bash")]},
                "discard": {"ordering": "java_group_index_ascending", "top": "last", "cards": if played { vec![card("defend-uuid", "Defend_R")] } else { vec![] }},
                "exhaust": {"ordering": "java_group_index_ascending", "top": "last", "cards": []},
                "hand": {"ordering": "java_group_index_ascending", "top": "last", "cards": if played { vec![card("strike-uuid", "Strike_R")] } else { vec![card("strike-uuid", "Strike_R"), card("defend-uuid", "Defend_R")] }},
                "limbo": {"ordering": "java_group_index_ascending", "top": "last", "cards": []}
            }
        },
        "rng_internal": {"streams": [
            rng_stream(AI_RNG, "floor", "0123456789ABCDEF", "FEDCBA9876543210", 2),
            rng_stream(SHUFFLE_RNG, "floor", "1123456789ABCDEF", "EEDCBA9876543210", 1),
            rng_stream(CARD_RANDOM_RNG, "floor", "2123456789ABCDEF", "DEDCBA9876543210", 0),
            rng_stream(MISC_RNG, "floor", "3123456789ABCDEF", "CEDCBA9876543210", 1),
            rng_stream(MONSTER_HP_RNG, "floor", "4123456789ABCDEF", "BEDCBA9876543210", 2),
            rng_stream(POTION_RNG, "run", "5123456789ABCDEF", "AEDCBA9876543210", 0)
        ]}
    })
}

fn artifact_and_receipt() -> (Vec<u8>, CaptureReceipt) {
    let records = [
        json!({"kind": "hello"}),
        json!({"kind": "hello_ack"}),
        frame(1, None, false),
        frame(2, Some(1), true),
    ];
    let mut artifact = records
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    artifact.push(b'\n');
    let receipt = CaptureReceipt {
        schema: RECEIPT_SCHEMA.to_owned(),
        status: "completed".to_owned(),
        completed_at_unix_ms: 0,
        capture_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        producer_session_id: "22222222-2222-4222-8222-222222222222".to_owned(),
        connection_id: "33333333-3333-4333-8333-333333333333".to_owned(),
        source_commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        source_dirty: false,
        loaded_mod_jar_sha256: "A".repeat(64),
        sts_jar_sha256: "B".repeat(64),
        mod_the_spire_jar_sha256: "C".repeat(64),
        requested_frames: 2,
        captured_frames: 2,
        first_frame_id: 1,
        last_frame_id: 2,
        collector: CollectorReceipt {
            name: "sts_java_parity_capture".to_owned(),
            version: "0.1.0".to_owned(),
            executable_path: "collector".to_owned(),
            executable_sha256: "D".repeat(64),
        },
        artifact: ArtifactReceipt {
            path: "capture.jsonl".to_owned(),
            bytes: artifact.len() as u64,
            sha256: sha256_hex(&artifact),
        },
    };
    (artifact, receipt)
}

fn targeted_strike_frame(frame_id: u64, previous_frame_id: Option<u64>, played: bool) -> Value {
    let strike = card("strike-live-uuid", "Strike_R");
    let defend = card("defend-live-uuid", "Defend_R");
    let hand = if played {
        vec![defend.clone()]
    } else {
        vec![strike.clone(), defend.clone()]
    };
    let discard = if played { vec![strike] } else { vec![] };
    json!({
        "kind": "state_frame",
        "producer_session_id": "22222222-2222-4222-8222-222222222222",
        "connection_id": "33333333-3333-4333-8333-333333333333",
        "capture_id": "11111111-1111-4111-8111-111111111111",
        "frame_id": frame_id,
        "previous_frame_id": previous_frame_id,
        "profile": "parity_run_control",
        "grants": {
            "observation": ["public_base", "parity_internal", "rng_internal"],
            "control": [
                "run_start", "run_choose_event", "run_choose_map_node",
                "run_claim_combat_reward", "run_close_combat_reward",
                "run_acknowledge_ftue", "combat_play_card", "combat_end_turn"
            ]
        },
        "boundary": {"kind": "combat_player_decision", "stable": true},
        "public_base": {
            "run": {"ascension": 0, "character": "IRONCLAD", "act": 1, "floor": 1},
            "combat": {
                "turn": 1,
                "player": {
                    "current_hp": 80,
                    "max_hp": 80,
                    "block": 0,
                    "energy": if played { 2 } else { 3 }
                },
                "hand": hand,
                "draw_count": 1,
                "discard": {"ordering": "unspecified", "cards": discard},
                "exhaust": {"ordering": "unspecified", "cards": []},
                "monsters": [
                    {
                        "index": 0,
                        "instance_id": 10,
                        "id": "SpikeSlime_S",
                        "current_hp": 11,
                        "max_hp": 11,
                        "block": 0,
                        "dead_or_escaped": false
                    },
                    {
                        "index": 1,
                        "instance_id": 20,
                        "id": "AcidSlime_M",
                        "current_hp": if played { 24 } else { 30 },
                        "max_hp": 30,
                        "block": 0,
                        "dead_or_escaped": false
                    }
                ]
            }
        },
        "parity_internal": {
            "run_seed": "42",
            "zones": {
                "draw": {"ordering": "java_group_index_ascending", "top": "last", "cards": [card("draw-live-uuid", "Bash")]},
                "discard": {"ordering": "java_group_index_ascending", "top": "last", "cards": if played { vec![card("strike-live-uuid", "Strike_R")] } else { vec![] }},
                "exhaust": {"ordering": "java_group_index_ascending", "top": "last", "cards": []},
                "hand": {"ordering": "java_group_index_ascending", "top": "last", "cards": if played { vec![card("defend-live-uuid", "Defend_R")] } else { vec![card("strike-live-uuid", "Strike_R"), card("defend-live-uuid", "Defend_R")] }},
                "limbo": {"ordering": "java_group_index_ascending", "top": "last", "cards": []}
            }
        },
        "rng_internal": {"streams": [
            rng_stream(AI_RNG, "floor", "0123456789ABCDEF", "FEDCBA9876543210", 2),
            rng_stream(SHUFFLE_RNG, "floor", "1123456789ABCDEF", "EEDCBA9876543210", 1),
            rng_stream(CARD_RANDOM_RNG, "floor", "2123456789ABCDEF", "DEDCBA9876543210", 0),
            rng_stream(MISC_RNG, "floor", "3123456789ABCDEF", "CEDCBA9876543210", 1),
            rng_stream(MONSTER_HP_RNG, "floor", "4123456789ABCDEF", "BEDCBA9876543210", 2),
            rng_stream(POTION_RNG, "run", "5123456789ABCDEF", "AEDCBA9876543210", 0)
        ]},
        "action_space": {
            "kind": "combat_action_space",
            "actions": if played {
                vec![json!({"action_id": "end_turn", "kind": "end_turn"})]
            } else {
                vec![json!({
                    "action_id": "play_card:strike-live-uuid:monster:20",
                    "kind": "play_card",
                    "card_uuid": "strike-live-uuid",
                    "card_id": "Strike_R",
                    "target_monster_instance_id": 20,
                    "target_monster_index": 1,
                    "target_monster_id": "AcidSlime_M"
                })]
            }
        }
    })
}

fn whirlwind_frame(frame_id: u64, previous_frame_id: Option<u64>, played: bool) -> Value {
    let mut frame = targeted_strike_frame(frame_id, previous_frame_id, false);
    let whirlwind = card_with_cost("whirlwind-live-uuid", "Whirlwind", -1);
    frame["public_base"]["combat"]["player"]["energy"] = json!(if played { 0 } else { 3 });
    frame["public_base"]["combat"]["hand"] = json!(if played {
        vec![]
    } else {
        vec![whirlwind.clone()]
    });
    frame["public_base"]["combat"]["discard"]["cards"] = json!(if played {
        vec![whirlwind.clone()]
    } else {
        vec![]
    });
    frame["public_base"]["combat"]["monsters"] = json!([{
        "index": 0,
        "instance_id": 10,
        "id": "Cultist",
        "current_hp": if played { 41 } else { 56 },
        "max_hp": 56,
        "block": 0,
        "dead_or_escaped": false
    }]);
    frame["parity_internal"]["zones"]["hand"]["cards"] = json!(if played {
        vec![]
    } else {
        vec![whirlwind.clone()]
    });
    frame["parity_internal"]["zones"]["discard"]["cards"] =
        json!(if played { vec![whirlwind] } else { vec![] });
    frame["action_space"]["actions"] = if played {
        json!([{"action_id": "end_turn", "kind": "end_turn"}])
    } else {
        json!([{
            "action_id": "play_card:whirlwind-live-uuid:none",
            "kind": "play_card",
            "card_uuid": "whirlwind-live-uuid",
            "card_id": "Whirlwind"
        }])
    };
    frame
}

fn constructed_chemical_x_whirlwind_transition() -> LoadedLiveTransition {
    let recipe: CombatScenarioRecipe = serde_json::from_value(json!({
        "schema": "communicationmod.parity-combat-recipe.v2",
        "provenance": "constructed_validated",
        "scenario_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        "case_id": "cultist-chemical-x-whirlwind-test",
        "recipe_id": "fresh_neow_to_combat_root.v1",
        "capability_registry": "communicationmod.parity-combat-capabilities.v2",
        "capabilities": [
            {
                "capability_id": "combat.encounter.exact.v1",
                "payload": {"encounter_id": "Cultist"}
            },
            {
                "capability_id": "player.health.v1",
                "payload": {"current_hp": 80, "max_hp": 80}
            },
            {
                "capability_id": "player.master_deck.exact.v1",
                "payload": {"cards": [{"card_id": "Whirlwind", "upgrades": 0}]}
            },
            {
                "capability_id": "player.relics.fresh_exact.v1",
                "payload": {"relics": [{"relic_id": "Chemical X"}]}
            }
        ]
    }))
    .unwrap();
    let mut before_value = whirlwind_frame(1, None, false);
    let mut after_value = whirlwind_frame(2, Some(1), true);
    after_value["public_base"]["combat"]["monsters"][0]["current_hp"] = json!(31);
    for frame in [&mut before_value, &mut after_value] {
        frame["profile"] = json!(SCENARIO_CONTROL_PROFILE);
        frame["grants"]["control"] = json!(SCENARIO_CONTROL_GRANTS);
        frame["public_base"]["run"]["ascension"] = json!(20);
        frame["scenario"] = serde_json::to_value(&recipe).unwrap();
        frame["parity_internal"]["relics"] = json!([{"index": 0, "relic_id": "Chemical X"}]);
    }

    let mut loaded = constructed_cultist_transition();
    loaded.before = serde_json::from_value(before_value).unwrap();
    loaded.after = serde_json::from_value(after_value).unwrap();
    loaded.receipt.based_on_frame_id = 1;
    loaded.receipt.successor_frame_id = Some(2);
    loaded.receipt.action_id = "play_card:whirlwind-live-uuid:none".to_owned();
    loaded.receipt.parameters = json!({});
    loaded.receipt.producer_result = Some(json!({
        "kind": "action_result",
        "status": "accepted",
        "connection_id": loaded.manifest.connection_id.clone(),
        "request_id": loaded.receipt.request_id.clone(),
        "based_on_frame_id": 1,
        "action_id": "play_card:whirlwind-live-uuid:none"
    }));
    loaded
}

fn cultist_turn_one_frame(
    frame_id: u64,
    previous_frame_id: Option<u64>,
    after_end_turn: bool,
) -> Value {
    let draw_defend = card("draw-defend", "Defend_R");
    let draw_strike_a = card("draw-strike-a", "Strike_R");
    let draw_strike_b = card("draw-strike-b", "Strike_R");
    let draw_defend_b = card("draw-defend-b", "Defend_R");
    let draw_strike_c = card("draw-strike-c", "Strike_R");
    let draw_defend_c = card("draw-defend-c", "Defend_R");
    let bash = card_with_cost("hand-bash", "Bash", 2);
    let bane = card_with_cost("hand-bane", "AscendersBane", -2);
    let hand_strike = card("hand-strike", "Strike_R");
    let discard_defend = card("discard-defend", "Defend_R");
    let discard_strike = card("discard-strike", "Strike_R");

    let before_draw = vec![
        draw_defend.clone(),
        draw_strike_a.clone(),
        draw_strike_b.clone(),
        draw_defend_b.clone(),
        draw_strike_c.clone(),
        draw_defend_c.clone(),
    ];
    let before_hand = vec![bash.clone(), bane.clone(), hand_strike.clone()];
    let before_discard = vec![discard_defend.clone(), discard_strike.clone()];
    let after_draw = vec![draw_defend.clone()];
    let after_hand = vec![
        draw_defend_c,
        draw_strike_c,
        draw_defend_b,
        draw_strike_b,
        draw_strike_a,
    ];
    let after_discard = vec![discard_defend, discard_strike, hand_strike, bash];
    let after_exhaust = vec![bane];
    let (turn, energy, block, hand, draw, discard, exhaust, ai_rng) = if after_end_turn {
        (
            2,
            3,
            0,
            after_hand,
            after_draw,
            after_discard,
            after_exhaust,
            rng_stream(AI_RNG, "floor", "B722F2631CB7B14D", "AB89235541870F7D", 2),
        )
    } else {
        (
            1,
            1,
            5,
            before_hand,
            before_draw,
            before_discard,
            vec![],
            rng_stream(AI_RNG, "floor", "7D6E4AC38B2B1BE2", "B722F2631CB7B14D", 1),
        )
    };
    let internal_monsters = if after_end_turn {
        json!([{
            "instance_id": 1,
            "next_move_id": 1,
            "intent": "ATTACK",
            "move_history": [3, 1],
            "powers": [{
                "index": 0,
                "id": "Ritual",
                "class_name": "com.megacrit.cardcrawl.powers.RitualPower",
                "amount": 5,
                "type": "BUFF",
                "ritual_skip_first": false,
                "ritual_player_controlled": false
            }]
        }])
    } else {
        json!([{
            "instance_id": 1,
            "next_move_id": 3,
            "intent": "BUFF",
            "move_history": [3],
            "powers": []
        }])
    };
    json!({
        "kind": "state_frame",
        "producer_session_id": "22222222-2222-4222-8222-222222222222",
        "connection_id": "33333333-3333-4333-8333-333333333333",
        "capture_id": "11111111-1111-4111-8111-111111111111",
        "frame_id": frame_id,
        "previous_frame_id": previous_frame_id,
        "profile": "parity_run_control",
        "grants": {
            "observation": ["public_base", "parity_internal", "rng_internal"],
            "control": [
                "run_start", "run_choose_event", "run_choose_map_node",
                "run_claim_combat_reward", "run_close_combat_reward",
                "run_acknowledge_ftue", "combat_play_card", "combat_end_turn"
            ]
        },
        "boundary": {"kind": "combat_player_decision", "stable": true},
        "public_base": {
            "run": {"ascension": 20, "character": "IRONCLAD", "act": 1, "floor": 1},
            "combat": {
                "turn": turn,
                "player": {"current_hp": 68, "max_hp": 75, "block": block, "energy": energy},
                "hand": hand,
                "draw_count": draw.len(),
                "discard": {"ordering": "unspecified", "cards": discard},
                "exhaust": {"ordering": "unspecified", "cards": exhaust},
                "monsters": [{
                    "index": 0,
                    "instance_id": 1,
                    "id": "Cultist",
                    "current_hp": 50,
                    "max_hp": 56,
                    "block": 0,
                    "dead_or_escaped": false
                }]
            }
        },
        "parity_internal": {
            "monsters": internal_monsters,
            "zones": {
                "draw": {"ordering": "java_group_index_ascending", "top": "last", "cards": draw},
                "discard": {"ordering": "java_group_index_ascending", "top": "last", "cards": discard},
                "exhaust": {"ordering": "java_group_index_ascending", "top": "last", "cards": exhaust},
                "hand": {"ordering": "java_group_index_ascending", "top": "last", "cards": hand},
                "limbo": {"ordering": "java_group_index_ascending", "top": "last", "cards": []}
            }
        },
        "rng_internal": {"streams": [
            ai_rng,
            rng_stream(SHUFFLE_RNG, "floor", "7D6E4AC38B2B1BE2", "B722F2631CB7B14D", 1),
            rng_stream(CARD_RANDOM_RNG, "floor", "B456BCFC34C2CB2C", "7D6E4AC38B2B1BE2", 0),
            rng_stream(MISC_RNG, "floor", "B456BCFC34C2CB2C", "7D6E4AC38B2B1BE2", 0),
            rng_stream(MONSTER_HP_RNG, "floor", "7D6E4AC38B2B1BE2", "B722F2631CB7B14D", 1),
            rng_stream(POTION_RNG, "run", "9253C52FCFBD932B", "6151E5F6A4A2DF86", 3)
        ]},
        "action_space": {
            "kind": "combat_action_space",
            "actions": [{"action_id": "end_turn", "kind": "end_turn"}]
        }
    })
}

fn block_potion_frame(frame_id: u64, previous_frame_id: Option<u64>, used: bool) -> Value {
    let mut frame = targeted_strike_frame(frame_id, previous_frame_id, false);
    frame["grants"]["control"] = json!([
        "run_start",
        "run_choose_event",
        "run_choose_map_node",
        "run_claim_combat_reward",
        "run_close_combat_reward",
        "run_acknowledge_ftue",
        "combat_use_potion",
        "combat_play_card",
        "combat_end_turn"
    ]);
    frame["public_base"]["combat"]["player"]["block"] = json!(if used { 12 } else { 0 });
    frame["public_base"]["combat"]["potions"] = if used {
        json!([
            {
                "slot_index": 0,
                "potion_slot": -1,
                "potion_id": "Potion Slot",
                "empty": true,
                "potency": 0,
                "can_use": false,
                "can_discard": false,
                "target_required": false
            },
            {
                "slot_index": 1,
                "potion_slot": 1,
                "potion_id": "Elixir",
                "empty": false,
                "potency": 0,
                "can_use": true,
                "can_discard": true,
                "target_required": false
            }
        ])
    } else {
        json!([
            {
                "slot_index": 0,
                "potion_slot": 0,
                "potion_id": "Block Potion",
                "empty": false,
                "potency": 12,
                "can_use": true,
                "can_discard": true,
                "target_required": false
            },
            {
                "slot_index": 1,
                "potion_slot": 1,
                "potion_id": "Elixir",
                "empty": false,
                "potency": 0,
                "can_use": true,
                "can_discard": true,
                "target_required": false
            }
        ])
    };
    frame["action_space"]["actions"] = if used {
        json!([{"action_id": "end_turn", "kind": "end_turn"}])
    } else {
        json!([{
            "action_id": "use_potion:0:Block Potion:none",
            "kind": "use_potion",
            "potion_slot": 0,
            "potion_id": "Block Potion",
            "potion_potency": 12
        }])
    };
    frame
}

fn fire_potion_frame(frame_id: u64, previous_frame_id: Option<u64>, used: bool) -> Value {
    let mut frame = block_potion_frame(frame_id, previous_frame_id, false);
    frame["public_base"]["combat"]["player"]["block"] = json!(0);
    frame["public_base"]["combat"]["monsters"][1]["current_hp"] = json!(if used { 10 } else { 30 });
    frame["public_base"]["combat"]["potions"][0] = if used {
        json!({
            "slot_index": 0,
            "potion_slot": -1,
            "potion_id": "Potion Slot",
            "empty": true,
            "potency": 0,
            "can_use": false,
            "can_discard": false,
            "target_required": false
        })
    } else {
        json!({
            "slot_index": 0,
            "potion_slot": 0,
            "potion_id": "Fire Potion",
            "empty": false,
            "potency": 20,
            "can_use": true,
            "can_discard": true,
            "target_required": true
        })
    };
    frame["action_space"]["actions"] = if used {
        json!([{"action_id": "end_turn", "kind": "end_turn"}])
    } else {
        json!([{
            "action_id": "use_potion:0:Fire Potion:monster:20",
            "kind": "use_potion",
            "potion_slot": 0,
            "potion_id": "Fire Potion",
            "potion_potency": 20,
            "target_monster_instance_id": 20,
            "target_monster_index": 1,
            "target_monster_id": "AcidSlime_M"
        }])
    };
    frame
}

fn constructed_cultist_recipe() -> CombatScenarioRecipe {
    serde_json::from_value(json!({
        "schema": "communicationmod.parity-combat-recipe.v2",
        "provenance": "constructed_validated",
        "scenario_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "case_id": "cultist-comparator-test",
        "recipe_id": "fresh_neow_to_combat_root.v1",
        "capability_registry": "communicationmod.parity-combat-capabilities.v1",
        "capabilities": [
            {
                "capability_id": "combat.encounter.exact.v1",
                "payload": {"encounter_id": "Cultist"}
            },
            {
                "capability_id": "player.health.v1",
                "payload": {"current_hp": 68, "max_hp": 75}
            },
            {
                "capability_id": "player.master_deck.exact.v1",
                "payload": {"cards": [
                    {"card_id": "Strike_R", "upgrades": 0},
                    {"card_id": "Bash", "upgrades": 0}
                ]}
            }
        ]
    }))
    .unwrap()
}

fn constructed_block_potion_recipe() -> CombatScenarioRecipe {
    serde_json::from_value(json!({
        "schema": "communicationmod.parity-combat-recipe.v2",
        "provenance": "constructed_validated",
        "scenario_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
        "case_id": "cultist-block-potion-test",
        "recipe_id": "fresh_neow_to_combat_root.v1",
        "capability_registry": "communicationmod.parity-combat-capabilities.v3",
        "capabilities": [
            {
                "capability_id": "combat.encounter.exact.v1",
                "payload": {"encounter_id": "Cultist"}
            },
            {
                "capability_id": "player.health.v1",
                "payload": {"current_hp": 68, "max_hp": 75}
            },
            {
                "capability_id": "player.master_deck.exact.v1",
                "payload": {"cards": [
                    {"card_id": "Strike_R", "upgrades": 0}
                ]}
            },
            {
                "capability_id": "player.potions.fresh_exact.v1",
                "payload": {"slots": [
                    {"slot_index": 0, "potion_id": "Block Potion"},
                    {"slot_index": 1, "potion_id": "Potion Slot"}
                ]}
            }
        ]
    }))
    .unwrap()
}

fn constructed_block_potion_transition() -> LoadedLiveTransition {
    let recipe = constructed_block_potion_recipe();
    let mut before_value = block_potion_frame(20, Some(19), false);
    let mut after_value = block_potion_frame(21, Some(20), true);
    for frame in [&mut before_value, &mut after_value] {
        frame["profile"] = json!(SCENARIO_CONTROL_PROFILE);
        frame["grants"]["control"] = json!(SCENARIO_CONTROL_GRANTS);
        frame["scenario"] = serde_json::to_value(&recipe).unwrap();
        frame["public_base"]["combat"]["potions"][1] = json!({
            "slot_index": 1,
            "potion_slot": -1,
            "potion_id": "Potion Slot",
            "empty": true,
            "potency": 0,
            "can_use": false,
            "can_discard": false,
            "target_required": false
        });
    }
    let mut loaded = constructed_cultist_transition();
    loaded.before = serde_json::from_value(before_value).unwrap();
    loaded.after = serde_json::from_value(after_value).unwrap();
    loaded
}

fn constructed_fire_potion_recipe() -> CombatScenarioRecipe {
    serde_json::from_value(json!({
        "schema": "communicationmod.parity-combat-recipe.v2",
        "provenance": "constructed_validated",
        "scenario_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "case_id": "cultist-fire-potion-test",
        "recipe_id": "fresh_neow_to_combat_root.v1",
        "capability_registry": "communicationmod.parity-combat-capabilities.v3",
        "capabilities": [
            {
                "capability_id": "combat.encounter.exact.v1",
                "payload": {"encounter_id": "Cultist"}
            },
            {
                "capability_id": "player.health.v1",
                "payload": {"current_hp": 68, "max_hp": 75}
            },
            {
                "capability_id": "player.master_deck.exact.v1",
                "payload": {"cards": [
                    {"card_id": "Strike_R", "upgrades": 0}
                ]}
            },
            {
                "capability_id": "player.potions.fresh_exact.v1",
                "payload": {"slots": [
                    {"slot_index": 0, "potion_id": "Fire Potion"},
                    {"slot_index": 1, "potion_id": "Potion Slot"}
                ]}
            }
        ]
    }))
    .unwrap()
}

fn constructed_fire_potion_transition() -> LoadedLiveTransition {
    let recipe = constructed_fire_potion_recipe();
    let mut before_value = fire_potion_frame(30, Some(29), false);
    let mut after_value = fire_potion_frame(31, Some(30), true);
    for frame in [&mut before_value, &mut after_value] {
        frame["profile"] = json!(SCENARIO_CONTROL_PROFILE);
        frame["grants"]["control"] = json!(SCENARIO_CONTROL_GRANTS);
        frame["scenario"] = serde_json::to_value(&recipe).unwrap();
        frame["public_base"]["combat"]["potions"][1] = json!({
            "slot_index": 1,
            "potion_slot": -1,
            "potion_id": "Potion Slot",
            "empty": true,
            "potency": 0,
            "can_use": false,
            "can_discard": false,
            "target_required": false
        });
    }
    let mut loaded = constructed_cultist_transition();
    loaded.before = serde_json::from_value(before_value).unwrap();
    loaded.after = serde_json::from_value(after_value).unwrap();
    loaded.receipt.based_on_frame_id = 30;
    loaded.receipt.successor_frame_id = Some(31);
    loaded.receipt.action_id = "use_potion:0:Fire Potion:monster:20".to_owned();
    loaded.receipt.parameters = json!({});
    loaded.receipt.producer_result = Some(json!({
        "kind": "action_result",
        "status": "accepted",
        "connection_id": loaded.manifest.connection_id.clone(),
        "request_id": loaded.receipt.request_id.clone(),
        "based_on_frame_id": 30,
        "action_id": "use_potion:0:Fire Potion:monster:20"
    }));
    loaded
}

fn constructed_cultist_transition() -> LoadedLiveTransition {
    let recipe = constructed_cultist_recipe();
    let mut before_value = cultist_turn_one_frame(12, Some(11), false);
    let mut after_value = cultist_turn_one_frame(13, Some(12), true);
    for frame in [&mut before_value, &mut after_value] {
        frame["profile"] = json!(SCENARIO_CONTROL_PROFILE);
        frame["grants"]["control"] = json!(SCENARIO_CONTROL_GRANTS);
        frame["scenario"] = serde_json::to_value(&recipe).unwrap();
    }
    let before: StateFrameProjection = serde_json::from_value(before_value).unwrap();
    let after: StateFrameProjection = serde_json::from_value(after_value).unwrap();
    let manifest = SessionManifest {
        schema: "sts_java_parity_live_session_v3".to_owned(),
        session_id: "44444444-4444-4444-8444-444444444444".to_owned(),
        created_at_unix_ms: 0,
        process_id: std::process::id(),
        session_dir: "unused-by-in-memory-test".to_owned(),
        capture_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        producer_session_id: "22222222-2222-4222-8222-222222222222".to_owned(),
        connection_id: "33333333-3333-4333-8333-333333333333".to_owned(),
        profile: SCENARIO_CONTROL_PROFILE.to_owned(),
        source_commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        source_dirty: false,
        loaded_mod_jar_sha256: "A".repeat(64),
        sts_jar_sha256: "B".repeat(64),
        mod_the_spire_jar_sha256: "C".repeat(64),
        launch: Some(LaunchEvidence {
            profile: "accelerated".to_owned(),
            control_profile: SCENARIO_CONTROL_PROFILE.to_owned(),
            plan_path: "scenario-launch-plan.json".to_owned(),
            plan_sha256: "1".repeat(64),
            storage_class: "isolated_working_directory_v1".to_owned(),
            working_directory: "isolated-working-directory".to_owned(),
            source_game_directory: "installed-game-directory".to_owned(),
            profile_substrate: "canonical_tutorial_complete_preferences_v1".to_owned(),
            tutorial_policy: "precompleted_via_profile_v1".to_owned(),
            isolation_id: Some("66666666-6666-4666-8666-666666666666".to_owned()),
        }),
    };
    let receipt = ActionReceipt {
        schema: "sts_java_parity_live_action_receipt_v2".to_owned(),
        status: "completed".to_owned(),
        code: None,
        message: "completed".to_owned(),
        session_id: manifest.session_id.clone(),
        connection_id: manifest.connection_id.clone(),
        request_id: "55555555-5555-4555-8555-555555555555".to_owned(),
        based_on_frame_id: 12,
        action_id: "end_turn".to_owned(),
        parameters: json!({}),
        completed_at_unix_ms: 1,
        settlement_wall_time_ms: Some(500),
        producer_result: Some(json!({
            "kind": "action_result",
            "status": "accepted",
            "connection_id": manifest.connection_id,
            "request_id": "55555555-5555-4555-8555-555555555555",
            "based_on_frame_id": 12,
            "action_id": "end_turn"
        })),
        successor_frame_id: Some(13),
        successor_frame_path: Some("unused-by-in-memory-test".to_owned()),
    };
    LoadedLiveTransition {
        manifest,
        receipt,
        manifest_path: PathBuf::from("unused-session.json"),
        receipt_path: PathBuf::from("unused-receipt.json"),
        before_path: PathBuf::from("unused-before.json"),
        after_path: PathBuf::from("unused-after.json"),
        manifest_sha256: "D".repeat(64),
        receipt_sha256: "E".repeat(64),
        before_sha256: "F".repeat(64),
        after_sha256: "0".repeat(64),
        before,
        after,
    }
}

fn write_live_transition_fixture(
    label: &str,
    before: Value,
    after: Value,
    action_id: &str,
) -> (PathBuf, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let session_dir = std::env::temp_dir().join(format!(
        "sts-java-parity-{label}-{}-{nonce}",
        std::process::id()
    ));
    let frames_dir = session_dir.join("frames");
    let receipts_dir = session_dir.join("receipts");
    fs::create_dir_all(&frames_dir).unwrap();
    fs::create_dir(&receipts_dir).unwrap();
    let before_id = before["frame_id"].as_u64().unwrap();
    let after_id = after["frame_id"].as_u64().unwrap();
    let before_path = frames_dir.join(format!("frame-{before_id:020}.json"));
    let after_path = frames_dir.join(format!("frame-{after_id:020}.json"));
    fs::write(&before_path, serde_json::to_vec(&before).unwrap()).unwrap();
    fs::write(&after_path, serde_json::to_vec(&after).unwrap()).unwrap();
    let canonical_session = fs::canonicalize(&session_dir).unwrap();
    let manifest = SessionManifest {
        schema: "sts_java_parity_live_session_v3".to_owned(),
        session_id: "44444444-4444-4444-8444-444444444444".to_owned(),
        created_at_unix_ms: 1,
        process_id: std::process::id(),
        session_dir: canonical_session.to_string_lossy().into_owned(),
        capture_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        producer_session_id: "22222222-2222-4222-8222-222222222222".to_owned(),
        connection_id: "33333333-3333-4333-8333-333333333333".to_owned(),
        profile: before["profile"].as_str().unwrap().to_owned(),
        source_commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        source_dirty: false,
        loaded_mod_jar_sha256: "A".repeat(64),
        sts_jar_sha256: "B".repeat(64),
        mod_the_spire_jar_sha256: "C".repeat(64),
        launch: None,
    };
    fs::write(
        session_dir.join("session.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let receipt = ActionReceipt {
        schema: "sts_java_parity_live_action_receipt_v2".to_owned(),
        status: "completed".to_owned(),
        code: None,
        message: "completed".to_owned(),
        session_id: manifest.session_id.clone(),
        connection_id: manifest.connection_id.clone(),
        request_id: "55555555-5555-4555-8555-555555555555".to_owned(),
        based_on_frame_id: before_id,
        action_id: action_id.to_owned(),
        parameters: json!({}),
        completed_at_unix_ms: 2,
        settlement_wall_time_ms: Some(500),
        producer_result: Some(json!({
            "kind": "action_result",
            "status": "accepted",
            "connection_id": manifest.connection_id,
            "request_id": "55555555-5555-4555-8555-555555555555",
            "based_on_frame_id": before_id,
            "action_id": action_id
        })),
        successor_frame_id: Some(after_id),
        successor_frame_path: Some(
            fs::canonicalize(&after_path)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
    };
    let receipt_path = receipts_dir.join("55555555-5555-4555-8555-555555555555.json");
    fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    (session_dir, receipt_path)
}

#[test]
fn base_defend_fixture_matches_the_production_stepper() {
    let (artifact, receipt) = artifact_and_receipt();
    let text = std::str::from_utf8(&artifact).unwrap();
    let frames: Vec<StateFrameProjection> = text
        .lines()
        .skip(2)
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let before = &frames[0];
    let after = &frames[1];

    validate_adjacent_frames(before, after, &receipt.capture_id).unwrap();
    let moved = infer_base_defend_transition(before, after).unwrap();
    let simulator = run_minimal_simulator_transition(before, after, &moved).unwrap();

    assert_eq!(moved.card_id, BASE_DEFEND_JAVA_ID);
    assert_eq!(simulator.energy_after, 2);
    assert_eq!(simulator.block_after, 5);
    assert!(simulator.moved_card_reached_discard);
    assert!(simulator.six_rng_streams_unchanged);
}

#[test]
fn base_defend_fixture_rejects_rng_consumption() {
    let (artifact, receipt) = artifact_and_receipt();
    let text = std::str::from_utf8(&artifact).unwrap();
    let mut frames: Vec<StateFrameProjection> = text
        .lines()
        .skip(2)
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    frames[1].rng_internal.streams[0].counter += 1;

    let error = infer_base_defend_transition(&frames[0], &frames[1])
        .err()
        .unwrap();

    assert!(error.contains("RNG streams changed"), "{error}");
    assert_eq!(receipt.captured_frames, 2);
}

#[test]
fn completed_live_targeted_strike_matches_the_production_stepper() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let session_dir = std::env::temp_dir().join(format!(
        "sts-java-parity-compare-{}-{nonce}",
        std::process::id()
    ));
    let frames_dir = session_dir.join("frames");
    let receipts_dir = session_dir.join("receipts");
    fs::create_dir_all(&frames_dir).unwrap();
    fs::create_dir(&receipts_dir).unwrap();
    let before_path = frames_dir.join("frame-00000000000000000001.json");
    let after_path = frames_dir.join("frame-00000000000000000002.json");
    fs::write(
        &before_path,
        serde_json::to_vec(&targeted_strike_frame(1, None, false)).unwrap(),
    )
    .unwrap();
    fs::write(
        &after_path,
        serde_json::to_vec(&targeted_strike_frame(2, Some(1), true)).unwrap(),
    )
    .unwrap();
    let canonical_session = fs::canonicalize(&session_dir).unwrap();
    let manifest = SessionManifest {
        schema: "sts_java_parity_live_session_v3".to_owned(),
        session_id: "44444444-4444-4444-8444-444444444444".to_owned(),
        created_at_unix_ms: 0,
        process_id: std::process::id(),
        session_dir: canonical_session.to_string_lossy().into_owned(),
        capture_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        producer_session_id: "22222222-2222-4222-8222-222222222222".to_owned(),
        connection_id: "33333333-3333-4333-8333-333333333333".to_owned(),
        profile: "parity_run_control".to_owned(),
        source_commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        source_dirty: false,
        loaded_mod_jar_sha256: "A".repeat(64),
        sts_jar_sha256: "B".repeat(64),
        mod_the_spire_jar_sha256: "C".repeat(64),
        launch: None,
    };
    fs::write(
        session_dir.join("session.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let receipt = ActionReceipt {
        schema: "sts_java_parity_live_action_receipt_v2".to_owned(),
        status: "completed".to_owned(),
        code: None,
        message: "completed".to_owned(),
        session_id: manifest.session_id.clone(),
        connection_id: manifest.connection_id.clone(),
        request_id: "55555555-5555-4555-8555-555555555555".to_owned(),
        based_on_frame_id: 1,
        action_id: "play_card:strike-live-uuid:monster:20".to_owned(),
        parameters: serde_json::json!({}),
        completed_at_unix_ms: 1,
        settlement_wall_time_ms: Some(500),
        producer_result: Some(json!({
            "kind": "action_result",
            "status": "accepted",
            "connection_id": manifest.connection_id,
            "request_id": "55555555-5555-4555-8555-555555555555",
            "based_on_frame_id": 1,
            "action_id": "play_card:strike-live-uuid:monster:20"
        })),
        successor_frame_id: Some(2),
        successor_frame_path: Some(
            fs::canonicalize(&after_path)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
    };
    let receipt_path = receipts_dir.join("55555555-5555-4555-8555-555555555555.json");
    fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();

    let report = compare_base_strike_live(&receipt_path).unwrap();

    assert_eq!(report.status, "matched");
    assert_eq!(report.transition.target_monster_instance_id, 20);
    assert_eq!(report.original_game.target_hp_after, 24);
    assert_eq!(report.simulator.target_hp_after, 24);
    assert!(report.simulator.other_monsters_unchanged);
    assert!(report.simulator.six_rng_streams_unchanged);
    fs::remove_dir_all(session_dir).unwrap();
}

#[test]
fn ordinary_whirlwind_commits_all_current_energy_without_a_caller_x_parameter() {
    let (session_dir, receipt_path) = write_live_transition_fixture(
        "whirlwind",
        whirlwind_frame(1, None, false),
        whirlwind_frame(2, Some(1), true),
        "play_card:whirlwind-live-uuid:none",
    );

    let report = compare_live_action(&receipt_path).unwrap();
    let JavaParityLiveComparisonReportV1::Whirlwind(report) = report else {
        panic!("Whirlwind receipt routed to the wrong comparator")
    };
    assert_eq!(report.status, "matched");
    assert!(report.transition.action_parameters_empty);
    assert_eq!(report.original_game.energy_committed, 3);
    assert_eq!(report.original_game.energy_after, 0);
    assert_eq!(report.original_game.damage_per_enemy_total, 15);
    assert_eq!(report.original_game.monster_hp_before, vec![56]);
    assert_eq!(report.original_game.monster_hp_after, vec![41]);
    assert_eq!(report.original_game, report.simulator);
    assert!(report.claims.no_caller_supplied_x_parameter_verified);
    assert!(report.claims.full_current_energy_commitment_matches);
    fs::remove_dir_all(session_dir).unwrap();
}

#[test]
fn typed_live_comparator_rejects_missing_evidence_before_dispatch() {
    let missing = std::env::temp_dir().join("missing-java-parity-action-receipt.json");

    let failure = compare_live_action_typed(&missing).unwrap_err();

    assert_eq!(failure.schema, "sts_java_parity_live_comparison_failure_v1");
    assert_eq!(
        failure.kind,
        JavaParityLiveComparisonFailureKindV1::EvidenceRejected
    );
    assert_eq!(
        failure.stage,
        JavaParityLiveComparisonFailureStageV1::LoadEvidence
    );
    assert!(failure.details.is_none());
}

#[test]
fn typed_live_comparator_keeps_valid_unregistered_actions_unsupported() {
    let mut before = whirlwind_frame(1, None, false);
    let mut after = whirlwind_frame(2, Some(1), true);
    before["public_base"]["combat"]["hand"][0]["card_id"] = json!("Uppercut");
    before["parity_internal"]["zones"]["hand"]["cards"][0]["card_id"] = json!("Uppercut");
    after["public_base"]["combat"]["discard"]["cards"][0]["card_id"] = json!("Uppercut");
    after["parity_internal"]["zones"]["discard"]["cards"][0]["card_id"] = json!("Uppercut");
    before["action_space"]["actions"][0]["card_id"] = json!("Uppercut");
    let (session_dir, receipt_path) = write_live_transition_fixture(
        "unsupported-uppercut",
        before,
        after,
        "play_card:whirlwind-live-uuid:none",
    );

    let failure = compare_live_action_typed(&receipt_path).unwrap_err();

    assert_eq!(
        failure.kind,
        JavaParityLiveComparisonFailureKindV1::Unsupported
    );
    assert_eq!(
        failure.stage,
        JavaParityLiveComparisonFailureStageV1::SelectComparator
    );
    assert!(failure.details.is_none());
    fs::remove_dir_all(session_dir).unwrap();
}

#[test]
fn fresh_exact_chemical_x_adds_two_whirlwind_repetitions_without_changing_energy_cost() {
    let loaded = constructed_chemical_x_whirlwind_transition();
    validate_live_adjacent_frames(&loaded).unwrap();
    let report = compare_whirlwind_loaded(loaded).unwrap();

    assert_eq!(report.status, "matched");
    assert_eq!(report.original_game.energy_committed, 3);
    assert_eq!(report.original_game.energy_after, 0);
    assert_eq!(report.original_game.effect_repetitions, 5);
    assert_eq!(report.original_game.chemical_x_bonus_repetitions, 2);
    assert_eq!(report.original_game.damage_per_enemy_total, 25);
    assert_eq!(report.original_game.monster_hp_after, vec![31]);
    assert_eq!(report.original_game.observed_relic_ids, vec!["Chemical X"]);
    assert_eq!(report.original_game, report.simulator);
    assert!(
        report
            .claims
            .relic_identity_projection_matches_bound_context
    );
    assert!(report.claims.declared_x_cost_modifier_matches);
}

#[test]
fn cultist_turn_one_end_turn_matches_zones_and_six_rng_streams() {
    let before: StateFrameProjection =
        serde_json::from_value(cultist_turn_one_frame(12, Some(11), false)).unwrap();
    let after: StateFrameProjection =
        serde_json::from_value(cultist_turn_one_frame(13, Some(12), true)).unwrap();

    validate_cultist_turn_one_original_transition(&before, &after).unwrap();
    let original = project_end_turn_frames(&before, &after).unwrap();
    let simulator = run_cultist_turn_one_end_turn_transition(&before, &after).unwrap();

    assert_eq!(simulator, original);
    assert_eq!(simulator.turn_after, 2);
    assert_eq!(simulator.player_block_after, 0);
    assert_eq!(simulator.hand_count_after, 5);
    assert_eq!(simulator.exhaust_count_after, 1);
    let ai = simulator
        .rng_transitions
        .iter()
        .find(|transition| transition.owner == AI_RNG)
        .unwrap();
    assert_eq!(ai.counter_after, 2);
    assert!(ai.state_changed);
    assert!(simulator
        .rng_transitions
        .iter()
        .filter(|transition| transition.owner != AI_RNG)
        .all(|transition| !transition.state_changed));
}

#[test]
fn ordinary_block_potion_matches_slot_block_and_six_rng_streams() {
    let before: StateFrameProjection =
        serde_json::from_value(block_potion_frame(20, Some(19), false)).unwrap();
    let after: StateFrameProjection =
        serde_json::from_value(block_potion_frame(21, Some(20), true)).unwrap();

    let original = validate_block_potion_original_transition(&before, &after, 0).unwrap();
    let simulator = run_block_potion_transition(&before, &after, 0).unwrap();

    assert_eq!(simulator, original);
    assert_eq!(simulator.player_block_after, 12);
    assert!(simulator.slot_empty_after);
    assert!(simulator.card_zones_unchanged);
    assert!(simulator.monsters_unchanged);
    assert!(simulator.six_rng_streams_unchanged);
}

#[test]
fn block_potion_projection_mismatch_is_a_typed_mechanics_divergence() {
    let before: StateFrameProjection =
        serde_json::from_value(block_potion_frame(20, Some(19), false)).unwrap();
    let after: StateFrameProjection =
        serde_json::from_value(block_potion_frame(21, Some(20), true)).unwrap();
    let original = validate_block_potion_original_transition(&before, &after, 0).unwrap();
    let mut simulator = run_block_potion_transition(&before, &after, 0).unwrap();
    simulator.player_block_after += 1;

    let failure = ensure_block_potion_projection_match(&original, &simulator).unwrap_err();

    assert_eq!(
        failure.kind,
        JavaParityLiveComparisonFailureKindV1::MechanicsDivergence
    );
    assert_eq!(
        failure.stage,
        JavaParityLiveComparisonFailureStageV1::CompareProjection
    );
    let artifact = serde_json::to_value(&failure).unwrap();
    assert_eq!(
        artifact["schema"],
        "sts_java_parity_live_comparison_failure_v1"
    );
    assert_eq!(artifact["kind"], "mechanics_divergence");
    assert_eq!(
        artifact["details"]["kind"],
        "block_potion_projection_mismatch"
    );
    let Some(JavaParityLiveComparisonFailureDetailsV1::BlockPotionProjectionMismatch {
        original_game,
        simulator,
    }) = failure.details
    else {
        panic!("typed projection divergence omitted its structured details")
    };
    assert_eq!(original_game.player_block_after, 12);
    assert_eq!(simulator.player_block_after, 13);
}

#[test]
fn constructed_block_potion_requires_its_bound_fresh_exact_inventory() {
    let mut loaded = constructed_block_potion_transition();
    assert!(validate_fresh_potion_scenario_binding(&loaded, 0, "Block Potion").unwrap());

    let recipe = loaded.before.scenario.as_mut().unwrap();
    let slots = recipe
        .capabilities
        .iter_mut()
        .find_map(|capability| match capability {
            ScenarioCapability::FreshPotions(potions) => Some(&mut potions.slots),
            _ => None,
        })
        .unwrap();
    slots[0].potion_id = "ElixirPotion".to_owned();
    assert!(validate_fresh_potion_scenario_binding(&loaded, 0, "Block Potion").is_err());
}

#[test]
fn targeted_fire_potion_matches_slot_target_damage_and_six_rng_streams() {
    let before: StateFrameProjection =
        serde_json::from_value(fire_potion_frame(30, Some(29), false)).unwrap();
    let after: StateFrameProjection =
        serde_json::from_value(fire_potion_frame(31, Some(30), true)).unwrap();

    let original = validate_fire_potion_original_transition(&before, &after, 0, 1).unwrap();
    let simulator = run_fire_potion_transition(&before, &after, 0, 1).unwrap();

    assert_eq!(simulator, original);
    assert_eq!(simulator.target_hp_before, 30);
    assert_eq!(simulator.target_hp_after, 10);
    assert!(simulator.slot_empty_after);
    assert!(simulator.card_zones_unchanged);
    assert!(simulator.other_monsters_unchanged);
    assert!(simulator.six_rng_streams_unchanged);
}

#[test]
fn constructed_fire_potion_binds_exact_inventory_target_and_non_training_scope() {
    let mut loaded = constructed_fire_potion_transition();
    validate_live_adjacent_frames(&loaded).unwrap();
    assert!(validate_fresh_potion_scenario_binding(&loaded, 0, "Fire Potion").unwrap());

    let report = compare_fire_potion_loaded(loaded).unwrap();
    assert_eq!(report.status, "matched");
    assert_eq!(report.transition.target_monster_index, 1);
    assert_eq!(report.transition.target_monster_instance_id, 20);
    assert_eq!(report.transition.target_monster_id, "AcidSlime_M");
    assert_eq!(report.original_game.target_hp_after, 10);
    assert_eq!(report.original_game, report.simulator);
    assert!(report.evidence_scope.constructed_root_verified);
    assert!(!report.evidence_scope.natural_reachability);
    assert!(!report.evidence_scope.strategy_evidence);
    assert!(!report.evidence_scope.training_eligible);

    loaded = constructed_fire_potion_transition();
    let slots = loaded
        .before
        .scenario
        .as_mut()
        .unwrap()
        .capabilities
        .iter_mut()
        .find_map(|capability| match capability {
            ScenarioCapability::FreshPotions(potions) => Some(&mut potions.slots),
            _ => None,
        })
        .unwrap();
    slots[0].potion_id = "Block Potion".to_owned();
    assert!(validate_fresh_potion_scenario_binding(&loaded, 0, "Fire Potion").is_err());
}

#[test]
fn constructed_profile_accepts_exact_recipe_and_never_claims_natural_evidence() {
    let loaded = constructed_cultist_transition();

    validate_live_frame_pair(&loaded).unwrap();
    let (origin, context) = live_transition_origin(&loaded).unwrap();
    let evidence = live_comparison_evidence_scope(&loaded).unwrap();

    assert_eq!(origin, "constructed_validated");
    assert_eq!(context, Some(constructed_cultist_recipe()));
    assert!(!evidence.natural_reachability);
    assert!(!evidence.strategy_evidence);
    assert!(!evidence.training_eligible);
    assert!(evidence.constructed_root_verified);
    assert!(evidence.original_game_transition_captured);
    assert!(evidence.simulator_comparison);
    assert!(evidence.narrow_mechanics_parity);
    assert!(!evidence.strict_runtime_profile);
}

#[test]
fn constructed_profile_rejects_changed_successor_recipe() {
    let mut loaded = constructed_cultist_transition();
    loaded.after.scenario.as_mut().unwrap().scenario_id =
        "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_owned();

    let error = validate_live_frame_pair(&loaded).unwrap_err();

    assert!(
        error.contains("did not preserve its exact validated recipe"),
        "{error}"
    );
}

#[test]
fn constructed_profile_rejects_missing_isolated_launch_evidence() {
    let mut loaded = constructed_cultist_transition();
    loaded.manifest.launch = None;

    let error = validate_live_frame_pair(&loaded).unwrap_err();

    assert!(error.contains("omitted typed launch evidence"), "{error}");
}

#[test]
fn rng_transition_projection_keeps_each_changed_owner_visible() {
    let before: StateFrameProjection =
        serde_json::from_value(cultist_turn_one_frame(12, Some(11), false)).unwrap();
    let mut after: StateFrameProjection =
        serde_json::from_value(cultist_turn_one_frame(13, Some(12), true)).unwrap();
    let shuffle = after
        .rng_internal
        .streams
        .iter_mut()
        .find(|stream| stream.owner == SHUFFLE_RNG)
        .unwrap();
    shuffle.counter += 2;
    shuffle.state_hex = vec!["0123456789ABCDEF".to_owned(), "0011223344556677".to_owned()];

    let transitions =
        project_rng_transitions(&before.rng_internal.streams, &after.rng_internal.streams).unwrap();

    assert_eq!(transitions.len(), 6);
    let shuffle = transitions
        .iter()
        .find(|transition| transition.owner == SHUFFLE_RNG)
        .unwrap();
    assert_eq!(shuffle.counter_before, 1);
    assert_eq!(shuffle.counter_after, 3);
    assert!(shuffle.state_changed);
    assert_eq!(
        transitions
            .iter()
            .filter(|transition| transition.state_changed)
            .map(|transition| transition.owner.as_str())
            .collect::<Vec<_>>(),
        vec![AI_RNG, SHUFFLE_RNG]
    );
}

#[test]
fn constructed_workflow_binds_probe_receipt_comparison_and_evidence_scope() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "sts-java-constructed-workflow-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let receipt_path = directory.join("action-receipt.json");
    let receipt_bytes = b"{\"schema\":\"test-action-receipt\"}\n";
    fs::write(&receipt_path, receipt_bytes).unwrap();

    let mut loaded = constructed_cultist_transition();
    loaded.receipt_path = receipt_path.clone();
    loaded.receipt_sha256 = sha256_hex(receipt_bytes);
    let manifest = loaded.manifest.clone();
    let recipe = loaded.before.scenario.clone().unwrap();
    let comparison = JavaParityLiveComparisonReportV1::CultistEndTurn(
        compare_cultist_turn_one_end_turn_loaded(loaded).unwrap(),
    );
    let comparison_path = directory.join("comparison.json");
    write_json_report_new(&comparison_path, &comparison).unwrap();
    let probe_plan_path = directory.join("probe-plan.json");
    let probe_plan_bytes = serde_json::to_vec_pretty(&json!({
        "schema": "sts_java_play_scenario_probe_plan_v2",
        "provenance": "constructed_validated",
        "created_at_unix_ms": 1,
        "probe_case_id": "cultist-test-end-turn",
        "probe_draft_path": "probe-draft.json",
        "probe_draft_sha256": "0".repeat(64),
        "scenario_case_id": "cultist-comparator-test",
        "scenario_plan_path": "scenario-plan.json",
        "scenario_plan_sha256": "1".repeat(64),
        "transition": {"transition_id": "combat.end_turn.v1"}
    }))
    .unwrap();
    fs::write(&probe_plan_path, &probe_plan_bytes).unwrap();

    let probe: ConstructedScenarioProbeReport = serde_json::from_value(json!({
        "schema": "sts_java_parity_constructed_probe_report_v2",
        "provenance": "constructed_validated",
        "evidence_scope": {
            "natural_reachability": false,
            "strategy_evidence": false,
            "training_eligible": false,
            "constructed_root_verified": true,
            "original_game_transition_captured": true,
            "simulator_comparison": false
        },
        "created_at_unix_ms": 1,
        "completed_at_unix_ms": 2,
        "passed": true,
        "failure": null,
        "probe_case_id": "cultist-test-end-turn",
        "probe_plan_path": probe_plan_path,
        "probe_plan_sha256": sha256_hex(&probe_plan_bytes),
        "scenario_report_path": "scenario-report.json",
        "scenario_report_sha256": "2".repeat(64),
        "transition_id": "combat.end_turn.v1",
        "transition": {"transition_id": "combat.end_turn.v1"},
        "session": manifest,
        "root_frame_id": 12,
        "action": {
            "name": "capture_transition",
            "request_id": "55555555-5555-4555-8555-555555555555",
            "action_id": "end_turn",
            "based_on_frame_id": 12,
            "successor_frame_id": 13,
            "status": "completed",
            "settlement_wall_time_ms": 1,
            "parameters": {},
            "receipt_path": receipt_path,
            "receipt_sha256": sha256_hex(receipt_bytes),
            "successor_frame_path": "successor.json",
            "successor_frame_sha256": "3".repeat(64)
        },
        "checks": [],
        "successor": {
            "frame_id": 13,
            "frame_path": "successor.json",
            "frame_sha256": "3".repeat(64),
            "boundary": "combat_player_decision",
            "scenario_context": recipe,
            "observed_player": {},
            "observed_monsters": []
        }
    }))
    .unwrap();
    let probe_path = directory.join("probe-report.json");
    fs::write(&probe_path, serde_json::to_vec_pretty(&probe).unwrap()).unwrap();
    let workflow_path = directory.join("workflow.json");

    let workflow = write_constructed_comparison_workflow_new(
        &probe_path,
        &comparison_path,
        &workflow_path,
        &comparison,
    )
    .unwrap();

    assert!(workflow.passed);
    assert_eq!(workflow.case_id, "cultist-comparator-test");
    assert_eq!(workflow.from_frame_id, 12);
    assert_eq!(workflow.to_frame_id, 13);
    assert_eq!(workflow.launch_profile, "accelerated");
    assert!(!workflow.evidence_scope.natural_reachability);
    assert!(workflow.evidence_scope.simulator_comparison);
    assert!(!workflow.evidence_scope.strict_runtime_profile);
    assert!(workflow_path.is_file());

    fs::write(&probe_plan_path, b"{}\n").unwrap();
    let tampered_plan_workflow_path = directory.join("tampered-plan-workflow.json");
    let error = write_constructed_comparison_workflow_new(
        &probe_path,
        &comparison_path,
        &tampered_plan_workflow_path,
        &comparison,
    )
    .unwrap_err();
    assert!(
        error.contains("probe plan changed after capture"),
        "{error}"
    );
    assert!(!tampered_plan_workflow_path.exists());
    fs::write(&probe_plan_path, &probe_plan_bytes).unwrap();

    fs::write(&comparison_path, b"{}\n").unwrap();
    let rejected_workflow_path = directory.join("rejected-workflow.json");
    let error = write_constructed_comparison_workflow_new(
        &probe_path,
        &comparison_path,
        &rejected_workflow_path,
        &comparison,
    )
    .unwrap_err();
    assert!(error.contains("does not equal the comparison"), "{error}");
    assert!(!rejected_workflow_path.exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn relic_counter_projection_is_additive_for_immutable_legacy_frames() {
    let legacy: RelicIdentityProjection = serde_json::from_value(serde_json::json!({
        "index": 0,
        "relic_id": "Burning Blood"
    }))
    .unwrap();
    assert_eq!(legacy.counter, None);

    let current: RelicIdentityProjection = serde_json::from_value(serde_json::json!({
        "index": 0,
        "relic_id": "Letter Opener",
        "counter": 2
    }))
    .unwrap();
    assert_eq!(current.counter, Some(2));
}
