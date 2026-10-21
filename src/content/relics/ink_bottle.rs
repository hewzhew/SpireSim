use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Ink Bottle: Whenever you play 10 cards, draw 1 card.
/// Counter persists across combats (Java: no reset in atBattleStart).
/// Java: onUseCard() → ++counter, if counter==10 → counter=0, addToBot(DrawCardAction(1))
pub fn on_use_card(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    relic_state.counter += 1;
    if relic_state.counter == 10 {
        relic_state.counter = 0;
        actions.push(ActionInfo {
            action: Action::DrawCards(1),
            insertion_mode: AddTo::Bottom, // Java: addToBot(DrawCardAction(1))
        });
    }

    actions
}
