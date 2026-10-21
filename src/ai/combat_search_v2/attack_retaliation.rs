use crate::content::powers::{resolve_power_on_attacked, store::powers_snapshot_for, PowerId};
use crate::runtime::action::{Action, DamageInfo, DamageType};
use crate::runtime::combat::CombatState;
use crate::runtime::monster_move::{MoveStep, MoveTarget};
use crate::EntityId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AttackRetaliationEventProjectionV1 {
    pub(super) trigger_count: usize,
    pub(super) raw_player_damage: i32,
    pub(super) player_block_loss: i32,
    pub(super) player_hp_loss: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AttackRetaliationTargetFactV1 {
    pub(super) source_entity_id: EntityId,
    pub(super) power_source_count: usize,
    pub(super) raw_player_damage_per_damage_event: i32,
    pub(super) projected_player_block_loss_for_next_damage_event: i32,
    pub(super) projected_player_hp_loss_for_next_damage_event: i32,
    pub(super) visible_growth_amount: i32,
}

pub(super) fn attack_retaliation_for_event(
    combat: &CombatState,
    projection_state: &mut Option<CombatState>,
    info: &DamageInfo,
) -> AttackRetaliationEventProjectionV1 {
    if info.source != 0
        || !combat
            .entities
            .monsters
            .iter()
            .any(|monster| monster.id == info.target && monster.is_alive_for_action())
    {
        return AttackRetaliationEventProjectionV1::default();
    }
    let actions = powers_snapshot_for(combat, info.target)
        .into_iter()
        .flat_map(|power| {
            resolve_power_on_attacked(
                power.power_type,
                combat,
                info.target,
                info.output.max(info.base),
                info.source,
                info.damage_type,
                power.amount,
            )
        })
        .collect::<Vec<_>>();
    project_player_retaliation_actions(combat, projection_state, actions)
}

pub(super) fn attack_retaliation_for_target(
    combat: &CombatState,
    entity_id: EntityId,
) -> Option<AttackRetaliationTargetFactV1> {
    let owner = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == entity_id && monster.is_alive_for_action())?;
    let mut source_count = 0usize;
    let mut actions = Vec::new();
    for power in powers_snapshot_for(combat, entity_id) {
        let power_actions = resolve_power_on_attacked(
            power.power_type,
            combat,
            entity_id,
            1,
            0,
            DamageType::Normal,
            power.amount,
        );
        if power_actions
            .iter()
            .any(|action| raw_player_damage_from_action(action) > 0)
        {
            source_count += 1;
            actions.extend(power_actions);
        }
    }
    let mut projection_state = None;
    let projection = project_player_retaliation_actions(combat, &mut projection_state, actions);
    (projection.raw_player_damage > 0).then_some(AttackRetaliationTargetFactV1 {
        source_entity_id: entity_id,
        power_source_count: source_count,
        raw_player_damage_per_damage_event: projection.raw_player_damage,
        projected_player_block_loss_for_next_damage_event: projection.player_block_loss,
        projected_player_hp_loss_for_next_damage_event: projection.player_hp_loss,
        visible_growth_amount: owner
            .turn_plan()
            .steps
            .iter()
            .filter_map(|step| match step {
                MoveStep::ApplyPower(power)
                    if power.target == MoveTarget::SelfTarget
                        && power.power_id == PowerId::Thorns
                        && power.amount > 0 =>
                {
                    Some(power.amount)
                }
                _ => None,
            })
            .sum(),
    })
}

fn project_player_retaliation_actions(
    combat: &CombatState,
    projection_state: &mut Option<CombatState>,
    actions: impl IntoIterator<Item = Action>,
) -> AttackRetaliationEventProjectionV1 {
    let mut projection = AttackRetaliationEventProjectionV1::default();
    for action in actions {
        match action {
            Action::Damage(info) if info.target == 0 => {
                let state = projection_state.get_or_insert_with(|| combat.clone());
                let resolution =
                    crate::engine::action_handlers::damage::resolve_player_damage(&info, state);
                if resolution.raw_damage <= 0 {
                    continue;
                }
                projection.trigger_count = projection.trigger_count.saturating_add(1);
                projection.raw_player_damage = projection
                    .raw_player_damage
                    .saturating_add(resolution.raw_damage);
                projection.player_block_loss = projection
                    .player_block_loss
                    .saturating_add(resolution.block_consumed);
                projection.player_hp_loss =
                    projection.player_hp_loss.saturating_add(resolution.hp_loss);
            }
            Action::LoseHp {
                target: 0, amount, ..
            } if amount > 0 => {
                let state = projection_state.get_or_insert_with(|| combat.clone());
                projection.trigger_count = projection.trigger_count.saturating_add(1);
                projection.raw_player_damage = projection.raw_player_damage.saturating_add(amount);
                projection.player_hp_loss = projection.player_hp_loss.saturating_add(
                    crate::content::relics::hooks::on_lose_hp_last(state, amount).max(0),
                );
            }
            _ => {}
        }
    }
    projection
}

fn raw_player_damage_from_action(action: &Action) -> i32 {
    match action {
        Action::Damage(info) if info.target == 0 => info.output.max(info.base).max(0),
        Action::LoseHp {
            target: 0, amount, ..
        } => (*amount).max(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::PowerId;
    use crate::runtime::action::{DamageInfo, DamageType};
    use crate::runtime::combat::{CombatState, Power, PowerPayload};
    use crate::runtime::monster_move::{BuffSpec, MonsterMoveSpec};
    use crate::test_support::{blank_test_combat, test_monster};

    fn thorns_combat(amount: i32) -> CombatState {
        let mut combat = blank_test_combat();
        let mut spiker = test_monster(EnemyId::Spiker);
        spiker.id = 7;
        spiker.set_planned_steps(
            MonsterMoveSpec::Buff(BuffSpec {
                power_id: PowerId::Thorns,
                amount: 2,
            })
            .to_steps(),
        );
        combat.entities.monsters = vec![spiker];
        combat.entities.power_db.insert(
            7,
            vec![Power {
                power_type: PowerId::Thorns,
                instance_id: None,
                amount,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        combat
    }

    #[test]
    fn target_fact_reports_engine_resolved_thorns_and_visible_growth() {
        let combat = thorns_combat(3);

        assert_eq!(
            attack_retaliation_for_target(&combat, 7),
            Some(AttackRetaliationTargetFactV1 {
                source_entity_id: 7,
                power_source_count: 1,
                raw_player_damage_per_damage_event: 3,
                projected_player_block_loss_for_next_damage_event: 0,
                projected_player_hp_loss_for_next_damage_event: 3,
                visible_growth_amount: 2,
            })
        );
    }

    #[test]
    fn event_loss_uses_actual_damage_source_and_type() {
        let combat = thorns_combat(5);
        let info = DamageInfo {
            source: 0,
            target: 7,
            base: 6,
            output: 6,
            damage_type: DamageType::Normal,
            is_modified: false,
        };

        let mut projection_state = None;
        assert_eq!(
            attack_retaliation_for_event(&combat, &mut projection_state, &info).player_hp_loss,
            5
        );
        let mut projection_state = None;
        assert_eq!(
            attack_retaliation_for_event(
                &combat,
                &mut projection_state,
                &DamageInfo {
                    damage_type: DamageType::Thorns,
                    ..info
                }
            )
            .player_hp_loss,
            0
        );
    }

    #[test]
    fn target_fact_is_absent_without_player_damage_retaliation() {
        let mut combat = thorns_combat(0);
        combat.entities.power_db.remove(&7);

        assert_eq!(attack_retaliation_for_target(&combat, 7), None);
    }
}
