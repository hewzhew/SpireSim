/// Java Lizard Tail requests healing of 50% max HP, minimum 1, before modifiers.
///
/// The actual trigger is handled inline during the player death check
/// (`AbstractPlayer.damage`), not through a generic on-lose-hp relic bus.
pub fn revive_amount(max_hp: i32) -> i32 {
    std::cmp::max(1, max_hp / 2)
}
