use crate::content::powers::PowerId;
use crate::runtime::action::{Action, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "damage to each enemy; separate self HP loss is stored alongside it",
    effect: "At player turn end, first lose the stored self HP loss, then deal amount non-Attack damage to each enemy. Block does not prevent the self HP loss. Each additional deployment adds its damage and 1 to self HP loss; this loss can trigger Rupture.",
    timing: "Player turn end; persists for the rest of combat. Damage and self HP loss stack independently.",
    round_duration: false,
};

/// Java: CombustPower.atEndOfTurn(isPlayer)
///
/// The Java power has TWO state values:
///   - `amount`  → damage dealt to ALL enemies (stored in Power.amount)
///   - `hpLoss`  → HP lost by the player (stored in Power.extra_data)
///
/// On stack (playing Combust again):
///   - `amount += magicNumber`  (normal stack via ApplyPower)
///   - `hpLoss += 1`            (always +1, handled in action_handlers.rs)
///
/// At end of turn:
///   1. LoseHPAction(owner, owner, hpLoss, FIRE)
///   2. DamageAllEnemiesAction(null, createDamageMatrix(amount, true), THORNS, FIRE)
pub fn at_end_of_turn(
    state: &CombatState,
    owner: crate::EntityId,
    amount: i32,
) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();

    if state.are_monsters_basically_dead_java() {
        return actions;
    }

    // Get hpLoss from extra_data (defaults to 1 if somehow missing)
    let hp_loss = state
        .entities
        .power_db
        .get(&owner)
        .and_then(|ps| ps.iter().find(|p| p.power_type == PowerId::Combust))
        .map(|p| p.extra_data)
        .unwrap_or(crate::content::cards::mechanics::COMBUST_HP_LOSS_PER_APPLICATION_V1);

    actions.push(Action::LoseHp {
        target: owner,
        amount: hp_loss,
        triggers_rupture: true,
    });
    actions.push(Action::DamageAllEnemies {
        source: NO_SOURCE,
        damages: crate::runtime::action::repeated_damage_matrix(
            state.entities.monsters.len(),
            amount,
        ),
        damage_type: DamageType::Thorns, // Java: DamageInfo.DamageType.THORNS
        is_modified: false,
    });
    actions
}
