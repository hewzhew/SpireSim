use crate::runtime::action::MonsterRuntimePatch;
use crate::runtime::combat::CombatState;

pub(super) fn handle_update_awakened_one_state(
    monster_id: usize,
    form1: Option<bool>,
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
        if let Some(value) = form1 {
            monster.awakened_one.form1 = value;
        }
        if let Some(value) = first_turn {
            monster.awakened_one.first_turn = value;
        }
        if let Some(value) = protocol_seeded {
            monster.awakened_one.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_darkling_state(
    monster_id: usize,
    first_move: Option<bool>,
    nip_dmg: Option<i32>,
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
            monster.darkling.first_move = value;
        }
        if let Some(value) = nip_dmg {
            monster.darkling.nip_dmg = value;
        }
        if let Some(value) = protocol_seeded {
            monster.darkling.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_writhing_mass_state(
    monster_id: usize,
    first_move: Option<bool>,
    used_mega_debuff: Option<bool>,
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
            monster.writhing_mass.first_move = value;
        }
        if let Some(value) = used_mega_debuff {
            monster.writhing_mass.used_mega_debuff = value;
        }
        if let Some(value) = protocol_seeded {
            monster.writhing_mass.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_spiker_state(
    monster_id: usize,
    thorns_count: Option<u8>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = thorns_count {
            monster.spiker.thorns_count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.spiker.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_reptomancer_state(
    monster_id: usize,
    first_move: Option<bool>,
    dagger_slots: Option<[Option<usize>; 4]>,
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
            monster.reptomancer.first_move = value;
        }
        if let Some(value) = dagger_slots {
            monster.reptomancer.dagger_slots = value;
        }
        if let Some(value) = protocol_seeded {
            monster.reptomancer.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_nemesis_state(
    monster_id: usize,
    first_move: Option<bool>,
    scythe_cooldown: Option<i32>,
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
            monster.nemesis.first_move = value;
        }
        if let Some(value) = scythe_cooldown {
            monster.nemesis.scythe_cooldown = value;
        }
        if let Some(value) = protocol_seeded {
            monster.nemesis.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_giant_head_state(
    monster_id: usize,
    count: Option<i32>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = count {
            monster.giant_head.count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.giant_head.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_time_eater_state(
    monster_id: usize,
    used_haste: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = used_haste {
            monster.time_eater.used_haste = value;
        }
        if let Some(value) = protocol_seeded {
            monster.time_eater.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_donu_state(
    monster_id: usize,
    is_attacking: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = is_attacking {
            monster.donu.is_attacking = value;
        }
        if let Some(value) = protocol_seeded {
            monster.donu.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_deca_state(
    monster_id: usize,
    is_attacking: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = is_attacking {
            monster.deca.is_attacking = value;
        }
        if let Some(value) = protocol_seeded {
            monster.deca.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_transient_state(
    monster_id: usize,
    count: Option<i32>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = count {
            monster.transient.count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.transient.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_exploder_state(
    monster_id: usize,
    turn_count: Option<i32>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = turn_count {
            monster.exploder.turn_count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.exploder.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_maw_state(
    monster_id: usize,
    roared: Option<bool>,
    turn_count: Option<i32>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = roared {
            monster.maw.roared = value;
        }
        if let Some(value) = turn_count {
            monster.maw.turn_count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.maw.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_snake_dagger_state(
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
            monster.snake_dagger.first_move = value;
        }
        if let Some(value) = protocol_seeded {
            monster.snake_dagger.protocol_seeded = value;
        }
    }
}

pub(super) fn try_handle_patch(
    monster_id: usize,
    patch: MonsterRuntimePatch,
    state: &mut CombatState,
) -> Result<(), MonsterRuntimePatch> {
    match patch {
        MonsterRuntimePatch::AwakenedOne {
            form1,
            first_turn,
            protocol_seeded,
        } => {
            handle_update_awakened_one_state(monster_id, form1, first_turn, protocol_seeded, state)
        }
        MonsterRuntimePatch::Darkling {
            first_move,
            nip_dmg,
            protocol_seeded,
        } => handle_update_darkling_state(monster_id, first_move, nip_dmg, protocol_seeded, state),
        MonsterRuntimePatch::WrithingMass {
            first_move,
            used_mega_debuff,
            protocol_seeded,
        } => handle_update_writhing_mass_state(
            monster_id,
            first_move,
            used_mega_debuff,
            protocol_seeded,
            state,
        ),
        MonsterRuntimePatch::Spiker {
            thorns_count,
            protocol_seeded,
        } => handle_update_spiker_state(monster_id, thorns_count, protocol_seeded, state),
        MonsterRuntimePatch::Reptomancer {
            first_move,
            dagger_slots,
            protocol_seeded,
        } => handle_update_reptomancer_state(
            monster_id,
            first_move,
            dagger_slots,
            protocol_seeded,
            state,
        ),
        MonsterRuntimePatch::Nemesis {
            first_move,
            scythe_cooldown,
            protocol_seeded,
        } => handle_update_nemesis_state(
            monster_id,
            first_move,
            scythe_cooldown,
            protocol_seeded,
            state,
        ),
        MonsterRuntimePatch::GiantHead {
            count,
            protocol_seeded,
        } => handle_update_giant_head_state(monster_id, count, protocol_seeded, state),
        MonsterRuntimePatch::TimeEater {
            used_haste,
            protocol_seeded,
        } => handle_update_time_eater_state(monster_id, used_haste, protocol_seeded, state),
        MonsterRuntimePatch::Donu {
            is_attacking,
            protocol_seeded,
        } => handle_update_donu_state(monster_id, is_attacking, protocol_seeded, state),
        MonsterRuntimePatch::Deca {
            is_attacking,
            protocol_seeded,
        } => handle_update_deca_state(monster_id, is_attacking, protocol_seeded, state),
        MonsterRuntimePatch::Transient {
            count,
            protocol_seeded,
        } => handle_update_transient_state(monster_id, count, protocol_seeded, state),
        MonsterRuntimePatch::Exploder {
            turn_count,
            protocol_seeded,
        } => handle_update_exploder_state(monster_id, turn_count, protocol_seeded, state),
        MonsterRuntimePatch::Maw {
            roared,
            turn_count,
            protocol_seeded,
        } => handle_update_maw_state(monster_id, roared, turn_count, protocol_seeded, state),
        MonsterRuntimePatch::SnakeDagger {
            first_move,
            protocol_seeded,
        } => handle_update_snake_dagger_state(monster_id, first_move, protocol_seeded, state),
        other => return Err(other),
    }
    Ok(())
}
