pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Strength gained per trigger",
    effect: "Repeatedly grants amount Strength; the displayed amount is growth per trigger, not current Strength.",
    timing: "Player-controlled Ritual triggers at end of player turn. Monster Ritual triggers at end of round and skips its initial trigger.",
    round_duration: false,
};

use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

const PLAYER_CONTROLLED_FLAG: i32 = 1 << 0;
const SKIP_FIRST_FLAG: i32 = 1 << 1;

pub fn extra_data(player_controlled: bool, skip_first: bool) -> i32 {
    let mut extra = 0;
    if player_controlled {
        extra |= PLAYER_CONTROLLED_FLAG;
    }
    if skip_first {
        extra |= SKIP_FIRST_FLAG;
    }
    extra
}

pub fn infer_extra_data(owner: EntityId, just_applied: bool) -> i32 {
    // RitualPower initializes skipFirst for both owners. Its player-turn
    // branch grants Strength immediately without reading or clearing the flag.
    extra_data(owner == 0, owner == 0 || just_applied)
}

pub fn at_end_of_turn(
    owner: EntityId,
    amount: i32,
    extra_data: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    if extra_data & PLAYER_CONTROLLED_FLAG == 0 {
        return smallvec::smallvec![];
    }

    smallvec::smallvec![Action::ApplyPower {
        source: owner,
        target: owner,
        power_id: PowerId::Strength,
        amount,
    }]
}

pub fn at_end_of_round(
    state: &crate::runtime::combat::CombatState,
    owner: EntityId,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    let extra = state
        .entities
        .power_db
        .get(&owner)
        .and_then(|ps| ps.iter().find(|p| p.power_type == PowerId::Ritual))
        .map(|p| p.extra_data)
        .unwrap_or(0);

    if extra & PLAYER_CONTROLLED_FLAG != 0 {
        return actions;
    }

    if extra & SKIP_FIRST_FLAG != 0 {
        actions.push(Action::UpdatePowerExtraData {
            target: owner,
            power_id: PowerId::Ritual,
            value: extra & !SKIP_FIRST_FLAG,
        });
    } else {
        actions.push(Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::Strength,
            amount,
        });
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cultist_potion_preserves_skip_first_without_delaying_player_strength() {
        let state = crate::test_support::blank_test_combat();
        let actions = crate::content::potions::potion_effects::get_potion_actions(
            &state,
            crate::content::potions::PotionId::CultistPotion,
            None,
            1,
        );
        let extra = actions
            .iter()
            .find_map(|info| match &info.action {
                Action::ApplyPowerDetailed {
                    power_id: PowerId::Ritual,
                    extra_data,
                    ..
                } => *extra_data,
                _ => None,
            })
            .unwrap();
        assert_eq!(extra, infer_extra_data(0, false));
        assert_ne!(extra & SKIP_FIRST_FLAG, 0);
        let triggers = at_end_of_turn(0, 1, extra);
        assert!(matches!(
            triggers.as_slice(),
            [Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Strength,
                amount: 1,
            }]
        ));
        assert!(at_end_of_turn(1, 1, infer_extra_data(1, true)).is_empty());
    }
}
