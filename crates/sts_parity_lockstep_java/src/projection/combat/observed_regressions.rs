use super::*;
use crate::observed_regression_fixtures::observation;

mod constricted;
mod corruption;
mod exhaust_selection;
mod headbutt;
mod orange_pellets;
mod rampage;
mod reactive;

#[test]
fn character_state_requires_original_samples_and_preserves_orb_order() {
    let mut frame = observation("reduced-flight.json");
    frame.frame["public_base"]["run"]["character"] = "DEFECT".into();
    let player = &mut frame.frame["public_base"]["combat"]["player"];
    player["cards_discarded_this_turn"] = 0.into();
    assert!(project_player_v4(&frame, false).is_err());
    let state = serde_json::json!({"stance":"Neutral", "max_orbs":2,"orbs":[
        {"orb_id":"Dark","base_passive_amount":6,"base_evoke_amount":6,"passive_amount":9,"evoke_amount":42},
        {"orb_id":"Empty","base_passive_amount":0,"base_evoke_amount":0,"passive_amount":0,"evoke_amount":0}
    ]});
    frame.frame["public_base"]["combat"]["player"]["character_state"] = state;
    let projected = project_player_v4(&frame, false).unwrap();
    let actual = projected.character_state.as_ref().unwrap();
    assert_eq!(actual.stance, "neutral");
    assert_eq!(actual.orbs[0].evoke_amount, 42);
    assert_eq!(actual.orbs[1].orb_id, "empty");
    frame.frame["public_base"]["combat"]["player"]["character_state"]["orbs"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert_ne!(project_player_v4(&frame, false).unwrap(), projected);
    frame.frame["public_base"]["combat"]["player"]["character_state"]["max_orbs"] = 1.into();
    assert!(project_player_v4(&frame, false).is_err());
    frame.frame["public_base"]["combat"]["player"]["character_state"] =
        serde_json::json!({"stance":"Wrath","max_orbs":0,"orbs":[]});
    frame.frame["public_base"]["run"]["character"] = "WATCHER".into();
    frame.frame["public_base"]["combat"]["player"]["mantra_gained_this_combat"] = 0.into();
    assert_eq!(
        project_player_v4(&frame, false)
            .unwrap()
            .character_state
            .unwrap()
            .stance,
        "wrath"
    );
}

#[test]
fn single_exhaust_projects_the_suspended_card_and_requires_complete_zones() {
    use crate::action::JavaHandSelectProfileV1::SingleExhaust;

    // Projection-only construction, not a recorded exhaust transition. The
    // existing frame supplies creatures; the card has an explicit test source.
    let mut frame = observation("reduced-flight.json");
    for zone in ["draw", "hand", "discard", "exhaust", "limbo"] {
        frame.frame["parity_internal"]["zones"][zone]["cards"] = serde_json::json!([]);
    }
    let card = serde_json::json!({
        "instance_uuid": "test-played-card", "card_id": "Burning Pact",
        "upgrades": 0, "combat_cost": 1, "cost_for_turn": 1
    });
    frame.frame["parity_internal"]["zones"]["card_in_use"] = card.clone();
    let bindings = BTreeMap::from([("test-played-card".into(), "constructed/played".into())]);
    let project = |frame: &JavaRuntimeObservationV1| {
        project_combat_v4(
            frame,
            Some(&bindings),
            "natural_hand_select",
            Some(SingleExhaust),
        )
    };
    let projected = project(&frame).unwrap();
    let NaturalRunCombatV4::PlayerDecision { state } = &projected else {
        panic!("single exhaust must retain the active combat");
    };
    assert_eq!(state.zones.limbo.len(), 1);
    assert_eq!(state.zones.limbo[0].binding, "constructed/played");
    assert_eq!(state.zones.limbo[0].card_id, "burningpact");
    assert!(!state.monsters.is_empty());

    frame.frame["parity_internal"]["zones"]["limbo"]["cards"] = serde_json::json!([card]);
    assert_eq!(project(&frame).unwrap(), projected);
    frame.frame["parity_internal"]["zones"]["limbo"]["cards"][0]["cost_for_turn"] = 0.into();
    assert!(project(&frame).is_err());
    frame.frame["parity_internal"]["zones"]["limbo"]["cards"] = serde_json::json!([]);
    frame.frame["parity_internal"]["zones"]["card_in_use"] = Value::Null;
    assert!(project(&frame).is_err());
    frame.frame["parity_internal"]["zones"]
        .as_object_mut()
        .unwrap()
        .remove("card_in_use");
    assert!(project(&frame).is_err());
}

#[test]
fn discard_grid_distinguishes_potion_and_card_play_in_flight() {
    // Construct a projection-only case from an existing frame. Empty card zones
    // isolate the selection lifecycle from source binding; this is not an oracle
    // recording of a natural Liquid Memories transition.
    let mut frame = observation("reduced-flight.json");
    for zone in ["draw", "hand", "discard", "exhaust", "limbo"] {
        frame.frame["parity_internal"]["zones"][zone]["cards"] = serde_json::json!([]);
    }
    frame.frame["parity_internal"]["zones"]
        .as_object_mut()
        .unwrap()
        .remove("card_in_use");
    let bindings = BTreeMap::new();
    let before = project_combat_v4(&frame, Some(&bindings), "natural_combat", None).unwrap();
    frame.frame["boundary"]["kind"] = serde_json::json!("run_decision");
    frame.frame["public_base"]["decision"] = serde_json::json!({
        "kind": "grid_select", "grid_select": {
            "source_pile": "Discard", "selection_reason": "DiscardToHand"
        }
    });
    let grid =
        project_combat_v4(&frame, Some(&bindings), "natural_combat_grid_select", None).unwrap();
    assert_eq!(before, grid);
    frame.frame["public_base"]["decision"]["grid_select"] = serde_json::json!({
        "source_pile": "Draw", "selection_reason": "SkillFromDeckToHand"
    });
    assert!(
        project_combat_v4(&frame, Some(&bindings), "natural_combat_grid_select", None).is_err()
    );
    frame.frame["public_base"]["decision"]["grid_select"] = serde_json::json!({
        "source_pile": "Discard", "selection_reason": "DiscardToHandNoCostChange"
    });
    assert!(
        project_combat_v4(&frame, Some(&bindings), "natural_combat_grid_select", None).is_err()
    );
    frame.frame["parity_internal"]["zones"]["card_in_use"] = serde_json::json!({
        "instance_uuid":"test-hologram", "card_id":"Hologram",
        "upgrades":0, "combat_cost":1, "cost_for_turn":1
    });
    let bindings = BTreeMap::from([("test-hologram".into(), "constructed/hologram".into())]);
    let projected =
        project_combat_v4(&frame, Some(&bindings), "natural_combat_grid_select", None).unwrap();
    let NaturalRunCombatV4::PlayerDecision { state } = projected else {
        panic!("Hologram selection must retain the active combat");
    };
    assert_eq!(state.zones.limbo.len(), 1);
    assert_eq!(state.zones.limbo[0].card_id, "hologram");
}

#[test]
fn recorded_flight_keeps_the_recovery_amount_after_multiple_hits() {
    let observed = observation("reduced-flight.json");
    let raw = &observed.frame["parity_internal"]["monsters"][0]["powers"];
    let projected = project_powers_v4(raw, "Byrd").unwrap();
    let flight = projected.iter().find(|p| p.power_id == "flight").unwrap();
    assert_eq!(flight.amount, 2);
    assert_eq!(
        flight.state,
        NaturalCombatPowerStateV4::Flight { stored_amount: 4 }
    );

    // This diagnostic copy is deliberately incomplete; the retained raw file
    // is unchanged. Falling back to current stacks must never make it pass.
    let mut missing = raw.clone();
    let power = missing
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "Flight")
        .unwrap();
    power
        .as_object_mut()
        .unwrap()
        .remove("flight_stored_amount");
    assert!(project_powers_v4(&missing, "Byrd").is_err());
}

#[test]
fn elixir_keeps_active_combat_without_requiring_a_suspended_card() {
    let mut frame = observation("reduced-flight.json");
    for zone in ["draw", "hand", "discard", "exhaust", "limbo"] {
        frame.frame["parity_internal"]["zones"][zone]["cards"] = serde_json::json!([]);
    }
    frame.frame["parity_internal"]["zones"]["card_in_use"] = Value::Null;
    let bindings = BTreeMap::new();
    let before = project_combat_v4(&frame, Some(&bindings), "natural_combat", None).unwrap();
    let pending = project_combat_v4(
        &frame,
        Some(&bindings),
        "natural_hand_select",
        Some(crate::action::JavaHandSelectProfileV1::PotionExhaust),
    )
    .unwrap();
    assert_eq!(pending, before);
    frame.frame["public_base"]["combat"]["player"]
        .as_object_mut()
        .unwrap()
        .remove("energy");
    assert!(project_combat_v4(
        &frame,
        Some(&bindings),
        "natural_hand_select",
        Some(crate::action::JavaHandSelectProfileV1::PotionExhaust)
    )
    .is_err());
}

#[test]
fn recorded_heart_keeps_invincible_when_its_current_budget_reaches_zero() {
    let observed = observation("exhausted-invincible.json");
    let raw = &observed.frame["parity_internal"]["monsters"][0]["powers"];
    let powers = project_powers_v4(raw, "Heart").unwrap();
    let cap = powers.iter().find(|p| p.power_id == "invincible").unwrap();
    assert_eq!(cap.amount, 0);
    assert_eq!(
        cap.state,
        NaturalCombatPowerStateV4::Invincible { max_amount: 200 }
    );
    let mut missing = raw.clone();
    let power = missing
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "Invincible")
        .unwrap();
    power
        .as_object_mut()
        .unwrap()
        .remove("invincible_max_amount");
    assert!(project_powers_v4(&missing, "Heart").is_err());
}
