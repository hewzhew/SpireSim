use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "own turns remaining before disappearance",
    effect: "After the owner's queued move, decrease by 1; at 1 or less, kill the owner instead. Its current attack still executes before disappearance. You can win by surviving until this countdown completes.",
    timing: "Runs during the owner's turn after its move actions, before the next monster's move actions. Does not tick at end of round.",
    round_duration: false,
};

pub fn during_turn(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    if amount <= 1 {
        actions.push(Action::Suicide {
            target: owner,
            trigger_relics: true,
        });
    } else {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: PowerId::Fading,
            amount: 1,
        });
    }

    actions
}
