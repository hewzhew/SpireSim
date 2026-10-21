use crate::runtime::action::MonsterRuntimePatch;
use crate::runtime::combat::CombatState;

pub(super) fn handle_update_corrupt_heart_state(
    monster_id: usize,
    first_move: Option<bool>,
    move_count: Option<u8>,
    buff_count: Option<u8>,
    blood_hit_count: Option<u8>,
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
            monster.corrupt_heart.first_move = value;
        }
        if let Some(value) = move_count {
            monster.corrupt_heart.move_count = value;
        }
        if let Some(value) = buff_count {
            monster.corrupt_heart.buff_count = value;
        }
        if let Some(value) = blood_hit_count {
            monster.corrupt_heart.blood_hit_count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.corrupt_heart.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_spire_shield_state(
    monster_id: usize,
    move_count: Option<u8>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = move_count {
            monster.spire_shield.move_count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.spire_shield.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_spire_spear_state(
    monster_id: usize,
    move_count: Option<u8>,
    skewer_count: Option<u8>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = move_count {
            monster.spire_spear.move_count = value;
        }
        if let Some(value) = skewer_count {
            monster.spire_spear.skewer_count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.spire_spear.protocol_seeded = value;
        }
    }
}

pub(super) fn try_handle_patch(
    monster_id: usize,
    patch: MonsterRuntimePatch,
    state: &mut CombatState,
) -> Result<(), MonsterRuntimePatch> {
    match patch {
        MonsterRuntimePatch::CorruptHeart {
            first_move,
            move_count,
            buff_count,
            blood_hit_count,
            protocol_seeded,
        } => handle_update_corrupt_heart_state(
            monster_id,
            first_move,
            move_count,
            buff_count,
            blood_hit_count,
            protocol_seeded,
            state,
        ),
        MonsterRuntimePatch::SpireShield {
            move_count,
            protocol_seeded,
        } => handle_update_spire_shield_state(monster_id, move_count, protocol_seeded, state),
        MonsterRuntimePatch::SpireSpear {
            move_count,
            skewer_count,
            protocol_seeded,
        } => handle_update_spire_spear_state(
            monster_id,
            move_count,
            skewer_count,
            protocol_seeded,
            state,
        ),
        other => return Err(other),
    }
    Ok(())
}
