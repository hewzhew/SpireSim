use crate::content::powers::PowerId;

pub const PREVENTED_POWER_ID: PowerId = PowerId::Frail;

pub const MECHANICS: &str = "Prevent new Frail applications to the player. This happens before Artifact, so blocked Frail does not consume Artifact. Does not remove existing Frail or prevent Weak or Vulnerable.";

/// Java ApplyPowerAction blocks player-targeted Frail before Artifact.
pub fn on_receive_power_modify(power_id: PowerId, amount: i32) -> i32 {
    if power_id == PREVENTED_POWER_ID {
        return 0;
    }
    amount
}
