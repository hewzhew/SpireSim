//! A scoped constructed engine regression against unchanged adjacent oracle
//! frames. It deliberately establishes no natural card bindings.
use super::super::*;
use crate::{
    observed_regression_fixtures::{load_from, observation_from, validate_adjacent},
    RecordedJavaActionV1,
};
use sts_oracle_run_control::{
    content::{
        cards::CardId,
        monsters::EnemyId,
        powers::{core::surrounded, store, PowerId},
    },
    engine::core::tick_until_stable_turn,
    runtime::combat::{CombatCard, CombatState, Power, PowerPayload},
    state::{
        core::{ClientInput, EngineState, PendingChoice},
        selection::{SelectionResolution, SelectionScope},
    },
    test_support::{combat_with_monsters, test_monster},
};
use sts_parity_lockstep::{DecisionSurfaceV1, SemanticActionV1};

#[derive(Debug, PartialEq)]
struct FacingAndMarkers {
    facing_left: bool,
    // Ordered by canonical enemy identity, not Java runtime instance IDs.
    monsters: Vec<(String, bool, Option<i32>)>,
}

fn java_reading(raw: &JavaRuntimeObservationV1) -> Result<FacingAndMarkers, String> {
    let creatures = project_monster_identities_v4(raw)?;
    let powers = project_powers_with_sources_v4(
        &raw.frame["parity_internal"]["player_powers"],
        "player",
        &creatures,
    )?;
    if !powers.iter().any(|p| p.power_id == "surrounded") {
        return Err("Headbutt fixture must observe Surrounded".into());
    }
    let mut monsters = project_monsters_v4(raw, true)?
        .into_iter()
        .map(|monster| {
            assert!(!monster.dead_or_escaped);
            let marker = monster.powers.iter().find(|p| p.power_id == "backattack");
            (
                monster.enemy_id,
                monster.left_of_player.unwrap(),
                marker.map(|p| p.amount),
            )
        })
        .collect::<Vec<_>>();
    monsters.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(FacingAndMarkers {
        facing_left: required_bool(&raw.frame["public_base"]["combat"]["player"], "facing_left")?,
        monsters,
    })
}

fn rust_reading(state: &CombatState) -> FacingAndMarkers {
    let monsters = [("spireshield", 1), ("spirespear", 2)]
        .into_iter()
        .map(|(enemy, id)| {
            (
                enemy.to_owned(),
                surrounded::monster_is_left_of_player(state, id).unwrap(),
                store::has_power(state, id, PowerId::BackAttack)
                    .then(|| store::power_amount(state, id, PowerId::BackAttack)),
            )
        })
        .collect();
    FacingAndMarkers {
        facing_left: state.entities.player.facing_left,
        monsters,
    }
}

#[test]
fn recorded_headbutt_keeps_the_old_marker_until_selection_settles() {
    let before = observation_from("headbutt_surrounded", "before.json");
    let pending = observation_from("headbutt_surrounded", "pending.json");
    let settled = observation_from("headbutt_surrounded", "settled.json");
    let play: RecordedJavaActionV1 =
        serde_json::from_value(load_from("headbutt_surrounded", "play.json")).unwrap();
    let select: RecordedJavaActionV1 =
        serde_json::from_value(load_from("headbutt_surrounded", "select.json")).unwrap();
    assert!(validate_adjacent(&before, &play, &pending));
    assert!(validate_adjacent(&pending, &select, &settled));
    assert!(
        matches!(&play.action, SemanticActionV1::PlayCard {card_id, target: Some(target), ..}
        if card_id == "headbutt" && target.kind == "monster" && target.id == "spireshield")
    );
    assert!(matches!(&select.action, SemanticActionV1::SubmitSelection {
        surface: DecisionSurfaceV1::GridSelect, choices
    } if choices.len() == 1 && choices[0].kind == "discard_card"));
    assert_eq!(
        pending.frame["public_base"]["decision"]["grid_select"]["source_pile"],
        "Discard"
    );
    assert_eq!(
        pending.frame["public_base"]["decision"]["grid_select"]["selection_reason"],
        "MoveToDrawPile"
    );
    assert_eq!(before.frame["boundary"]["kind"], "combat_player_decision");
    assert_eq!(settled.frame["boundary"]["kind"], "combat_player_decision");

    // Deliberately minimal constructed input; these local scenario lineages
    // do not stand for any Java card, acquisition or captured natural root.
    let mut shield = test_monster(EnemyId::SpireShield);
    shield.id = 1;
    let mut spear = test_monster(EnemyId::SpireSpear);
    spear.id = 2;
    let mut state = combat_with_monsters(vec![shield, spear]);
    store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Surrounded,
            amount: -1,
            instance_id: None,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Headbutt, 0)];
    let selected = CombatCard::for_scenario(CardId::Strike, 1);
    let lineage = selected.lineage;
    state.zones.discard_pile = vec![selected, CombatCard::for_scenario(CardId::Defend, 2)].into();
    surrounded::sync_back_attack_markers(&mut state);
    let mut engine = EngineState::CombatPlayerTurn;
    assert_eq!(rust_reading(&state), java_reading(&before).unwrap());
    tick_until_stable_turn(
        &mut engine,
        &mut state,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
    );
    assert!(matches!(
        engine,
        EngineState::PendingChoice(PendingChoice::GridSelect { .. })
    ));
    assert_eq!(rust_reading(&state), java_reading(&pending).unwrap());
    // Damage uses real facing even while the visible marker is still stale.
    assert!(!surrounded::monster_has_back_attack_from_facing(&state, 1));
    assert!(surrounded::monster_has_back_attack_from_facing(&state, 2));
    tick_until_stable_turn(
        &mut engine,
        &mut state,
        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Grid,
            [lineage],
        )),
    );
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(rust_reading(&state), java_reading(&settled).unwrap());

    let mut wrong = pending.clone();
    wrong.producer.session_epoch.push_str("-other");
    assert!(!validate_adjacent(&before, &play, &wrong));
    let mut wrong = play.clone();
    wrong.receipt.successor_frame_id = Some(settled.frame_id);
    assert!(!validate_adjacent(&before, &wrong, &pending));
    let mut wrong = pending.clone();
    wrong.frame["boundary"]["stable"] = serde_json::json!(false);
    assert!(!validate_adjacent(&before, &play, &wrong));
    let mut missing = pending.clone();
    missing.frame["public_base"]["combat"]["player"]
        .as_object_mut()
        .unwrap()
        .remove("facing_left");
    assert!(java_reading(&missing).is_err());
}
