//! Bounded, analyst-authored mechanism experiments. Neither a natural-run
//! importer nor a full-state combat comparator. Live collectors and offline
//! regressions call the same verifier; raw evidence remains immutable.

use serde_json::Value;
use sts_oracle_run_control::{
    content::monsters::{beyond::time_eater::TimeEater, MonsterBehavior},
    engine::{action_handlers::execute_action, core::tick_until_stable_turn},
    runtime::{
        combat::{Power, PowerId, PowerPayload},
        rng::StsRng,
    },
    state::core::{ClientInput, EngineState},
};
use sts_parity_lockstep::{
    compare_snapshots_v1, DomainEvidenceV1, DomainObservationV1, LockstepAdapterV1,
    NaturalCombatMonsterV4, NaturalCombatPowerStateV4, NaturalRunRngStreamV2, ParityDomainV1,
    ParitySnapshotV1, ProducerIdentityV1, SemanticActionV1, SnapshotComparisonV1,
    SnapshotContractV1,
};
use sts_parity_lockstep_java::{
    project_monster_ai_mechanism_v1, RecordedJavaActionPhaseV1, RecordedJavaActionV1,
    RecordedJavaBoundaryV1,
};
use sts_parity_lockstep_rust::RustLockstepAdapterV1;

pub const HASTE_MECHANISM_SCOPE_V1: &str =
    "Time Eater EndTurn: monster HP/block/powers/intent/history and floor.ai state/count only; no player/deck/full RNG ledger or natural-run certification";

#[cfg(test)]
mod tests;

mod batch;
mod end_turn;
mod hp_draw;
mod hp_draw_batch;
pub use hp_draw::{
    replay_hp_draw_mechanism_v1, HpDrawCaseV1, HpDrawPowerCaseV1, HP_DRAW_MECHANISM_SCOPE_V1,
};
pub use hp_draw_batch::{
    record_hp_draw_batch_v1, record_hp_draw_powers_batch_v1, record_hp_draw_prevention_batch_v1,
    replay_hp_draw_batch_v1,
};
mod regret;
mod regret_batch;
pub use batch::{record_shackled_haste_batch_v1, replay_shackled_haste_batch_v1};
pub use regret::{
    replay_regret_mechanism_v1, COMBUST_MECHANISM_SCOPE_V1, REGRET_MECHANISM_SCOPE_V1,
    REGRET_ORDER_MECHANISM_SCOPE_V1, RUPTURE_MECHANISM_SCOPE_V1,
};
pub use regret_batch::{
    record_combust_batch_v1, record_regret_batch_v1, record_regret_order_batch_v1,
    record_rupture_batch_v1, replay_combust_batch_v1, replay_regret_batch_v1,
    replay_regret_order_batch_v1, replay_rupture_batch_v1,
};
mod constricted;
mod constricted_batch;
pub use constricted::{replay_constricted_mechanism_v1, CONSTRICTED_MECHANISM_SCOPE_V1};
pub use constricted_batch::{record_constricted_batch_v1, replay_constricted_batch_v1};

/// Rebuild only the declared mechanism input, execute the current Rust engine,
/// and compare through the shared comparator. Does not contact Java or search.
pub fn replay_haste_mechanism_v1(
    before: &RecordedJavaBoundaryV1,
    action: &RecordedJavaActionV1,
    after: &RecordedJavaBoundaryV1,
) -> Result<SnapshotComparisonV1, String> {
    validate_end_turn_evidence(before, action, after)?;
    let ascension = before.observation.frame["public_base"]["run"]["ascension"]
        .as_u64()
        .filter(|level| *level <= 20)
        .ok_or("Haste mechanism omitted a supported ascension")? as u8;
    if after.observation.frame["public_base"]["run"]["ascension"] != serde_json::json!(ascension) {
        return Err("Haste mechanism changed ascension".to_owned());
    }
    let (monsters, ai) = project_monster_ai_mechanism_v1(&before.observation)?;
    let [input] = monsters.as_slice() else {
        return Err("Haste mechanism requires exactly one monster".to_owned());
    };
    if input.enemy_id != "timeeater"
        || input.next_move_id != 5
        || input.dead_or_escaped
        || input.current_hp <= 0
        || input.current_hp >= input.max_hp / 2
    {
        return Err(
            "Haste mechanism requires a living Time Eater below half HP planning Haste".to_owned(),
        );
    }
    let (mut session, bootstrap) = time_eater_mechanism_input(ascension)?;
    let combat = &mut session
        .active_combat
        .as_mut()
        .ok_or("Rust combat missing")?
        .combat_state;
    let monster = &mut combat.entities.monsters[0];
    monster.current_hp = input.current_hp;
    monster.max_hp = input.max_hp;
    monster.block = input.block;
    monster.move_history_mut().clear();
    monster
        .move_history_mut()
        .extend(input.move_history.iter().copied());
    monster.time_eater.used_haste = false;
    let plan = TimeEater::roll_move_plan(&mut StsRng::new(1), monster, ascension, 0);
    if plan.move_id != 5 {
        return Err("current Rust input does not plan Haste".to_owned());
    }
    let updates = TimeEater::on_roll_move(ascension, monster, 0, &plan);
    monster.set_planned_move_id(plan.move_id);
    monster.set_planned_steps(plan.steps);
    monster.set_planned_visible_spec(plan.visible_spec);
    for update in updates {
        execute_action(update, combat);
    }
    let powers = input
        .powers
        .iter()
        .map(|power| {
            let just_applied = match power.state {
                NaturalCombatPowerStateV4::Stateless => false,
                NaturalCombatPowerStateV4::RoundDuration { just_applied } => just_applied,
                _ => {
                    return Err(format!(
                        "unsupported Haste input power state: {}",
                        power.power_id
                    ))
                }
            };
            Ok(Power {
                power_type: match power.power_id.as_str() {
                    "timewarp" => PowerId::TimeWarp,
                    "strength" => PowerId::Strength,
                    "vulnerable" => PowerId::Vulnerable,
                    "weak" => PowerId::Weak,
                    "shackled" => PowerId::Shackled,
                    other => return Err(format!("unsupported Haste input power: {other}")),
                },
                amount: power.amount,
                instance_id: None,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    combat.entities.power_db.insert(1, powers);
    combat.rng.ai_rng.seed0 =
        u64::from_str_radix(&ai.state_hex[0], 16).map_err(|e| e.to_string())?;
    combat.rng.ai_rng.seed1 =
        u64::from_str_radix(&ai.state_hex[1], 16).map_err(|e| e.to_string())?;
    combat.rng.ai_rng.counter = ai.counter.ok_or("Haste AI counter missing")?;
    if !tick_until_stable_turn(&mut session.engine_state, combat, ClientInput::EndTurn)
        || session.engine_state != EngineState::CombatPlayerTurn
        || !combat.entities.monsters[0].time_eater.used_haste
    {
        return Err("Haste mechanism did not settle at the next Rust player turn".to_owned());
    }
    let mut rust = RustLockstepAdapterV1::new(
        session,
        bootstrap,
        env!("CARGO_PKG_VERSION"),
        "haste-mechanism",
    )?;
    let snapshot = rust.observe().map_err(|error| error.message)?;
    let monster: NaturalCombatMonsterV4 = serde_json::from_value(
        observed_domain(&snapshot, ParityDomainV1::Combat)?["state"]["monsters"][0].clone(),
    )
    .map_err(|error| error.to_string())?;
    let rng = observed_domain(&snapshot, ParityDomainV1::RngStreams)?
        .as_array()
        .ok_or("Rust RNG streams missing")?
        .iter()
        .find(|stream| stream["owner"] == "floor.ai")
        .ok_or("Rust floor.ai missing")?;
    let rng: NaturalRunRngStreamV2 =
        serde_json::from_value(rng.clone()).map_err(|error| error.to_string())?;
    let rust = mechanism_snapshot(snapshot.producer, vec![monster], rng)?;
    let (monsters, ai) = project_monster_ai_mechanism_v1(&after.observation)?;
    let java = mechanism_snapshot(after.observation.producer.clone(), monsters, ai)?;
    compare_snapshots_v1(&java, &rust)
}

fn validate_end_turn_evidence(
    before: &RecordedJavaBoundaryV1,
    action: &RecordedJavaActionV1,
    after: &RecordedJavaBoundaryV1,
) -> Result<(), String> {
    before.validate()?;
    action.validate()?;
    after.validate()?;
    for boundary in [before, after] {
        if boundary.observation.frame["boundary"]["stable"] != true
            || boundary.observation.frame["boundary"]["kind"] != "combat_player_decision"
        {
            return Err("mechanism evidence is not a stable combat player decision".to_owned());
        }
    }
    let before = &before.observation;
    let after = &after.observation;
    if action.phase != RecordedJavaActionPhaseV1::Committed
        || action.action != SemanticActionV1::EndTurn
        || action.receipt.status != "completed"
        || action.receipt.based_on_frame_id != before.frame_id
        || action.receipt.successor_frame_id != Some(after.frame_id)
        || after.frame_id <= before.frame_id
        || before.producer != after.producer
        || before.frame["frame_id"] != serde_json::json!(before.frame_id)
        || after.frame["frame_id"] != serde_json::json!(after.frame_id)
        || after.frame["previous_frame_id"] != serde_json::json!(before.frame_id)
        || !before.actions.iter().any(|candidate| {
            candidate.kind == "end_turn"
                && candidate.action_id == action.receipt.action_id
                && !candidate.parameters_required
        })
    {
        return Err(
            "mechanism evidence is not one completed, same-producer EndTurn transition".to_owned(),
        );
    }
    Ok(())
}

fn observed_domain(snapshot: &ParitySnapshotV1, domain: ParityDomainV1) -> Result<&Value, String> {
    match snapshot
        .domains
        .get(&domain)
        .map(|observation| &observation.evidence)
    {
        Some(DomainEvidenceV1::Observed { value, .. }) => Ok(value),
        _ => Err(format!("Rust mechanism omitted {domain:?}")),
    }
}

fn mechanism_snapshot(
    producer: ProducerIdentityV1,
    monsters: Vec<NaturalCombatMonsterV4>,
    ai: NaturalRunRngStreamV2,
) -> Result<ParitySnapshotV1, String> {
    // Separate contract: these domain subsets must never masquerade as the
    // complete natural Combat/RNG snapshot contract.
    Ok(ParitySnapshotV1 {
        observation_id: "haste-mechanism-successor".to_owned(),
        boundary_kind: "mechanism_monster_end_turn".to_owned(),
        producer,
        contract: SnapshotContractV1::new(
            "sts-haste-monster-ai-mechanism-v1",
            [
                (ParityDomainV1::Combat, true),
                (ParityDomainV1::RngStreams, true),
            ],
        )?,
        domains: [
            (
                ParityDomainV1::Combat,
                DomainObservationV1::observed(
                    "sts-mechanism-monsters-v4",
                    serde_json::json!(monsters),
                ),
            ),
            (
                ParityDomainV1::RngStreams,
                DomainObservationV1::observed("sts-mechanism-ai-v2", serde_json::json!([ai])),
            ),
        ]
        .into_iter()
        .collect(),
    })
}

fn time_eater_mechanism_input(
    ascension_level: u8,
) -> Result<
    (
        sts_oracle_run_control::eval::run_control::RunControlSession,
        sts_parity_lockstep::NaturalRunBootstrapV1,
    ),
    String,
> {
    combat_mechanism_input(
        ascension_level,
        sts_oracle_run_control::content::monsters::factory::EncounterId::TimeEater,
        sts_oracle_run_control::state::map::RoomType::MonsterRoomBoss,
    )
}

fn combat_mechanism_input(
    ascension_level: u8,
    encounter: sts_oracle_run_control::content::monsters::factory::EncounterId,
    room: sts_oracle_run_control::state::map::RoomType,
) -> Result<
    (
        sts_oracle_run_control::eval::run_control::RunControlSession,
        sts_parity_lockstep::NaturalRunBootstrapV1,
    ),
    String,
> {
    use sts_oracle_run_control::{
        engine::run_loop::tick_run_active_with_observer,
        eval::run_control::{RunControlConfig, RunControlSession},
        state::core::CombatStartRequest,
    };
    let config = RunControlConfig {
        ascension_level,
        seed: 1,
        ..Default::default()
    };
    let bootstrap = sts_parity_lockstep_rust::natural_run_bootstrap_v1(&config)?;
    let mut session = RunControlSession::new(config);
    session.run_state.event_state = None;
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 0;
    session.run_state.map.graph[0][0].class = Some(room);
    session.engine_state = EngineState::CombatStart(CombatStartRequest::room(encounter, room));
    tick_run_active_with_observer(
        &mut session.engine_state,
        &mut session.run_state,
        &mut session.active_combat,
        None,
    );
    Ok((session, bootstrap))
}
