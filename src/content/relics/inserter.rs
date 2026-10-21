use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct Inserter;

impl Inserter {
    /// Inserter (Defect Boss)
    /// Java: atTurnStart() — counter increments, fires IncreaseMaxOrbAction(1) every 2 turns
    /// Same counter pattern as HappyFlower: counter = (counter == -1) ? counter+2 : ++counter
    /// Fire when counter == 2, reset to 0
    pub fn at_turn_start(
        relic_state: &mut crate::content::relics::RelicState,
    ) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        // Java: this.counter = this.counter == -1 ? (this.counter += 2) : ++this.counter;
        relic_state.counter = if relic_state.counter == -1 {
            relic_state.counter + 2
        } else {
            relic_state.counter + 1
        };

        if relic_state.counter == 2 {
            relic_state.counter = 0;
            // Java: addToBot(IncreaseMaxOrbAction(1))
            actions.push(ActionInfo {
                action: Action::IncreaseMaxOrb(1),
                insertion_mode: AddTo::Bottom,
            });
        }
        actions
    }
}
