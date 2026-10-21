use crate::runtime::combat::{CombatState, OrbEntity};

use super::super::types::{
    CombatExactPlayerKey, CombatOrbKey, CombatPlayerFutureKey, CombatRelicKey,
};

pub(super) fn player_exact_key(combat: &CombatState) -> CombatExactPlayerKey {
    let player = &combat.entities.player;
    CombatExactPlayerKey {
        current_hp: player.current_hp,
        block: player.block,
        future_relevant: player_future_key(combat),
    }
}

pub(super) fn player_future_key(combat: &CombatState) -> CombatPlayerFutureKey {
    let player = &combat.entities.player;
    CombatPlayerFutureKey {
        entity_id: player.id,
        max_hp: player.max_hp,
        facing_left: player.facing_left,
        gold_delta_this_combat: player.gold_delta_this_combat,
        gold: player.gold,
        max_orbs: player.max_orbs,
        orbs: player.orbs.iter().map(orb_key).collect(),
        stance: player.stance,
        relics: player
            .relics
            .iter()
            .map(|relic| CombatRelicKey {
                id: relic.id,
                counter: relic.counter,
                used_up: relic.used_up,
                amount: relic.amount,
            })
            .collect(),
        energy_master: player.energy_master,
    }
}

fn orb_key(orb: &OrbEntity) -> CombatOrbKey {
    CombatOrbKey {
        id: orb.id,
        base_passive_amount: orb.base_passive_amount,
        base_evoke_amount: orb.base_evoke_amount,
        passive_amount: orb.passive_amount,
        evoke_amount: orb.evoke_amount,
    }
}
