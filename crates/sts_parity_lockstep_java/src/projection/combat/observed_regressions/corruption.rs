//! Identity-free card cost facts from a raw Java EndTurn, compared with an
//! explicitly constructed Rust mechanism input. No natural card bindings.
use super::super::*;
use crate::{
    observed_regression_fixtures::{load_from, observation_from, validate_adjacent},
    RecordedJavaActionV1,
};
use sts_oracle_run_control::{
    content::{
        cards::CardId,
        monsters::{exordium::jaw_worm, EnemyId},
        powers::{store, PowerId},
    },
    engine::core::tick_until_stable_turn,
    runtime::combat::{CombatCard, Power, PowerPayload},
    state::core::{ClientInput, EngineState},
    test_support::{combat_with_monsters, planned_monster},
};
use sts_parity_lockstep::SemanticActionV1;

#[derive(Debug, PartialEq)]
struct CardCosts {
    card_id: String,
    upgrades: u8,
    combat: i32,
    turn: i32,
}

// Deliberately reads only observed cost facts. Calling the full card projector
// would require source-proven natural bindings that this mechanism test lacks.
fn java_costs(raw: &JavaRuntimeObservationV1, zone: &str) -> Result<Vec<CardCosts>, String> {
    let zone = &raw.frame["parity_internal"]["zones"][zone];
    if zone["ordering"] != "java_group_index_ascending" || zone["top"] != "last" {
        return Err("card cost evidence omitted its ordering".into());
    }
    zone["cards"]
        .as_array()
        .ok_or("card cost evidence omitted cards")?
        .iter()
        .map(|card| {
            Ok(CardCosts {
                card_id: canonical_game_id(required_str(card, "card_id")?),
                upgrades: required_u64(card, "upgrades")?
                    .try_into()
                    .map_err(|_| "card upgrades exceed u8")?,
                combat: required_i32(card, "combat_cost")?,
                turn: required_i32(card, "cost_for_turn")?,
            })
        })
        .collect()
}

fn rust_costs<'a>(cards: impl IntoIterator<Item = &'a CombatCard>) -> Vec<CardCosts> {
    cards
        .into_iter()
        .map(|card| CardCosts {
            card_id: canonical_game_id(card.id.stable_id()),
            upgrades: card.upgrades,
            combat: card.combat_cost_without_turn_override_java(),
            turn: card.cost_for_turn_java(),
        })
        .collect()
}

fn recipe(slot: u32, id: CardId, combat_cost: i32, turn_cost: i32) -> CombatCard {
    let mut card = CombatCard::for_scenario(id, slot);
    card.set_combat_cost_preserving_turn_java(combat_cost);
    card.set_cost_for_turn_java(turn_cost);
    card
}

#[test]
fn recorded_corruption_end_turn_refreshes_remaining_hand_before_each_discard() {
    let before = observation_from("corruption_discard", "before.json");
    let after = observation_from("corruption_discard", "after.json");
    let end: RecordedJavaActionV1 =
        serde_json::from_value(load_from("corruption_discard", "end-turn.json")).unwrap();
    assert!(validate_adjacent(&before, &end, &after));
    assert_eq!(end.action, SemanticActionV1::EndTurn);
    for raw in [&before, &after] {
        assert_eq!(raw.frame["boundary"]["kind"], "combat_player_decision");
        let powers =
            project_powers_v4(&raw.frame["parity_internal"]["player_powers"], "player").unwrap();
        let corruption = powers.iter().find(|p| p.power_id == "corruption").unwrap();
        assert_eq!(corruption.amount, -1);
        assert_eq!(corruption.state, NaturalCombatPowerStateV4::Stateless);
    }

    // UUIDs below relate Java objects only to themselves across its adjacent
    // frames. They never label, select or bind a Rust scenario card.
    let mut previous = BTreeMap::new();
    for zone in ["hand", "discard"] {
        for card in before.frame["parity_internal"]["zones"][zone]["cards"]
            .as_array()
            .unwrap()
        {
            let uuid = required_str(card, "instance_uuid").unwrap();
            assert!(previous
                .insert(
                    uuid.to_owned(),
                    (card["card_id"].clone(), card["upgrades"].clone())
                )
                .is_none());
        }
    }
    for card in after.frame["parity_internal"]["zones"]["discard"]["cards"]
        .as_array()
        .unwrap()
    {
        let uuid = required_str(card, "instance_uuid").unwrap();
        assert_eq!(
            previous.remove(uuid),
            Some((card["card_id"].clone(), card["upgrades"].clone()))
        );
    }
    assert!(
        previous.is_empty(),
        "all original hand/discard objects remain in Java discard"
    );

    // This is a recipe, not a capture import. The full ordered cost facts are
    // checked before executing the current engine, with no UUID correspondence.
    let hand = [
        (CardId::FlameBarrier, 0, 0),
        (CardId::Anger, 0, 0),
        (CardId::Wound, -2, -2),
        (CardId::DoubleTap, 0, 0),
        (CardId::Warcry, 0, 0),
        (CardId::Whirlwind, -1, -1),
        (CardId::InfernalBlade, 1, 0),
        (CardId::HeavyBlade, 2, 2),
        (CardId::Strike, 1, 1),
        (CardId::Brutality, 0, 0),
    ]
    .into_iter()
    .enumerate()
    .map(|(slot, (id, combat, turn))| recipe(slot as u32, id, combat, turn))
    .collect::<Vec<_>>();
    let discard = [
        CardId::BurningPact,
        CardId::WildStrike,
        CardId::FireBreathing,
        CardId::Rage,
    ]
    .into_iter()
    .enumerate()
    .map(|(slot, id)| CombatCard::for_scenario(id, 10 + slot as u32))
    .collect::<Vec<_>>();
    assert_eq!(rust_costs(&hand), java_costs(&before, "hand").unwrap());
    assert_eq!(
        rust_costs(&discard),
        java_costs(&before, "discard").unwrap()
    );
    let mut monster = planned_monster(EnemyId::JawWorm, 2);
    jaw_worm::initialize_runtime_state(&mut monster, false);
    let mut state = combat_with_monsters(vec![monster]);
    state.zones.hand = hand;
    state.zones.discard_pile = discard.into();
    // Enough fresh draw cards prevent a shuffle from reintroducing discarded
    // skills into the hand and masking an incorrect persistent cost.
    state.zones.draw_pile = (20..25)
        .map(|slot| CombatCard::for_scenario(CardId::Strike, slot))
        .collect::<Vec<_>>()
        .into();
    store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Corruption,
            amount: -1,
            extra_data: 0,
            instance_id: None,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let turn_before = state.turn.turn_count;
    let mut engine = EngineState::CombatPlayerTurn;
    assert!(tick_until_stable_turn(
        &mut engine,
        &mut state,
        ClientInput::EndTurn
    ));
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(state.turn.turn_count, turn_before + 1);
    assert_eq!(state.zones.hand.len(), 5);
    assert!(state.zones.draw_pile.is_empty());
    assert!(state.zones.exhaust_pile.is_empty());
    assert_eq!(
        rust_costs(&state.zones.discard_pile),
        java_costs(&after, "discard").unwrap()
    );
    let blade = state
        .zones
        .discard_pile
        .iter()
        .find(|card| card.id == CardId::InfernalBlade)
        .unwrap();
    assert_eq!(
        (
            blade.combat_cost_without_turn_override_java(),
            blade.cost_for_turn_java()
        ),
        (0, 0)
    );
}

#[test]
fn recorded_corruption_cost_evidence_requires_both_costs_and_pile_order() {
    let before = observation_from("corruption_discard", "before.json");
    for key in ["combat_cost", "cost_for_turn"] {
        let mut missing = before.clone();
        missing.frame["parity_internal"]["zones"]["hand"]["cards"][6]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(java_costs(&missing, "hand").is_err());
    }
    let mut unordered = before.clone();
    unordered.frame["parity_internal"]["zones"]["hand"]["top"] = serde_json::json!("first");
    assert!(java_costs(&unordered, "hand").is_err());
}
