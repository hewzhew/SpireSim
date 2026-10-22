use crate::content::monsters::exordium::{
    apply_power_action, attack_actions, gain_block_action, set_next_move_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, DamageKind, DebuffSpec, DefendSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};

pub struct BanditBear;

const MAUL: u8 = 1;
const BEAR_HUG: u8 = 2;
const LUNGE: u8 = 3;

fn maul_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        20
    } else {
        18
    }
}

fn lunge_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        10
    } else {
        9
    }
}

fn lunge_block() -> i32 {
    9
}

fn dexterity_reduction(ascension_level: u8) -> i32 {
    if ascension_level >= 17 {
        -4
    } else {
        -2
    }
}

fn maul_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        MAUL,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: maul_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn bear_hug_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BEAR_HUG,
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Dexterity,
            amount: dexterity_reduction(ascension_level),
            strength: EffectStrength::Strong,
        }),
    )
}

fn lunge_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        LUNGE,
        MonsterMoveSpec::AttackDefend(
            AttackSpec {
                base_damage: lunge_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DefendSpec {
                block: lunge_block(),
            },
        ),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        MAUL => maul_plan(ascension_level),
        BEAR_HUG => bear_hug_plan(ascension_level),
        LUNGE => lunge_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(ascension_level: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            (
                Text::new("bandit_bear.bearHug"),
                bear_hug_plan(ascension_level),
            ),
            (Text::new("bandit_bear.lunge"), lunge_plan(ascension_level)),
            (Text::new("bandit_bear.maul"), maul_plan(ascension_level)),
        ],
        special_moves: vec![],
        notes: Text::new("bandit_bear.notes"),
    }
}

impl MonsterBehavior for BanditBear {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        // Java BanditBear.getMove(int) always sets BEAR_HUG. The Maul/Lunge
        // chain is advanced by SetMoveAction in takeTurn.
        bear_hug_plan(ascension_level)
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
            (
                BEAR_HUG,
                [MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::Player,
                    power_id: PowerId::Dexterity,
                    effect: PowerEffectKind::Debuff,
                    ..
                })],
            ) => vec![
                apply_power_action(
                    entity,
                    &ApplyPowerStep {
                        target: MoveTarget::Player,
                        power_id: PowerId::Dexterity,
                        amount: dexterity_reduction(state.meta.ascension_level),
                        effect: PowerEffectKind::Debuff,
                        visible_strength: EffectStrength::Strong,
                    },
                ),
                set_next_move_action(entity, lunge_plan(state.meta.ascension_level)),
            ],
            (
                MAUL,
                [MoveStep::Attack(crate::semantics::combat::AttackStep {
                    target: MoveTarget::Player,
                    attack,
                })],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(
                    entity,
                    lunge_plan(state.meta.ascension_level),
                ));
                actions
            }
            (
                LUNGE,
                [MoveStep::Attack(crate::semantics::combat::AttackStep {
                    target: MoveTarget::Player,
                    attack,
                }), MoveStep::GainBlock(block)],
            ) if block.target == MoveTarget::SelfTarget => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(gain_block_action(entity, block));
                actions.push(set_next_move_action(
                    entity,
                    maul_plan(state.meta.ascension_level),
                ));
                actions
            }
            (_, []) => panic!("bandit bear plan missing locked truth"),
            (move_id, steps) => panic!("bandit bear plan/steps mismatch: {} {:?}", move_id, steps),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BanditBear, BEAR_HUG, LUNGE, MAUL};
    use crate::content::monsters::{EnemyId, MonsterBehavior};

    #[test]
    fn roll_move_always_returns_bear_hug_like_java_get_move() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut bear = crate::testing::support::test_monster(EnemyId::BanditBear);
        bear.move_history_mut().extend([BEAR_HUG, LUNGE, MAUL]);

        assert_eq!(
            BanditBear::roll_move_plan(&mut rng, &bear, 17, 99).move_id,
            BEAR_HUG,
            "Java BanditBear.getMove(int) ignores move history; attack chaining is SetMoveAction-only"
        );
    }
}
