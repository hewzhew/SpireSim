use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Block gained by the next qualifying hit; stored base is the reset amount",
    effect: "Each nonlethal normal hit that causes positive HP loss queues Block equal to the current amount, then immediately increases the amount by 1. Fully blocked hits, lethal hits, Thorns and HP-loss effects do not trigger it. Counts hits, not cards; queued Block resolves at its existing queue position.",
    timing: "After the monster's turn ends, the amount resets to its stored base. Does not lose duration each round.",
    round_duration: false,
};

/// Java MalleablePower.onAttacked():
///   if (damageAmount < owner.currentHealth && damageAmount > 0 && info.type == NORMAL) {
///       addToBot(GainBlockAction(owner, amount));  // monster: addToBot
///       ++this.amount;                              // immediate, not via action
///   }
/// The caller (action_handlers) must:
///   1. push GainBlock to BACK of queue (addToBot)
///   2. immediately increment Malleable amount in power_db
pub fn on_attacked(
    state: &CombatState,
    owner: EntityId,
    damage: i32,
    power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    // Java: damageAmount < this.owner.currentHealth && damageAmount > 0
    let owner_hp = if owner == 0 {
        state.entities.player.current_hp
    } else {
        state
            .entities
            .monsters
            .iter()
            .find(|m| m.id == owner)
            .map_or(0, |m| m.current_hp)
    };

    if damage > 0 && damage < owner_hp {
        actions.push(Action::GainBlock {
            target: owner,
            amount: power_amount,
        });
        // NOTE: ++amount is handled by the caller (immediate mutation, not via action queue)
    }
    actions
}

pub fn on_monster_turn_ended(
    state: &CombatState,
    owner: EntityId,
    _power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    // reset amount to extra_data (basePower in Java)
    if let Some(power_list) = state.entities.power_db.get(&owner) {
        if let Some(power) = power_list
            .iter()
            .find(|p| p.power_type == PowerId::Malleable)
        {
            let base_power = power.extra_data;
            if power.amount != base_power {
                let diff = base_power - power.amount;
                actions.push(Action::ApplyPower {
                    source: owner,
                    target: owner,
                    power_id: PowerId::Malleable,
                    amount: diff,
                });
            }
        }
    }
    actions
}
