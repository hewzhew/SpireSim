// Marker read by unique kill-reward checks; encounter behavior owns escape.

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Minion,
        round_duration: false,
    };

pub struct MinionPower;

impl MinionPower {
    // Basic structural stub, not strictly required to have methods unless we add specific hooks.
}
