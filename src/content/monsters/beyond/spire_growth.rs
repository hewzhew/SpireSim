use crate::content::monsters::exordium::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::{MonsterBehavior, MonsterRollContext};
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, DamageKind, DebuffSpec, EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep,
};

pub struct SpireGrowth;

const QUICK_TACKLE: u8 = 1;
const CONSTRICT: u8 = 2;
const SMASH: u8 = 3;

fn tackle_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        18
    } else {
        16
    }
}

fn smash_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        25
    } else {
        22
    }
}

fn constrict_amount(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        12
    } else {
        10
    }
}

fn tackle_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        QUICK_TACKLE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: tackle_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn constrict_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        CONSTRICT,
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Constricted,
            amount: constrict_amount(ascension_level),
            strength: EffectStrength::Strong,
        }),
    )
}

fn smash_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SMASH,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: smash_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let mut history = entity.move_history().iter().rev();
    matches!(
        (history.next().copied(), history.next().copied()),
        (Some(a), Some(b)) if a == move_id && b == move_id
    )
}

impl SpireGrowth {
    fn roll_move_custom_plan(
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        player_has_constricted: bool,
    ) -> MonsterTurnPlan {
        let last_move = entity.move_history().back().copied();

        if ascension_level >= 17 && !player_has_constricted && last_move != Some(CONSTRICT) {
            return constrict_plan(ascension_level);
        }
        if num < 50 && !last_two_moves(entity, QUICK_TACKLE) {
            return tackle_plan(ascension_level);
        }
        if !player_has_constricted && last_move != Some(CONSTRICT) {
            return constrict_plan(ascension_level);
        }
        if !last_two_moves(entity, SMASH) {
            return smash_plan(ascension_level);
        }
        tackle_plan(ascension_level)
    }
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        QUICK_TACKLE => tackle_plan(ascension_level),
        CONSTRICT => constrict_plan(ascension_level),
        SMASH => smash_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            ("Quick Tackle", tackle_plan(asc)),
            ("Constrict", constrict_plan(asc)),
            ("Smash", smash_plan(asc)),
        ],
        special_moves: vec![],
        notes: if asc >= 17 {
            "Prioritizes Constrict when the player lacks Constricted and the previous move was not Constrict, including its opening. Otherwise chooses Quick Tackle or Smash, with at most two consecutive uses of either attack. Constricted persists and deals blockable damage at the player's turn end; its application can be blocked by Artifact and it can be removed by debuff removal."
        } else {
            "Chooses Quick Tackle, Constrict or Smash. Constrict requires the player to lack Constricted and the previous move to differ from Constrict. Either attack can occur at most twice consecutively. Constricted persists and deals blockable damage at the player's turn end; its application can be blocked by Artifact and it can be removed by debuff removal."
        },
    }
}

impl MonsterBehavior for SpireGrowth {
    fn roll_move_plan_with_context(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        ctx: MonsterRollContext<'_>,
    ) -> MonsterTurnPlan {
        Self::roll_move_custom_plan(
            entity,
            ascension_level,
            num,
            ctx.player_has_power(PowerId::Constricted),
        )
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
            (QUICK_TACKLE, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (CONSTRICT, [MoveStep::ApplyPower(power)]) => vec![apply_power_action(entity, power)],
            (SMASH, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (move_id, steps) => panic!("spire growth plan/steps mismatch: {} {:?}", move_id, steps),
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
    use crate::content::monsters::{EnemyId, MonsterRollContext};
    use crate::runtime::combat::{Power, PowerPayload};
    use crate::runtime::rng::StsRng;

    fn player_power(power_type: PowerId, amount: i32) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    fn ctx_with_powers(player_powers: &[Power]) -> MonsterRollContext<'_> {
        MonsterRollContext {
            monsters: &[],
            player_powers,
        }
    }

    #[test]
    fn asc17_without_constricted_forces_constrict_before_rng_branch() {
        let mut growth = crate::test_support::test_monster(EnemyId::SpireGrowth);
        growth.move_history_mut().push_back(SMASH);
        let ctx = ctx_with_powers(&[]);

        let plan =
            SpireGrowth::roll_move_plan_with_context(&mut StsRng::new(0), &growth, 17, 0, ctx);

        assert_eq!(plan.move_id, CONSTRICT);
        assert!(matches!(
            plan.steps.as_slice(),
            [MoveStep::ApplyPower(
                crate::semantics::combat::ApplyPowerStep {
                    power_id: PowerId::Constricted,
                    amount: 12,
                    ..
                }
            )]
        ));
    }

    #[test]
    fn below_asc17_low_roll_tackle_precedes_constrict_branch() {
        let growth = crate::test_support::test_monster(EnemyId::SpireGrowth);
        let ctx = ctx_with_powers(&[]);

        let plan =
            SpireGrowth::roll_move_plan_with_context(&mut StsRng::new(0), &growth, 16, 49, ctx);

        assert_eq!(plan.move_id, QUICK_TACKLE);
        assert_eq!(plan.attack().map(|attack| attack.base_damage), Some(18));
    }

    #[test]
    fn player_constricted_and_last_two_smash_falls_back_to_tackle() {
        let mut growth = crate::test_support::test_monster(EnemyId::SpireGrowth);
        growth.move_history_mut().extend([SMASH, SMASH]);
        let player_powers = vec![player_power(PowerId::Constricted, 10)];
        let ctx = ctx_with_powers(&player_powers);

        let plan =
            SpireGrowth::roll_move_plan_with_context(&mut StsRng::new(0), &growth, 2, 99, ctx);

        assert_eq!(plan.move_id, QUICK_TACKLE);
        assert_eq!(plan.attack().map(|attack| attack.base_damage), Some(18));
    }

    #[test]
    fn constrict_turn_queues_constricted_before_roll_move() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 17;
        let growth = crate::test_support::test_monster(EnemyId::SpireGrowth);

        let actions = SpireGrowth::take_turn_plan(&mut state, &growth, &constrict_plan(17));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Constricted,
                    amount: 12
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn smash_turn_uses_asc2_damage_before_roll_move() {
        let mut state = crate::test_support::blank_test_combat();
        let growth = crate::test_support::test_monster(EnemyId::SpireGrowth);

        let actions = SpireGrowth::take_turn_plan(&mut state, &growth, &smash_plan(2));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 25,
                    damage_kind: DamageKind::Normal
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }
}
