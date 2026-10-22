pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Invincible,
        round_duration: false,
    };

// InvinciblePower mechanic logic.
// Handled directly inside engine.rs because it strictly bounds all damage pathways (LoseHp vs Normal).

pub fn no_op() {}
