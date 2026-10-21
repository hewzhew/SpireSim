use crate::content::cards::{CardId, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct BlueCandle;

impl BlueCandle {
    pub fn on_use_card(card_id: CardId) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        let def = crate::content::cards::get_card_definition(card_id);

        if def.card_type == CardType::Curse {
            // Java BlueCandle.onUseCard() uses LoseHPAction(player, player, 1, FIRE),
            // so this self-loss should trigger Rupture.
            actions.push(ActionInfo {
                action: Action::LoseHp {
                    target: 0,
                    amount: 1,
                    triggers_rupture: true,
                },
                insertion_mode: AddTo::Bottom,
            });
            // The card exhausts itself natively during resolution via properties typically,
            // but we can enforce it if engine requires. The engine's UseCard handler
            // will need to know to exhaust it. Let's make sure the engine supports it!
        }
        actions
    }
}
