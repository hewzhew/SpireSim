use crate::content::powers::PowerId;

pub const PREVENTED_POWER_ID: PowerId = PowerId::Weak;

pub const MECHANICS: &str = "Prevent new Weak applications to the player. This happens before Artifact, so blocked Weak does not consume Artifact. Does not remove existing Weak or prevent Frail or Vulnerable.";

/// Java ApplyPowerAction blocks player-targeted Weakened before Artifact.
pub fn on_receive_power_modify(power_id: PowerId, amount: i32) -> i32 {
    if power_id == PREVENTED_POWER_ID {
        return 0;
    }
    amount
}
