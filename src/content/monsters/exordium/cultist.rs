use super::{attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, EffectStrength, MonsterTurnPlan, MoveStep,
    MoveTarget, PowerEffectKind,
};

pub struct Cultist;

const ATTACK: u8 = 1;
const RITUAL: u8 = 3;

fn ritual_amount(asc: u8) -> i32 {
    if asc >= 17 {
        5
    } else if asc >= 2 {
        4
    } else {
        3
    }
}

fn attack_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        ATTACK,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: 6,
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn ritual_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        RITUAL,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Ritual,
            amount: ritual_amount(asc),
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        ATTACK => attack_plan(),
        RITUAL => ritual_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        special_moves: vec![],
        moves: vec![("Incantation", ritual_plan(asc)), ("Dark Strike", attack_plan())],
        notes: "Starts by gaining Ritual, then attacks. Monster Ritual skips its first end-of-round trigger; later triggers grant Strength repeatedly.",
    }
}

fn runtime(entity: &MonsterEntity) -> bool {
    assert!(
        entity.cultist.protocol_seeded,
        "cultist runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.cultist.first_move
}

fn cultist_runtime_update(entity: &MonsterEntity, first_move: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Cultist {
            first_move,
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for Cultist {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        asc: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if runtime(entity) {
            ritual_plan(asc)
        } else {
            attack_plan()
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if runtime(entity) {
            vec![cultist_runtime_update(entity, Some(false))]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match plan.steps.as_slice() {
            [MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Ritual,
                amount,
                effect: PowerEffectKind::Buff,
                ..
            })] => vec![
                Action::ApplyPowerDetailed {
                    source: entity.id,
                    target: entity.id,
                    power_id: PowerId::Ritual,
                    amount: *amount,
                    instance_id: None,
                    extra_data: Some(crate::content::powers::core::ritual::extra_data(
                        false, true,
                    )),
                },
                Action::RollMonsterMove {
                    monster_id: entity.id,
                },
            ],
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })] => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
            [] => panic!("cultist plan missing locked truth"),
            steps => panic!("cultist plan/steps mismatch: {:?}", steps),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cultist, ATTACK, RITUAL};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::{Action, MonsterRuntimePatch};

    #[test]
    fn cultist_first_roll_uses_private_first_move_and_marks_it() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let monster = crate::testing::support::test_monster(EnemyId::Cultist);
        let plan = Cultist::roll_move_plan(&mut rng, &monster, 0, 99);

        assert_eq!(plan.move_id, RITUAL);
        assert_eq!(
            Cultist::on_roll_move(0, &monster, 99, &plan),
            vec![Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::Cultist {
                    first_move: Some(false),
                    protocol_seeded: Some(true),
                },
            }]
        );
    }

    #[test]
    fn cultist_first_move_is_private_runtime_not_empty_history() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut monster = crate::testing::support::test_monster(EnemyId::Cultist);
        monster.cultist.first_move = false;

        assert_eq!(
            Cultist::roll_move_plan(&mut rng, &monster, 0, 99).move_id,
            ATTACK
        );
    }
}
