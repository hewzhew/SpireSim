pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "remaining HP damage allowance before the owner's next turn start",
    effect: "Caps cumulative HP damage, including HP loss. Damage consumes the remaining allowance; at zero, further damage is prevented. The stored reset cap restores the allowance at the owner's turn start, before start-of-turn damage such as Poison. The enemy definition supplies the initial allowance for the encounter's Ascension; the combat reading supplies the current remaining allowance.",
    timing: "Resets at the owner's turn start, not after every hit or at player turn end.",
    round_duration: false,
};

// InvinciblePower mechanic logic.
// Handled directly inside engine.rs because it strictly bounds all damage pathways (LoseHp vs Normal).

pub fn no_op() {}
