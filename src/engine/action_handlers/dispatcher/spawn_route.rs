use crate::engine::action_handlers::spawning;

use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub(super) fn try_execute(action: Action, state: &mut CombatState) -> Result<(), Action> {
    match action {
        // === Spawning / Monster lifecycle domain ===
        Action::SpawnMonster {
            monster_id,
            slot,
            current_hp,
            max_hp,
            logical_position,
            protocol_draw_x,
            is_minion,
        } => {
            let _ = spawning::handle_spawn_monster(
                monster_id,
                slot,
                current_hp,
                max_hp,
                logical_position,
                protocol_draw_x,
                is_minion,
                state,
            );
        }
        Action::SpawnMonsterSmart {
            monster_id,
            logical_position,
            hp,
            protocol_draw_x,
            is_minion,
        } => spawning::handle_spawn_monster_smart(
            monster_id,
            logical_position,
            hp,
            protocol_draw_x,
            is_minion,
            state,
        ),
        Action::SpawnCollectorTorch {
            collector_id,
            slot,
            logical_position,
            hp,
            protocol_draw_x,
        } => spawning::handle_spawn_collector_torch(
            collector_id,
            slot,
            logical_position,
            hp,
            protocol_draw_x,
            state,
        ),
        Action::SpawnGremlinLeaderMinion {
            leader_id,
            slot,
            monster_id,
            logical_position,
            hp,
            protocol_draw_x,
        } => spawning::handle_spawn_gremlin_leader_minion(
            leader_id,
            slot,
            monster_id,
            logical_position,
            hp,
            protocol_draw_x,
            state,
        ),
        Action::SpawnReptomancerDagger {
            reptomancer_id,
            slot,
            logical_position,
            hp,
            protocol_draw_x,
        } => spawning::handle_spawn_reptomancer_dagger(
            reptomancer_id,
            slot,
            logical_position,
            hp,
            protocol_draw_x,
            state,
        ),
        Action::Suicide {
            target,
            trigger_relics,
        } => spawning::handle_suicide(target, trigger_relics, state),
        Action::Escape { target } => spawning::handle_escape(target, state),
        Action::AddCombatReward { item } => spawning::handle_add_combat_reward(item, state),
        Action::RollMonsterMove { monster_id } => {
            spawning::handle_roll_monster_move(monster_id, state)
        }
        Action::SetMonsterMove {
            monster_id,
            next_move_byte,
            planned_steps,
            planned_visible_spec,
        } => spawning::handle_set_monster_move(
            monster_id,
            next_move_byte,
            planned_steps,
            planned_visible_spec,
            state,
        ),
        Action::UpdateMonsterRuntime { monster_id, patch } => {
            spawning::handle_update_monster_runtime(monster_id, patch, state)
        }
        Action::GuardianModeShiftThresholdTriggered {
            monster_id,
            hp_lost,
        } => {
            crate::content::monsters::exordium::the_guardian::handle_mode_shift_threshold_triggered(
                monster_id, hp_lost, state,
            )
        }
        Action::GuardianEnterDefensiveMode {
            monster_id,
            next_threshold,
        } => crate::content::monsters::exordium::the_guardian::handle_enter_defensive_mode(
            monster_id,
            next_threshold,
            state,
        ),
        Action::ReviveMonster { target } => spawning::handle_revive_monster(target, state),
        Action::UpdateRelicCounter { relic_id, counter } => {
            spawning::handle_update_relic_counter(relic_id, counter, state)
        }
        Action::UpdateRelicAmount { relic_id, amount } => {
            spawning::handle_update_relic_amount(relic_id, amount, state)
        }
        Action::UpdateRelicUsedUp { relic_id, used_up } => {
            spawning::handle_update_relic_used_up(relic_id, used_up, state)
        }

        other => return Err(other),
    }
    Ok(())
}
