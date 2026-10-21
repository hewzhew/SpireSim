// Shovel: You can now Dig for loot at Rest Sites.
// Metagame logic checked during rest site evaluation.

pub const MECHANICS: &str = "At a campfire, you may choose Dig to receive a random relic through a reward screen. Dig uses the campfire's action, so compare it with Rest, Smith, Recall or another available action. The relic identity is determined when Dig resolves; merely entering a campfire gives no relic.";

pub fn has_shovel() -> bool {
    true
}
