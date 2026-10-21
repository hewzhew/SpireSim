use crate::runtime::action::ActionInfo;
use crate::runtime::combat::CombatState;

pub fn at_battle_start(
    state: &mut CombatState,
    relic: &mut crate::content::relics::RelicState,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let actions = smallvec::SmallVec::new();

    if is_elite_or_boss_combat(state) {
        state.entities.player.energy_master += 1;
        state.turn.energy = state.turn.energy.saturating_add(1);
        relic.amount = 1;
    } else {
        relic.amount = 0;
    }

    actions
}

pub fn is_elite_or_boss_combat(state: &CombatState) -> bool {
    state.meta.is_elite_fight
        || state.meta.is_boss_fight
        || state
            .entities
            .monsters
            .iter()
            .any(|monster| monster.enemy_id.is_boss())
}

pub fn on_victory(
    state: &mut CombatState,
    relic: &mut crate::content::relics::RelicState,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let actions = smallvec::SmallVec::new();
    // Java tracks its temporary energy grant with pulse, never counter.
    // Keep the combat-local equivalent in amount so counter remains -1.
    if relic.amount == 1 {
        state.entities.player.energy_master -= 1;
        relic.amount = 0;
    }
    actions
}
