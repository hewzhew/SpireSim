use super::*;
use crate::observed_regression_fixtures::{load, observation};
use sts_parity_lockstep::{DecisionSurfaceV1, SemanticActionV1};

#[test]
fn recorded_repeated_electrodynamics_keeps_the_original_nonstacking_marker() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_defect_power_triggers", "electro-before.json");
    let after = observation_from("observed_defect_power_triggers", "electro-after.json");
    let action: crate::RecordedJavaActionV1 = serde_json::from_value(load_from(
        "observed_defect_power_triggers",
        "electro-action.json",
    ))
    .unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(matches!(action.action,
        SemanticActionV1::PlayCard { ref card_id, .. } if card_id == "electrodynamics"));
    for boundary in [&before, &after] {
        let powers = combat::project_powers_v4(
            &boundary.frame["parity_internal"]["player_powers"],
            "player",
        )
        .unwrap();
        let electro = powers.iter().find(|p| p.power_id == "electro").unwrap();
        assert_eq!(electro.amount, -1);
        assert_eq!(
            electro.state,
            sts_parity_lockstep::NaturalCombatPowerStateV4::Stateless
        );
    }
    use sts_oracle_run_control::{
        content::powers::{store, PowerId},
        engine::action_handlers::powers::handle_apply_power,
    };
    let mut rust = sts_oracle_run_control::test_support::blank_test_combat();
    handle_apply_power(0, 0, PowerId::Electro, -1, &mut rust);
    handle_apply_power(0, 0, PowerId::Electro, -1, &mut rust);
    assert_eq!(store::power_amount(&rust, 0, PowerId::Electro), -1);
}

#[test]
fn recorded_machine_learning_triggers_heatsink_and_keeps_the_persistent_draw_power() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_defect_power_triggers", "draw-before.json");
    let after = observation_from("observed_defect_power_triggers", "draw-after.json");
    let action: crate::RecordedJavaActionV1 = serde_json::from_value(load_from(
        "observed_defect_power_triggers",
        "draw-action.json",
    ))
    .unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(matches!(action.action,
        SemanticActionV1::PlayCard { ref card_id, .. } if card_id == "machinelearning"));
    let previous =
        combat::project_powers_v4(&before.frame["parity_internal"]["player_powers"], "player")
            .unwrap();
    let next =
        combat::project_powers_v4(&after.frame["parity_internal"]["player_powers"], "player")
            .unwrap();
    assert!(!previous.iter().any(|p| p.power_id == "draw"));
    for id in ["heatsink", "draw"] {
        let power = next.iter().find(|p| p.power_id == id).unwrap();
        assert_eq!(power.amount, 1);
        assert_eq!(
            power.state,
            sts_parity_lockstep::NaturalCombatPowerStateV4::Stateless
        );
    }
    let previous_zones = &before.frame["parity_internal"]["zones"];
    let next_zones = &after.frame["parity_internal"]["zones"];
    assert_eq!(previous_zones["hand"]["cards"].as_array().unwrap().len(), 2);
    assert_eq!(next_zones["hand"]["cards"].as_array().unwrap().len(), 2);
    assert_eq!(
        previous_zones["draw"]["cards"].as_array().unwrap().len(),
        next_zones["draw"]["cards"].as_array().unwrap().len() + 1
    );
}

#[test]
fn recorded_masked_bandits_constructs_hp_before_the_first_event_choice() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_masked_bandits", "before.json");
    let after = observation_from("observed_masked_bandits", "after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_masked_bandits", "action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(matches!(
        action.action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            ..
        }
    ));
    assert_eq!(room_base(&after).unwrap()["event_id"], "maskedbandits");
    assert_eq!(room_base(&after).unwrap()["screen"], 0);
    let calls = after.frame["rng_call_witness"]["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 19);
    assert_eq!(
        &calls[16..],
        &json!([
            {"owner":"floor.monster_hp", "owner_ordinal":0,
                "operation":{"method":"random_int_range","start":34,"end":34,"result":34}},
            {"owner":"floor.monster_hp", "owner_ordinal":1,
                "operation":{"method":"random_int_range","start":37,"end":41,"result":39}},
            {"owner":"floor.monster_hp", "owner_ordinal":2,
                "operation":{"method":"random_int_range","start":40,"end":44,"result":43}},
        ])
        .as_array()
        .unwrap()[..]
    );
}

#[test]
fn recorded_masked_bandits_fight_keeps_constructor_hp_and_original_pointy_identity() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_masked_bandits", "after.json");
    let after = observation_from("observed_masked_bandits", "fight-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_masked_bandits", "fight-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(matches!(action.action,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, ref choice }
            if choice.id == "maskedbandits" && choice.occurrence == 1));
    let monsters = after.frame["public_base"]["combat"]["monsters"]
        .as_array()
        .unwrap();
    assert_eq!(
        monsters
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["BanditChild", "BanditLeader", "BanditBear"]
    );
    assert_eq!(
        monsters
            .iter()
            .map(|m| m["current_hp"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [34, 39, 43]
    );
    assert_eq!(
        sts_java_parity_protocol::canonical_game_id(monsters[0]["id"].as_str().unwrap()),
        "banditpointy"
    );
    assert!(after.frame["rng_call_witness"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .all(|call| call["owner"] != "floor.monster_hp"));
}

#[test]
fn recorded_loop_repeats_only_the_first_orb_at_turn_start() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_loop", "before.json");
    let after = observation_from("observed_loop", "after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_loop", "action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert_eq!(action.action, SemanticActionV1::EndTurn);
    for boundary in [&before, &after] {
        let powers = combat::project_powers_v4(
            &boundary.frame["parity_internal"]["player_powers"],
            "player",
        )
        .unwrap();
        let loop_power = powers.iter().find(|p| p.power_id == "loop").unwrap();
        assert_eq!(loop_power.amount, 1);
        assert_eq!(
            loop_power.state,
            sts_parity_lockstep::NaturalCombatPowerStateV4::Stateless
        );
    }
    let previous_player = &before.frame["public_base"]["combat"]["player"];
    let next_player = &after.frame["public_base"]["combat"]["player"];
    let previous_orbs = &previous_player["character_state"]["orbs"];
    let next_orbs = &next_player["character_state"]["orbs"];
    assert_eq!(previous_orbs[0]["orb_id"], "Frost");
    assert_eq!(previous_orbs[1]["orb_id"], "Dark");
    assert_eq!(previous_orbs[2]["orb_id"], "Frost");
    assert_eq!(next_player["block"], 2);
    assert_eq!(previous_orbs[1]["evoke_amount"], 6);
    assert_eq!(next_orbs[1]["evoke_amount"], 12);
}

#[test]
fn recorded_ghosts_refusal_enters_its_distinct_original_result_page() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_ghosts", "before.json");
    let after = observation_from("observed_ghosts", "after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_ghosts", "action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(matches!(action.action,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, ref choice }
            if choice.id == "ghosts" && choice.occurrence == 1));
    assert_eq!(room_base(&before).unwrap()["screen"], 0);
    assert_eq!(room_base(&after).unwrap()["screen"], 2);
    assert_eq!(
        before.frame["public_base"]["run"],
        after.frame["public_base"]["run"]
    );
    assert!(crate::action::resolved_actions_v1(&after, None, None).unwrap().iter().any(|resolved|
        matches!(&resolved.semantic,
            SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, choice }
                if choice.id == "ghosts" && choice.occurrence == 0
                    && choice.attributes["screen"] == sts_parity_lockstep::SemanticScalarV1::Integer(2))));
}

#[test]
fn recorded_dead_adventurer_leave_enters_escape_instead_of_success() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_dead_adventurer", "before.json");
    let after = observation_from("observed_dead_adventurer", "after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_dead_adventurer", "action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert_eq!(room_base(&before).unwrap()["screen"], 0);
    assert_eq!(room_base(&after).unwrap()["screen"], 3);
    assert!(matches!(action.action,
        SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, ref choice }
            if choice.id == "deadadventurer" && choice.occurrence == 1));
    assert!(crate::action::resolved_actions_v1(&after, None, None).unwrap().iter().any(|resolved|
        matches!(&resolved.semantic,
            SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, choice }
                if choice.id == "deadadventurer" && choice.occurrence == 0
                    && choice.attributes["screen"] == sts_parity_lockstep::SemanticScalarV1::Integer(3))));
}

#[test]
fn recorded_genetic_algorithm_grows_master_and_battle_instances_before_the_next_decision() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_genetic_algorithm", "before.json");
    let after = observation_from("observed_genetic_algorithm", "after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_genetic_algorithm", "action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    let SemanticActionV1::PlayCard {
        card_binding,
        card_id,
        ..
    } = action.action
    else {
        panic!("recorded action must play Genetic Algorithm")
    };
    assert_eq!(card_id, "geneticalgorithm");
    let original = before.frame["parity_internal"]["master_deck"]
        .as_array()
        .unwrap()
        .iter()
        .find(|card| {
            serde_json::from_value::<NaturalRunCardSourceV1>(card["source"].clone())
                .ok()
                .and_then(|source| natural_run_card_binding_v1(&source).ok())
                .as_ref()
                == Some(&card_binding)
        })
        .unwrap();
    let uuid = original["card_uuid"].as_str().unwrap();
    let grown = after.frame["parity_internal"]["master_deck"]
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["card_uuid"] == uuid)
        .unwrap();
    assert_eq!(combat::persistent_card_misc(original).unwrap(), Some(1));
    assert_eq!(combat::persistent_card_misc(grown).unwrap(), Some(3));
    let exhausted = after.frame["parity_internal"]["zones"]["exhaust"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["instance_uuid"] == uuid)
        .unwrap();
    assert_eq!(combat::persistent_card_misc(exhausted).unwrap(), Some(3));
    assert_eq!(before.frame["public_base"]["combat"]["player"]["block"], 0);
    assert_eq!(after.frame["public_base"]["combat"]["player"]["block"], 1);
    assert_eq!(boundary_kind(&after).unwrap(), "natural_combat");
}

#[test]
fn recorded_manual_discard_does_not_discount_the_departing_eviscerate() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_discard_cost", "before.json");
    let after = observation_from("observed_discard_cost", "after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_discard_cost", "action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(
        matches!(action.action, SemanticActionV1::SubmitSelection { surface: DecisionSurfaceV1::HandSelect, ref choices }
        if choices.len() == 1 && choices[0].attributes["card_id"] == sts_parity_lockstep::SemanticScalarV1::Text("eviscerate".into()))
    );
    let hand = before.frame["parity_internal"]["zones"]["hand"]["cards"]
        .as_array()
        .unwrap();
    let eviscerate = hand.iter().find(|c| c["card_id"] == "Eviscerate").unwrap();
    let discarded = after.frame["parity_internal"]["zones"]["discard"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["instance_uuid"] == eviscerate["instance_uuid"])
        .unwrap();
    assert_eq!(eviscerate["cost_for_turn"], 3);
    assert_eq!(discarded["cost_for_turn"], 3);
    assert_eq!(
        before.frame["public_base"]["combat"]["player"]["cards_discarded_this_turn"],
        0
    );
    assert_eq!(
        after.frame["public_base"]["combat"]["player"]["cards_discarded_this_turn"],
        1
    );
}

#[test]
fn recorded_codex_is_a_skippable_combat_choice_with_a_source_witnessed_draw_copy() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_codex", "offers-before.json");
    let offers = observation_from("observed_codex", "offers-after.json");
    let selected = observation_from("observed_codex", "selected-after.json");
    let end: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_codex", "offers-action.json")).unwrap();
    let choose: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_codex", "selection-action.json")).unwrap();
    assert!(validate_adjacent(&before, &end, &offers));
    assert!(validate_adjacent(&offers, &choose, &selected));
    assert_eq!(end.action, SemanticActionV1::EndTurn);
    assert_eq!(boundary_kind(&offers).unwrap(), "natural_combat_codex");
    let decision = &offers.frame["public_base"]["decision"]["card_reward"];
    assert_eq!(decision["source_kind"], "codex");
    assert!(decision.get("discovery_context").is_none());
    let room = room_base(&offers).unwrap();
    assert_eq!(room["kind"], "combat_codex");
    assert_eq!(room["selection"]["can_skip"], true);
    assert_eq!(
        room["selection"]["cards"],
        json!([
            {"card_id":"backflip", "upgrades":0},
            {"card_id":"dodgeandroll", "upgrades":0},
            {"card_id":"grandfinale", "upgrades":0},
        ])
    );
    assert!(crate::action::resolved_actions_v1(&offers, None, None)
        .unwrap()
        .iter()
        .any(|a| matches!(
            &a.semantic,
            SemanticActionV1::Cancel {
                surface: DecisionSurfaceV1::CombatCodex
            }
        )));
    assert!(
        matches!(choose.action, SemanticActionV1::Select { surface: DecisionSurfaceV1::CombatCodex, ref choice }
        if choice.kind == "codex_card" && choice.id == "backflip")
    );
    let witness = &selected.frame["card_birth_witness"];
    assert_eq!(witness["cause"]["kind"], "combat_selection_resolution");
    assert_eq!(witness["from_frame_id"], offers.frame_id);
    assert_eq!(witness["to_frame_id"], selected.frame_id);
    assert_eq!(witness["complete"], true);
    let product = &witness["products"][0];
    assert_eq!(witness["products"].as_array().unwrap().len(), 1);
    assert_eq!(product["card_id"], "Backflip");
    assert_eq!(product["first_zone"], "draw");
    let card = ["draw", "hand", "discard", "exhaust", "limbo"]
        .into_iter()
        .flat_map(|z| {
            selected.frame["parity_internal"]["zones"][z]["cards"]
                .as_array()
                .unwrap()
        })
        .find(|c| c["instance_uuid"] == product["java_uuid"])
        .unwrap();
    assert_eq!(card["cost_for_turn"], 1);
    assert_eq!(card["combat_cost"], 1);
}

#[test]
fn recorded_codex_doppelganger_keeps_the_original_next_turn_draw_power_identity() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_codex", "draw-before.json");
    let after = observation_from("observed_codex", "draw-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_codex", "draw-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(
        matches!(action.action, SemanticActionV1::PlayCard { ref card_id, .. } if card_id == "doppelganger")
    );
    let powers =
        combat::project_powers_v4(&after.frame["parity_internal"]["player_powers"], "player")
            .unwrap();
    let draw = powers.iter().find(|p| p.power_id == "drawcard").unwrap();
    assert_eq!(draw.amount, 2);
    assert_eq!(
        draw.state,
        sts_parity_lockstep::NaturalCombatPowerStateV4::Stateless
    );
}

#[test]
fn recorded_cursed_tome_preserves_the_native_draw_and_reward_return_to_end_dialog() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_cursed_tome", "book-before.json");
    let after = observation_from("observed_cursed_tome", "book-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_cursed_tome", "book-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(
        matches!(action.action, SemanticActionV1::Select { surface: DecisionSurfaceV1::Event, ref choice }
        if choice.id == "cursedtome" && choice.occurrence == 0)
    );
    assert_eq!(before.frame["public_base"]["run"]["current_hp"], 53);
    assert_eq!(after.frame["public_base"]["run"]["current_hp"], 38);
    assert_eq!(
        after.frame["rng_call_witness"]["calls"],
        json!([{
            "owner": "floor.misc", "owner_ordinal": 0,
            "operation": {"method": "random_int", "range": 2, "result": 2}
        }])
    );
    let projected = rewards::project_rewards_v1(&after, "natural_combat_reward").unwrap();
    let sts_parity_lockstep::NaturalRunRewardsV2::Combat { items, .. } = projected else {
        panic!("book is delivered on the real reward screen");
    };
    assert_eq!(
        items,
        vec![sts_parity_lockstep::NaturalRunRewardItemV1::Relic {
            relic_id: "nilryscodex".into()
        }]
    );
    let return_before = observation_from("observed_cursed_tome", "return-before.json");
    let returned = observation_from("observed_cursed_tome", "return-after.json");
    let close: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_cursed_tome", "return-action.json")).unwrap();
    assert!(validate_adjacent(&return_before, &close, &returned));
    assert_eq!(
        close.action,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward
        }
    );
    assert_eq!(boundary_kind(&returned).unwrap(), "natural_event");
    assert_eq!(
        returned.frame["public_base"]["decision"]["event_screen_key"],
        "END"
    );
    assert_eq!(returned.frame["public_base"]["run"]["current_hp"], 38);
    assert_eq!(
        returned.frame["parity_internal"]["relics"],
        return_before.frame["parity_internal"]["relics"]
    );
    assert_eq!(returned.frame["rng_call_witness"]["calls"], json!([]));
}

#[test]
fn recorded_codex_choke_preserves_the_positive_debuff_and_rejects_a_false_class() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_codex", "choke-before.json");
    let after = observation_from("observed_codex", "choke-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_codex", "choke-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(
        matches!(action.action, SemanticActionV1::PlayCard { ref card_id, .. } if card_id == "choke")
    );
    let creatures = combat::project_monster_identities_v4(&after).unwrap();
    let raw = &after.frame["parity_internal"]["monsters"][0]["powers"];
    let powers = combat::project_powers_with_sources_v4(raw, "monster", &creatures).unwrap();
    let choked = powers.iter().find(|p| p.power_id == "choked").unwrap();
    assert_eq!(choked.amount, 3);
    assert_eq!(
        choked.state,
        sts_parity_lockstep::NaturalCombatPowerStateV4::Stateless
    );
    let mut false_class = raw.clone();
    false_class
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "Choked")
        .unwrap()["class_name"] = json!("modded.ChokePower");
    assert!(combat::project_powers_with_sources_v4(&false_class, "monster", &creatures).is_err());
}

#[test]
fn recorded_boss_chest_entry_pays_maw_bank_before_opening_the_chest() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_room_entry", "boss-chest-before.json");
    let entered = observation_from("observed_room_entry", "boss-chest-after.json");
    let opened = observation_from("observed_room_entry", "boss-chest-open-after.json");
    let entry: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_room_entry", "boss-chest-action.json")).unwrap();
    let open: crate::RecordedJavaActionV1 = serde_json::from_value(load_from(
        "observed_room_entry",
        "boss-chest-open-action.json",
    ))
    .unwrap();
    assert!(validate_adjacent(&before, &entry, &entered));
    assert!(
        crate::observed_regression_fixtures::validate_adjacent_phase(
            &entered,
            &open,
            &opened,
            crate::RecordedJavaActionPhaseV1::InterstitialSettlement,
        )
    );
    assert_eq!(
        entry.action,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward
        }
    );
    assert_eq!(before.frame["public_base"]["run"]["floor"], 16);
    assert_eq!(entered.frame["public_base"]["run"]["floor"], 17);
    assert_eq!(before.frame["public_base"]["run"]["gold"], 176);
    for observation in [&entered, &opened] {
        assert_eq!(observation.frame["public_base"]["run"]["gold"], 188);
        let bank = observation.frame["parity_internal"]["relics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|relic| relic["relic_id"] == "MawBank")
            .unwrap();
        assert_eq!(bank["used_up"], false);
    }
}

#[test]
fn recorded_nob_anger_precedes_survivor_discard_selection() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_nob", "anger-before.json");
    let after = observation_from("observed_nob", "anger-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_nob", "anger-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(
        matches!(action.action, SemanticActionV1::PlayCard { ref card_id, .. } if card_id == "survivor")
    );
    assert_eq!(boundary_kind(&after).unwrap(), "natural_hand_select");
    for (observation, expected) in [(&before, 6), (&after, 9)] {
        let powers = combat::project_powers_v4(
            &observation.frame["parity_internal"]["monsters"][0]["powers"],
            "monster",
        )
        .unwrap();
        assert_eq!(
            powers
                .iter()
                .find(|p| p.power_id == "strength")
                .unwrap()
                .amount,
            expected
        );
        assert_eq!(
            powers
                .iter()
                .find(|p| p.power_id == "anger")
                .unwrap()
                .amount,
            3
        );
    }
}

#[test]
fn recorded_lagavulin_wakes_before_dagger_throw_discard_resolution() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_lagavulin", "wake-before.json");
    let after = observation_from("observed_lagavulin", "wake-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_lagavulin", "wake-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(
        matches!(action.action, SemanticActionV1::PlayCard { ref card_id, .. } if card_id == "daggerthrow")
    );
    assert_eq!(boundary_kind(&after).unwrap(), "natural_hand_select");
    let previous = &before.frame["parity_internal"]["monsters"][0];
    let current = &after.frame["parity_internal"]["monsters"][0];
    assert_eq!(previous["move_history"], json!([5, 5, 5, 5, 5]));
    assert_eq!(current["move_history"], json!([5, 5, 5, 5, 5, 4]));
    assert_eq!(current["next_move_id"], 4);
    assert_eq!(current["intent"], "STUN");
    assert_eq!(current["powers"], previous["powers"]);
    assert_eq!(current["powers"][0]["id"], "Metallicize");
    assert_eq!(current["powers"][0]["amount"], 8);
}

#[test]
fn recorded_player_ritual_retains_its_constructor_skip_flag() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    use sts_parity_lockstep::NaturalCombatPowerStateV4;
    let before = observation_from("observed_ritual", "cultist-before.json");
    let after = observation_from("observed_ritual", "cultist-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_ritual", "cultist-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert!(
        matches!(action.action, SemanticActionV1::UsePotion { slot: 0, ref potion_id, target: None } if potion_id == "cultistpotion")
    );
    let powers =
        combat::project_powers_v4(&after.frame["parity_internal"]["player_powers"], "player")
            .unwrap();
    let ritual = powers
        .iter()
        .find(|power| power.power_id == "ritual")
        .unwrap();
    assert_eq!(ritual.amount, 1);
    assert_eq!(
        ritual.state,
        NaturalCombatPowerStateV4::Ritual {
            skip_first: true,
            player_controlled: true,
        }
    );
    // The player end-turn branch ignores skipFirst; it does not clear it.
    let extra = sts_oracle_run_control::content::powers::core::ritual::infer_extra_data(0, false);
    let skip = sts_oracle_run_control::content::powers::core::ritual::extra_data(false, true);
    assert_ne!(
        extra & skip,
        0,
        "Rust must preserve the observed constructor state"
    );
}

#[test]
fn recorded_neow_reward_close_preserves_rewards_under_the_entrance_map() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_neow", "neow-map-before.json");
    let after = observation_from("observed_neow", "neow-map-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_neow", "neow-map-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert_eq!(
        action.action,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward
        }
    );
    assert_eq!(boundary_kind(&after).unwrap(), "natural_map");
    assert_eq!(current_map_room(&after).unwrap(), "neow");
    assert_eq!(
        room(&after).unwrap(),
        json!({"kind":"map_preview", "current":null})
    );
    let sts_parity_lockstep::NaturalRunRewardsV2::Combat { items, skippable } =
        rewards::project_rewards_v1(&after, "natural_map").unwrap()
    else {
        panic!("the map overlay must preserve its underlying reward screen")
    };
    assert!(items.is_empty());
    assert!(skippable);
    assert_eq!(public_map(&after).unwrap()["current"], Value::Null);
}

#[test]
fn recorded_neow_potion_grant_is_a_reward_without_a_map_node() {
    use crate::observed_regression_fixtures::{load_from, observation_from, validate_adjacent};
    let before = observation_from("observed_neow", "neow-potions-before.json");
    let after = observation_from("observed_neow", "neow-potions-after.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load_from("observed_neow", "neow-potions-action.json")).unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert_eq!(current_map_room(&after).unwrap(), "neow");
    assert_eq!(boundary_kind(&after).unwrap(), "natural_combat_reward");
    assert_eq!(
        room(&after).unwrap(),
        json!({"kind": "combat_reward", "current": null})
    );
    assert_eq!(public_map(&after).unwrap()["current"], Value::Null);
    let projected = rewards::project_rewards_v1(&after, "natural_combat_reward").unwrap();
    let sts_parity_lockstep::NaturalRunRewardsV2::Combat { items, .. } = projected else {
        panic!("expected potion rewards")
    };
    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|item| matches!(
        item,
        sts_parity_lockstep::NaturalRunRewardItemV1::Potion { .. }
    )));
    // Replay the real natural choices too: equal visible potions alone must
    // not conceal the card reward that Java generates and then removes.
    use sts_oracle_run_control::eval::run_control::{RunControlConfig, RunControlSession};
    use sts_parity_lockstep::{
        compare_natural_run_rng_calls_v1, compare_snapshots_v1, ComparisonStatusV1,
        LockstepAdapterV1, NaturalRunRngCallLedgerV1,
    };
    use sts_parity_lockstep_rust::{natural_run_bootstrap_v1, RustLockstepAdapterV1};
    let config = RunControlConfig {
        seed: 2026101821,
        player_class: "Silent",
        ascension_level: 20,
        final_act: true,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
    let mut rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap.clone(),
        "test",
        "neow-potions",
    )
    .unwrap();
    let mut actual = rust.observe().unwrap();
    for selected in [
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::Neow,
        },
        action.action,
    ] {
        let prepared = rust.prepare(&selected, &actual).unwrap();
        actual = rust.execute(prepared).unwrap();
    }
    let expected =
        project_snapshot_v1(&after, &bootstrap, &BTreeMap::new(), &BTreeMap::new(), None)
            .unwrap()
            .snapshot;
    let comparison = compare_snapshots_v1(&expected, &actual).unwrap();
    let differences = comparison
        .domains
        .iter()
        .filter_map(|domain| domain.reason.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(
        comparison.status,
        ComparisonStatusV1::Exact,
        "{differences:?}"
    );
    let java_calls = NaturalRunRngCallLedgerV1::new(
        serde_json::from_value(after.frame["rng_call_witness"]["calls"].clone()).unwrap(),
    )
    .unwrap();
    assert!(
        compare_natural_run_rng_calls_v1(&java_calls, rust.last_rng_call_ledger_v1().unwrap())
            .unwrap()
            .stop_reason()
            .is_none()
    );
    assert_eq!(
        rng::project_rng_streams_v1(
            &after,
            "natural_combat_reward",
            sts_parity_lockstep::NaturalRunNeowEligibilityV1::Rich
        )
        .unwrap()
        .unwrap()
        .as_array()
        .unwrap()
        .len(),
        13
    );
    for (act, floor, x, y) in [(2, 0, -1, -1), (1, 1, -1, -1), (1, 0, -2, -1)] {
        let mut invalid = after.clone();
        invalid.frame["public_base"]["run"]["act"] = json!(act);
        invalid.frame["public_base"]["run"]["floor"] = json!(floor);
        invalid.frame["public_base"]["map"]["current_x"] = json!(x);
        invalid.frame["public_base"]["map"]["current_y"] = json!(y);
        assert!(current_map_room(&invalid).is_err());
    }
}

#[test]
fn shrine_pool_retires_only_at_standard_act_exits() {
    let mut frame = observation("heart-entrance.json").frame;
    let shrines = json!(["Upgrade Shrine", "Wheel of Change"]);
    frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] = shrines;
    let baseline = run::project_run_v1(&frame).unwrap().unwrap();
    for act in 1..=4 {
        for (kind, event) in [
            ("boss_relic", ""),
            ("treasure", ""),
            ("map", "SpireHeart"),
            ("event", "SpireHeart"),
            ("event", "Upgrade Shrine"),
            ("event", ""),
            ("combat_reward", ""),
        ] {
            for daily in [false, true] {
                let mut candidate = frame.clone();
                candidate["public_base"]["run"]["act"] = json!(act);
                candidate["public_base"]["decision"]["kind"] = json!(kind);
                candidate["public_base"]["decision"]["event_id"] = json!(event);
                candidate["parity_internal"]["run_state"]["daily"] = json!(daily);
                let actual = run::project_run_v1(&candidate).unwrap().unwrap();
                let retired = !daily
                    && ((act <= 2 && kind == "boss_relic")
                        || (act == 3 && kind == "event" && event == "SpireHeart"));
                assert_eq!(
                    actual["event_schedule"]["shrines"]
                        .as_array()
                        .unwrap()
                        .is_empty(),
                    retired
                );
                if !retired {
                    assert_eq!(
                        actual["event_schedule"]["shrines"],
                        baseline["event_schedule"]["shrines"]
                    );
                }
                for field in ["events", "one_time_events", "chance_bits"] {
                    assert_eq!(
                        actual["event_schedule"][field],
                        baseline["event_schedule"][field]
                    );
                }
            }
        }
    }
    // Retirement is not permission to omit or malform the raw observation.
    for (act, kind) in [(1, "boss_relic"), (3, "event")] {
        frame["public_base"]["run"]["act"] = json!(act);
        frame["public_base"]["decision"]["kind"] = json!(kind);
        frame["public_base"]["decision"]["event_id"] = json!("SpireHeart");
        for malformed in [
            json!(["Upgrade Shrine", "Upgrade Shrine"]),
            json!([""]),
            json!(null),
        ] {
            frame["parity_internal"]["run_state"]["event_schedule"]["shrines"] = malformed;
            assert!(run::project_run_v1(&frame).is_err());
        }
    }
}

#[test]
fn recorded_ending_boss_sentinel_resolves_to_one_shared_room_identity() {
    let observed = observation("heart-entrance.json");
    let raw_map = &observed.frame["public_base"]["map"];
    assert_eq!(
        (raw_map["current_x"].as_i64(), raw_map["current_y"].as_i64()),
        (Some(-1), Some(15))
    );
    assert_eq!(boundary_kind(&observed).unwrap(), "natural_combat");
    assert_eq!(current_node(&observed).unwrap().as_deref(), Some("F04X03"));
    assert_eq!(public_map(&observed).unwrap()["current"], "F04X03");
    assert_eq!(room(&observed).unwrap()["current"], "F04X03");
    assert_eq!(current_map_room(&observed).unwrap(), "boss");
}

#[test]
fn recorded_heart_proceed_connects_the_choice_to_a_distinct_terminal_room() {
    let before = observation("heart-proceed.json");
    let after = observation("heart-terminal.json");
    let action: crate::RecordedJavaActionV1 =
        serde_json::from_value(load("heart-proceed-action.json")).unwrap();
    action.validate().unwrap();
    assert_eq!(before.producer, after.producer);
    assert_eq!(action.phase, crate::RecordedJavaActionPhaseV1::Committed);
    assert_eq!(action.receipt.based_on_frame_id, before.frame_id);
    assert_eq!(action.receipt.successor_frame_id, Some(after.frame_id));
    assert_eq!(
        action.action,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::TrueVictory
        }
    );
    let choices = resolved_actions_v1(&before, None, None).unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].semantic, action.action);
    assert_eq!(
        boundary_kind(&before).unwrap(),
        "natural_true_victory_complete"
    );
    assert_eq!(boundary_kind(&after).unwrap(), "natural_heart_victory");
    assert_eq!(room(&before).unwrap()["current"], "F04X03");
    assert_eq!(room(&after).unwrap()["current"], "F05X03");
    let map = public_map(&after).unwrap();
    assert!(map["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["id"] == "F05X03"
            && node["room"] == "victory"
            && node["links"] == json!([])));
    // Managed host reuse is present in the original terminal frame, while
    // the gameplay adapter correctly exposes no further player choices.
    assert_eq!(after.actions.len(), 1);
    assert_eq!(after.actions[0].kind, "run_discard_to_main_menu");
    assert!(resolved_actions_v1(&after, None, None).unwrap().is_empty());
}
