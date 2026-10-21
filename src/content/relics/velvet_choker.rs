/// VelvetChoker: You may not play more than 6 cards per turn.
/// Java also keeps the public relic counter synchronized with cards played.
/// Playability uses this relic's own counter, including for queued autoplay.
pub const CARD_PLAY_LIMIT: u32 = 6;

pub fn at_battle_start(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = 0;
}

pub fn at_turn_start(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = 0;
}

pub fn on_use_card(relic_state: &mut crate::content::relics::RelicState) {
    if relic_state.counter < CARD_PLAY_LIMIT as i32 {
        relic_state.counter += 1;
    }
}

pub fn on_victory(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = -1;
}

/// Check if the player can play a card (called by engine before card play).
pub fn can_play_card(relic_counter: i32) -> bool {
    remaining_card_plays(relic_counter) > 0
}

pub fn remaining_card_plays(relic_counter: i32) -> u32 {
    (i64::from(CARD_PLAY_LIMIT) - i64::from(relic_counter)).max(0) as u32
}
