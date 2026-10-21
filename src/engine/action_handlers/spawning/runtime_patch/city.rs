use crate::runtime::action::MonsterRuntimePatch;
use crate::runtime::combat::CombatState;

pub(super) fn handle_update_byrd_state(
    monster_id: usize,
    first_move: Option<bool>,
    is_flying: Option<bool>,
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
            monster.byrd.first_move = value;
        }
        if let Some(value) = is_flying {
            monster.byrd.is_flying = value;
        }
        if let Some(value) = protocol_seeded {
            monster.byrd.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_chosen_state(
    monster_id: usize,
    first_turn: Option<bool>,
    used_hex: Option<bool>,
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
            monster.chosen.first_turn = value;
        }
        if let Some(value) = used_hex {
            monster.chosen.used_hex = value;
        }
        if let Some(value) = protocol_seeded {
            monster.chosen.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_snecko_state(
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
            monster.snecko.first_turn = value;
        }
        if let Some(value) = protocol_seeded {
            monster.snecko.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_shelled_parasite_state(
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
            monster.shelled_parasite.first_move = value;
        }
        if let Some(value) = protocol_seeded {
            monster.shelled_parasite.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_bronze_automaton_state(
    monster_id: usize,
    first_turn: Option<bool>,
    num_turns: Option<u8>,
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
            monster.bronze_automaton.first_turn = value;
        }
        if let Some(value) = num_turns {
            monster.bronze_automaton.num_turns = value;
        }
        if let Some(value) = protocol_seeded {
            monster.bronze_automaton.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_bronze_orb_state(
    monster_id: usize,
    used_stasis: Option<bool>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = used_stasis {
            monster.bronze_orb.used_stasis = value;
        }
        if let Some(value) = protocol_seeded {
            monster.bronze_orb.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_collector_state(
    monster_id: usize,
    initial_spawn: Option<bool>,
    ult_used: Option<bool>,
    turns_taken: Option<u8>,
    enemy_slots: Option<[Option<usize>; 2]>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = initial_spawn {
            monster.collector.initial_spawn = value;
        }
        if let Some(value) = ult_used {
            monster.collector.ult_used = value;
        }
        if let Some(value) = turns_taken {
            monster.collector.turns_taken = value;
        }
        if let Some(value) = enemy_slots {
            monster.collector.enemy_slots = value;
        }
        if let Some(value) = protocol_seeded {
            monster.collector.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_champ_state(
    monster_id: usize,
    first_turn: Option<bool>,
    num_turns: Option<u8>,
    forge_times: Option<u8>,
    threshold_reached: Option<bool>,
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
            monster.champ.first_turn = value;
        }
        if let Some(value) = num_turns {
            monster.champ.num_turns = value;
        }
        if let Some(value) = forge_times {
            monster.champ.forge_times = value;
        }
        if let Some(value) = threshold_reached {
            monster.champ.threshold_reached = value;
        }
        if let Some(value) = protocol_seeded {
            monster.champ.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_book_of_stabbing_state(
    monster_id: usize,
    stab_count: Option<u8>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = stab_count {
            monster.book_of_stabbing.stab_count = value;
        }
        if let Some(value) = protocol_seeded {
            monster.book_of_stabbing.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_slaver_red_state(
    monster_id: usize,
    first_turn: Option<bool>,
    used_entangle: Option<bool>,
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
            monster.slaver_red.first_turn = value;
        }
        if let Some(value) = used_entangle {
            monster.slaver_red.used_entangle = value;
        }
        if let Some(value) = protocol_seeded {
            monster.slaver_red.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_gremlin_leader_state(
    monster_id: usize,
    gremlin_slots: Option<[Option<usize>; 3]>,
    protocol_seeded: Option<bool>,
    state: &mut CombatState,
) {
    if let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == monster_id)
    {
        if let Some(value) = gremlin_slots {
            monster.gremlin_leader.gremlin_slots = value;
        }
        if let Some(value) = protocol_seeded {
            monster.gremlin_leader.protocol_seeded = value;
        }
    }
}

pub(super) fn handle_update_spheric_guardian_state(
    monster_id: usize,
    first_move: Option<bool>,
    second_move: Option<bool>,
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
            monster.spheric_guardian.first_move = value;
        }
        if let Some(value) = second_move {
            monster.spheric_guardian.second_move = value;
        }
        if let Some(value) = protocol_seeded {
            monster.spheric_guardian.protocol_seeded = value;
        }
    }
}

pub(super) fn try_handle_patch(
    monster_id: usize,
    patch: MonsterRuntimePatch,
    state: &mut CombatState,
) -> Result<(), MonsterRuntimePatch> {
    match patch {
        MonsterRuntimePatch::Byrd {
            first_move,
            is_flying,
            protocol_seeded,
        } => handle_update_byrd_state(monster_id, first_move, is_flying, protocol_seeded, state),
        MonsterRuntimePatch::Chosen {
            first_turn,
            used_hex,
            protocol_seeded,
        } => handle_update_chosen_state(monster_id, first_turn, used_hex, protocol_seeded, state),
        MonsterRuntimePatch::Snecko {
            first_turn,
            protocol_seeded,
        } => handle_update_snecko_state(monster_id, first_turn, protocol_seeded, state),
        MonsterRuntimePatch::ShelledParasite {
            first_move,
            protocol_seeded,
        } => handle_update_shelled_parasite_state(monster_id, first_move, protocol_seeded, state),
        MonsterRuntimePatch::BronzeAutomaton {
            first_turn,
            num_turns,
            protocol_seeded,
        } => handle_update_bronze_automaton_state(
            monster_id,
            first_turn,
            num_turns,
            protocol_seeded,
            state,
        ),
        MonsterRuntimePatch::BronzeOrb {
            used_stasis,
            protocol_seeded,
        } => handle_update_bronze_orb_state(monster_id, used_stasis, protocol_seeded, state),
        MonsterRuntimePatch::BookOfStabbing {
            stab_count,
            protocol_seeded,
        } => handle_update_book_of_stabbing_state(monster_id, stab_count, protocol_seeded, state),
        MonsterRuntimePatch::Collector {
            initial_spawn,
            ult_used,
            turns_taken,
            enemy_slots,
            protocol_seeded,
        } => handle_update_collector_state(
            monster_id,
            initial_spawn,
            ult_used,
            turns_taken,
            enemy_slots,
            protocol_seeded,
            state,
        ),
        MonsterRuntimePatch::Champ {
            first_turn,
            num_turns,
            forge_times,
            threshold_reached,
            protocol_seeded,
        } => handle_update_champ_state(
            monster_id,
            first_turn,
            num_turns,
            forge_times,
            threshold_reached,
            protocol_seeded,
            state,
        ),
        MonsterRuntimePatch::SlaverRed {
            first_turn,
            used_entangle,
            protocol_seeded,
        } => handle_update_slaver_red_state(
            monster_id,
            first_turn,
            used_entangle,
            protocol_seeded,
            state,
        ),
        MonsterRuntimePatch::GremlinLeader {
            gremlin_slots,
            protocol_seeded,
        } => handle_update_gremlin_leader_state(monster_id, gremlin_slots, protocol_seeded, state),
        MonsterRuntimePatch::SphericGuardian {
            first_move,
            second_move,
            protocol_seeded,
        } => handle_update_spheric_guardian_state(
            monster_id,
            first_move,
            second_move,
            protocol_seeded,
            state,
        ),
        other => return Err(other),
    }
    Ok(())
}
