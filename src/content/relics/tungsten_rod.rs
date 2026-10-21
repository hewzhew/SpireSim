pub const MECHANICS: &str = "Reduce each positive HP-loss event by 1, to a minimum of 0. Applied after Block and other damage modifiers, including Torii, and also applies to direct HP loss such as Offering. Torii reducing a qualifying hit to 1 lets Tungsten Rod reduce its final HP loss to 0. Heart's Beat of Death is reduced only when it would reach HP after Block. Effects triggered by actual HP loss use the reduced amount.";

/// Modifier: reduce HP loss by 1 (minimum 0)
pub fn modify_hp_loss(amount: i32) -> i32 {
    (amount - 1).max(0)
}
