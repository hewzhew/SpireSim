use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Lantern: Gain 1 Energy on the first turn of each combat.
/// The compact `used_up` flag stores `!Lantern.firstTurn`, never the original
/// `AbstractRelic.usedUp`. Public projections must keep those meanings separate.
pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Lantern,
};

pub fn at_pre_battle(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    relic_state.used_up = false;
    SmallVec::new()
}

pub fn at_turn_start(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    if !relic_state.used_up {
        relic_state.used_up = true;
        actions.push(ActionInfo {
            action: Action::GainEnergy { amount: 1 },
            insertion_mode: AddTo::Top,
        });
    }

    actions
}
