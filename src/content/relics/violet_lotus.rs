use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Violet Lotus (Watcher Boss Relic): Whenever you exit Calm, gain 1 Energy.
pub fn on_change_stance(prev_stance: &str, new_stance: &str) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    if prev_stance != new_stance && prev_stance == "Calm" {
        actions.push(ActionInfo {
            action: Action::GainEnergy { amount: 1 },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
