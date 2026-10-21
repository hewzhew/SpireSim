use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// Enchiridion
/// Event Relic
/// At the start of each combat, add a random Power card to your hand. It costs 0 this turn.
/// Java: atPreBattle() → returnTrulyRandomCardInCombat(CardType.POWER) → setCostForTurn(0) → addToBot(MakeTempCardInHandAction)
pub fn at_battle_start(
    state: &mut CombatState,
    _relic: &mut RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    if let Some(mut card) = crate::content::cards::make_random_class_card_copy_for_combat(
        state,
        Some(crate::content::cards::CardType::Power),
    ) {
        if card.combat_cost_without_turn_override_java() != -1 {
            card.set_cost_for_turn_java(0);
        }
        crate::content::cards::apply_master_reality_to_generated_blueprint(&mut card, state, 1);
        actions.push(ActionInfo {
            action: Action::MakeConstructedCopyInHand {
                original: Box::new(card),
                amount: 1,
            },
            insertion_mode: AddTo::Bottom, // Java: addToBot
        });
    }

    actions
}
