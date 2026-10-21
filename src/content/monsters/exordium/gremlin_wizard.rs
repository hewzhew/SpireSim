use super::{attack_actions, set_next_move_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, AttackStep, DamageKind, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget,
};

const DOPE_MAGIC: u8 = 1;
const CHARGE: u8 = 2;
const ESCAPE: u8 = 99;

pub struct GremlinWizard;

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.gremlin_wizard.protocol_seeded = true;
    entity.gremlin_wizard.current_charge = 1;
}

enum GremlinWizardTurn<'a> {
    Charge,
    DopeMagic(&'a AttackSpec),
    Escape,
}

fn magic_damage(asc: u8) -> i32 {
    if asc >= 2 {
        30
    } else {
        25
    }
}

fn charge_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        CHARGE,
        smallvec::smallvec![MoveStep::Charge],
        MonsterMoveSpec::Unknown,
    )
}

fn dope_magic_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        DOPE_MAGIC,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: magic_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn escape_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(ESCAPE, MoveStep::Escape)
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        CHARGE => charge_plan(),
        DOPE_MAGIC => dope_magic_plan(asc),
        ESCAPE => escape_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            ("Charging", charge_plan()),
            ("Ultimate Blast", dope_magic_plan(asc)),
            ("Escape", escape_plan()),
        ],
        special_moves: vec![],
        notes: if asc >= 17 {
            "Starts with two non-attacking Charge turns, then Ultimate Blast every turn without recharging. Escape is a separately assigned move."
        } else {
            "Starts with two non-attacking Charge turns, then Ultimate Blast. After each Blast it charges for three turns before the next Blast. Escape is a separately assigned move."
        },
    }
}

fn runtime(entity: &MonsterEntity) -> u8 {
    assert!(
        entity.gremlin_wizard.protocol_seeded,
        "gremlin wizard runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.gremlin_wizard.current_charge
}

fn gremlin_wizard_runtime_update(entity: &MonsterEntity, current_charge: u8) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::GremlinWizard {
            current_charge: Some(current_charge),
            protocol_seeded: Some(true),
        },
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> GremlinWizardTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (CHARGE, [MoveStep::Charge]) => GremlinWizardTurn::Charge,
        (
            DOPE_MAGIC,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => GremlinWizardTurn::DopeMagic(attack),
        (ESCAPE, [MoveStep::Escape]) => GremlinWizardTurn::Escape,
        (_, []) => panic!("gremlin wizard plan missing locked truth"),
        (move_id, steps) => panic!(
            "gremlin wizard plan/steps mismatch: {} {:?}",
            move_id, steps
        ),
    }
}

impl MonsterBehavior for GremlinWizard {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        _asc: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        charge_plan()
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match decode_turn(plan) {
            GremlinWizardTurn::Charge => {
                let next_charge = runtime(entity).saturating_add(1);
                let next_plan = if next_charge == 3 {
                    dope_magic_plan(state.meta.ascension_level)
                } else {
                    charge_plan()
                };
                vec![
                    gremlin_wizard_runtime_update(entity, next_charge),
                    set_next_move_action(entity, next_plan),
                ]
            }
            GremlinWizardTurn::DopeMagic(attack) => {
                let mut actions = vec![gremlin_wizard_runtime_update(entity, 0)];
                let next_plan = if state.meta.ascension_level >= 17 {
                    dope_magic_plan(state.meta.ascension_level)
                } else {
                    charge_plan()
                };
                // Java resets currentCharge, queues DamageAction, then calls
                // setMove(...) synchronously. Model that move update before the
                // queued damage can be interrupted.
                actions.push(set_next_move_action(entity, next_plan));
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions
            }
            GremlinWizardTurn::Escape => vec![
                Action::Escape { target: entity.id },
                set_next_move_action(entity, escape_plan()),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GremlinWizard, CHARGE, DOPE_MAGIC};
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::action::{Action, MonsterRuntimePatch};

    #[test]
    fn gremlin_wizard_current_charge_drives_attack_without_move_history() {
        let mut state = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::GremlinWizard);
        monster.id = 7;
        monster.gremlin_wizard.current_charge = 2;
        monster.move_history_mut().clear();
        monster.set_planned_move_id(CHARGE);
        let plan = GremlinWizard::turn_plan(&state, &monster);

        let actions = GremlinWizard::take_turn_plan(&mut state, &monster, &plan);

        assert_eq!(
            actions,
            vec![
                Action::UpdateMonsterRuntime {
                    monster_id: 7,
                    patch: MonsterRuntimePatch::GremlinWizard {
                        current_charge: Some(3),
                        protocol_seeded: Some(true),
                    },
                },
                Action::SetMonsterMove {
                    monster_id: 7,
                    next_move_byte: DOPE_MAGIC,
                    planned_steps: super::dope_magic_plan(0).steps,
                    planned_visible_spec: super::dope_magic_plan(0).visible_spec,
                },
            ],
            "Java uses currentCharge, not reconstructed consecutive Charge history"
        );
    }

    #[test]
    fn gremlin_wizard_dope_magic_resets_current_charge_before_followup_move() {
        let mut state = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::GremlinWizard);
        monster.id = 7;
        monster.set_planned_move_id(DOPE_MAGIC);
        let plan = GremlinWizard::turn_plan(&state, &monster);

        let actions = GremlinWizard::take_turn_plan(&mut state, &monster, &plan);

        assert!(matches!(
            actions.first(),
            Some(Action::UpdateMonsterRuntime {
                monster_id: 7,
                patch: MonsterRuntimePatch::GremlinWizard {
                    current_charge: Some(0),
                    protocol_seeded: Some(true),
                },
            })
        ));
        assert!(matches!(
            actions.get(1),
            Some(Action::SetMonsterMove {
                monster_id: 7,
                next_move_byte: CHARGE,
                ..
            })
        ));
    }

    #[test]
    fn gremlin_wizard_escape_still_queues_escape_intent_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let monster = crate::test_support::test_monster(EnemyId::GremlinWizard);

        let actions = GremlinWizard::take_turn_plan(&mut state, &monster, &super::escape_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::Escape { .. },
                Action::SetMonsterMove {
                    next_move_byte: super::ESCAPE,
                    ..
                }
            ]
        ));
    }
}
