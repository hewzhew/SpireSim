pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Block gained once",
    effect: "On the first qualifying damage hit, gains amount Block after damage and removes Curl Up. Requires positive damage, a living owner and a damage source.",
    timing: "Triggers once, not on every hit. A lethal hit does not gain Block.",
    round_duration: false,
};

use crate::runtime::action::{Action, NO_SOURCE};
use crate::runtime::combat::{CombatState, PowerId};
use crate::EntityId;

pub fn on_attacked(
    state: &CombatState,
    target: EntityId,
    amount: i32,
    source: EntityId,
    power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    // Java CurlUp.onAttacked conditions:
    //   !this.triggered && damageAmount < this.owner.currentHealth
    //   && damageAmount > 0 && info.owner != null && info.type == NORMAL
    //
    // Java keeps `amount` unchanged and records `triggered` separately while
    // the queued GainBlock/RemoveSpecificPower actions are waiting. Rust uses
    // CurlUp.extra_data == 1 for that private flag; this matters when another
    // queued hit kills the owner before RemoveSpecificPower can settle.
    let not_triggered = crate::content::powers::store::powers_for(state, target)
        .and_then(|powers| {
            powers
                .iter()
                .find(|power| power.power_type == PowerId::CurlUp)
        })
        .is_some_and(|power| power.extra_data == 0);
    if let Some(m) = state.entities.monsters.iter().find(|m| m.id == target) {
        if not_triggered
            && power_amount > 0
            && amount > 0
            && m.current_hp > 0
            && source != NO_SOURCE
        {
            actions.push(Action::GainBlock {
                target,
                amount: power_amount,
            });
            actions.push(Action::RemovePower {
                target,
                power_id: PowerId::CurlUp,
            });
        }
    }
    actions
}
