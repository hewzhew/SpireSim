use crate::content::monsters::EnemyId;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::EntityId;
use smallvec::{smallvec, SmallVec};

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "presence (not a stack count)",
    effect: "For a living Large Slime or Slime Boss, reaching half Max HP or below replaces the current intent with Split. Each child's current and Max HP equal the parent's remaining HP at split execution. More damage before splitting reduces the children; killing the parent prevents splitting.",
    timing: "Checked after damage; Split executes on the enemy turn. Medium and Small Slimes do not split.",
    round_duration: false,
};

pub fn on_hp_lost(state: &CombatState, owner: EntityId, _amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = smallvec![];

    // Java large slimes and Slime Boss run this from `damage()` after
    // `super.damage(info)`: the split interrupt only fires when the monster is
    // not dying, is at or below half HP, and is not already on the split move.
    if let Some(monster) = state.entities.monsters.iter().find(|m| m.id == owner) {
        if !monster.is_dying
            && monster.current_hp <= monster.max_hp / 2
            && monster.planned_move_id() != 3
        {
            let enemy_id = monster.enemy_id;
            let plan = match enemy_id {
                EnemyId::AcidSlimeL => {
                    if crate::content::monsters::exordium::large_slime_split_triggered(monster) {
                        return actions;
                    }
                    crate::content::monsters::exordium::acid_slime::split_plan(EnemyId::AcidSlimeM)
                }
                EnemyId::SpikeSlimeL => {
                    if crate::content::monsters::exordium::large_slime_split_triggered(monster) {
                        return actions;
                    }
                    crate::content::monsters::exordium::spike_slime::split_plan()
                }
                EnemyId::SlimeBoss => crate::content::monsters::exordium::slime_boss::split_plan(),
                _ => return actions,
            };
            actions.push(Action::SetMonsterMove {
                monster_id: owner,
                next_move_byte: plan.move_id,
                planned_steps: plan.steps,
                planned_visible_spec: plan.visible_spec,
            });
            if matches!(enemy_id, EnemyId::AcidSlimeL | EnemyId::SpikeSlimeL) {
                actions.push(
                    crate::content::monsters::exordium::mark_large_slime_split_triggered_action(
                        monster,
                    ),
                );
            }
        }
    }

    actions
}
