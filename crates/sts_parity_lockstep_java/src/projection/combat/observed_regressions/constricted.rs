//! Separate raw EndTurn fragments checked against explicit mechanism recipes.
//! High block hides the exact tick amount; no natural card identities are bound.
use super::super::*;
use crate::{
    observed_regression_fixtures::{load_from, observation_from, validate_adjacent},
    project_monster_ai_mechanism_v1, RecordedJavaActionV1,
};
use serde_json::json;
use sts_oracle_run_control::{
    content::{
        cards::CardId,
        monsters::{beyond::spire_growth::SpireGrowth, EnemyId, MonsterBehavior},
        powers::{store, PowerId},
    },
    engine::core::tick_until_stable_turn,
    runtime::{
        combat::{CombatCard, CombatState, Power, PowerPayload},
        rng::{begin_sts_rng_trace, StsRngCall, StsRngTraceEvent},
    },
    state::core::{ClientInput, EngineState},
    test_support::{combat_with_monsters, test_monster},
    EntityId,
};
use sts_parity_lockstep::{
    NaturalCombatCreatureV4, NaturalRunRngCallLedgerV1, NaturalRunRngOperationV1,
    NaturalRunRngStreamV2, SemanticActionV1,
};

// Explicit scenario identity, unrelated to Java's captured runtime number.
const GROWTH: EntityId = 73;

fn player_powers(raw: &JavaRuntimeObservationV1) -> Result<Vec<NaturalCombatPowerV4>, String> {
    let creatures = project_monster_identities_v4(raw)?;
    project_powers_with_sources_v4(
        &raw.frame["parity_internal"]["player_powers"],
        "player",
        &creatures,
    )
}

fn mechanism_power(power: &NaturalCombatPowerV4) -> Power {
    let mut result = Power {
        power_type: match power.power_id.as_str() {
            "strength" => PowerId::Strength,
            "brutality" => PowerId::Brutality,
            "corruption" => PowerId::Corruption,
            "vulnerable" => PowerId::Vulnerable,
            "weak" => PowerId::Weak,
            "berserk" => PowerId::Berserk,
            "losestrength" => PowerId::LoseStrength,
            "feelnopain" => PowerId::FeelNoPain,
            "darkembrace" => PowerId::DarkEmbrace,
            "flamebarrier" => PowerId::FlameBarrier,
            "constricted" => PowerId::Constricted,
            other => panic!("unreviewed Constricted mechanism power: {other}"),
        },
        amount: power.amount,
        instance_id: None,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    };
    match &power.state {
        NaturalCombatPowerStateV4::Stateless => {}
        NaturalCombatPowerStateV4::RoundDuration { just_applied } => {
            result.just_applied = *just_applied;
        }
        NaturalCombatPowerStateV4::Constricted {
            source:
                NaturalCombatCreatureV4::Monster {
                    enemy_id,
                    occurrence,
                },
        } => {
            assert_eq!((enemy_id.as_str(), *occurrence), ("spiregrowth", 0));
            result.payload = PowerPayload::SourceEntity(GROWTH);
        }
        other => panic!("unreviewed Constricted mechanism state: {other:?}"),
    }
    result
}

fn assert_observed_state(state: &CombatState, raw: &JavaRuntimeObservationV1) {
    let player = &raw.frame["public_base"]["combat"]["player"];
    assert_eq!(
        state.entities.player.current_hp,
        required_i32(player, "current_hp").unwrap()
    );
    assert_eq!(
        state.entities.player.max_hp,
        required_i32(player, "max_hp").unwrap()
    );
    assert_eq!(
        state.entities.player.block,
        required_i32(player, "block").unwrap()
    );
    assert_eq!(
        store::powers_for(state, 0).unwrap(),
        player_powers(raw)
            .unwrap()
            .iter()
            .map(mechanism_power)
            .collect::<Vec<_>>()
    );
    let (monsters, ai) = project_monster_ai_mechanism_v1(raw).unwrap();
    let [java] = monsters.as_slice() else {
        panic!("expected one recorded monster")
    };
    let [rust] = state.entities.monsters.as_slice() else {
        panic!("expected one scenario monster")
    };
    assert_eq!(java.enemy_id, "spiregrowth");
    assert_eq!(rust.id, GROWTH);
    assert_eq!(rust.current_hp, java.current_hp);
    assert_eq!(rust.max_hp, java.max_hp);
    assert_eq!(rust.block, java.block);
    assert_eq!(rust.is_dead_or_escaped(), java.dead_or_escaped);
    assert_eq!(rust.planned_move_id(), java.next_move_id);
    assert_eq!(
        rust.move_history().iter().copied().collect::<Vec<_>>(),
        java.move_history
    );
    assert_eq!(
        store::powers_snapshot_for(state, GROWTH),
        java.powers.iter().map(mechanism_power).collect::<Vec<_>>()
    );
    let rng = &state.rng.ai_rng;
    assert_eq!(
        NaturalRunRngStreamV2::new(
            "floor.ai",
            "floor",
            [format!("{:016X}", rng.seed0), format!("{:016X}", rng.seed1)],
            Some(rng.counter),
        )
        .unwrap(),
        ai
    );
}

fn end_turn_fragment(prefix: &str) -> CombatState {
    let before = observation_from("constricted", &format!("{prefix}-before.json"));
    let after = observation_from("constricted", &format!("{prefix}-after.json"));
    let action: RecordedJavaActionV1 =
        serde_json::from_value(load_from("constricted", &format!("{prefix}-end-turn.json")))
            .unwrap();
    assert!(validate_adjacent(&before, &action, &after));
    assert_eq!(action.action, SemanticActionV1::EndTurn);
    for raw in [&before, &after] {
        assert_eq!(raw.frame["boundary"]["kind"], "combat_player_decision");
        assert_eq!(raw.frame["public_base"]["run"]["ascension"], 20);
    }
    let (monsters, ai) = project_monster_ai_mechanism_v1(&before).unwrap();
    let [input] = monsters.as_slice() else {
        panic!("expected one recorded monster")
    };
    assert_eq!(input.enemy_id, "spiregrowth");
    let mut monster = test_monster(EnemyId::SpireGrowth);
    monster.id = GROWTH;
    monster.current_hp = input.current_hp;
    monster.max_hp = input.max_hp;
    monster.block = input.block;
    monster.set_planned_move_id(input.next_move_id);
    monster.move_history_mut().clear();
    monster
        .move_history_mut()
        .extend(input.move_history.iter().copied());
    let mut state = combat_with_monsters(vec![monster]);
    state.meta.ascension_level = 20;
    let plan = SpireGrowth::turn_plan(&state, &state.entities.monsters[0]);
    state.entities.monsters[0].set_planned_steps(plan.steps);
    state.entities.monsters[0].set_planned_visible_spec(plan.visible_spec);
    store::set_powers_for(
        &mut state,
        GROWTH,
        input.powers.iter().map(mechanism_power).collect(),
    );
    store::set_powers_for(
        &mut state,
        0,
        player_powers(&before)
            .unwrap()
            .iter()
            .map(mechanism_power)
            .collect(),
    );
    let player = &before.frame["public_base"]["combat"]["player"];
    state.entities.player.current_hp = required_i32(player, "current_hp").unwrap();
    state.entities.player.max_hp = required_i32(player, "max_hp").unwrap();
    state.entities.player.block = required_i32(player, "block").unwrap();
    // Neither observed hand has an end-turn curse/status or Ethereal card.
    // Safe scenario draw avoids introducing unrelated shuffle or draw effects.
    state.zones.draw_pile = (0..8)
        .map(|slot| CombatCard::for_scenario(CardId::Strike, slot))
        .collect::<Vec<_>>()
        .into();
    state.rng.ai_rng.seed0 = u64::from_str_radix(&ai.state_hex[0], 16).unwrap();
    state.rng.ai_rng.seed1 = u64::from_str_radix(&ai.state_hex[1], 16).unwrap();
    state.rng.ai_rng.counter = ai.counter.unwrap();
    state.rng.pool.bind_trace_owners();
    assert_observed_state(&state, &before);
    let mut engine = EngineState::CombatPlayerTurn;
    let trace = begin_sts_rng_trace().unwrap();
    assert!(tick_until_stable_turn(
        &mut engine,
        &mut state,
        ClientInput::EndTurn
    ));
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    let calls = trace.finish().unwrap();
    assert_observed_state(&state, &after);
    let ledger = NaturalRunRngCallLedgerV1::new(
        serde_json::from_value(after.frame["rng_call_witness"]["calls"].clone()).unwrap(),
    )
    .unwrap();
    let [call] = ledger.0.as_slice() else {
        panic!("expected one AI call")
    };
    assert_eq!(call.owner, "floor.ai");
    assert_eq!(call.owner_ordinal, 0);
    let NaturalRunRngOperationV1::RandomInt { range, result } = call.operation else {
        panic!("expected random_int")
    };
    assert_eq!(
        calls,
        vec![StsRngTraceEvent {
            owner: Some("floor.ai"),
            call: StsRngCall::RandomInt { range, result },
        }]
    );
    state
}

#[test]
fn recorded_constricted_application_retains_the_original_monster_source() {
    let state = end_turn_fragment("apply");
    assert_eq!(state.entities.player.current_hp, 77);
    assert_eq!(state.entities.monsters[0].current_hp, 108);
    let powers = store::powers_snapshot_for(&state, 0);
    let constricted = powers
        .iter()
        .find(|p| p.power_type == PowerId::Constricted)
        .unwrap();
    assert_eq!(
        (constricted.amount, &constricted.payload),
        (12, &PowerPayload::SourceEntity(GROWTH))
    );
}

#[test]
fn recorded_constricted_blocked_tick_does_not_trigger_flame_barrier() {
    let state = end_turn_fragment("tick");
    assert_eq!(state.entities.player.current_hp, 76);
    // Only the monster's normal attack provokes the observed four damage.
    assert_eq!(state.entities.monsters[0].current_hp, 33);
    assert_eq!(store::power_amount(&state, 0, PowerId::Constricted), 12);
}

#[test]
fn recorded_constricted_requires_a_source_in_the_current_creature_boundary() {
    let before = observation_from("constricted", "tick-before.json");
    assert!(player_powers(&before).is_ok());
    for source in [
        Value::Null,
        json!({"kind":"unobserved"}),
        json!({"kind":"monster","monster_instance_id":999}),
    ] {
        let mut invalid = before.clone();
        let power = invalid.frame["parity_internal"]["player_powers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["id"] == "Constricted")
            .unwrap();
        if source.is_null() {
            power.as_object_mut().unwrap().remove("constricted_source");
        } else {
            power["constricted_source"] = source;
        }
        assert!(player_powers(&invalid).is_err());
    }
}
