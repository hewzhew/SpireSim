use super::{
    combat_mechanism_input, end_turn::compare_creature_end_turn, validate_end_turn_evidence,
};
use serde_json::Value;
use sts_oracle_run_control::{
    content::{
        cards::CardId,
        monsters::{beyond::spire_growth::SpireGrowth, factory::EncounterId, MonsterBehavior},
        powers::{store, PowerId},
        relics::{RelicId, RelicState},
    },
    engine::core::tick_until_stable_turn,
    runtime::{
        combat::{CombatCard, Power, PowerPayload},
        rng::begin_sts_rng_trace,
    },
    state::{
        core::{ClientInput, EngineState},
        map::RoomType,
    },
};
use sts_parity_lockstep::{NaturalCombatPowerStateV4, SnapshotComparisonV1};
use sts_parity_lockstep_java::{
    project_combat_rng_mechanism_v1, project_monster_ai_mechanism_v1, project_player_mechanism_v1,
    RecordedJavaActionV1, RecordedJavaBoundaryV1,
};

pub const REGRET_MECHANISM_SCOPE_V1: &str = "Constructed A20 Regret EndTurn with Barricade/block, optional Buffer/Tungsten Rod: player scalars/powers, monster state, AI/shuffle states and complete RNG calls; no cross-runtime card bindings, full relic/deck comparison or natural certification";

pub const REGRET_ORDER_MECHANISM_SCOPE_V1: &str = "Constructed A20 EndTurn: one-point Regret with Tungsten Rod/optional Buffer, or ordered Regret/Burn/Wound with zero/five block and optional Buffer; creatures, AI/shuffle states and complete RNG calls only; no card bindings, full relic/deck comparison or natural certification";

pub const RUPTURE_MECHANISM_SCOPE_V1: &str = "Constructed A20 Rupture+ EndTurn with Regret or Brutality, optional Buffer/Tungsten Rod: creatures, AI/shuffle states and complete RNG calls only; no card bindings, full relic/deck comparison or natural certification";

pub const COMBUST_MECHANISM_SCOPE_V1: &str = "Constructed A20 Rupture+ with normal/upgraded/stacked Combust and optional Regret/Buffer/Tungsten Rod: creatures including independent Combust HP loss, AI/shuffle states and complete RNG calls only; no card bindings, full relic/deck comparison or natural certification";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CombustSetup {
    Base,
    Upgraded,
    Stacked,
}
impl CombustSetup {
    pub(super) const fn values(self) -> (i32, i32) {
        match self {
            Self::Base => (5, 1),
            Self::Upgraded => (7, 1),
            Self::Stacked => (12, 2),
        }
    }
    pub(super) fn upgrades(self) -> &'static [usize] {
        match self {
            Self::Base => &[0],
            Self::Upgraded => &[1],
            Self::Stacked => &[0, 1],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Preparation {
    Barricade,
    Rupture,
    RuptureBrutality,
    RuptureCombust(CombustSetup),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RegretHand {
    Empty,
    Alone,
    Padded,
    Mixed,
}

#[derive(Debug)]
pub(super) struct RegretInput {
    pub(super) preparation: Preparation,
    pub(super) buffer: bool,
    pub(super) tungsten: bool,
    pub(super) block: i32,
    pub(super) shape: RegretHand,
    // Ordered semantic ingredients, not cross-runtime instance bindings.
    pub(super) hand: Vec<CardId>,
}

/// Closed input recipe, not a Java combat importer. Rust-only scenario slots
/// never bind to a Java UUID. Hand order is a transition input; its shape is
/// closed to Regret alone, Regret/Wound/Wound, Regret/Burn/Wound, or
/// empty with Rupture+ and Brutality/Combust. Combust accepts only the three
/// declared normal/upgraded/stacked preparations; it is not a power importer.
pub fn replay_regret_mechanism_v1(
    before: &RecordedJavaBoundaryV1,
    action: &RecordedJavaActionV1,
    after: &RecordedJavaBoundaryV1,
) -> Result<SnapshotComparisonV1, String> {
    validate_end_turn_evidence(before, action, after)?;
    let recipe = validate_input(before)?;
    if after.observation.frame["scenario"] != before.observation.frame["scenario"]
        || after.observation.frame["public_base"]["run"]["ascension"] != 20
        || after.observation.frame["public_base"]["run"]["character"] != "IRONCLAD"
    {
        return Err("Regret successor changed constructed scenario".into());
    }
    let (player, powers) = project_player_mechanism_v1(&before.observation)?;
    let (monsters, _) = project_monster_ai_mechanism_v1(&before.observation)?;
    let input = &monsters[0];
    let (mut session, _) =
        combat_mechanism_input(20, EncounterId::SpireGrowth, RoomType::MonsterRoom)?;
    let combat = &mut session
        .active_combat
        .as_mut()
        .ok_or("Rust combat missing")?
        .combat_state;
    let monster = &mut combat.entities.monsters[0];
    monster.current_hp = input.current_hp;
    monster.max_hp = input.max_hp;
    monster.block = input.block;
    monster.set_planned_move_id(input.next_move_id);
    monster.move_history_mut().clear();
    monster
        .move_history_mut()
        .extend(input.move_history.iter().copied());
    let plan = SpireGrowth::turn_plan(combat, &combat.entities.monsters[0]);
    combat.entities.monsters[0].set_planned_steps(plan.steps);
    combat.entities.monsters[0].set_planned_visible_spec(plan.visible_spec);
    store::set_powers_for(
        combat,
        0,
        powers
            .iter()
            .map(|p| Power {
                power_type: match p.power_id.as_str() {
                    "barricade" => PowerId::Barricade,
                    "buffer" => PowerId::Buffer,
                    "rupture" => PowerId::Rupture,
                    "brutality" => PowerId::Brutality,
                    "combust" => PowerId::Combust,
                    _ => unreachable!("closed input powers were validated"),
                },
                amount: p.amount,
                instance_id: None,
                extra_data: match p.state {
                    NaturalCombatPowerStateV4::Combust { hp_loss } => hp_loss,
                    _ => 0,
                },
                payload: PowerPayload::None,
                just_applied: false,
            })
            .collect(),
    );
    combat.entities.player.current_hp = player.current_hp;
    combat.entities.player.max_hp = player.max_hp;
    combat.entities.player.block = player.block;
    combat.turn.energy = player.energy;
    combat.entities.player.relics.clear();
    combat.entities.player.relic_buses = Default::default();
    if recipe.buffer {
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::FossilizedHelix));
    }
    if recipe.tungsten {
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::TungstenRod));
    }
    combat.zones.hand = recipe
        .hand
        .iter()
        .enumerate()
        .map(|(slot, id)| CombatCard::for_scenario(*id, slot as u32))
        .collect();
    combat.zones.draw_pile.clear();
    combat.zones.discard_pile.clear();
    if recipe.block == 5 {
        combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Defend, 3)].into();
    }
    combat.zones.exhaust_pile.clear();
    let streams = project_combat_rng_mechanism_v1(&before.observation)?;
    for owner in ["floor.ai", "floor.shuffle"] {
        let rng = if owner == "floor.ai" {
            &mut combat.rng.ai_rng
        } else {
            &mut combat.rng.shuffle_rng
        };
        let stream = streams
            .0
            .iter()
            .find(|s| s.owner == owner)
            .ok_or("Regret RNG input missing")?;
        rng.seed0 = u64::from_str_radix(&stream.state_hex[0], 16).map_err(|e| e.to_string())?;
        rng.seed1 = u64::from_str_radix(&stream.state_hex[1], 16).map_err(|e| e.to_string())?;
        rng.counter = stream.counter.ok_or("Regret RNG counter missing")?;
    }
    combat.rng.pool.bind_trace_owners();
    let trace = begin_sts_rng_trace().map_err(|e| e.to_string())?;
    if !tick_until_stable_turn(&mut session.engine_state, combat, ClientInput::EndTurn)
        || session.engine_state != EngineState::CombatPlayerTurn
    {
        return Err("Regret did not settle at next player decision".into());
    }
    let calls = trace.finish().map_err(|e| e.to_string())?;
    compare_creature_end_turn(
        "sts-regret-creature-rng-mechanism-v1",
        combat,
        before,
        action,
        after,
        &calls,
    )
}

pub(super) fn validate_input(before: &RecordedJavaBoundaryV1) -> Result<RegretInput, String> {
    let raw = &before.observation;
    let (player, powers) = project_player_mechanism_v1(raw)?;
    let (monsters, _) = project_monster_ai_mechanism_v1(raw)?;
    let has_power = |id: &str, amount| {
        powers
            .iter()
            .any(|p| p.power_id == id && p.amount == amount)
    };
    let preparation = if has_power("barricade", -1) {
        Preparation::Barricade
    } else if has_power("rupture", 2) {
        if let Some(combust) = powers.iter().find(|p| p.power_id == "combust") {
            let setup = [
                CombustSetup::Base,
                CombustSetup::Upgraded,
                CombustSetup::Stacked,
            ]
            .into_iter()
            .find(|setup| {
                let (damage, hp_loss) = setup.values();
                combust.amount == damage
                    && combust.state == (NaturalCombatPowerStateV4::Combust { hp_loss })
            })
            .ok_or("Combust is not a declared normal/upgraded/stacked input")?;
            Preparation::RuptureCombust(setup)
        } else if has_power("brutality", 1) {
            Preparation::RuptureBrutality
        } else {
            Preparation::Rupture
        }
    } else {
        return Err("EndTurn requires declared Barricade or Rupture+ preparation".into());
    };
    let energy = match preparation {
        Preparation::Barricade => {
            if player.block == 5 {
                0
            } else {
                1
            }
        }
        Preparation::Rupture | Preparation::RuptureBrutality => 2,
        Preparation::RuptureCombust(setup) => 2 - setup.values().1,
    };
    let valid = raw.frame["scenario"]["provenance"] == "constructed_validated"
        && raw.frame["public_base"]["run"]["ascension"] == 20
        && raw.frame["public_base"]["run"]["character"] == "IRONCLAD"
        && raw.frame["public_base"]["combat"]["turn"] == 1
        && player.current_hp == 80
        && player.max_hp == 80
        && [0, 5].contains(&player.block)
        && i32::from(player.energy) == energy
        && (preparation == Preparation::Barricade || player.block == 0)
        && monsters.len() == 1
        && monsters[0].enemy_id == "spiregrowth"
        && !monsters[0].dead_or_escaped
        && monsters[0].current_hp > 0
        && monsters[0].next_move_id == 2
        && monsters[0].powers.is_empty();
    if !valid {
        return Err("Regret preparation did not reach declared block/Spire Growth input".into());
    }
    let relics = raw.frame["parity_internal"]["relics"]
        .as_array()
        .ok_or("Regret relic input missing")?;
    let ids = relics
        .iter()
        .map(|r| r["relic_id"].as_str().ok_or("Regret relic ID missing"))
        .collect::<Result<Vec<_>, _>>()?;
    let buffer = ids.contains(&"FossilizedHelix");
    let tungsten = ids.contains(&"TungstenRod");
    if ids.len() != usize::from(buffer) + usize::from(tungsten)
        || ids
            .iter()
            .any(|id| !["FossilizedHelix", "TungstenRod"].contains(id))
        || powers.len()
            != 1 + usize::from(buffer)
                + usize::from(matches!(
                    preparation,
                    Preparation::RuptureBrutality | Preparation::RuptureCombust(_)
                ))
        || (buffer
            && !powers
                .iter()
                .any(|p| p.power_id == "buffer" && p.amount == 1))
        || powers.iter().any(|p| {
            p.state != NaturalCombatPowerStateV4::Stateless
                && !matches!(
                    (&*p.power_id, &p.state),
                    ("combust", NaturalCombatPowerStateV4::Combust { .. })
                )
        })
    {
        return Err(
            "EndTurn requires only its declared preparation and Buffer/Tungsten Rod input".into(),
        );
    }
    let zones = &raw.frame["parity_internal"]["zones"];
    let hand = ordered_cards(zones, "hand")?;
    let count = |id| hand.iter().filter(|&&card| card == id).count();
    let shape = if hand.is_empty()
        && matches!(
            preparation,
            Preparation::RuptureBrutality | Preparation::RuptureCombust(_)
        ) {
        RegretHand::Empty
    } else if preparation == Preparation::RuptureBrutality {
        return Err("Brutality recipe requires empty hand".into());
    } else if hand == [CardId::Regret] {
        RegretHand::Alone
    } else if hand.len() == 3 && count(CardId::Regret) == 1 && count(CardId::Wound) == 2 {
        RegretHand::Padded
    } else if hand.len() == 3
        && count(CardId::Regret) == 1
        && count(CardId::Burn) == 1
        && count(CardId::Wound) == 1
    {
        RegretHand::Mixed
    } else {
        return Err("Regret requires a supported closed hand recipe".into());
    };
    let expected_discard = if player.block == 5 {
        vec![CardId::Defend]
    } else {
        Vec::new()
    };
    if ordered_cards(zones, "discard")? != expected_discard
        || !ordered_cards(zones, "draw")?.is_empty()
        || !ordered_cards(zones, "exhaust")?.is_empty()
        || !ordered_cards(zones, "limbo")?.is_empty()
    {
        return Err(
            "Regret requires empty draw/exhaust/limbo and only its played Defend discard".into(),
        );
    }
    Ok(RegretInput {
        preparation,
        buffer,
        tungsten,
        block: player.block,
        shape,
        hand,
    })
}

fn ordered_cards(zones: &Value, zone: &str) -> Result<Vec<CardId>, String> {
    if zones[zone]["ordering"] != "java_group_index_ascending" || zones[zone]["top"] != "last" {
        return Err(format!("Regret {zone} omitted its original card order"));
    }
    zones[zone]["cards"]
        .as_array()
        .ok_or_else(|| format!("Regret {zone} missing"))?
        .iter()
        .map(|card| {
            if card["upgrades"] != 0 {
                return Err("Regret recipe requires unupgraded hand/discard cards".into());
            }
            match card["card_id"].as_str() {
                Some("Regret") => Ok(CardId::Regret),
                Some("Burn") => Ok(CardId::Burn),
                Some("Wound") => Ok(CardId::Wound),
                Some("Defend_R") => Ok(CardId::Defend),
                _ => Err("Regret recipe has an unsupported card".into()),
            }
        })
        .collect()
}
