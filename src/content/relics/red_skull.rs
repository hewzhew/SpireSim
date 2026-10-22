use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::RedSkull,
};

/// RedSkull: While your HP is at or below 50%, you have 3 additional Strength.
/// Java: atBattleStart() queues a custom action that checks HP when that action resolves.
/// During combat it also reacts to onBloodied/onNotBloodied threshold crossings.
///
/// Rust stores Java's private `isActive` flag in `RelicState.used_up`.
pub fn at_battle_start(
    relic: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    relic.used_up = false;
    smallvec::smallvec![ActionInfo {
        action: Action::RedSkullBattleStartCheck,
        insertion_mode: AddTo::Bottom,
    }]
}

pub fn battle_start_check(state: &mut CombatState) {
    if state.entities.player.current_hp > state.entities.player.max_hp / 2 {
        return;
    }

    let Some(relic) = state
        .entities
        .player
        .relics
        .iter_mut()
        .find(|relic| relic.id == crate::content::relics::RelicId::RedSkull)
    else {
        return;
    };
    if relic.used_up {
        return;
    }

    relic.used_up = true;
    crate::engine::action_handlers::powers::handle_apply_power(
        0,
        0,
        crate::content::powers::PowerId::Strength,
        3,
        state,
    );
}

pub fn on_player_hp_changed(
    state: &mut CombatState,
    previous_hp: i32,
    current_hp: i32,
    max_hp: i32,
) -> SmallVec<[ActionInfo; 4]> {
    let was_bloodied = previous_hp <= max_hp / 2;
    let is_bloodied = current_hp <= max_hp / 2;

    let mut actions = SmallVec::new();
    let Some(relic) = state
        .entities
        .player
        .relics
        .iter_mut()
        .find(|relic| relic.id == crate::content::relics::RelicId::RedSkull)
    else {
        return actions;
    };

    if !was_bloodied && is_bloodied && !relic.used_up {
        relic.used_up = true;
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::Strength,
                amount: 3,
            },
            insertion_mode: AddTo::Top,
        });
    } else if was_bloodied && !is_bloodied && relic.used_up {
        relic.used_up = false;
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::Strength,
                amount: -3,
            },
            insertion_mode: AddTo::Top,
        });
    }
    actions
}

pub fn on_victory(relic: &mut crate::content::relics::RelicState) {
    relic.used_up = false;
}
