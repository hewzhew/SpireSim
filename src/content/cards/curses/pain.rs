// Pain triggers whenever you play ANOTHER card. We will hook this directly in the player input sequence where cards are played.
pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Pain,
    };

pub fn on_other_card_played() -> crate::runtime::action::ActionInfo {
    crate::runtime::action::ActionInfo {
        action: crate::runtime::action::Action::LoseHp {
            target: 0,
            amount: 1,
            triggers_rupture: true,
        },
        insertion_mode: crate::runtime::action::AddTo::Top,
    }
}
