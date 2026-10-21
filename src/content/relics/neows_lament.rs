use crate::content::relics::RelicState;
use crate::runtime::action::ActionInfo;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: &str = "At the start of each of your next 3 combats, set every enemy present at that time to 1 current HP. Normal, elite and boss combats all count. The counter records remaining future activations and decreases immediately at combat start: a displayed 2 during the first combat means this combat was affected and 2 future combats remain. After the third activation the relic is used up, while that combat's initial enemies still have 1 HP. Enemies summoned later are not affected by this start-of-combat effect.";

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
