//! A constructed same-instance replay checked against raw adjacent Java
//! boundaries. Java UUID continuity never binds a Rust scenario lineage.
use super::super::*;
use crate::{
    observed_regression_fixtures::{load_from, observation_from, validate_adjacent},
    RecordedJavaActionV1,
};
use sts_oracle_run_control::{
    content::{
        cards::{get_card_definition, CardId},
        monsters::EnemyId,
        powers::{store, PowerId},
    },
    engine::core::tick_until_stable_turn,
    runtime::{
        combat::{CombatCard, Power, PowerPayload},
        rng::begin_sts_rng_trace,
    },
    state::core::{ClientInput, EngineState},
    test_support::{combat_with_monsters, test_monster},
};
use sts_parity_lockstep::{
    NaturalRunNeowEligibilityV1, NaturalRunRngCallLedgerV1, SemanticActionV1,
};

struct RampageFacts {
    java_uuid: String,
    pile: &'static str,
    upgrades: u8,
    base_damage: i32,
    combat_cost: i32,
    turn_cost: i32,
}

fn java_rampage(raw: &JavaRuntimeObservationV1) -> Result<RampageFacts, String> {
    let mut found = Vec::new();
    for pile in ["draw", "hand", "discard", "exhaust", "limbo"] {
        let cards = raw.frame["parity_internal"]["zones"][pile]["cards"]
            .as_array()
            .ok_or("Rampage evidence omitted a card zone")?;
        for card in cards {
            if canonical_game_id(required_str(card, "card_id")?) == "rampage" {
                found.push((pile, card));
            }
        }
    }
    let [(pile, card)] = found.as_slice() else {
        return Err("this scoped Rampage fixture requires exactly one observed instance".into());
    };
    Ok(RampageFacts {
        java_uuid: required_str(card, "instance_uuid")?.to_owned(),
        pile,
        upgrades: required_u64(card, "upgrades")?
            .try_into()
            .map_err(|_| "upgrades exceed u8")?,
        // Use the same persistent-damage reader as the current Java adapter.
        base_damage: persistent_card_damage(card)?.ok_or("Rampage omitted persistent damage")?,
        combat_cost: required_i32(card, "combat_cost")?,
        turn_cost: required_i32(card, "cost_for_turn")?,
    })
}

fn power(power_type: PowerId, amount: i32) -> Power {
    Power {
        power_type,
        amount,
        instance_id: None,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    }
}

#[test]
fn recorded_rampage_replay_uses_growth_before_its_second_damage() {
    let before = observation_from("rampage_replay", "before.json");
    let after = observation_from("rampage_replay", "after.json");
    let play: RecordedJavaActionV1 =
        serde_json::from_value(load_from("rampage_replay", "play.json")).unwrap();
    assert!(validate_adjacent(&before, &play, &after));
    assert!(
        matches!(&play.action, SemanticActionV1::PlayCard { card_id, target: Some(target), .. }
        if card_id == "rampage" && target.kind == "monster" && target.id == "slaverblue" && target.occurrence == 0)
    );
    for raw in [&before, &after] {
        assert_eq!(raw.frame["boundary"]["kind"], "combat_player_decision");
    }
    let start = java_rampage(&before).unwrap();
    let end = java_rampage(&after).unwrap();
    assert_eq!(
        start.java_uuid, end.java_uuid,
        "Java-local object continuity"
    );
    assert_eq!((start.pile, end.pile), ("hand", "discard"));
    assert_eq!((start.base_damage, end.base_damage), (8, 18));
    assert_eq!((start.upgrades, end.upgrades), (0, 0));
    let player_before = &before.frame["public_base"]["combat"]["player"];
    let player_after = &after.frame["public_base"]["combat"]["player"];
    let powers =
        project_powers_v4(&before.frame["parity_internal"]["player_powers"], "player").unwrap();
    assert_eq!(powers.len(), 1);
    assert_eq!(
        (powers[0].power_id.as_str(), powers[0].amount),
        ("doubletap", 1)
    );
    assert!(
        project_powers_v4(&after.frame["parity_internal"]["player_powers"], "player")
            .unwrap()
            .is_empty()
    );
    let target = |raw: &JavaRuntimeObservationV1| {
        project_monsters_v4(raw, false)
            .unwrap()
            .into_iter()
            .find(|monster| monster.enemy_id == "slaverblue" && monster.occurrence == 0)
            .unwrap()
    };
    let target_before = target(&before);
    let target_after = target(&after);
    assert_eq!(
        (target_before.current_hp, target_after.current_hp),
        (52, 31)
    );
    assert_eq!(target_before.powers.len(), 1);
    assert_eq!(
        (
            target_before.powers[0].power_id.as_str(),
            target_before.powers[0].amount
        ),
        ("strength", -2)
    );
    assert_eq!(target_before.powers, target_after.powers);

    // Explicit mechanism input. Neither this lineage nor local target id 7
    // stands for a captured Java runtime ID or a natural acquisition binding.
    let mut monster = test_monster(EnemyId::SlaverBlue);
    monster.id = 7;
    monster.current_hp = target_before.current_hp;
    monster.max_hp = target_before.max_hp;
    monster.block = target_before.block;
    let mut state = combat_with_monsters(vec![monster]);
    state.entities.player.current_hp = required_i32(player_before, "current_hp").unwrap();
    state.entities.player.max_hp = required_i32(player_before, "max_hp").unwrap();
    state.entities.player.block = required_i32(player_before, "block").unwrap();
    state.turn.energy = required_i32(player_before, "energy")
        .unwrap()
        .try_into()
        .unwrap();
    store::set_powers_for(
        &mut state,
        0,
        vec![power(PowerId::DoubleTap, powers[0].amount)],
    );
    store::set_powers_for(
        &mut state,
        7,
        vec![power(PowerId::Strength, target_before.powers[0].amount)],
    );
    let card = CombatCard::for_scenario(CardId::Rampage, 0);
    assert_eq!(get_card_definition(card.id).base_damage, start.base_damage);
    assert_eq!(card.upgrades, start.upgrades);
    assert_eq!(
        card.combat_cost_without_turn_override_java(),
        start.combat_cost
    );
    assert_eq!(card.cost_for_turn_java(), start.turn_cost);
    let scenario_lineage = card.lineage;
    state.zones.hand = vec![card];
    let rng_before = state.rng.pool.clone();
    let trace = begin_sts_rng_trace().unwrap();
    let mut engine = EngineState::CombatPlayerTurn;
    assert!(tick_until_stable_turn(
        &mut engine,
        &mut state,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(7)
        }
    ));
    let calls = trace.finish().unwrap();
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(
        state.entities.monsters[0].current_hp,
        target_after.current_hp
    );
    assert_eq!(state.entities.monsters[0].block, target_after.block);
    assert_eq!(
        state.entities.player.current_hp,
        required_i32(player_after, "current_hp").unwrap()
    );
    assert_eq!(
        state.entities.player.block,
        required_i32(player_after, "block").unwrap()
    );
    assert_eq!(
        i32::from(state.turn.energy),
        required_i32(player_after, "energy").unwrap()
    );
    assert!(!store::has_power(&state, 0, PowerId::DoubleTap));
    assert!(state.zones.hand.is_empty() && state.zones.limbo.is_empty());
    assert!(state.zones.draw_pile.is_empty() && state.zones.exhaust_pile.is_empty());
    assert!(state.zones.queued_cards.is_empty() && !state.has_pending_actions());
    assert_eq!(
        state.zones.discard_pile.len(),
        1,
        "replay copies must be purged"
    );
    let played = &state.zones.discard_pile[0];
    assert_eq!(
        played.lineage, scenario_lineage,
        "Rust-local instance continuity"
    );
    assert_eq!(played.id, CardId::Rampage);
    assert_eq!(played.upgrades, end.upgrades);
    assert_eq!(played.base_damage_override, Some(end.base_damage));
    assert_eq!(
        played.combat_cost_without_turn_override_java(),
        end.combat_cost
    );
    assert_eq!(played.cost_for_turn_java(), end.turn_cost);

    // No calls and unchanged streams on each endpoint. Their constructed
    // initial RNG states are not asserted to be equal across runtimes.
    let ledger = NaturalRunRngCallLedgerV1::new(
        serde_json::from_value(after.frame["rng_call_witness"]["calls"].clone()).unwrap(),
    )
    .unwrap();
    assert!(ledger.0.is_empty());
    assert!(calls.is_empty());
    assert_eq!(state.rng.pool, rng_before);
    let java_rng = |raw: &JavaRuntimeObservationV1| {
        crate::projection::rng::project_rng_streams_v1(
            raw,
            "natural_combat",
            NaturalRunNeowEligibilityV1::Rich,
        )
        .unwrap()
        .expect("recorded RNG streams")
    };
    assert_eq!(java_rng(&before), java_rng(&after));
}

#[test]
fn recorded_rampage_requires_observed_growth_and_unambiguous_instance() {
    let after = observation_from("rampage_replay", "after.json");
    let mut missing = after.clone();
    let cards = missing.frame["parity_internal"]["zones"]["discard"]["cards"]
        .as_array_mut()
        .unwrap();
    let card = cards
        .iter_mut()
        .find(|card| card["card_id"] == "Rampage")
        .unwrap();
    card.as_object_mut().unwrap().remove("base_damage");
    assert!(java_rampage(&missing).is_err());
    let mut ambiguous = after.clone();
    let cards = ambiguous.frame["parity_internal"]["zones"]["discard"]["cards"]
        .as_array_mut()
        .unwrap();
    let copy = cards
        .iter()
        .find(|card| card["card_id"] == "Rampage")
        .unwrap()
        .clone();
    cards.push(copy);
    assert!(java_rampage(&ambiguous).is_err());
}
