use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::EntityId;

pub const MONSTER_MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        amount_unit: "remaining owner turns of monster Intangible",
        effect: "While positive, cap each final damage or direct HP-loss instance above 1 to 1. Multi-hit attacks still apply a separate capped instance for each hit. This is the monster version; the player's version has stable ID IntangiblePlayer.",
        timing: "Decreases by 1 at the owner's turn end, unless just applied skips that tick.",
        round_duration: false,
    };

pub const PLAYER_MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        amount_unit: "remaining rounds of player Intangible",
        effect: "While positive, cap each final damage or direct HP-loss instance above 1 to 1. Multi-hit attacks still apply a separate capped instance for each hit. Normal damage can then be absorbed by Block; Intangible itself does not create Block.",
        timing: "Decreases by 1 at end of round, after the enemy turn. One stack gained during your turn protects through that enemy turn; additional stacks extend duration.",
        round_duration: true,
    };

pub fn on_calculate_damage_received_from_attack(_damage: i32, amount: i32) -> i32 {
    // Intangible caps attack damage received to 1
    if amount > 0 && _damage > 1 {
        1
    } else {
        _damage
    }
}

pub fn at_damage_final_receive(
    damage: i32,
    amount: i32,
    _damage_type: crate::runtime::action::DamageType,
) -> i32 {
    if amount > 0 && damage > 1 {
        1
    } else {
        damage
    }
}

pub fn at_end_of_turn(
    owner: EntityId,
    amount: i32,
    just_applied: bool,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    if just_applied {
        return actions;
    }

    if amount == 0 {
        actions.push(Action::RemovePower {
            target: owner,
            power_id: PowerId::Intangible,
        });
    } else {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: PowerId::Intangible,
            amount: 1,
        });
    }

    actions
}

pub fn at_end_of_round(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    if amount == 0 {
        actions.push(Action::RemovePower {
            target: owner,
            power_id: PowerId::IntangiblePlayer,
        });
    } else {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: PowerId::IntangiblePlayer,
            amount: 1,
        });
    }

    actions
}
