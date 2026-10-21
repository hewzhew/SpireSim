use crate::content::monsters::exordium::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, BuffSpec, DamageKind, MonsterMoveSpec, MonsterTurnPlan, MoveStep,
};

pub struct Spiker;

const ATTACK: u8 = 1;
const BUFF_THORNS: u8 = 2;

fn attack_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        9
    } else {
        7
    }
}

pub(crate) fn starting_thorns(ascension_level: u8) -> i32 {
    let base = if ascension_level >= 2 { 4 } else { 3 };
    if ascension_level >= 17 {
        base + 3
    } else {
        base
    }
}

fn attack_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        ATTACK,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: attack_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn buff_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BUFF_THORNS,
        MonsterMoveSpec::Buff(BuffSpec {
            power_id: PowerId::Thorns,
            amount: 2,
        }),
    )
}

fn buff_count(entity: &MonsterEntity) -> usize {
    assert!(
        entity.spiker.protocol_seeded,
        "spiker runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.spiker.thorns_count as usize
}

fn increment_spiker_thorns_count(entity: &MonsterEntity) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Spiker {
            thorns_count: Some(entity.spiker.thorns_count.saturating_add(1)),
            protocol_seeded: Some(true),
        },
    }
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        ATTACK => attack_plan(ascension_level),
        BUFF_THORNS => buff_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Attack", attack_plan(asc)), ("Spike", buff_plan())],
        special_moves: vec![],
        notes: "Starts with persistent Thorns; it can increase Thorns by using Spike. Attack cannot occur twice consecutively while Spike remains available. After six completed Spike moves, always attacks. Thorns retaliates against each qualifying hit and can be blocked; see its definition for the damage type and trigger.",
    }
}

impl MonsterBehavior for Spiker {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::Thorns,
            amount: starting_thorns(ascension_level),
        }]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if buff_count(entity) > 5 {
            return attack_plan(ascension_level);
        }
        if num < 50 && entity.move_history().back().copied() != Some(ATTACK) {
            attack_plan(ascension_level)
        } else {
            buff_plan()
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
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (ATTACK, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (BUFF_THORNS, [MoveStep::ApplyPower(power)]) => vec![
                increment_spiker_thorns_count(entity),
                apply_power_action(entity, power),
            ],
            (move_id, steps) => panic!("spiker plan/steps mismatch: {} {:?}", move_id, steps),
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
    fn pre_battle_thorns_amount_matches_java_ascension_gates() {
        let mut state = crate::test_support::blank_test_combat();
        let spiker = crate::test_support::test_monster(EnemyId::Spiker);

        let normal = Spiker::use_pre_battle_actions(
            &mut state,
            &spiker,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );
        state.meta.ascension_level = 2;
        let asc2 = Spiker::use_pre_battle_actions(
            &mut state,
            &spiker,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );
        state.meta.ascension_level = 17;
        let asc17 = Spiker::use_pre_battle_actions(
            &mut state,
            &spiker,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );

        assert!(matches!(
            normal.as_slice(),
            [Action::ApplyPower {
                power_id: PowerId::Thorns,
                amount: 3,
                ..
            }]
        ));
        assert!(matches!(
            asc2.as_slice(),
            [Action::ApplyPower {
                power_id: PowerId::Thorns,
                amount: 4,
                ..
            }]
        ));
        assert!(matches!(
            asc17.as_slice(),
            [Action::ApplyPower {
                power_id: PowerId::Thorns,
                amount: 7,
                ..
            }]
        ));
    }

    #[test]
    fn executed_thorns_count_over_java_cap_forces_attack() {
        let mut spiker = crate::test_support::test_monster(EnemyId::Spiker);
        spiker.spiker.thorns_count = 6;
        spiker.move_history_mut().push_back(BUFF_THORNS);

        let plan = Spiker::roll_move_plan(&mut StsRng::new(0), &spiker, 2, 99);

        assert_eq!(plan.move_id, ATTACK);
        assert_eq!(plan.attack().map(|attack| attack.base_damage), Some(9));
    }

    #[test]
    fn planned_but_unexecuted_thorns_buff_does_not_advance_spiker_count() {
        let mut spiker = crate::test_support::test_monster(EnemyId::Spiker);
        spiker.spiker.thorns_count = 5;
        spiker.move_history_mut().extend([BUFF_THORNS; 6]);

        let plan =
            <Spiker as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &spiker, 20, 80);

        assert_eq!(
            plan.move_id, BUFF_THORNS,
            "Java thornsCount increments only when BUFF_THORNS executes; a planned move in moveHistory must not count as already executed"
        );
    }

    #[test]
    fn low_roll_attacks_only_when_previous_move_was_not_attack() {
        let mut after_buff = crate::test_support::test_monster(EnemyId::Spiker);
        after_buff.spiker.thorns_count = 0;
        after_buff.move_history_mut().push_back(BUFF_THORNS);
        let mut after_attack = crate::test_support::test_monster(EnemyId::Spiker);
        after_attack.spiker.thorns_count = 0;
        after_attack.move_history_mut().push_back(ATTACK);

        let attack_plan = Spiker::roll_move_plan(&mut StsRng::new(0), &after_buff, 0, 49);
        let buff_plan = Spiker::roll_move_plan(&mut StsRng::new(0), &after_attack, 0, 49);

        assert_eq!(attack_plan.move_id, ATTACK);
        assert_eq!(buff_plan.move_id, BUFF_THORNS);
    }

    #[test]
    fn attack_turn_queues_damage_before_roll_move() {
        let mut state = crate::test_support::blank_test_combat();
        let spiker = crate::test_support::test_monster(EnemyId::Spiker);

        let actions = Spiker::take_turn_plan(&mut state, &spiker, &attack_plan(2));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 9,
                    damage_kind: DamageKind::Normal
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn thorns_buff_execution_updates_runtime_before_apply_power() {
        let mut state = crate::test_support::blank_test_combat();
        let mut spiker = crate::test_support::test_monster(EnemyId::Spiker);
        spiker.id = 47;
        spiker.spiker.thorns_count = 5;

        let actions =
            <Spiker as MonsterBehavior>::take_turn_plan(&mut state, &spiker, &buff_plan());

        assert_eq!(
            actions,
            vec![
                Action::UpdateMonsterRuntime {
                    monster_id: 47,
                    patch: MonsterRuntimePatch::Spiker {
                        thorns_count: Some(6),
                        protocol_seeded: Some(true),
                    },
                },
                Action::ApplyPower {
                    source: 47,
                    target: 47,
                    power_id: PowerId::Thorns,
                    amount: 2,
                },
                Action::RollMonsterMove { monster_id: 47 },
            ],
            "Java increments thornsCount before queuing the Thorns ApplyPowerAction"
        );
    }
}
