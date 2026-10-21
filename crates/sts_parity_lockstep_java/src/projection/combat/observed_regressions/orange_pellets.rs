//! Scoped constructed regressions for observed Orange Pellets transitions.
//! The two source fragments are independent and grant no natural card bindings.
use super::super::*;
use crate::{
    observed_regression_fixtures::{load_from, observation_from, validate_adjacent},
    projection::relics::project_relics_v3,
    RecordedJavaActionV1,
};
use sts_oracle_run_control::{
    content::{
        cards::{upgrade_card_once_java, CardId},
        monsters::{exordium::jaw_worm, EnemyId},
        powers::{store, PowerId},
        relics::{RelicId, RelicState},
    },
    engine::core::tick_until_stable_turn,
    runtime::combat::{CombatCard, CombatState, Power, PowerPayload},
    state::core::{ClientInput, EngineState},
    test_support::{combat_with_monsters, planned_monster, test_monster},
};
use sts_parity_lockstep::{NaturalRelicCombatStateV1, NaturalRelicV3, SemanticActionV1};

fn java_flags(raw: &JavaRuntimeObservationV1) -> Result<(bool, bool, bool), String> {
    let relics: Vec<NaturalRelicV3> = serde_json::from_value(
        project_relics_v3(&raw.frame, None, true)?.ok_or("relic state is unobserved")?,
    )
    .map_err(|error| error.to_string())?;
    let relic = relics
        .iter()
        .find(|relic| relic.id == "orangepellets")
        .ok_or("Orange Pellets missing")?;
    // Java's inherited display counter is unrelated to its static combo flags.
    assert_eq!(relic.counter, -1);
    match relic.combat_state {
        Some(NaturalRelicCombatStateV1::OrangePellets {
            attack_played,
            skill_played,
            power_played,
        }) => Ok((attack_played, skill_played, power_played)),
        _ => Err("Orange Pellets combo flags missing".into()),
    }
}

fn rust_flags(state: &CombatState) -> (bool, bool, bool) {
    let counter = state
        .entities
        .player
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::OrangePellets)
        .unwrap()
        .counter;
    assert!((0..=7).contains(&counter));
    (counter & 1 != 0, counter & 2 != 0, counter & 4 != 0)
}

fn seed_pellets(state: &mut CombatState, raw: &JavaRuntimeObservationV1) {
    let (attack, skill, power) = java_flags(raw).unwrap();
    let mut relic = RelicState::new(RelicId::OrangePellets);
    relic.counter = i32::from(attack) | (i32::from(skill) << 1) | (i32::from(power) << 2);
    state.entities.player.add_relic(relic);
    assert_eq!(rust_flags(state), java_flags(raw).unwrap());
}

fn action(name: &str) -> RecordedJavaActionV1 {
    serde_json::from_value(load_from("orange_pellets", name)).unwrap()
}

fn boundary(name: &str) -> JavaRuntimeObservationV1 {
    let raw = observation_from("orange_pellets", name);
    assert_eq!(raw.frame["boundary"]["kind"], "combat_player_decision");
    raw
}

#[test]
fn recorded_orange_pellets_resets_at_turn_start_before_the_next_power() {
    let before = boundary("reset-before.json");
    let next_turn = boundary("reset-after.json");
    let after_power = boundary("power-after.json");
    let end = action("end-turn.json");
    let play = action("dark-embrace.json");
    assert!(validate_adjacent(&before, &end, &next_turn));
    assert!(validate_adjacent(&next_turn, &play, &after_power));
    assert_eq!(end.action, SemanticActionV1::EndTurn);
    assert!(
        matches!(&play.action, SemanticActionV1::PlayCard { card_id, target: None, .. }
        if card_id == "darkembrace")
    );
    assert_eq!(java_flags(&before).unwrap(), (true, true, false));
    assert_eq!(java_flags(&next_turn).unwrap(), (false, false, false));
    assert_eq!(java_flags(&after_power).unwrap(), (false, false, true));

    // Only combo flags are compared in this fragment. The benign enemy turn
    // and recipe draw are constructed, not a reconstruction of Shield/Spear.
    let mut monster = planned_monster(EnemyId::JawWorm, 2);
    jaw_worm::initialize_runtime_state(&mut monster, false);
    let mut state = combat_with_monsters(vec![monster]);
    seed_pellets(&mut state, &before);
    let mut power = CombatCard::for_scenario(CardId::DarkEmbrace, 0);
    assert!(upgrade_card_once_java(&mut power));
    let mut draw = vec![power];
    draw.extend((1..5).map(|slot| CombatCard::for_scenario(CardId::Strike, slot)));
    state.zones.draw_pile = draw.into();
    let mut engine = EngineState::CombatPlayerTurn;
    let turn = state.turn.turn_count;
    assert!(tick_until_stable_turn(
        &mut engine,
        &mut state,
        ClientInput::EndTurn
    ));
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(state.turn.turn_count, turn + 1);
    assert_eq!(rust_flags(&state), java_flags(&next_turn).unwrap());
    assert_eq!(state.zones.hand[0].id, CardId::DarkEmbrace);
    assert!(tick_until_stable_turn(
        &mut engine,
        &mut state,
        ClientInput::PlayCard {
            card_index: 0,
            target: None
        }
    ));
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(rust_flags(&state), java_flags(&after_power).unwrap());
    assert_eq!(store::power_amount(&state, 0, PowerId::DarkEmbrace), 1);
}

fn mechanism_power(power: &NaturalCombatPowerV4) -> Power {
    assert_eq!(power.state, NaturalCombatPowerStateV4::Stateless);
    Power {
        power_type: match power.power_id.as_str() {
            "strength" => PowerId::Strength,
            "losestrength" => PowerId::LoseStrength,
            "darkembrace" => PowerId::DarkEmbrace,
            other => panic!("unreviewed Orange Pellets mechanism power: {other}"),
        },
        amount: power.amount,
        instance_id: None,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    }
}

#[test]
fn recorded_orange_pellets_combo_removes_strength_loss_without_removing_strength() {
    let before = boundary("cleanse-before.json");
    let after = boundary("cleanse-after.json");
    let play = action("anger.json");
    assert!(validate_adjacent(&before, &play, &after));
    assert!(
        matches!(&play.action, SemanticActionV1::PlayCard { card_id, target: Some(target), .. }
        if card_id == "anger" && target.kind == "monster" && target.id == "cultist" && target.occurrence == 0)
    );
    assert_eq!(java_flags(&before).unwrap(), (false, true, true));
    assert_eq!(java_flags(&after).unwrap(), (false, false, false));
    let java_powers = |raw: &JavaRuntimeObservationV1| {
        project_powers_v4(&raw.frame["parity_internal"]["player_powers"], "player")
            .unwrap()
            .iter()
            .map(mechanism_power)
            .collect::<Vec<_>>()
    };

    // Play an actual Anger through the current engine. This local target and
    // scenario lineage establish no binding to the captured natural objects.
    let mut target = test_monster(EnemyId::Cultist);
    target.current_hp = 54;
    target.max_hp = 54;
    let mut state = combat_with_monsters(vec![target]);
    seed_pellets(&mut state, &before);
    store::set_powers_for(&mut state, 0, java_powers(&before));
    assert_eq!(store::powers_for(&state, 0).unwrap(), java_powers(&before));
    assert_eq!(store::power_amount(&state, 0, PowerId::Strength), 10);
    assert_eq!(store::power_amount(&state, 0, PowerId::LoseStrength), 9);
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Anger, 0)];
    let mut engine = EngineState::CombatPlayerTurn;
    assert!(tick_until_stable_turn(
        &mut engine,
        &mut state,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1)
        }
    ));
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(rust_flags(&state), java_flags(&after).unwrap());
    assert_eq!(store::powers_for(&state, 0).unwrap(), java_powers(&after));
    assert_eq!(store::power_amount(&state, 0, PowerId::Strength), 10);
    assert!(!store::has_power(&state, 0, PowerId::LoseStrength));
    assert_eq!(store::power_amount(&state, 0, PowerId::DarkEmbrace), 1);
}

#[test]
fn recorded_orange_pellets_requires_each_flag_even_with_a_display_counter() {
    let before = boundary("reset-before.json");
    for flag in ["attack_played", "skill_played", "power_played"] {
        let mut missing = before.clone();
        let relic = missing.frame["parity_internal"]["relics"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|relic| relic["relic_id"] == "OrangePellets")
            .unwrap();
        relic["orange_pellets"]
            .as_object_mut()
            .unwrap()
            .remove(flag);
        assert!(java_flags(&missing).is_err());
    }
    let mut missing = before.clone();
    let relic = missing.frame["parity_internal"]["relics"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|relic| relic["relic_id"] == "OrangePellets")
        .unwrap();
    relic.as_object_mut().unwrap().remove("orange_pellets");
    assert!(java_flags(&missing).is_err());
}
