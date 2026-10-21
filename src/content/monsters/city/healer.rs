use crate::content::monsters::exordium::{attack_actions, PLAYER};
use crate::content::monsters::{MonsterBehavior, MonsterRollContext};
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, DebuffSpec, EffectStrength, HealStep,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct Healer;

const ATTACK: u8 = 1;
const HEAL: u8 = 2;
const BUFF: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn heal_keeps_the_original_buff_icon_and_locked_group_healing() {
        for (ascension, amount) in [(0, 16), (17, 20)] {
            let plan = heal_plan(ascension);
            assert_eq!(
                crate::runtime::combat::Intent::from_visible_move_spec(&plan.summary_spec()),
                crate::runtime::combat::Intent::Buff
            );
            assert!(matches!(
                plan.steps.as_slice(),
                [MoveStep::Heal(HealStep { target: MoveTarget::AllMonsters, amount: actual })]
                    if *actual == amount
            ));
            let healer = crate::test_support::test_monster(EnemyId::Healer);
            let mut state = crate::test_support::combat_with_monsters(vec![healer.clone()]);
            let actions = Healer::take_turn_plan(&mut state, &healer, &plan);
            assert!(matches!(actions.as_slice(),
                [Action::Heal { target: 1, amount: actual }, Action::RollMonsterMove { monster_id: 1 }]
                    if *actual == amount
            ));
        }
    }

    #[test]
    fn heal_roll_counts_zero_hp_non_dying_allies_like_java_need_to_heal() {
        let healer = crate::test_support::test_monster(EnemyId::Healer);
        let mut ally = crate::test_support::test_monster(EnemyId::Centurion);
        ally.id = 2;
        ally.current_hp = 0;
        ally.is_dying = false;
        ally.is_escaped = false;
        let monsters = vec![healer.clone(), ally];

        let plan = Healer::roll_move_custom_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &healer,
            0,
            0,
            &monsters,
        );

        assert_eq!(plan.move_id, HEAL);
    }

    #[test]
    fn heal_turn_targets_zero_hp_non_dying_allies_like_java_loop() {
        let healer = crate::test_support::test_monster(EnemyId::Healer);
        let mut ally = crate::test_support::test_monster(EnemyId::Centurion);
        ally.id = 2;
        ally.current_hp = 0;
        ally.is_dying = false;
        ally.is_escaped = false;
        let mut state = crate::test_support::combat_with_monsters(vec![healer.clone(), ally]);

        let actions = Healer::take_turn_plan(&mut state, &healer, &heal_plan(0));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::Heal {
                    target: 1,
                    amount: 16
                },
                Action::Heal {
                    target: 2,
                    amount: 16
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }
}

enum HealerTurn<'a> {
    Attack(&'a AttackSpec, &'a ApplyPowerStep),
    Heal(&'a HealStep),
    Buff(&'a ApplyPowerStep),
}

fn attack_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        9
    } else {
        8
    }
}

fn heal_amount(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        20
    } else {
        16
    }
}

fn heal_threshold(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        20
    } else {
        15
    }
}

fn strength_amount(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        4
    } else if ascension_level >= 2 {
        3
    } else {
        2
    }
}

fn attack_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        ATTACK,
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: attack_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Frail,
                amount: 2,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn heal_plan(ascension_level: u8) -> MonsterTurnPlan {
    let amount = heal_amount(ascension_level);
    MonsterTurnPlan::with_visible_spec(
        HEAL,
        smallvec![MoveStep::Heal(HealStep {
            target: MoveTarget::AllMonsters,
            amount,
        })],
        MonsterMoveSpec::BuffIntent,
    )
}

fn buff_plan(ascension_level: u8) -> MonsterTurnPlan {
    let amount = strength_amount(ascension_level);
    MonsterTurnPlan::with_visible_spec(
        BUFF,
        smallvec![MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::AllMonsters,
            power_id: PowerId::Strength,
            amount,
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        })],
        MonsterMoveSpec::Buff(crate::semantics::combat::BuffSpec {
            power_id: PowerId::Strength,
            amount,
        }),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        ATTACK => attack_plan(ascension_level),
        HEAL => heal_plan(ascension_level),
        BUFF => buff_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Attack and Frail", attack_plan(asc)), ("Heal", heal_plan(asc)), ("Strengthen", buff_plan(asc))],
        special_moves: vec![("Move selection", format!("When living monsters' total missing HP is greater than {}, chooses Heal unless the last two moves were Heal. Otherwise chooses Attack and Frail or Strengthen. Heal and Strengthen cannot each repeat three times consecutively; at A17+ Attack and Frail cannot repeat on consecutive turns. These conditions are checked when selecting a move, not reevaluated when an already selected move executes.", heal_threshold(asc)))],
        notes: "Heal restores the listed HP to each living monster, including Mystic, up to each monster's Max HP. Strengthen grants persistent Strength to each living monster, including Mystic. Both Heal and Strengthen have a Buff intent icon: that icon alone does not distinguish them. Attack and Frail applies Frail after its hit, even if Block absorbs all damage.",
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let history = entity.move_history();
    history.len() >= 2
        && history[history.len() - 1] == move_id
        && history[history.len() - 2] == move_id
}

fn total_missing_hp(monsters: &[MonsterEntity]) -> i32 {
    monsters
        .iter()
        .filter(|monster| !monster.is_dying && !monster.is_escaped)
        .map(|monster| (monster.max_hp - monster.current_hp).max(0))
        .sum()
}

fn living_monster_ids(state: &CombatState) -> Vec<usize> {
    state
        .entities
        .monsters
        .iter()
        .filter(|monster| !monster.is_dying && !monster.is_escaped)
        .map(|monster| monster.id)
        .collect()
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> HealerTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            ATTACK,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }), MoveStep::ApplyPower(
                frail @ ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Frail,
                    effect: PowerEffectKind::Debuff,
                    ..
                },
            )],
        ) => HealerTurn::Attack(attack, frail),
        (
            HEAL,
            [MoveStep::Heal(
                heal @ HealStep {
                    target: MoveTarget::AllMonsters,
                    ..
                },
            )],
        ) => HealerTurn::Heal(heal),
        (
            BUFF,
            [MoveStep::ApplyPower(
                power @ ApplyPowerStep {
                    target: MoveTarget::AllMonsters,
                    power_id: PowerId::Strength,
                    effect: PowerEffectKind::Buff,
                    ..
                },
            )],
        ) => HealerTurn::Buff(power),
        (_, []) => panic!("healer plan missing locked truth"),
        (move_id, steps) => panic!("healer plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl Healer {
    fn roll_move_custom_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        monsters: &[MonsterEntity],
    ) -> MonsterTurnPlan {
        if total_missing_hp(monsters) > heal_threshold(ascension_level)
            && !last_two_moves(entity, HEAL)
        {
            return heal_plan(ascension_level);
        }

        if ascension_level >= 17 {
            if num >= 40 && !last_move(entity, ATTACK) {
                return attack_plan(ascension_level);
            }
        } else if num >= 40 && !last_two_moves(entity, ATTACK) {
            return attack_plan(ascension_level);
        }

        if !last_two_moves(entity, BUFF) {
            return buff_plan(ascension_level);
        }

        attack_plan(ascension_level)
    }
}

impl MonsterBehavior for Healer {
    fn roll_move_plan_with_context(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        ctx: MonsterRollContext<'_>,
    ) -> MonsterTurnPlan {
        Self::roll_move_custom_plan(rng, entity, ascension_level, num, ctx.monsters)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match decode_turn(plan) {
            HealerTurn::Attack(attack, frail) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(Action::ApplyPower {
                    source: entity.id,
                    target: PLAYER,
                    power_id: frail.power_id,
                    amount: frail.amount,
                });
                actions
            }
            HealerTurn::Heal(heal) => living_monster_ids(state)
                .into_iter()
                .map(|target| Action::Heal {
                    target,
                    amount: heal.amount,
                })
                .collect(),
            HealerTurn::Buff(power) => living_monster_ids(state)
                .into_iter()
                .map(|target| Action::ApplyPower {
                    source: entity.id,
                    target,
                    power_id: power.power_id,
                    amount: power.amount,
                })
                .collect(),
        };

        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
