use crate::content::monsters::exordium::{
    attack_actions, gain_block_action, set_next_move_action, steal_gold_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, AttackStep, BlockStep, DamageKind, MonsterMoveSpec, MonsterTurnPlan, MoveStep,
    MoveTarget, StealGoldStep,
};

const MUG: u8 = 1;
const SMOKE_BOMB: u8 = 2;
const ESCAPE: u8 = 3;
const BIG_SWIPE: u8 = 4;

pub struct Mugger;

enum MuggerTurn<'a> {
    Mug(&'a StealGoldStep, &'a AttackSpec),
    SmokeBomb(&'a BlockStep),
    Escape,
    BigSwipe(&'a StealGoldStep, &'a AttackSpec),
}

fn gold_amount(asc: u8) -> i32 {
    if asc >= 17 {
        20
    } else {
        15
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn mugger_mug_burns_java_audio_talk_and_branch_rng() {
        let mut state = crate::test_support::blank_test_combat();
        let mut entity = crate::test_support::test_monster(EnemyId::Mugger);

        entity.thief.slash_count = 0;
        let before_first = state.rng.ai_rng.counter;
        let _ = <Mugger as MonsterBehavior>::take_turn_plan(&mut state, &entity, &mug_plan(0));
        assert_eq!(
            state.rng.ai_rng.counter - before_first,
            1,
            "first Mugger Mug burns aiRng.random(2) for Java playSfx"
        );

        entity.thief.slash_count = 1;
        let before_second = state.rng.ai_rng.counter;
        let _ = <Mugger as MonsterBehavior>::take_turn_plan(&mut state, &entity, &mug_plan(0));
        assert_eq!(
            state.rng.ai_rng.counter - before_second,
            3,
            "second Mugger Mug burns playSfx, TalkAction randomBoolean(0.6), and the smoke-bomb/big-swipe branch"
        );
    }

    #[test]
    fn mugger_big_swipe_burns_java_audio_rng() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::Mugger);

        let before = state.rng.ai_rng.counter;
        let _ =
            <Mugger as MonsterBehavior>::take_turn_plan(&mut state, &entity, &big_swipe_plan(0));
        assert_eq!(
            state.rng.ai_rng.counter - before,
            1,
            "Mugger.java Big Swipe calls playSfx, which burns aiRng.random(2)"
        );
    }

    #[test]
    fn mugger_big_swipe_sets_smoke_bomb_before_queued_damage_like_java_immediate_set_move() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::Mugger);

        let actions =
            <Mugger as MonsterBehavior>::take_turn_plan(&mut state, &entity, &big_swipe_plan(0));

        assert!(
            matches!(actions.first(), Some(Action::SetMonsterMove { next_move_byte: SMOKE_BOMB, .. })),
            "Mugger.java BIGSWIPE calls setMove(SMOKE_BOMB) synchronously after queueing damage; Rust must apply that move before queued steal/damage can be interrupted"
        );
    }

    #[test]
    fn mugger_escape_still_queues_escape_intent_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::Mugger);

        let actions =
            <Mugger as MonsterBehavior>::take_turn_plan(&mut state, &entity, &escape_plan());

        assert!(
            matches!(
                actions.as_slice(),
                [
                    Action::Escape { .. },
                    Action::SetMonsterMove {
                        next_move_byte: ESCAPE,
                        ..
                    }
                ]
            ),
            "Mugger.java escape turn queues EscapeAction and then SetMoveAction(ESCAPE)"
        );
    }

    #[test]
    fn mugger_death_burns_java_death_sfx_rng_even_without_stolen_gold() {
        let mut state = crate::test_support::blank_test_combat();
        let entity = crate::test_support::test_monster(EnemyId::Mugger);

        let before = state.rng.ai_rng.counter;
        let actions = Mugger::on_death(&mut state, &entity);

        assert_eq!(state.rng.ai_rng.counter - before, 1);
        assert!(actions.is_empty());
    }
}

fn mug_damage(asc: u8) -> i32 {
    if asc >= 2 {
        11
    } else {
        10
    }
}

fn big_swipe_damage(asc: u8) -> i32 {
    if asc >= 2 {
        18
    } else {
        16
    }
}

fn smoke_bomb_block(asc: u8) -> i32 {
    if asc >= 17 {
        17
    } else {
        11
    }
}

fn mug_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        MUG,
        smallvec::smallvec![
            MoveStep::StealGold(StealGoldStep {
                amount: gold_amount(asc),
            }),
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: mug_damage(asc),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
        ],
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: mug_damage(asc),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn big_swipe_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        BIG_SWIPE,
        smallvec::smallvec![
            MoveStep::StealGold(StealGoldStep {
                amount: gold_amount(asc),
            }),
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: big_swipe_damage(asc),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
        ],
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: big_swipe_damage(asc),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn smoke_bomb_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        SMOKE_BOMB,
        MoveStep::GainBlock(BlockStep {
            target: MoveTarget::SelfTarget,
            amount: smoke_bomb_block(asc),
        }),
    )
}

fn escape_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(ESCAPE, MoveStep::Escape)
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        MUG => mug_plan(asc),
        SMOKE_BOMB => smoke_bomb_plan(asc),
        ESCAPE => escape_plan(),
        BIG_SWIPE => big_swipe_plan(asc),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            ("Mug", mug_plan(asc)),
            ("Big Swipe", big_swipe_plan(asc)),
            ("Smoke Bomb", smoke_bomb_plan(asc)),
            ("Escape", escape_plan()),
        ],
        special_moves: vec![],
        notes: "Starts with two Mugs, then has a 50% chance of Big Swipe before Smoke Bomb, otherwise immediately Smoke Bomb. Smoke Bomb is followed by Escape. Gold theft occurs before attack damage, even if Block prevents HP loss. Killing the Mugger recovers its stolen Gold as a reward; Escape does not recover it.",
    }
}

fn current_slash_count(entity: &MonsterEntity) -> u8 {
    assert!(
        entity.thief.protocol_seeded,
        "thief runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.thief.slash_count
}

fn current_stolen_gold(entity: &MonsterEntity) -> i32 {
    assert!(
        entity.thief.protocol_seeded,
        "thief runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.thief.stolen_gold
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> MuggerTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (
            MUG,
            [MoveStep::StealGold(steal), MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => MuggerTurn::Mug(steal, attack),
        (
            BIG_SWIPE,
            [MoveStep::StealGold(steal), MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => MuggerTurn::BigSwipe(steal, attack),
        (SMOKE_BOMB, [MoveStep::GainBlock(block)]) if block.target == MoveTarget::SelfTarget => {
            MuggerTurn::SmokeBomb(block)
        }
        (ESCAPE, [MoveStep::Escape]) => MuggerTurn::Escape,
        (_, []) => panic!("mugger plan missing locked truth"),
        (move_id, steps) => panic!("mugger plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

impl MonsterBehavior for Mugger {
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
            power_id: PowerId::Thievery,
            amount: gold_amount(ascension_level),
        }]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        mug_plan(ascension_level)
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
            MuggerTurn::Mug(steal, attack) => {
                // Java Mugger.playSfx uses AbstractDungeon.aiRng.random(2), so
                // even this audio-only branch must burn gameplay RNG.
                let _ = state.rng.ai_rng.random(2);
                if current_slash_count(entity) == 1 {
                    let _ = state.rng.ai_rng.random_boolean_chance(0.6);
                }
                let next_slash_count = current_slash_count(entity).saturating_add(1);
                let mut actions = vec![steal_gold_action(entity, steal)];
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                if next_slash_count == 2 {
                    if state.rng.ai_rng.random_boolean_chance(0.5) {
                        // Java calls setMove(SMOKE_BOMB) synchronously here,
                        // not through SetMoveAction.
                        actions.insert(
                            0,
                            set_next_move_action(
                                entity,
                                smoke_bomb_plan(state.meta.ascension_level),
                            ),
                        );
                    } else {
                        actions.push(set_next_move_action(
                            entity,
                            big_swipe_plan(state.meta.ascension_level),
                        ));
                    };
                } else {
                    actions.push(set_next_move_action(
                        entity,
                        mug_plan(state.meta.ascension_level),
                    ));
                }
                actions
            }
            MuggerTurn::BigSwipe(steal, attack) => {
                let _ = state.rng.ai_rng.random(2);
                // Java calls setMove(SMOKE_BOMB) synchronously after queueing
                // damage, so the move update is not vulnerable to later action
                // queue cleanup.
                let mut actions = vec![set_next_move_action(
                    entity,
                    smoke_bomb_plan(state.meta.ascension_level),
                )];
                actions.push(steal_gold_action(entity, steal));
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions
            }
            MuggerTurn::SmokeBomb(block) => vec![
                gain_block_action(entity, block),
                set_next_move_action(entity, escape_plan()),
            ],
            MuggerTurn::Escape => vec![
                Action::Escape { target: entity.id },
                set_next_move_action(entity, escape_plan()),
            ],
        }
    }

    fn on_death(state: &mut CombatState, entity: &MonsterEntity) -> Vec<Action> {
        // Java Mugger.die() calls playDeathSfx(), which consumes aiRng.random(2)
        // even though the selected sound itself is presentation-only.
        let _ = state.rng.ai_rng.random(2);
        let stolen_gold = current_stolen_gold(entity);
        state.add_stolen_gold_to_rewards(stolen_gold);
        Vec::new()
    }
}
