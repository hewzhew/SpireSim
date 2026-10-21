use super::*;
use sts_parity_lockstep::LockstepModeV1;

pub(super) fn recorded(
    case: HpDrawCaseV1,
) -> (
    RecordedJavaBoundaryV1,
    RecordedJavaActionV1,
    RecordedJavaBoundaryV1,
) {
    let fixture = match case {
        HpDrawCaseV1::Power(_) => "hp_loss_draw_powers",
        _ => "hp_loss_draw",
    };
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sts_parity_lockstep_java/tests/fixtures")
        .join(fixture)
        .join(case.name());
    (
        super::super::batch::read(&directory.join("before.json")).unwrap(),
        super::super::batch::read(&directory.join("action.json")).unwrap(),
        super::super::batch::read(&directory.join("after.json")).unwrap(),
    )
}

#[test]
fn recorded_hp_draw_matrix_matches_cards_relics_creatures_and_rng() {
    for &case in HpDrawCaseV1::ALL.iter().chain(HpDrawCaseV1::PREVENTION) {
        let (before, action, after) = recorded(case);
        let report = replay_hp_draw_mechanism_v1(case, &before, &action, &after).unwrap();
        assert_eq!(
            report.status,
            ComparisonStatusV1::Exact,
            "{case:?}: {:?}",
            report.first_stop(LockstepModeV1::Audit)
        );
        assert_eq!(
            report.domains.iter().map(|d| d.domain).collect::<Vec<_>>(),
            [
                ParityDomainV1::Relics,
                ParityDomainV1::Combat,
                ParityDomainV1::RngStreams
            ]
        );
        let recipe = CapturedHpDrawRecipeV1::compile(case, &before).unwrap();
        let (zones, relics) =
            project_cards_relics_mechanism_v1(&after.observation, &recipe.java_bindings).unwrap();
        assert_eq!(zones.hand.len(), 5);
        let (player, _) = project_player_mechanism_v1(&after.observation).unwrap();
        if HpDrawCaseV1::PREVENTION.contains(&case) {
            assert_eq!(player.current_hp, 80);
            assert_eq!(zones.draw_next_first.len(), 0);
            assert_eq!(zones.discard_bottom_first.len(), 5);
            for relic in relics.iter().filter(|r| r.id == "centennialpuzzle") {
                assert_eq!(
                    relic.combat_state,
                    Some(NaturalRelicCombatStateV1::CentennialPuzzle {
                        used_this_combat: false
                    })
                );
                assert!(!relic.used_up);
            }
        }
        if matches!(
            case,
            HpDrawCaseV1::PuzzleBurn | HpDrawCaseV1::PuzzleBurnReset | HpDrawCaseV1::CubeBurn
        ) {
            // Only the five cards in the original hand trigger. Newly drawn
            // Burns leave via the final discard instead of dealing more HP loss.
            assert_eq!(player.current_hp, 70);
        }
    }
    let (_, _, after) = recorded(HpDrawCaseV1::PuzzleBurn);
    let (next, _, _) = recorded(HpDrawCaseV1::PuzzleBurnReset);
    assert_eq!(after.observation.producer, next.observation.producer);
    assert!(after.observation.frame_id < next.observation.frame_id);
    assert_eq!(
        after.observation.frame["parity_internal"]["relics"][0]["centennial_puzzle"]
            ["used_this_combat"],
        true
    );
    assert_eq!(
        next.observation.frame["parity_internal"]["relics"][0]["centennial_puzzle"]
            ["used_this_combat"],
        false
    );
}

#[test]
fn recorded_hp_draw_detects_same_name_card_reordering_and_private_relic_changes() {
    let case = HpDrawCaseV1::PuzzleBurn;
    let (before, action, after) = recorded(case);
    let mut changed = after.clone();
    let cards = changed.observation.frame["parity_internal"]["zones"]["hand"]["cards"]
        .as_array_mut()
        .unwrap();
    assert_eq!(cards[0]["card_id"], cards[1]["card_id"]);
    cards.swap(0, 1);
    let report = replay_hp_draw_mechanism_v1(case, &before, &action, &changed).unwrap();
    assert_eq!(report.status, ComparisonStatusV1::Mismatch);
    assert!(report
        .first_stop(LockstepModeV1::Audit)
        .unwrap()
        .contains("zones.hand[0].binding"));
    let mut changed = after.clone();
    changed.observation.frame["parity_internal"]["relics"][0]["centennial_puzzle"]
        ["used_this_combat"] = json!(false);
    let report = replay_hp_draw_mechanism_v1(case, &before, &action, &changed).unwrap();
    assert_eq!(report.status, ComparisonStatusV1::Mismatch);
    assert!(report
        .first_stop(LockstepModeV1::Audit)
        .unwrap()
        .contains("used_this_combat"));
    let mut changed = after.clone();
    changed.observation.frame["public_base"]["combat"]["player"]["current_hp"] = json!(69);
    assert_eq!(
        replay_hp_draw_mechanism_v1(case, &before, &action, &changed)
            .unwrap()
            .status,
        ComparisonStatusV1::Mismatch
    );
}

#[test]
fn recorded_hp_draw_rejects_missing_observation_or_guessed_successor_bindings() {
    let case = HpDrawCaseV1::PuzzleBurn;
    let (before, action, after) = recorded(case);
    let mut missing = after.clone();
    missing.observation.frame["parity_internal"]["relics"][0]
        .as_object_mut()
        .unwrap()
        .remove("centennial_puzzle");
    assert!(
        replay_hp_draw_mechanism_v1(case, &before, &action, &missing)
            .unwrap_err()
            .contains("unobserved")
    );
    let mut unknown = after.clone();
    unknown.observation.frame["parity_internal"]["zones"]["hand"]["cards"][0]["instance_uuid"] =
        json!("new-uuid-with-the-same-Burn-name");
    assert!(
        replay_hp_draw_mechanism_v1(case, &before, &action, &unknown)
            .unwrap_err()
            .contains("binding")
    );
    let mut duplicate = before.clone();
    let uuid = duplicate.observation.frame["parity_internal"]["zones"]["hand"]["cards"][0]
        ["instance_uuid"]
        .clone();
    duplicate.observation.frame["parity_internal"]["zones"]["draw"]["cards"][0]["instance_uuid"] =
        uuid;
    assert!(CapturedHpDrawRecipeV1::compile(case, &duplicate)
        .unwrap_err()
        .contains("repeats"));
    let mut unordered = before.clone();
    unordered.observation.frame["parity_internal"]["zones"]["draw"]["top"] = json!("first");
    assert!(CapturedHpDrawRecipeV1::compile(case, &unordered).is_err());
    let mut natural = before.clone();
    natural.observation.frame["scenario"] = serde_json::Value::Null;
    assert!(CapturedHpDrawRecipeV1::compile(case, &natural).is_err());
    let mut missing_rng = after.clone();
    missing_rng.observation.frame["rng_call_witness"]["complete"] = json!(false);
    assert!(replay_hp_draw_mechanism_v1(case, &before, &action, &missing_rng).is_err());
    let mut wrong = action.clone();
    wrong.receipt.successor_frame_id = Some(after.observation.frame_id + 1);
    assert!(replay_hp_draw_mechanism_v1(case, &before, &wrong, &after).is_err());
}
