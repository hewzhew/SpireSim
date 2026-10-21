use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct BirdFacedUrn;

impl BirdFacedUrn {
    pub fn on_use_card(card_id: crate::content::cards::CardId) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        let base_def = crate::content::cards::get_card_definition(card_id);

        if base_def.card_type == crate::content::cards::CardType::Power {
            actions.push(ActionInfo {
                action: Action::Heal {
                    target: 0,
                    amount: 2,
                },
                insertion_mode: AddTo::Top,
            });
        }
        actions
    }
}
