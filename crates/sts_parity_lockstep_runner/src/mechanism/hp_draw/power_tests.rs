use super::{tests::recorded, *};
use sts_parity_lockstep::LockstepModeV1;

#[test]
fn recorded_hp_draw_powers_match_loss_events_draw_order_and_rng() {
    use HpDrawPowerCaseV1::*;
    // Explicit oracle outcomes: HP, Strength, hand/draw/discard lengths and
    // shuffle counter. Expectations do not call the recipe's classification.
    let cases = [
        (PuzzleCombust, 79, 2, [5, 3, 0], 2),
        (PuzzleCombustBuffer, 80, 0, [5, 0, 3], 1),
        (PuzzleCombustTungsten, 80, 0, [5, 0, 3], 1),
        (CubeCombust, 79, 2, [5, 3, 0], 2),
        (CubeCombustBuffer, 80, 0, [5, 0, 3], 1),
        (CubeCombustTungsten, 80, 0, [5, 0, 3], 1),
        (PuzzleCombustRegret, 76, 4, [5, 3, 0], 2),
        (PuzzleCombustRegretBuffer, 79, 2, [5, 3, 0], 2),
        (CubeCombustRegret, 76, 4, [5, 3, 0], 2),
        (CubeCombustRegretBuffer, 79, 2, [5, 3, 0], 2),
        (PuzzleCombustStacked, 78, 2, [5, 2, 0], 2),
        (CubeCombustStacked, 78, 2, [5, 2, 0], 2),
        (PuzzleBrutality, 79, 2, [8, 0, 0], 3),
        (PuzzleBrutalityBuffer, 80, 0, [6, 2, 0], 2),
        (PuzzleBrutalityTungsten, 80, 0, [6, 2, 0], 2),
        (CubeBrutality, 79, 2, [7, 1, 0], 2),
        (CubeBrutalityBuffer, 80, 0, [6, 2, 0], 2),
        (CubeBrutalityTungsten, 80, 0, [6, 2, 0], 2),
    ];
    for (power_case, hp, strength, counts, shuffle_counter) in cases {
        let case = HpDrawCaseV1::Power(power_case);
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
        let (player, powers) = project_player_mechanism_v1(&after.observation).unwrap();
        assert_eq!(player.current_hp, hp, "{case:?}");
        assert_eq!(
            powers
                .iter()
                .find(|p| p.power_id == "strength")
                .map_or(0, |p| p.amount),
            strength,
            "{case:?}"
        );
        let recipe = CapturedHpDrawRecipeV1::compile(case, &before).unwrap();
        let (zones, relics) =
            project_cards_relics_mechanism_v1(&after.observation, &recipe.java_bindings).unwrap();
        assert_eq!(
            [
                zones.hand.len(),
                zones.draw_next_first.len(),
                zones.discard_bottom_first.len()
            ],
            counts,
            "{case:?}"
        );
        for relic in relics.iter().filter(|r| r.id == "centennialpuzzle") {
            assert_eq!(
                relic.combat_state,
                Some(NaturalRelicCombatStateV1::CentennialPuzzle {
                    used_this_combat: hp < 80
                }),
                "{case:?}"
            );
            assert!(!relic.used_up);
        }
        let streams = project_combat_rng_mechanism_v1(&after.observation)
            .unwrap()
            .0;
        assert_eq!(
            streams
                .iter()
                .find(|s| s.owner == "floor.shuffle")
                .unwrap()
                .counter,
            Some(shuffle_counter),
            "{case:?}"
        );
        // The first five are the regular draw, then Brutality draws one, then
        // the relic draws after actual HP loss. Puzzle exhausts this small deck.
        let expected_hand: Option<&[u32]> = match power_case {
            PuzzleBrutality => Some(&[7, 6, 5, 4, 3, 2, 1, 0]),
            CubeBrutality => Some(&[7, 6, 5, 4, 3, 2, 1]),
            PuzzleBrutalityBuffer
            | PuzzleBrutalityTungsten
            | CubeBrutalityBuffer
            | CubeBrutalityTungsten => Some(&[7, 6, 5, 4, 3, 2]),
            // Two stacked Combust cards create one HP-loss event: Cube draws
            // once, so slot 6 leaves the draw pile before the next regular draw.
            CubeCombustStacked => Some(&[5, 4, 3, 2, 6]),
            _ => None,
        };
        if let Some(slots) = expected_hand {
            assert_eq!(
                zones
                    .hand
                    .iter()
                    .map(|c| c.binding.clone())
                    .collect::<Vec<_>>(),
                slots
                    .iter()
                    .map(|slot| format!("captured-hp-draw/{slot}"))
                    .collect::<Vec<_>>(),
                "{case:?}"
            );
        }
    }
}

#[test]
fn recorded_hp_draw_powers_reject_unprepared_or_unobserved_input_before_execution() {
    let case = HpDrawCaseV1::Power(HpDrawPowerCaseV1::PuzzleCombustStacked);
    let (before, _, _) = recorded(case);
    let mut wrong = before.clone();
    wrong.observation.frame["parity_internal"]["player_powers"][1]["combust_hp_loss"] = json!(1);
    assert!(build_input(case, &wrong)
        .err()
        .unwrap()
        .contains("private state"));
    let mut missing = before.clone();
    missing.observation.frame["parity_internal"]["player_powers"][1]
        .as_object_mut()
        .unwrap()
        .remove("combust_hp_loss");
    assert!(build_input(case, &missing)
        .err()
        .unwrap()
        .contains("combust_hp_loss"));
    let mut wrong = before.clone();
    wrong.observation.frame["parity_internal"]["player_powers"][0]["amount"] = json!(1);
    assert!(build_input(case, &wrong)
        .err()
        .unwrap()
        .contains("undeclared powers"));
    let mut wrong = before.clone();
    wrong.observation.frame["public_base"]["combat"]["player"]["energy"] = json!(3);
    assert!(build_input(case, &wrong).is_err());
    assert!(build_input(
        HpDrawCaseV1::Power(HpDrawPowerCaseV1::PuzzleBrutality),
        &before
    )
    .is_err());
}

#[test]
fn recorded_hp_draw_powers_detect_wrong_self_loss_and_empty_shuffle_call() {
    let case = HpDrawCaseV1::Power(HpDrawPowerCaseV1::PuzzleCombustStacked);
    let (before, action, mut after) = recorded(case);
    after.observation.frame["parity_internal"]["player_powers"][1]["combust_hp_loss"] = json!(1);
    let report = replay_hp_draw_mechanism_v1(case, &before, &action, &after).unwrap();
    assert_eq!(report.status, ComparisonStatusV1::Mismatch);
    assert!(report
        .first_stop(LockstepModeV1::Audit)
        .unwrap()
        .contains("hp_loss"));

    let case = HpDrawCaseV1::Power(HpDrawPowerCaseV1::PuzzleBrutality);
    let (before, action, mut after) = recorded(case);
    // Keep the observed final RNG state untouched: dropping the second shuffle
    // must still fail the complete transition call comparison.
    let calls = after.observation.frame["rng_call_witness"]["calls"]
        .as_array_mut()
        .unwrap();
    assert_eq!(calls.len(), 3);
    let removed = calls.pop().unwrap();
    assert_eq!(removed["owner"], "floor.shuffle");
    assert_eq!(removed["owner_ordinal"], 1);
    assert!(replay_hp_draw_mechanism_v1(case, &before, &action, &after)
        .unwrap_err()
        .contains("RNG calls differ"));
}
