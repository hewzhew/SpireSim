use crate::runtime::action::MonsterRuntimePatch;
use crate::runtime::combat::CombatState;

pub(super) fn handle_update_hexaghost_state(
    monster_id: usize,
    activated: Option<bool>,
    orb_active_count: Option<u8>,
    burn_upgraded: Option<bool>,
    divider_damage: Option<i32>,
    clear_divider_damage: bool,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if activated.is_some()
            || orb_active_count.is_some()
            || burn_upgraded.is_some()
            || divider_damage.is_some()
            || clear_divider_damage
        {
            if let Some(value) = activated {
                monster.hexaghost.activated = value;
            }
            if let Some(value) = orb_active_count {
                monster.hexaghost.orb_active_count = value;
            }
            if let Some(value) = burn_upgraded {
                monster.hexaghost.burn_upgraded = value;
            }
            if clear_divider_damage {
                monster.hexaghost.divider_damage = None;
            }
            if let Some(value) = divider_damage {
                monster.hexaghost.divider_damage = Some(value);
            }
        }
    }
}

pub(super) fn handle_update_lagavulin_state(
    monster_id: usize,
    idle_count: Option<u8>,
    debuff_turn_count: Option<u8>,
    is_out: Option<bool>,
    is_out_triggered: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = idle_count {
            monster.lagavulin.idle_count = value;
        }
        if let Some(value) = debuff_turn_count {
            monster.lagavulin.debuff_turn_count = value;
        }
        if let Some(value) = is_out {
            monster.lagavulin.is_out = value;
        }
        if let Some(value) = is_out_triggered {
            monster.lagavulin.is_out_triggered = value;
        }
    }
}

pub(super) fn handle_update_jaw_worm_state(
    monster_id: usize,
    first_move: Option<bool>,
    hard_mode: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = first_move {
            monster.jaw_worm.first_move = value;
        }
        if let Some(value) = hard_mode {
            monster.jaw_worm.hard_mode = value;
        }
        if let Some(value) = protocol_seeded {
            monster.jaw_worm.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_guardian_state(
    monster_id: usize,
    damage_threshold: Option<i32>,
    damage_taken: Option<i32>,
    is_open: Option<bool>,
    close_up_triggered: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = damage_threshold {
            monster.guardian.damage_threshold = value;
        }
        if let Some(value) = damage_taken {
            monster.guardian.damage_taken = value;
        }
        if let Some(value) = is_open {
            monster.guardian.is_open = value;
        }
        if let Some(value) = close_up_triggered {
            monster.guardian.close_up_triggered = value;
        }
    }
}

pub(super) fn handle_update_gremlin_nob_state(
    monster_id: usize,
    used_bellow: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = used_bellow {
            monster.gremlin_nob.used_bellow = value;
        }
        if let Some(value) = protocol_seeded {
            monster.gremlin_nob.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_gremlin_wizard_state(
    monster_id: usize,
    current_charge: Option<u8>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = current_charge {
            monster.gremlin_wizard.current_charge = value;
        }
        if let Some(value) = protocol_seeded {
            monster.gremlin_wizard.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_cultist_state(
    monster_id: usize,
    first_move: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = first_move {
            monster.cultist.first_move = value;
        }
        if let Some(value) = protocol_seeded {
            monster.cultist.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_sentry_state(
    monster_id: usize,
    first_move: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = first_move {
            monster.sentry.first_move = value;
        }
        if let Some(value) = protocol_seeded {
            monster.sentry.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_slime_boss_state(
    monster_id: usize,
    first_turn: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = first_turn {
            monster.slime_boss.first_turn = value;
        }
        if let Some(value) = protocol_seeded {
            monster.slime_boss.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_large_slime_state(
    monster_id: usize,
    split_triggered: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = split_triggered {
            monster.large_slime.split_triggered = value;
        }
        if let Some(value) = protocol_seeded {
            monster.large_slime.protocol_seeded = value;
        }
    }
}

pub(super) fn try_handle_patch(
    monster_id: usize,
    patch: MonsterRuntimePatch,
    state: &mut CombatState,
) -> Result<(), MonsterRuntimePatch> {
    match patch {
        MonsterRuntimePatch::Hexaghost {
            activated,
            orb_active_count,
            burn_upgraded,
            divider_damage,
            clear_divider_damage,
        } => handle_update_hexaghost_state(
            monster_id,
            activated,
            orb_active_count,
            burn_upgraded,
            divider_damage,
            clear_divider_damage,
            state,
        ),
        MonsterRuntimePatch::Lagavulin {
            idle_count,
            debuff_turn_count,
            is_out,
            is_out_triggered,
        } => handle_update_lagavulin_state(
            monster_id,
            idle_count,
            debuff_turn_count,
            is_out,
            is_out_triggered,
            state,
        ),
        MonsterRuntimePatch::JawWorm {
            first_move,
            hard_mode,
            protocol_seeded,
        } => {
            handle_update_jaw_worm_state(monster_id, first_move, hard_mode, protocol_seeded, state)
        }
        MonsterRuntimePatch::Guardian {
            damage_threshold,
            damage_taken,
            is_open,
            close_up_triggered,
        } => handle_update_guardian_state(
            monster_id,
            damage_threshold,
            damage_taken,
            is_open,
            close_up_triggered,
            state,
        ),
        MonsterRuntimePatch::GremlinNob {
            used_bellow,
            protocol_seeded,
        } => handle_update_gremlin_nob_state(monster_id, used_bellow, protocol_seeded, state),
        MonsterRuntimePatch::GremlinWizard {
            current_charge,
            protocol_seeded,
        } => handle_update_gremlin_wizard_state(monster_id, current_charge, protocol_seeded, state),
        MonsterRuntimePatch::Cultist {
            first_move,
            protocol_seeded,
        } => handle_update_cultist_state(monster_id, first_move, protocol_seeded, state),
        MonsterRuntimePatch::Sentry {
            first_move,
            protocol_seeded,
        } => handle_update_sentry_state(monster_id, first_move, protocol_seeded, state),
        MonsterRuntimePatch::SlimeBoss {
            first_turn,
            protocol_seeded,
        } => handle_update_slime_boss_state(monster_id, first_turn, protocol_seeded, state),
        MonsterRuntimePatch::LargeSlime {
            split_triggered,
            protocol_seeded,
        } => handle_update_large_slime_state(monster_id, split_triggered, protocol_seeded, state),
        other => return Err(other),
    }
    Ok(())
}
