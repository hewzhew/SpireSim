//! A constructed mechanism input checked against unchanged adjacent Java
//! frames, not an importer of the original combat or its card identities.
use super::super::*;
use crate::{
    observed_regression_fixtures::{load_from, observation_from, validate_adjacent},
    project_monster_ai_mechanism_v1, RecordedJavaActionV1,
};
use sts_oracle_run_control::{
    agent::information::combat::{
        combat_public_intent_facts_v1, HiddenInformationReasonV1, ObservationEvidenceKindV1,
    },
    content::{
        cards::{upgrade_card_once_java, CardId},
        monsters::{beyond::writhing_mass::WrithingMass, EnemyId, MonsterBehavior},
        powers::{store, PowerId},
        relics::{RelicId, RelicState},
    },
    engine::core::tick_until_stable_turn,
    runtime::{
        combat::{CombatCard, CombatState, Intent, Power, PowerPayload},
        rng::{begin_sts_rng_trace, StsRngCall, StsRngTraceEvent},
    },
    state::core::{ClientInput, EngineState},
    test_support::{combat_with_monsters, test_monster},
};
use sts_parity_lockstep::{
    NaturalRunRngCallLedgerV1, NaturalRunRngOperationV1, NaturalRunRngStreamV2, SemanticActionV1,
};

fn mechanism_power(power: &NaturalCombatPowerV4) -> Power {
    Power {
        power_type: match power.power_id.as_str() {
            "malleable" => PowerId::Malleable,
            "strength" => PowerId::Strength,
            "reactive" => PowerId::Reactive,
            "corruption" => PowerId::Corruption,
            "darkembrace" => PowerId::DarkEmbrace,
            "flamebarrier" => PowerId::FlameBarrier,
            "feelnopain" => PowerId::FeelNoPain,
            other => panic!("unreviewed mechanism input power: {other}"),
        },
        amount: power.amount,
        extra_data: match power.state {
            NaturalCombatPowerStateV4::Malleable { base_power } => base_power,
            NaturalCombatPowerStateV4::Stateless => 0,
            ref other => panic!("unreviewed mechanism power state: {other:?}"),
        },
        instance_id: None,
        payload: PowerPayload::None,
        just_applied: false,
    }
}

fn assert_mechanism_state(state: &CombatState, raw: &JavaRuntimeObservationV1) {
    let (monsters, ai) = project_monster_ai_mechanism_v1(raw).unwrap();
    let [java] = monsters.as_slice() else {
        panic!("expected one recorded monster")
    };
    let [rust] = state.entities.monsters.as_slice() else {
        panic!("expected one constructed monster")
    };
    assert_eq!(java.enemy_id, "writhingmass");
    assert_eq!(rust.enemy_id, EnemyId::WrithingMass);
    assert_eq!(rust.current_hp, java.current_hp);
    assert_eq!(rust.max_hp, java.max_hp);
    assert_eq!(rust.block, java.block);
    assert_eq!(rust.is_dead_or_escaped(), java.dead_or_escaped);
    assert_eq!(rust.planned_move_id(), java.next_move_id);
    assert_eq!(
        rust.move_history().iter().copied().collect::<Vec<_>>(),
        java.move_history
    );
    // Compare all observed powers including Malleable's independent basePower.
    assert_eq!(
        store::powers_for(state, rust.id).unwrap(),
        java.powers.iter().map(mechanism_power).collect::<Vec<_>>()
    );
    let plan = rust.turn_plan();
    assert_eq!(
        Intent::from_visible_move_spec(plan.visible_spec.as_ref().unwrap()).kind_v1(),
        raw.frame["parity_internal"]["monsters"][0]["intent"]
            .as_str()
            .unwrap()
            .to_ascii_lowercase()
    );
    let public = combat_public_intent_facts_v1(state, rust.id);
    assert_eq!(public.evidence, ObservationEvidenceKindV1::Hidden);
    assert_eq!(
        public.hidden_reason,
        Some(HiddenInformationReasonV1::RunicDome)
    );
    assert!(public.intent.is_none() && public.preview_damage_per_hit.is_none());
    assert_eq!(
        java.public_intent.evidence,
        NaturalCombatIntentEvidenceV4::Hidden
    );
    assert_eq!(
        java.public_intent.hidden_reason.as_deref(),
        Some("runic_dome")
    );
    assert!(java.public_intent.kind.is_none());
    assert!(java.public_intent.damage_per_hit.is_none() && java.public_intent.hits.is_none());
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
        i32::from(state.turn.energy),
        required_i32(player, "energy").unwrap()
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

#[test]
fn recorded_reactive_large_nonlethal_hit_executes_the_reroll_and_rng_call() {
    let before = observation_from("reactive_large_hit", "before.json");
    let after = observation_from("reactive_large_hit", "after.json");
    let play: RecordedJavaActionV1 =
        serde_json::from_value(load_from("reactive_large_hit", "play.json")).unwrap();
    assert!(validate_adjacent(&before, &play, &after));
    assert!(
        matches!(&play.action, SemanticActionV1::PlayCard { card_id, target: Some(target), .. }
        if card_id == "bodyslam" && target.kind == "monster" && target.id == "writhingmass"
            && target.occurrence == 0)
    );
    for raw in [&before, &after] {
        assert_eq!(raw.frame["boundary"]["kind"], "combat_player_decision");
        assert_eq!(raw.frame["public_base"]["run"]["ascension"], 20);
    }
    let (monsters, ai) = project_monster_ai_mechanism_v1(&before).unwrap();
    let [input] = monsters.as_slice() else {
        panic!("expected one monster")
    };
    assert_eq!(
        (input.current_hp, input.block, input.next_move_id),
        (105, 6, 2)
    );
    let creatures = project_monster_identities_v4(&before).unwrap();
    let powers = project_powers_with_sources_v4(
        &before.frame["parity_internal"]["player_powers"],
        "player",
        &creatures,
    )
    .unwrap();
    let player = &before.frame["public_base"]["combat"]["player"];
    assert_eq!(required_i32(player, "block").unwrap(), 72);
    assert_eq!(
        powers
            .iter()
            .find(|p| p.power_id == "strength")
            .unwrap()
            .amount,
        1
    );
    // Validate the recipe's observed card attributes without creating a
    // cross-runtime identity binding from this unique-name fixture lookup.
    let cards = before.frame["parity_internal"]["zones"]["hand"]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|card| card["card_id"] == "Body Slam")
        .collect::<Vec<_>>();
    let [card] = cards.as_slice() else {
        panic!("expected one observed Body Slam recipe")
    };
    assert_eq!(card["upgrades"], 1);
    assert_eq!(card["combat_cost"], 0);
    assert_eq!(card["cost_for_turn"], 0);
    assert_eq!(card["base_damage"], player["block"]);
    let ledger = NaturalRunRngCallLedgerV1::new(
        serde_json::from_value(after.frame["rng_call_witness"]["calls"].clone()).unwrap(),
    )
    .unwrap();
    let [call] = ledger.0.as_slice() else {
        panic!("expected exactly one RNG call")
    };
    assert_eq!(call.owner, "floor.ai");
    assert_eq!(call.owner_ordinal, 0);
    assert_eq!(
        call.operation,
        NaturalRunRngOperationV1::RandomInt {
            range: 99,
            result: 34
        }
    );
    let NaturalRunRngOperationV1::RandomInt { range, result } = call.operation else {
        unreachable!()
    };

    // Private Java flags are not exported. This non-first-move scenario uses
    // both possible mega-debuff flags; roll 34 after move 2 depends on neither.
    for used_mega_debuff in [false, true] {
        let mut mass = test_monster(EnemyId::WrithingMass);
        mass.id = 1;
        mass.current_hp = input.current_hp;
        mass.max_hp = input.max_hp;
        mass.block = input.block;
        mass.set_planned_move_id(input.next_move_id);
        mass.move_history_mut().clear();
        mass.move_history_mut()
            .extend(input.move_history.iter().copied());
        mass.writhing_mass.protocol_seeded = true;
        mass.writhing_mass.first_move = false;
        mass.writhing_mass.used_mega_debuff = used_mega_debuff;
        let mut state = combat_with_monsters(vec![mass]);
        state.meta.ascension_level = 20;
        let plan = WrithingMass::turn_plan(&state, &state.entities.monsters[0]);
        state.entities.monsters[0].set_planned_steps(plan.steps);
        state.entities.monsters[0].set_planned_visible_spec(plan.visible_spec);
        store::set_powers_for(
            &mut state,
            1,
            input.powers.iter().map(mechanism_power).collect(),
        );
        store::set_powers_for(&mut state, 0, powers.iter().map(mechanism_power).collect());
        state.entities.player.current_hp = required_i32(player, "current_hp").unwrap();
        state.entities.player.max_hp = required_i32(player, "max_hp").unwrap();
        state.entities.player.block = required_i32(player, "block").unwrap();
        state.turn.energy = required_i32(player, "energy").unwrap().try_into().unwrap();
        state.entities.player.relics.clear();
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicDome));
        // This card has a constructed scenario lineage, not the Java UUID or
        // the trajectory's natural binding. Damage is computed from the input.
        let mut card = CombatCard::for_scenario(CardId::BodySlam, 0);
        assert!(upgrade_card_once_java(&mut card));
        state.zones.hand = vec![card];
        state.rng.ai_rng.seed0 = u64::from_str_radix(&ai.state_hex[0], 16).unwrap();
        state.rng.ai_rng.seed1 = u64::from_str_radix(&ai.state_hex[1], 16).unwrap();
        state.rng.ai_rng.counter = ai.counter.unwrap();
        state.rng.pool.bind_trace_owners();
        assert_mechanism_state(&state, &before);
        let mut engine = EngineState::CombatPlayerTurn;
        let trace = begin_sts_rng_trace().unwrap();
        assert!(tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1)
            },
        ));
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        let calls = trace.finish().unwrap();
        assert_mechanism_state(&state, &after);
        assert_eq!(
            calls,
            vec![StsRngTraceEvent {
                owner: Some("floor.ai"),
                call: StsRngCall::RandomInt { range, result },
            }]
        );
        assert_eq!(state.entities.monsters[0].current_hp, 38);
        assert_eq!(state.entities.monsters[0].block, 7);
        assert_eq!(store::power_amount(&state, 1, PowerId::Malleable), 8);
    }
}

#[test]
fn recorded_reactive_evidence_requires_complete_adjacent_rng_and_power_state() {
    let before = observation_from("reactive_large_hit", "before.json");
    let after = observation_from("reactive_large_hit", "after.json");
    let play: RecordedJavaActionV1 =
        serde_json::from_value(load_from("reactive_large_hit", "play.json")).unwrap();
    let mut wrong = after.clone();
    wrong.producer.session_epoch.push_str("-other");
    assert!(!validate_adjacent(&before, &play, &wrong));
    for (key, value) in [
        ("from_frame_id", serde_json::json!(before.frame_id - 1)),
        ("to_frame_id", serde_json::json!(after.frame_id + 1)),
        ("complete", serde_json::json!(false)),
    ] {
        let mut wrong = after.clone();
        wrong.frame["rng_call_witness"][key] = value;
        assert!(!validate_adjacent(&before, &play, &wrong));
    }
    let mut missing = before.clone();
    missing.frame["parity_internal"]["monsters"][0]["powers"][0]
        .as_object_mut()
        .unwrap()
        .remove("malleable_base_power");
    assert!(project_monster_ai_mechanism_v1(&missing).is_err());
}
