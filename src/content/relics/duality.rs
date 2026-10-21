use crate::content::cards::CardType;
use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub fn on_use_card(
    _state: &CombatState,
    _relic: &mut RelicState,
    card: &crate::runtime::combat::CombatCard,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    let card_def = crate::content::cards::get_card_definition(card.id);

    if card_def.card_type == CardType::Attack {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                target: 0,
                source: 0,
                power_id: crate::content::powers::PowerId::Dexterity,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom,
        });
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                target: 0,
                source: 0,
                power_id: crate::content::powers::PowerId::DexterityDown,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
