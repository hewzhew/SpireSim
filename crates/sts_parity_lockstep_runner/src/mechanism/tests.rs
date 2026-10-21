use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;
use sts_parity_lockstep::{ComparisonStatusV1, LockstepModeV1};

fn fixture<T: serde::de::DeserializeOwned>(group: &str, name: &str) -> T {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sts_parity_lockstep_java/tests/fixtures")
        .join(group);
    let sources: Value =
        serde_json::from_slice(&std::fs::read(directory.join("sources.json")).unwrap()).unwrap();
    let bytes = std::fs::read(directory.join(name)).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        sources["files"][name]["sha256"].as_str().unwrap()
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    if let Some(observation) = value.get("observation") {
        assert_eq!(observation["producer"], sources["files"][name]["producer"]);
    }
    serde_json::from_value(value).unwrap()
}

fn recorded_case(
    group: &str,
    case: &str,
) -> (
    RecordedJavaBoundaryV1,
    RecordedJavaActionV1,
    RecordedJavaBoundaryV1,
) {
    (
        fixture(group, &format!("{case}-before.json")),
        fixture(group, &format!("{case}-action.json")),
        fixture(group, &format!("{case}-after.json")),
    )
}

#[test]
fn recorded_combust_distinguishes_stacking_separate_events_and_prevention_order() {
    for (case, hp, strength, damage, hp_loss) in [
        ("single", 79, 2, 5, 1),
        ("upgraded", 79, 2, 7, 1),
        ("stacked", 78, 2, 12, 2),
        ("stacked-tungsten", 79, 2, 12, 2),
        ("stacked-buffer", 80, 0, 12, 2),
        ("regret", 78, 4, 5, 1),
        ("regret-buffer", 79, 2, 5, 1),
        ("regret-tungsten", 80, 0, 5, 1),
        ("regret-buffer-tungsten", 80, 0, 5, 1),
        ("regret-three-buffer", 79, 2, 5, 1),
    ] {
        let (before, action, after) = recorded_case("combust_rupture", case);
        let report = replay_regret_mechanism_v1(&before, &action, &after).unwrap();
        assert_eq!(
            report.status,
            ComparisonStatusV1::Exact,
            "{case}: {:?}",
            report.first_stop(LockstepModeV1::Audit)
        );
        let (player, powers) =
            sts_parity_lockstep_java::project_player_mechanism_v1(&after.observation).unwrap();
        let power = |id| {
            powers
                .iter()
                .find(|p| p.power_id == id)
                .map_or(0, |p| p.amount)
        };
        assert_eq!((player.current_hp, player.block, player.energy), (hp, 0, 3));
        assert_eq!(
            (power("strength"), power("buffer"), power("rupture")),
            (strength, 0, 2)
        );
        for frame in [&before, &after] {
            let (_, powers) =
                sts_parity_lockstep_java::project_player_mechanism_v1(&frame.observation).unwrap();
            let combust = powers.iter().find(|p| p.power_id == "combust").unwrap();
            assert_eq!(combust.amount, damage);
            assert_eq!(
                combust.state,
                NaturalCombatPowerStateV4::Combust { hp_loss }
            );
        }
        let (initial, _) = project_monster_ai_mechanism_v1(&before.observation).unwrap();
        let (successor, _) = project_monster_ai_mechanism_v1(&after.observation).unwrap();
        assert_eq!(initial[0].current_hp - successor[0].current_hp, damage);
    }
}

#[test]
fn recorded_combust_requires_private_input_and_detects_wrong_successor_loss() {
    let (before, action, after) = recorded_case("combust_rupture", "stacked-tungsten");
    let mut missing = before.clone();
    let combust = missing.observation.frame["parity_internal"]["player_powers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "Combust")
        .unwrap();
    combust.as_object_mut().unwrap().remove("combust_hp_loss");
    assert!(replay_regret_mechanism_v1(&missing, &action, &after).is_err());
    let mut changed = after;
    let combust = changed.observation.frame["parity_internal"]["player_powers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "Combust")
        .unwrap();
    combust["combust_hp_loss"] = json!(1);
    let report = replay_regret_mechanism_v1(&before, &action, &changed).unwrap();
    assert_eq!(report.status, ComparisonStatusV1::Mismatch);
    assert!(report
        .first_stop(LockstepModeV1::Audit)
        .unwrap()
        .contains("hp_loss"));
}

#[test]
fn recorded_rupture_regret_uses_actual_self_hp_loss_once_per_event() {
    for (case, hp, strength) in [
        ("regret-one", 79, 2),
        ("regret-three", 77, 2),
        ("regret-buffer", 80, 0),
        ("regret-tungsten", 80, 0),
        ("regret-buffer-tungsten", 80, 0),
    ] {
        assert_rupture_case(case, hp, strength);
    }
}

#[test]
fn recorded_rupture_brutality_obeys_lose_hp_prevention() {
    for (case, hp, strength) in [
        ("brutality-one", 79, 2),
        ("brutality-buffer", 80, 0),
        ("brutality-tungsten", 80, 0),
    ] {
        assert_rupture_case(case, hp, strength);
    }
}

fn assert_rupture_case(case: &str, hp: i32, strength: i32) {
    let (before, action, after) = recorded_case("rupture_hp_loss", case);
    let report = replay_regret_mechanism_v1(&before, &action, &after).unwrap();
    assert_eq!(
        report.status,
        ComparisonStatusV1::Exact,
        "{case}: {:?}",
        report.first_stop(LockstepModeV1::Audit)
    );
    let (output, powers) =
        sts_parity_lockstep_java::project_player_mechanism_v1(&after.observation).unwrap();
    let power = |id| {
        powers
            .iter()
            .find(|p| p.power_id == id)
            .map_or(0, |p| p.amount)
    };
    assert_eq!((output.current_hp, output.block, output.energy), (hp, 0, 3));
    assert_eq!(
        (power("rupture"), power("strength"), power("buffer")),
        (2, strength, 0)
    );
    assert_eq!(power("brutality"), i32::from(case.starts_with("brutality")));
    assert_eq!(power("constricted"), 12);
}

#[test]
fn recorded_rupture_rejects_undeclared_powers_and_changed_strength() {
    let (before, action, after) = recorded_case("rupture_hp_loss", "regret-one");
    for id in ["Strength", "Rupture"] {
        let mut changed = before.clone();
        let powers = changed.observation.frame["parity_internal"]["player_powers"]
            .as_array_mut()
            .unwrap();
        let mut extra = powers[0].clone();
        extra["id"] = json!(id);
        extra["amount"] = json!(1);
        extra["index"] = json!(powers.len());
        powers.push(extra);
        assert!(replay_regret_mechanism_v1(&changed, &action, &after).is_err());
    }
    let mut changed = after;
    let strength = changed.observation.frame["parity_internal"]["player_powers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "Strength")
        .unwrap();
    strength["amount"] = json!(6);
    assert_eq!(
        replay_regret_mechanism_v1(&before, &action, &changed)
            .unwrap()
            .status,
        ComparisonStatusV1::Mismatch
    );
}

#[test]
fn recorded_regret_order_matrix_checks_prevention_and_trigger_time_hand_size() {
    for (case, hand, hp, block, had_buffer) in [
        ("one-tungsten", vec!["Regret"], 80, 5, false),
        ("one-buffer-tungsten", vec!["Regret"], 80, 5, true),
        ("mixed-block", vec!["Burn", "Regret", "Wound"], 77, 3, false),
        (
            "mixed-buffer-a",
            vec!["Regret", "Burn", "Wound"],
            78,
            0,
            true,
        ),
        (
            "mixed-buffer-b",
            vec!["Burn", "Regret", "Wound"],
            77,
            0,
            true,
        ),
    ] {
        let (before, action, after) = recorded_case("regret_order", case);
        let observed_hand = before.observation.frame["parity_internal"]["zones"]["hand"]["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["card_id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(observed_hand, hand);
        let (input, powers) =
            sts_parity_lockstep_java::project_player_mechanism_v1(&before.observation).unwrap();
        assert_eq!(input.current_hp, 80);
        assert_eq!(
            powers
                .iter()
                .any(|p| p.power_id == "buffer" && p.amount == 1),
            had_buffer
        );
        let report = replay_regret_mechanism_v1(&before, &action, &after).unwrap();
        assert_eq!(
            report.status,
            ComparisonStatusV1::Exact,
            "{case}: {:?}",
            report.first_stop(LockstepModeV1::Audit)
        );
        let (output, powers) =
            sts_parity_lockstep_java::project_player_mechanism_v1(&after.observation).unwrap();
        assert_eq!((output.current_hp, output.block), (hp, block));
        assert!(!powers.iter().any(|p| p.power_id == "buffer"));
    }
}

#[test]
fn recorded_regret_order_uses_observed_hand_order_and_rejects_missing_order() {
    let (before, action, after) = recorded_case("regret_order", "mixed-buffer-a");
    let mut changed = before.clone();
    changed.observation.frame["parity_internal"]["zones"]["hand"]["cards"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    let report = replay_regret_mechanism_v1(&changed, &action, &after).unwrap();
    assert_eq!(report.status, ComparisonStatusV1::Mismatch);
    assert!(report
        .first_stop(LockstepModeV1::Audit)
        .unwrap()
        .contains("current_hp"));
    let mut missing = before.clone();
    missing.observation.frame["parity_internal"]["zones"]["hand"]
        .as_object_mut()
        .unwrap()
        .remove("ordering");
    assert!(replay_regret_mechanism_v1(&missing, &action, &after)
        .unwrap_err()
        .contains("order"));
    let mut changed = before;
    changed.observation.frame["parity_internal"]["zones"]["hand"]["cards"][1]["upgrades"] =
        json!(1);
    assert!(replay_regret_mechanism_v1(&changed, &action, &after).is_err());
}

#[test]
fn recorded_regret_preserves_block_and_obeys_buffer_and_tungsten() {
    for (case, hp, had_buffer) in [
        ("plain", 77, false),
        ("buffer", 80, true),
        ("tungsten", 78, false),
        ("buffer-tungsten", 80, true),
    ] {
        let (before, action, after) = recorded_case("regret_block", case);
        let (input, powers) =
            sts_parity_lockstep_java::project_player_mechanism_v1(&before.observation).unwrap();
        assert_eq!((input.current_hp, input.block), (80, 5));
        assert_eq!(
            powers
                .iter()
                .any(|p| p.power_id == "buffer" && p.amount == 1),
            had_buffer
        );
        let report = replay_regret_mechanism_v1(&before, &action, &after).unwrap();
        assert_eq!(
            report.status,
            ComparisonStatusV1::Exact,
            "{case}: {:?}",
            report.first_stop(LockstepModeV1::Audit)
        );
        let (output, powers) =
            sts_parity_lockstep_java::project_player_mechanism_v1(&after.observation).unwrap();
        assert_eq!((output.current_hp, output.block, output.energy), (hp, 5, 3));
        assert!(!powers.iter().any(|p| p.power_id == "buffer"));
        assert!(powers
            .iter()
            .any(|p| p.power_id == "barricade" && p.amount == -1));
        assert!(powers
            .iter()
            .any(|p| p.power_id == "constricted" && p.amount == 12));
    }
}

#[test]
fn recorded_regret_rejects_changed_recipe_receipt_rng_and_successor() {
    let (before, action, after) = recorded_case("regret_block", "plain");
    let mut changed = before.clone();
    changed.observation.frame["parity_internal"]["zones"]["hand"]["cards"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(replay_regret_mechanism_v1(&changed, &action, &after).is_err());
    let mut changed_action = action.clone();
    changed_action.receipt.successor_frame_id = Some(after.observation.frame_id + 1);
    assert!(replay_regret_mechanism_v1(&before, &changed_action, &after).is_err());
    let mut changed = after.clone();
    changed.observation.frame["rng_call_witness"]["calls"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(replay_regret_mechanism_v1(&before, &action, &changed)
        .unwrap_err()
        .contains("RNG calls differ"));
    for (field, value) in [("block", 2), ("current_hp", 80)] {
        let mut changed = after.clone();
        changed.observation.frame["public_base"]["combat"]["player"][field] = json!(value);
        let report = replay_regret_mechanism_v1(&before, &action, &changed).unwrap();
        assert_eq!(report.status, ComparisonStatusV1::Mismatch);
        assert!(report
            .first_stop(LockstepModeV1::Audit)
            .unwrap()
            .contains(field));
    }
    let (before, action, mut after) = recorded_case("regret_block", "buffer-tungsten");
    let mut buffer = before.observation.frame["parity_internal"]["player_powers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "Buffer")
        .unwrap()
        .clone();
    let powers = after.observation.frame["parity_internal"]["player_powers"]
        .as_array_mut()
        .unwrap();
    buffer["index"] = json!(powers.len());
    powers.push(buffer);
    assert_ne!(
        replay_regret_mechanism_v1(&before, &action, &after)
            .unwrap()
            .status,
        ComparisonStatusV1::Exact
    );
}

#[test]
fn recorded_constricted_low_block_matrix_checks_real_damage_and_timing() {
    for block in [0, 5] {
        for phase in ["apply", "tick"] {
            let (before, action, after) =
                recorded_case("constricted_low_block", &format!("block-{block}-{phase}"));
            let report = replay_constricted_mechanism_v1(&before, &action, &after).unwrap();
            assert_eq!(
                report.status,
                ComparisonStatusV1::Exact,
                "block {block} {phase}: {:?}",
                report.first_stop(LockstepModeV1::Audit)
            );
            let (input, _) =
                sts_parity_lockstep_java::project_player_mechanism_v1(&before.observation).unwrap();
            let (output, powers) =
                sts_parity_lockstep_java::project_player_mechanism_v1(&after.observation).unwrap();
            assert_eq!(input.current_hp, 80);
            assert_eq!(input.block, if phase == "tick" { block } else { 0 });
            assert_eq!(
                output.current_hp,
                if phase == "tick" { 68 + block } else { 80 }
            );
            assert_eq!(output.block, 0);
            let constricted = powers.iter().find(|p| p.power_id == "constricted").unwrap();
            assert_eq!(constricted.amount, 12);
            assert_eq!(
                constricted.state,
                NaturalCombatPowerStateV4::Constricted {
                    source: sts_parity_lockstep::NaturalCombatCreatureV4::Monster {
                        enemy_id: "spiregrowth".into(),
                        occurrence: 0
                    }
                }
            );
        }
    }
}

#[test]
fn recorded_constricted_rejects_missing_source_rng_calls_and_changed_hp() {
    let (before, action, after) = recorded_case("constricted_low_block", "block-5-tick");
    let mut missing = before.clone();
    let power = missing.observation.frame["parity_internal"]["player_powers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "Constricted")
        .unwrap();
    power.as_object_mut().unwrap().remove("constricted_source");
    assert!(replay_constricted_mechanism_v1(&missing, &action, &after).is_err());
    let mut missing = after.clone();
    missing.observation.frame["rng_call_witness"]["calls"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(replay_constricted_mechanism_v1(&before, &action, &missing)
        .unwrap_err()
        .contains("RNG calls differ"));
    let mut incomplete = after.clone();
    incomplete.observation.frame["rng_call_witness"]["complete"] = json!(false);
    assert!(replay_constricted_mechanism_v1(&before, &action, &incomplete).is_err());
    let mut changed = after;
    changed.observation.frame["public_base"]["combat"]["player"]["current_hp"] = json!(74);
    let report = replay_constricted_mechanism_v1(&before, &action, &changed).unwrap();
    assert_ne!(report.status, ComparisonStatusV1::Exact);
    assert!(report
        .first_stop(LockstepModeV1::Audit)
        .unwrap()
        .contains("current_hp"));
}

#[test]
fn recorded_shackled_haste_matrix_uses_shared_mechanism_verifier() {
    for ascension in [0, 20] {
        for (sign, strength) in [("negative", -6), ("positive", 2)] {
            let case = format!("a{ascension}-{sign}");
            let (before, action, after) = recorded_case("haste_shackled", &case);
            assert_eq!(
                before.observation.frame["scenario"]["provenance"],
                "constructed_validated"
            );
            let (input, _) = project_monster_ai_mechanism_v1(&before.observation).unwrap();
            assert_eq!(
                input[0]
                    .powers
                    .iter()
                    .find(|p| p.power_id == "strength")
                    .unwrap()
                    .amount,
                strength
            );
            assert_eq!(
                input[0]
                    .powers
                    .iter()
                    .find(|p| p.power_id == "shackled")
                    .unwrap()
                    .amount,
                6
            );
            let report = replay_haste_mechanism_v1(&before, &action, &after).unwrap();
            assert_eq!(
                report.status,
                ComparisonStatusV1::Exact,
                "{case}: {:?}",
                report.first_stop(LockstepModeV1::Audit)
            );
            let (output, _) = project_monster_ai_mechanism_v1(&after.observation).unwrap();
            assert_eq!(output[0].current_hp, output[0].max_hp / 2);
            assert_eq!(output[0].block, if ascension >= 19 { 32 } else { 0 });
            let expected = input[0]
                .powers
                .iter()
                .filter(|p| p.power_id == "timewarp" || (p.power_id == "strength" && p.amount > 0))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(output[0].powers, expected);
        }
    }
}

#[test]
fn recorded_natural_haste_end_turn_still_uses_the_same_verifier() {
    let (before, action, after) = recorded_case("observed_a20h", "haste-execution");
    assert_eq!(
        before.observation.frame["public_base"]["combat"]["monsters"][0]["current_hp"],
        51
    );
    let report = replay_haste_mechanism_v1(&before, &action, &after).unwrap();
    assert_eq!(
        report.status,
        ComparisonStatusV1::Exact,
        "{:?}",
        report.first_stop(LockstepModeV1::Audit)
    );
}

#[test]
fn recorded_haste_intent_matches_current_engine_and_projection() {
    let boundary: RecordedJavaBoundaryV1 = fixture("observed_a20h", "time-eater-haste.json");
    let (input, _) = project_monster_ai_mechanism_v1(&boundary.observation).unwrap();
    let (mut session, bootstrap) = time_eater_mechanism_input(20).unwrap();
    let monster = &mut session
        .active_combat
        .as_mut()
        .unwrap()
        .combat_state
        .entities
        .monsters[0];
    monster.current_hp = input[0].current_hp;
    monster.max_hp = input[0].max_hp;
    monster.time_eater.used_haste = false;
    let plan = TimeEater::roll_move_plan(&mut StsRng::new(1), monster, 20, 0);
    assert_eq!(plan.move_id, input[0].next_move_id);
    monster.set_planned_move_id(plan.move_id);
    monster.set_planned_steps(plan.steps);
    monster.set_planned_visible_spec(plan.visible_spec);
    let mut rust = RustLockstepAdapterV1::new(session, bootstrap, "test", "haste-intent").unwrap();
    let snapshot = rust.observe().unwrap();
    let intent = &observed_domain(&snapshot, ParityDomainV1::Combat).unwrap()["state"]["monsters"]
        [0]["public_intent"];
    assert_eq!(*intent, json!(input[0].public_intent));
    assert_eq!(input[0].public_intent.kind.as_deref(), Some("buff"));
    assert_eq!(input[0].public_intent.damage_per_hit, None);
}

#[test]
fn mechanism_reports_first_changed_field_and_rejects_broken_evidence() {
    let (before, action, after) = recorded_case("haste_shackled", "a20-negative");
    let mut changed = after.clone();
    changed.observation.frame["public_base"]["combat"]["monsters"][0]["block"] = json!(31);
    let report = replay_haste_mechanism_v1(&before, &action, &changed).unwrap();
    assert_eq!(report.status, ComparisonStatusV1::Mismatch);
    assert!(report
        .first_stop(LockstepModeV1::Audit)
        .unwrap()
        .contains("$[0].block"));
    assert!(!report.may_continue(LockstepModeV1::Audit));
    let mut changed_rng = after.clone();
    let ai = changed_rng.observation.frame["rng_internal"]["streams"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|stream| stream["owner"] == "com.megacrit.cardcrawl.dungeons.AbstractDungeon.aiRng")
        .unwrap();
    ai["counter"] = json!(ai["counter"].as_u64().unwrap() + 1);
    let report = replay_haste_mechanism_v1(&before, &action, &changed_rng).unwrap();
    assert_eq!(report.status, ComparisonStatusV1::Mismatch);
    assert!(report
        .first_stop(LockstepModeV1::Audit)
        .unwrap()
        .contains("$[0].counter"));
    let mut wrong_receipt = action.clone();
    wrong_receipt.receipt.successor_frame_id = Some(after.observation.frame_id + 1);
    assert!(replay_haste_mechanism_v1(&before, &wrong_receipt, &after).is_err());
    let mut other_epoch = after.clone();
    other_epoch
        .observation
        .producer
        .session_epoch
        .push_str("-other");
    assert!(replay_haste_mechanism_v1(&before, &action, &other_epoch).is_err());
    let mut unstable = after.clone();
    unstable.observation.frame["boundary"]["stable"] = json!(false);
    assert!(replay_haste_mechanism_v1(&before, &action, &unstable).is_err());
    let mut missing = before.clone();
    missing.observation.frame["rng_internal"]
        .as_object_mut()
        .unwrap()
        .remove("streams");
    assert!(replay_haste_mechanism_v1(&missing, &action, &after).is_err());
}
