use crate::content::relics::RelicState;
use crate::runtime::action::ActionInfo;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::NeowsLament,
};

/// NeowsLament: Enemies in your first 3 combats have 1 HP.
/// Uses relic counter: starts at 3, decremented each combat until 0.
pub fn at_battle_start(
    state: &mut CombatState,
    relic: &mut RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    if relic.counter > 0 {
        relic.counter -= 1;
        if relic.counter == 0 {
            relic.counter = -2;
            relic.used_up = true;
        }

        // Java mutates monster currentHealth directly in the relic hook; this is
        // not HP-loss damage and must not wait for the action queue to drain.
        for monster in &mut state.entities.monsters {
            monster.current_hp = 1;
        }
    }

    SmallVec::new()
}
