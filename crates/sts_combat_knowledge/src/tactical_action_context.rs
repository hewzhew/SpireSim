//! Encounter-owned context for the retained tactical action ranking.
//!
//! This selects semantic ranking modes from one exact state. It does not own
//! the atomic priority implementation, legal actions, or search controls.

use sts_core::content::monsters::EnemyId;
use sts_core::content::powers::{store, PowerId};
use sts_core::sim::combat::CombatPosition;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CombatTacticalActionPriorV1 {
    #[default]
    Default,
    KeyCardOnline,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CombatTacticalPhaseGuardV1 {
    #[default]
    Default,
    ChampSplit,
    TimeEaterClock,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatTacticalActionContextV1 {
    pub action_prior: CombatTacticalActionPriorV1,
    pub phase_guard: CombatTacticalPhaseGuardV1,
}

pub fn combat_tactical_action_context_v1(
    position: &CombatPosition,
) -> CombatTacticalActionContextV1 {
    let awakened_one_curiosity = position.combat.entities.monsters.iter().any(|monster| {
        monster.is_alive_for_action()
            && monster.enemy_id == EnemyId::AwakenedOne
            && store::has_power(&position.combat, monster.id, PowerId::Curiosity)
    });
    let phase_guard = if position
        .combat
        .entities
        .monsters
        .iter()
        .any(|monster| monster.is_alive_for_action() && monster.enemy_id == EnemyId::TimeEater)
    {
        CombatTacticalPhaseGuardV1::TimeEaterClock
    } else if position
        .combat
        .entities
        .monsters
        .iter()
        .any(|monster| monster.is_alive_for_action() && monster.enemy_id == EnemyId::Champ)
    {
        CombatTacticalPhaseGuardV1::ChampSplit
    } else {
        CombatTacticalPhaseGuardV1::Default
    };
    CombatTacticalActionContextV1 {
        phase_guard,
        // Curiosity is a real cost, but the ordinary phase rule assigns it
        // equally to every Power. Preserve the distinction between a key
        // engine/scaling Power and a low-impact Power. Lethal and survival
        // roles still rank ahead of this prior.
        action_prior: if awakened_one_curiosity {
            CombatTacticalActionPriorV1::KeyCardOnline
        } else {
            CombatTacticalActionPriorV1::Default
        },
    }
}
