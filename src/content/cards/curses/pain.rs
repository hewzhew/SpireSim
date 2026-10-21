// Pain triggers whenever you play ANOTHER card. We will hook this directly in the player input sequence where cards are played.
pub const MECHANICS: &str = "Unplayable. While this card is in your hand, lose 1 HP whenever you play another card; each Pain copy triggers separately. Block does not prevent this HP loss. Discarding or Exhausting it removes this trigger until it returns to hand; combat Exhaust does not remove it from your permanent deck.";

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
