use crate::content::powers::PowerId;

pub const PREVENTED_POWER_ID: PowerId = PowerId::Frail;

pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Turnip,
};

/// Java ApplyPowerAction blocks player-targeted Frail before Artifact.
pub fn on_receive_power_modify(power_id: PowerId, amount: i32) -> i32 {
    if power_id == PREVENTED_POWER_ID {
        return 0;
    }
    amount
}
