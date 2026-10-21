use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// OrangePellets: Whenever you play an Attack, Skill, AND Power in the same turn,
/// remove all of your debuffs.
/// Tracks which card types have been played via bit flags in the counter:
///   bit 0 = Attack played, bit 1 = Skill played, bit 2 = Power played
///   When all 3 set (counter & 0b111 == 0b111), remove debuffs and reset.
pub fn at_turn_start(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = 0;
}

pub fn on_use_card(
    card_id: crate::content::cards::CardId,
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    let def = crate::content::cards::get_card_definition(card_id);

    let bit = match def.card_type {
        crate::content::cards::CardType::Attack => 1,
        crate::content::cards::CardType::Skill => 2,
        crate::content::cards::CardType::Power => 4,
        _ => 0,
    };

    let new_counter = relic_state.counter.max(0) | bit;
    relic_state.counter = new_counter;

    // All 3 types played -> Java RemoveDebuffsAction removes all debuffs.
    if new_counter & 0b111 == 0b111 {
        actions.push(ActionInfo {
            action: Action::RemoveAllDebuffs { target: 0 },
            insertion_mode: AddTo::Bottom,
        });
        // Reset counter for next combo
        relic_state.counter = 0;
    }

    actions
}
