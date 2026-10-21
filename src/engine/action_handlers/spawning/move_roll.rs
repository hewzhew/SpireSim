use crate::runtime::combat::CombatState;
pub fn handle_roll_monster_move(monster_id: usize, state: &mut CombatState) {
    if let Some(monster_index) = state
        .entities
        .monsters
        .iter()
        .position(|monster| monster.id == monster_id)
    {
        // Java RollMoveAction.update() calls monster.rollMove() without checking
        // isDying/isEscaping. Queued rolls after thorns kills, SuicideAction, or
        // slime split still consume aiRng and update moveHistory.
        let outcome = {
            let entities = &state.entities;
            let rng = &mut state.rng.ai_rng;
            let num = rng.random(99);
            let player_powers = entities
                .power_db
                .get(&0)
                .map(Vec::as_slice)
                .unwrap_or_default();
            crate::content::monsters::roll_monster_turn_outcome(
                rng,
                &entities.monsters[monster_index],
                state.meta.ascension_level,
                num,
                &entities.monsters,
                player_powers,
            )
        };
        for action in outcome.setup_actions {
            crate::engine::action_handlers::execute_action(action, state);
        }
        let plan = outcome.plan;
        let mut updated_monster_id = None;
        if let Some(m) = state
            .entities
            .monsters
            .iter_mut()
            .find(|m| m.id == monster_id)
        {
            m.set_planned_move_id(plan.move_id);
            m.set_planned_steps(plan.steps);
            m.set_planned_visible_spec(plan.visible_spec);
            m.move_history_mut().push_back(plan.move_id);
            updated_monster_id = Some(m.id);
        }
        if let Some(updated_monster_id) = updated_monster_id {
            state.clear_monster_protocol_observation(updated_monster_id);
        }
    }
}

pub fn handle_set_monster_move(
    monster_id: usize,
    next_move_byte: u8,
    planned_steps: crate::runtime::monster_move::MonsterTurnSteps,
    planned_visible_spec: Option<crate::runtime::monster_move::MonsterMoveSpec>,
    state: &mut CombatState,
) {
    let mut updated_monster_id = None;
    if let Some(m) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        m.set_planned_move_id(next_move_byte);
        m.set_planned_steps(planned_steps);
        m.set_planned_visible_spec(planned_visible_spec);
        m.move_history_mut().push_back(next_move_byte);
        updated_monster_id = Some(m.id);
    }
    if let Some(updated_monster_id) = updated_monster_id {
        state.clear_monster_protocol_observation(updated_monster_id);
    }
}
