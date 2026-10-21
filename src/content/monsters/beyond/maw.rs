use crate::content::monsters::exordium::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MawRuntimeState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, BuffSpec, DamageKind, DebuffSpec, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct Maw;

const ROAR: u8 = 2;
const SLAM: u8 = 3;
const DROOL: u8 = 4;
const NOMNOMNOM: u8 = 5;
const NOM_DAMAGE: i32 = 5;

fn slam_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        30
    } else {
        25
    }
}

fn strength_up(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        5
    } else {
        3
    }
}

fn terrify_duration(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        5
    } else {
        3
    }
}

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.maw = MawRuntimeState {
        protocol_seeded: true,
        roared: false,
        turn_count: 1,
    };
}

fn runtime(entity: &MonsterEntity) -> &MawRuntimeState {
    assert!(
        entity.maw.protocol_seeded,
        "maw runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.maw
}

fn nom_hits_for_count(turn_count: i32) -> u8 {
    (turn_count / 2).max(1) as u8
}

fn maw_runtime_update(
    entity: &MonsterEntity,
    roared: Option<bool>,
    turn_count: Option<i32>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Maw {
            roared,
            turn_count,
            protocol_seeded: Some(true),
        },
    }
}

fn roar_plan(ascension_level: u8) -> MonsterTurnPlan {
    let terrify = terrify_duration(ascension_level);
    MonsterTurnPlan::with_visible_spec(
        ROAR,
        smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                amount: terrify,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Strong,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Frail,
                amount: terrify,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Strong,
            }),
        ],
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Weak,
            amount: terrify,
            strength: EffectStrength::Strong,
        }),
    )
}

fn slam_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SLAM,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: slam_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn drool_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        DROOL,
        MonsterMoveSpec::Buff(BuffSpec {
            power_id: PowerId::Strength,
            amount: strength_up(ascension_level),
        }),
    )
}

fn nom_plan(hits: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        NOMNOMNOM,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: NOM_DAMAGE,
            hits,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn plan_for(move_id: u8, ascension_level: u8, entity: &MonsterEntity) -> MonsterTurnPlan {
    match move_id {
        ROAR => roar_plan(ascension_level),
        SLAM => slam_plan(ascension_level),
        DROOL => drool_plan(ascension_level),
        NOMNOMNOM => nom_plan(nom_hits_for_count(runtime(entity).turn_count)),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            ("Roar", roar_plan(asc)),
            ("Slam", slam_plan(asc)),
            ("Drool", drool_plan(asc)),
        ],
        special_moves: vec![("Nom Nom Nom", format!("Attack player for {NOM_DAMAGE} base damage/hit. Hits grow with the enemy turn count: max(1, floor((enemy turn number + 1) / 2)), counting opening Roar as turn 1. Current visible intent gives the actual hit count and modified damage."))],
        notes: "Opens with Roar once, applying Weak and Frail. Afterward chooses Nom Nom Nom when its roll allows and that was not its previous move; otherwise uses Drool after Slam/Nom Nom Nom, or Slam after other moves. Drool grants persistent Strength, increasing every later normal attack hit.",
    }
}

impl MonsterBehavior for Maw {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        let next_turn_count = runtime(entity).turn_count + 1;
        if !runtime(entity).roared {
            return roar_plan(ascension_level);
        }

        let last_move = entity.move_history().back().copied();
        if num < 50 && last_move != Some(NOMNOMNOM) {
            return nom_plan(nom_hits_for_count(next_turn_count));
        }
        if matches!(last_move, Some(SLAM | NOMNOMNOM)) {
            return drool_plan(ascension_level);
        }
        slam_plan(ascension_level)
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        vec![maw_runtime_update(
            entity,
            None,
            Some(runtime(entity).turn_count + 1),
        )]
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level, entity)
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (ROAR, [MoveStep::ApplyPower(weak), MoveStep::ApplyPower(frail)]) => {
                vec![
                    maw_runtime_update(entity, Some(true), None),
                    apply_power_action(entity, weak),
                    apply_power_action(entity, frail),
                ]
            }
            (SLAM, [MoveStep::Attack(attack)]) => attack_actions(entity.id, PLAYER, &attack.attack),
            (DROOL, [MoveStep::ApplyPower(power)]) => vec![apply_power_action(entity, power)],
            (NOMNOMNOM, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (move_id, steps) => panic!("maw plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn imported_roared_false_forces_roar_even_with_roar_history() {
        let mut maw = crate::test_support::test_monster(EnemyId::Maw);
        maw.maw.roared = false;
        maw.maw.turn_count = 5;
        maw.move_history_mut().extend([ROAR, SLAM, DROOL]);

        let plan = Maw::roll_move_plan(&mut StsRng::new(0), &maw, 0, 99);

        assert_eq!(plan.move_id, ROAR);
    }

    #[test]
    fn imported_turn_count_drives_nom_hit_count() {
        let mut maw = crate::test_support::test_monster(EnemyId::Maw);
        maw.maw.roared = true;
        maw.maw.turn_count = 5;
        maw.move_history_mut().clear();

        let plan = Maw::roll_move_plan(&mut StsRng::new(0), &maw, 0, 0);

        assert_eq!(plan.move_id, NOMNOMNOM);
        assert_eq!(plan.attack().map(|attack| attack.hits), Some(3));
    }

    #[test]
    fn last_slam_or_nom_forces_drool_like_java() {
        let mut after_slam = crate::test_support::test_monster(EnemyId::Maw);
        after_slam.maw.roared = true;
        after_slam.move_history_mut().push_back(SLAM);
        let mut after_nom = crate::test_support::test_monster(EnemyId::Maw);
        after_nom.maw.roared = true;
        after_nom.move_history_mut().push_back(NOMNOMNOM);

        let slam_plan = Maw::roll_move_plan(&mut StsRng::new(0), &after_slam, 17, 99);
        let nom_plan = Maw::roll_move_plan(&mut StsRng::new(0), &after_nom, 17, 0);

        assert_eq!(slam_plan.move_id, DROOL);
        assert_eq!(nom_plan.move_id, DROOL);
        assert!(matches!(
            slam_plan.steps.as_slice(),
            [MoveStep::ApplyPower(ApplyPowerStep {
                power_id: PowerId::Strength,
                amount: 5,
                ..
            })]
        ));
    }

    #[test]
    fn high_roll_after_non_attack_selects_slam_like_java() {
        let mut maw = crate::test_support::test_monster(EnemyId::Maw);
        maw.maw.roared = true;
        maw.move_history_mut().push_back(DROOL);

        let plan = Maw::roll_move_plan(&mut StsRng::new(0), &maw, 2, 50);

        assert_eq!(plan.move_id, SLAM);
        assert_eq!(plan.attack().map(|attack| attack.base_damage), Some(30));
    }

    #[test]
    fn roll_move_increments_java_turn_count() {
        let mut maw = crate::test_support::test_monster(EnemyId::Maw);
        maw.maw.turn_count = 2;
        let plan = Maw::roll_move_plan(&mut StsRng::new(0), &maw, 0, 99);

        let actions = Maw::on_roll_move(0, &maw, 99, &plan);

        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::Maw {
                    roared: None,
                    turn_count: Some(3),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }

    #[test]
    fn roar_turn_marks_private_roared_before_queued_debuffs_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::Maw)];
        let maw = state.entities.monsters[0].clone();
        let plan = roar_plan(17);

        let actions = Maw::take_turn_plan(&mut state, &maw, &plan);

        assert!(matches!(
            actions.as_slice(),
            [
                Action::UpdateMonsterRuntime {
                    patch: MonsterRuntimePatch::Maw {
                        roared: Some(true),
                        turn_count: None,
                        protocol_seeded: Some(true),
                    },
                    ..
                },
                Action::ApplyPower {
                    power_id: PowerId::Weak,
                    amount: 5,
                    ..
                },
                Action::ApplyPower {
                    power_id: PowerId::Frail,
                    amount: 5,
                    ..
                },
                Action::RollMonsterMove { .. },
            ]
        ));
    }
}
