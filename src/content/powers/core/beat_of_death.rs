pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "damage to the player per counted card",
    effect: "Every counted card deals this much THORNS damage to the player. Block can absorb it; Strength, Weak and Vulnerable do not modify it. Torii's normal-hit reduction does not apply to this THORNS damage. It resolves after the card's own effects but before that card's final exhaust triggers Feel No Pain. Extra actions a card queues can have their own exhaust triggers; inspect the settled result for timing-sensitive sequences.",
    timing: "After-use trigger, not the start of card play; persists until removed.",
    round_duration: false,
};

use crate::runtime::action::{Action, DamageInfo, DamageType};
use crate::EntityId;

pub fn on_after_use_card(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::SmallVec::new();

    // BeatOfDeathPower.onAfterUseCard queues THORNS damage from UseCardAction.update.
    actions.push(Action::Damage(DamageInfo {
        source: owner,
        target: 0, // Player
        base: amount,
        output: amount,
        damage_type: DamageType::Thorns,
        is_modified: false,
    }));

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_of_death_damage_keeps_its_thorns_exclusion_from_torii() {
        for amount in [2, 3] {
            let actions = on_after_use_card(1, amount);
            let Action::Damage(info) = &actions[0] else {
                panic!("expected Beat of Death damage")
            };
            assert_eq!(info.damage_type, DamageType::Thorns);
            assert_eq!(
                crate::content::relics::torii::on_attacked_to_change_damage(info, amount),
                amount
            );
            let mut normal = info.clone();
            normal.damage_type = DamageType::Normal;
            assert_eq!(
                crate::content::relics::torii::on_attacked_to_change_damage(&normal, amount),
                1
            );
        }
    }
}
