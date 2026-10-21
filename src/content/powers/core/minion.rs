// Marker read by unique kill-reward checks; encounter behavior owns escape.

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "presence flag",
    effect: "Marks the owner as a minion and excludes it from unique kill-reward triggers. Its enemy behavior determines leader-death escape; in the Gremlin Leader encounter, other non-dying allies escape when the leader dies.",
    timing: "Persistent marker; encounter behavior handles escape.",
    round_duration: false,
};

pub struct MinionPower;

impl MinionPower {
    // Basic structural stub, not strictly required to have methods unless we add specific hooks.
}
