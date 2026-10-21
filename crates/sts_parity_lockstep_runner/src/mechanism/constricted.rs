use super::{combat_mechanism_input, validate_end_turn_evidence};
use sts_oracle_run_control::{
    content::{
        cards::CardId,
        monsters::{beyond::spire_growth::SpireGrowth, factory::EncounterId, MonsterBehavior},
        powers::{store, PowerId},
    },
    engine::core::tick_until_stable_turn,
    runtime::{
        combat::{Power, PowerPayload},
        rng::begin_sts_rng_trace,
    },
    state::{
        core::{ClientInput, EngineState},
        map::RoomType,
    },
};
use sts_parity_lockstep::{
    NaturalCombatCreatureV4, NaturalCombatPowerStateV4, NaturalCombatPowerV4, SnapshotComparisonV1,
};
use sts_parity_lockstep_java::{
    project_combat_rng_mechanism_v1, project_monster_ai_mechanism_v1, project_player_mechanism_v1,
    RecordedJavaActionV1, RecordedJavaBoundaryV1,
};

pub const CONSTRICTED_MECHANISM_SCOPE_V1: &str = "Constructed A20 Spire Growth EndTurn: player scalars/powers, monster state, floor.ai and floor.shuffle state plus complete calls; no cross-runtime card bindings, full combat or natural certification";

/// The live collector and offline fixtures execute this same restricted recipe.
/// Inputs come only from the pre-boundary; no successor HP delta seeds damage.
pub fn replay_constricted_mechanism_v1(
    before: &RecordedJavaBoundaryV1,
    action: &RecordedJavaActionV1,
    after: &RecordedJavaBoundaryV1,
) -> Result<SnapshotComparisonV1, String> {
    validate_end_turn_evidence(before, action, after)?;
    for raw in [&before.observation, &after.observation] {
        if raw.frame["scenario"]["provenance"] != "constructed_validated"
            || raw.frame["public_base"]["run"]["ascension"] != 20
        {
            return Err("Constricted experiment requires its constructed A20 provenance".into());
        }
    }
    let (player, powers) = project_player_mechanism_v1(&before.observation)?;
    let (monsters, ai) = project_monster_ai_mechanism_v1(&before.observation)?;
    let [input] = monsters.as_slice() else {
        return Err("Constricted requires one Spire Growth".into());
    };
    if input.enemy_id != "spiregrowth" || input.dead_or_escaped || player.current_hp <= 0 {
        return Err("Constricted requires living actors".into());
    }
    let (mut session, _bootstrap) =
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
    let source = monster.id;
    let plan = SpireGrowth::turn_plan(combat, &combat.entities.monsters[0]);
    combat.entities.monsters[0].set_planned_steps(plan.steps);
    combat.entities.monsters[0].set_planned_visible_spec(plan.visible_spec);
    store::set_powers_for(
        combat,
        source,
        input
            .powers
            .iter()
            .map(|p| mechanism_power(p, source))
            .collect::<Result<_, _>>()?,
    );
    store::set_powers_for(
        combat,
        0,
        powers
            .iter()
            .map(|p| mechanism_power(p, source))
            .collect::<Result<_, _>>()?,
    );
    combat.entities.player.current_hp = player.current_hp;
    combat.entities.player.max_hp = player.max_hp;
    combat.entities.player.block = player.block;
    combat.turn.energy = player.energy;
    // The played Defend is the only discard in the five-block case. Rebuild
    // that one-card shuffle with a Rust-owned starter Defend, without binding
    // it to the recorded Java UUID. Empty-discard cases need only safe draw.
    let discard = before.observation.frame["parity_internal"]["zones"]["discard"]["cards"]
        .as_array()
        .ok_or("mechanism discard missing")?;
    if !discard.is_empty() {
        if discard.len() != 1
            || discard[0]["card_id"] != "Defend_R"
            || discard[0]["upgrades"] != 0
            || !before.observation.frame["parity_internal"]["zones"]["draw"]["cards"]
                .as_array()
                .is_some_and(Vec::is_empty)
        {
            return Err(
                "Constricted recipe supports only the observed single Defend discard".into(),
            );
        }
        let defend = combat
            .zones
            .hand
            .iter()
            .chain(combat.zones.draw_pile.iter())
            .find(|c| c.id == CardId::Defend)
            .ok_or("Rust recipe Defend missing")?
            .clone();
        combat.zones.draw_pile.clear();
        combat.zones.discard_pile = vec![defend].into();
    } else if combat.zones.draw_pile.len() < 5 {
        return Err("mechanism safe draw missing".into());
    }
    combat.zones.hand.clear();
    let streams = project_combat_rng_mechanism_v1(&before.observation)?;
    let shuffle = streams
        .0
        .iter()
        .find(|s| s.owner == "floor.shuffle")
        .ok_or("shuffle stream missing")?;
    combat.rng.shuffle_rng.seed0 =
        u64::from_str_radix(&shuffle.state_hex[0], 16).map_err(|e| e.to_string())?;
    combat.rng.shuffle_rng.seed1 =
        u64::from_str_radix(&shuffle.state_hex[1], 16).map_err(|e| e.to_string())?;
    combat.rng.shuffle_rng.counter = shuffle.counter.ok_or("shuffle counter missing")?;
    combat.rng.ai_rng.seed0 =
        u64::from_str_radix(&ai.state_hex[0], 16).map_err(|e| e.to_string())?;
    combat.rng.ai_rng.seed1 =
        u64::from_str_radix(&ai.state_hex[1], 16).map_err(|e| e.to_string())?;
    combat.rng.ai_rng.counter = ai.counter.ok_or("AI counter missing")?;
    combat.rng.pool.bind_trace_owners();
    let trace = begin_sts_rng_trace().map_err(|e| e.to_string())?;
    if !tick_until_stable_turn(&mut session.engine_state, combat, ClientInput::EndTurn)
        || session.engine_state != EngineState::CombatPlayerTurn
    {
        return Err("Constricted did not reach the next player decision".into());
    }
    let calls = trace.finish().map_err(|e| e.to_string())?;
    super::end_turn::compare_creature_end_turn(
        "sts-constricted-creature-rng-mechanism-v1",
        combat,
        before,
        action,
        after,
        &calls,
    )
}

fn mechanism_power(
    power: &NaturalCombatPowerV4,
    source: sts_oracle_run_control::EntityId,
) -> Result<Power, String> {
    let power_type = match power.power_id.as_str() {
        "constricted" => PowerId::Constricted,
        "strength" => PowerId::Strength,
        "shackled" => PowerId::Shackled,
        "vulnerable" => PowerId::Vulnerable,
        "berserk" => PowerId::Berserk,
        other => return Err(format!("unsupported Constricted recipe power {other}")),
    };
    let (payload, just_applied) = match &power.state {
        NaturalCombatPowerStateV4::Stateless => (PowerPayload::None, false),
        NaturalCombatPowerStateV4::RoundDuration { just_applied } => {
            (PowerPayload::None, *just_applied)
        }
        NaturalCombatPowerStateV4::Constricted {
            source:
                NaturalCombatCreatureV4::Monster {
                    enemy_id,
                    occurrence,
                },
        } if enemy_id == "spiregrowth" && *occurrence == 0 => {
            (PowerPayload::SourceEntity(source), false)
        }
        other => {
            return Err(format!(
                "unsupported Constricted recipe power state {other:?}"
            ))
        }
    };
    Ok(Power {
        power_type,
        amount: power.amount,
        payload,
        just_applied,
        instance_id: None,
        extra_data: 0,
    })
}
