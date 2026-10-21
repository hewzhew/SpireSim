use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Nunchaku: Every time you play 10 Attacks, gain 1 Energy.
pub fn on_use_card(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    // The dispatcher only triggers `on_use_card` here if the card is an Attack.
    relic_state.counter += 1;

    if relic_state.counter % 10 == 0 {
        relic_state.counter = 0;
        actions.push(ActionInfo {
            action: Action::GainEnergy { amount: 1 },
            insertion_mode: AddTo::Bottom,
        });
    }

    actions
}
