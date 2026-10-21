use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: &str = "If you played no Attack cards during a player turn, gain 1 additional Energy at the start of your next player turn. This gives no first-turn bonus. Any played Attack counts, including 0-cost Attacks; damage from Skills, Powers or potions does not cancel the bonus. The condition is checked anew each turn.";

pub fn at_pre_battle(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    // Java: firstTurn = true; gainEnergyNext = true;
    // In our implementation, counter = -1 simulates firstTurn.
    relic_state.counter = -1;
    SmallVec::new()
}

/// Art of War: If you do not play any Attacks during your turn, gain an extra Energy next turn.
pub fn at_turn_start(
    _state: &CombatState,
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    if relic_state.counter == 1 {
        actions.push(ActionInfo {
            action: Action::GainEnergy { amount: 1 },
            insertion_mode: AddTo::Bottom, // Java addToBot
        });
    }

    relic_state.counter = 1;

    actions
}

pub fn on_use_card(
    _state: &CombatState,
    relic_state: &mut crate::content::relics::RelicState,
    card_id: crate::content::cards::CardId,
) -> SmallVec<[ActionInfo; 4]> {
    let actions = SmallVec::new();
    let def = crate::content::cards::get_card_definition(card_id);

    if def.card_type == crate::content::cards::CardType::Attack {
        relic_state.counter = 0;
    }

    actions
}
