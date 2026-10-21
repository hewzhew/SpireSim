use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

/// Dead Branch: Whenever you Exhaust a card, add a random card to your hand.
/// Java: MakeTempCardInHandAction(returnTrulyRandomCardInCombat().makeCopy())
pub fn on_exhaust(
    state: &mut CombatState,
    _relic: &mut RelicState,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    if state.are_monsters_basically_dead_java() {
        return actions;
    }

    if let Some(card) = crate::content::cards::make_random_class_card_copy_for_combat(state, None) {
        let mut card = card;
        crate::content::cards::apply_master_reality_to_generated_blueprint(&mut card, state, 1);
        actions.push(ActionInfo {
            action: Action::MakeConstructedCopyInHand {
                original: Box::new(card),
                amount: 1,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
