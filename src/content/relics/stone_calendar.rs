use crate::runtime::action::{Action, ActionInfo, AddTo, NO_SOURCE};
use smallvec::SmallVec;

/// Java StoneCalendar:
/// - atBattleStart() => counter = 0
/// - atTurnStart() => ++counter
/// - onPlayerEndTurn() => if counter == 7, deal 52 to all enemies
pub fn at_battle_start(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = 0;
}

pub fn at_turn_start(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    relic_state.counter += 1;
    SmallVec::new()
}

pub fn at_end_of_turn(
    state: &crate::runtime::combat::CombatState,
    counter: i32,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    if counter == 7 {
        let damages: smallvec::SmallVec<[i32; 5]> =
            state.entities.monsters.iter().map(|_| 52).collect();
        actions.push(ActionInfo {
            action: Action::DamageAllEnemies {
                source: NO_SOURCE,
                damages,
                damage_type: crate::runtime::action::DamageType::Thorns,
                is_modified: false,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}

pub fn on_victory(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = -1;
}
