use crate::content::monsters::factory::EncounterId;
use crate::sim::combat_start::build_requested_combat_start;
use crate::state::core::{ActiveCombat, CombatStartRequest, EngineState};
use crate::state::map::node::RoomType;
use crate::state::run::RunState;

pub(super) fn ensure_combat_started_if_needed(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    active_combat: &mut Option<ActiveCombat>,
) -> Result<(), String> {
    match engine_state {
        EngineState::CombatStart(request) if active_combat.is_none() => {
            let request = request.clone();
            let active = build_active_combat(run_state, request)?;
            *engine_state = active.engine_state.clone();
            *active_combat = Some(active);
        }
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_)
            if active_combat.is_none() =>
        {
            let room_type = run_state
                .map
                .get_current_room_type()
                .ok_or_else(|| "combat engine state without current map room".to_string())?;
            let encounter = encounter_for_current_room(run_state, room_type)?;
            let request = CombatStartRequest::room(encounter, room_type);
            let active = build_active_combat(run_state, request)?;
            *engine_state = active.engine_state.clone();
            *active_combat = Some(active);
        }
        _ => {}
    }
    Ok(())
}

fn build_active_combat(
    run_state: &mut RunState,
    mut request: CombatStartRequest,
) -> Result<ActiveCombat, String> {
    let (engine_state, combat_state) = build_requested_combat_start(run_state, &mut request)?;
    Ok(ActiveCombat::new_for_encounter(
        engine_state,
        combat_state,
        request.encounter_id,
        request.context,
    ))
}

fn encounter_for_current_room(
    run_state: &mut RunState,
    room_type: RoomType,
) -> Result<EncounterId, String> {
    match room_type {
        RoomType::MonsterRoom => run_state
            .peek_next_encounter()
            .ok_or_else(|| "normal encounter queue is empty".to_string()),
        RoomType::MonsterRoomElite => run_state
            .peek_next_elite()
            .ok_or_else(|| "elite encounter queue is empty".to_string()),
        RoomType::MonsterRoomBoss => run_state
            .next_boss()
            .ok_or_else(|| "boss encounter queue is empty".to_string()),
        other => Err(format!("room type {other:?} is not combat")),
    }
}
