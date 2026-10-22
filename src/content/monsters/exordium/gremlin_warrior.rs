use super::{apply_power_action, attack_actions, set_next_move_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, EffectStrength, MonsterTurnPlan, MoveStep,
    MoveTarget, PowerEffectKind,
};

const SCRATCH: u8 = 1;
const ESCAPE: u8 = 99;

pub struct GremlinWarrior;

enum GremlinWarriorTurn<'a> {
    Scratch(&'a AttackSpec),
    Escape,
}

fn scratch_damage(asc: u8) -> i32 {
    if asc >= 2 {
        5
    } else {
        4
    }
}

fn angry_amount(asc: u8) -> i32 {
    if asc >= 17 {
        2
    } else {
        1
    }
}

fn scratch_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        SCRATCH,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: scratch_damage(asc),
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
        SCRATCH => scratch_plan(asc),
        ESCAPE => escape_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![("Scratch", scratch_plan(asc)), ("Escape", escape_plan())],
        special_moves: vec![("Starting Angry", format!("amount {}. {}", angry_amount(asc), crate::content::powers::mechanics::get(PowerId::Angry).unwrap().effect(sts_presentation::Locale::En)))],
        notes: "Normally attacks every turn. Angry Strength increases the damage of later attacks. Escape is a separately assigned move, not part of the normal attack cycle.",
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> GremlinWarriorTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            SCRATCH,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => GremlinWarriorTurn::Scratch(attack),
        (ESCAPE, [MoveStep::Escape]) => GremlinWarriorTurn::Escape,
        (_, []) => panic!("gremlin warrior plan missing locked truth"),
        (move_id, steps) => panic!(
            "gremlin warrior plan/steps mismatch: {} {:?}",
            move_id, steps
        ),
    }
}

impl MonsterBehavior for GremlinWarrior {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        asc: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        scratch_plan(asc)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, asc) = crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![apply_power_action(
            entity,
            &ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Angry,
                amount: angry_amount(asc),
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            },
        )]
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match decode_turn(plan) {
            GremlinWarriorTurn::Scratch(attack) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(
                    entity,
                    scratch_plan(state.meta.ascension_level),
                ));
                actions
            }
            GremlinWarriorTurn::Escape => vec![
                Action::Escape { target: entity.id },
                set_next_move_action(entity, escape_plan()),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{escape_plan, scratch_plan, GremlinWarrior};
    use crate::content::monsters::{EnemyId, MonsterBehavior, PreBattleLegacyRng};
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;

    #[test]
    fn prebattle_angry_amount_matches_java_a17_threshold() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::GremlinWarrior);

        state.meta.ascension_level = 16;
        let actions =
            GremlinWarrior::use_pre_battle_actions(&mut state, &entity, PreBattleLegacyRng::Misc);
        assert!(matches!(
            actions.as_slice(),
            [Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::Angry,
                amount: 1,
            }]
        ));

        state.meta.ascension_level = 17;
        let actions =
            GremlinWarrior::use_pre_battle_actions(&mut state, &entity, PreBattleLegacyRng::Misc);
        assert!(matches!(
            actions.as_slice(),
            [Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::Angry,
                amount: 2,
            }]
        ));
    }

    #[test]
    fn scratch_damage_and_followup_setmove_match_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 2;
        let entity = crate::test_support::test_monster(EnemyId::GremlinWarrior);

        let actions = GremlinWarrior::take_turn_plan(&mut state, &entity, &scratch_plan(2));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 5,
                    ..
                },
                Action::SetMonsterMove {
                    monster_id: 1,
                    next_move_byte: super::SCRATCH,
                    ..
                },
            ]
        ));
    }

    #[test]
    fn escape_turn_queues_escape_intent_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::GremlinWarrior);

        let actions = GremlinWarrior::take_turn_plan(&mut state, &entity, &escape_plan());

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
