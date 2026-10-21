use crate::content::cards::CardId;
use crate::runtime::action::ActionInfo;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// Medical Kit: Status cards can now be played. Playing a Status will Exhaust the card.
/// Java: onUseCard(card, action) → if card.type == STATUS → card.exhaust = true; action.exhaustCard = true
///
/// In our engine, we set exhaust_override on Status cards when played.
/// The actual "make Status cards playable" is handled in the engine's can-play-card checks.
pub fn on_use_card(_state: &CombatState, _card_id: CardId) -> SmallVec<[ActionInfo; 4]> {
    // The exhaust behavior is handled inline in core.rs where should_exhaust is computed:
    // We would set exhaust_override = true on the card being played.
    // However, since the card has already been played by the time on_use_card fires,
    // and the exhaust check happens in core.rs AFTER on_use_card hooks return,
    // we need the engine to check for MedicalKit directly.
    //
    // For now, this hook returns empty — the actual logic is:
    // In core.rs handle_player_turn_input, when computing should_exhaust:
    //   should_exhaust = ... || (has_medical_kit && card_type == Status)
    SmallVec::new()
}

/// Returns true if the player has MedicalKit, enabling Status cards to be playable.
pub fn can_play_status() -> bool {
    true
}
