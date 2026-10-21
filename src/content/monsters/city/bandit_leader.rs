use crate::content::monsters::exordium::{
    apply_power_action, attack_actions, set_next_move_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, DamageKind, DebuffSpec, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};

pub struct BanditLeader;

const CROSS_SLASH: u8 = 1;
const MOCK: u8 = 2;
const AGONIZING_SLASH: u8 = 3;

fn cross_slash_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        17
    } else {
        15
    }
}

fn agonizing_slash_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        12
    } else {
        10
    }
}

fn weak_amount(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        3
    } else {
        2
    }
}

fn cross_slash_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        CROSS_SLASH,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: cross_slash_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn mock_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(MOCK, smallvec::smallvec![], MonsterMoveSpec::Unknown)
}

fn agonizing_slash_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        AGONIZING_SLASH,
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: agonizing_slash_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Weak,
                amount: weak_amount(ascension_level),
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        CROSS_SLASH => cross_slash_plan(ascension_level),
        MOCK => mock_plan(),
        AGONIZING_SLASH => agonizing_slash_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(ascension_level: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            ("Agonizing Slash", agonizing_slash_plan(ascension_level)),
            ("Cross Slash", cross_slash_plan(ascension_level)),
        ],
        special_moves: vec![(
            "Mock",
            "Does nothing this turn; plans Agonizing Slash next.".into(),
        )],
        notes: if ascension_level >= 17 {
            "Opens with Mock, then Agonizing Slash, then two consecutive Cross Slashes. Repeats Agonizing Slash followed by two Cross Slashes. Weak reduces the player's Attack damage for its remaining turns."
        } else {
            "Opens with Mock, then alternates Agonizing Slash and Cross Slash. Weak reduces the player's Attack damage for its remaining turns."
        },
    }
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().len() >= 2
        && entity.move_history()[entity.move_history().len() - 1] == move_id
        && entity.move_history()[entity.move_history().len() - 2] == move_id
}

impl MonsterBehavior for BanditLeader {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        _ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        // Java BanditLeader.getMove(int) always sets MOCK. The real attack chain
        // is advanced by SetMoveAction in takeTurn, not by RollMoveAction.
        mock_plan()
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match (plan.move_id, plan.steps.as_slice()) {
            (MOCK, []) => vec![set_next_move_action(
                entity,
                agonizing_slash_plan(state.meta.ascension_level),
            )],
            (
                AGONIZING_SLASH,
                [MoveStep::Attack(crate::semantics::combat::AttackStep {
                    target: MoveTarget::Player,
                    attack,
                }), MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Weak,
                    effect: PowerEffectKind::Debuff,
                    ..
                })],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(apply_power_action(
                    entity,
                    &ApplyPowerStep {
                        target: MoveTarget::Player,
                        power_id: PowerId::Weak,
                        amount: weak_amount(state.meta.ascension_level),
                        effect: PowerEffectKind::Debuff,
                        visible_strength: EffectStrength::Normal,
                    },
                ));
                actions.push(set_next_move_action(
                    entity,
                    cross_slash_plan(state.meta.ascension_level),
                ));
                actions
            }
            (
                CROSS_SLASH,
                [MoveStep::Attack(crate::semantics::combat::AttackStep {
                    target: MoveTarget::Player,
                    attack,
                })],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                let next_plan =
                    if state.meta.ascension_level >= 17 && !last_two_moves(entity, CROSS_SLASH) {
                        cross_slash_plan(state.meta.ascension_level)
                    } else {
                        agonizing_slash_plan(state.meta.ascension_level)
                    };
                actions.push(set_next_move_action(entity, next_plan));
                actions
            }
            (_, []) => panic!("bandit leader plan missing locked truth"),
            (move_id, steps) => {
                panic!("bandit leader plan/steps mismatch: {} {:?}", move_id, steps)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BanditLeader, AGONIZING_SLASH, CROSS_SLASH, MOCK};
    use crate::content::monsters::{EnemyId, MonsterBehavior};

    #[test]
    fn roll_move_always_returns_mock_like_java_get_move() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut leader = crate::testing::support::test_monster(EnemyId::BanditLeader);
        leader
            .move_history_mut()
            .extend([MOCK, AGONIZING_SLASH, CROSS_SLASH, CROSS_SLASH]);

        assert_eq!(
            BanditLeader::roll_move_plan(&mut rng, &leader, 17, 99).move_id,
            MOCK,
            "Java BanditLeader.getMove(int) ignores move history; attack chaining is SetMoveAction-only"
        );
    }
}
