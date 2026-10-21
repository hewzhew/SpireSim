use super::{apply_power_action, attack_actions, gain_block_action, set_next_move_action, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};

const DEBUFF: u8 = 1;
const STRONG_ATK: u8 = 3;
const OPEN: u8 = 4;
const IDLE: u8 = 5;
const OPEN_NATURAL: u8 = 6;
const ARMOR: i32 = 8;

pub struct Lagavulin;

enum LagavulinTurn<'a> {
    Debuff(&'a ApplyPowerStep, &'a ApplyPowerStep),
    Attack(&'a AttackSpec),
    Open,
    Idle,
    OpenNatural,
}

fn attack_damage(asc: u8) -> i32 {
    if asc >= 3 {
        20
    } else {
        18
    }
}

fn debuff_amount(asc: u8) -> i32 {
    if asc >= 18 {
        -2
    } else {
        -1
    }
}

fn attack_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        STRONG_ATK,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: attack_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn debuff_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        DEBUFF,
        smallvec::smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Dexterity,
                amount: debuff_amount(asc),
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Strength,
                amount: debuff_amount(asc),
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            })
        ],
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Dexterity,
            amount: debuff_amount(asc),
            strength: EffectStrength::Strong,
        }),
    )
}

fn open_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(OPEN, MoveStep::Stun)
}

fn idle_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(IDLE, MoveStep::Sleep)
}

fn open_natural_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        OPEN_NATURAL,
        smallvec::smallvec![MoveStep::Magic],
        MonsterMoveSpec::Unknown,
    )
}

fn plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        DEBUFF => debuff_plan(asc),
        STRONG_ATK => attack_plan(asc),
        OPEN => open_plan(),
        IDLE => idle_plan(),
        OPEN_NATURAL => open_natural_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![("Attack", attack_plan(asc)), ("Siphon Soul", debuff_plan(asc)),
            ("Sleep", idle_plan()), ("Wake from damage", open_plan())],
        special_moves: vec![("Sleeping start", format!("starts with {ARMOR} Block and {ARMOR} Metallicize. Positive HP loss wakes it, removes {ARMOR} Metallicize and replaces its current intent with Stun, giving a non-attacking wake turn. Blocked damage does not wake it. Otherwise it sleeps for three enemy turns, then removes {ARMOR} Metallicize and plans Attack.")),
            ("Natural wake", "wake without attacking on this enemy turn; remove sleeping Metallicize and plan Attack".into())],
        notes: "After waking from sleep, cycles two Attacks then Siphon Soul. The Strength and Dexterity losses persist; they are not temporary Shackled. An encounter that starts awake begins with Siphon Soul instead. Sleep is an intent, not an Asleep power.",
    }
}

fn decode_turn<'a>(plan: &'a MonsterTurnPlan) -> LagavulinTurn<'a> {
    match (plan.move_id, plan.steps.as_slice()) {
        (DEBUFF, [MoveStep::ApplyPower(dexterity), MoveStep::ApplyPower(strength)])
            if dexterity.target == MoveTarget::Player
                && dexterity.power_id == PowerId::Dexterity
                && dexterity.effect == PowerEffectKind::Debuff
                && strength.target == MoveTarget::Player
                && strength.power_id == PowerId::Strength
                && strength.effect == PowerEffectKind::Debuff =>
        {
            LagavulinTurn::Debuff(dexterity, strength)
        }
        (
            STRONG_ATK,
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })],
        ) => LagavulinTurn::Attack(attack),
        (OPEN, [MoveStep::Stun]) => LagavulinTurn::Open,
        (IDLE, [MoveStep::Sleep]) => LagavulinTurn::Idle,
        (OPEN_NATURAL, [MoveStep::Magic]) => LagavulinTurn::OpenNatural,
        (_, []) => panic!("lagavulin plan missing locked truth"),
        (move_id, steps) => panic!("lagavulin plan/steps mismatch: {} {:?}", move_id, steps),
    }
}

fn last_two_moves_are_attack(entity: &MonsterEntity) -> bool {
    let mut moves = entity.move_history().iter().rev().copied();
    matches!(moves.next(), Some(STRONG_ATK)) && matches!(moves.next(), Some(STRONG_ATK))
}

fn wake_effect_action(entity: &MonsterEntity) -> Action {
    Action::ReducePower {
        target: entity.id,
        power_id: PowerId::Metallicize,
        amount: ARMOR,
    }
}

fn lagavulin_runtime_update(
    entity: &MonsterEntity,
    idle_count: Option<u8>,
    debuff_turn_count: Option<u8>,
    is_out: Option<bool>,
    is_out_triggered: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Lagavulin {
            idle_count,
            debuff_turn_count,
            is_out,
            is_out_triggered,
        },
    }
}

impl MonsterBehavior for Lagavulin {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if !entity.lagavulin.is_out {
            return idle_plan();
        }
        if entity.lagavulin.debuff_turn_count < 2 {
            if last_two_moves_are_attack(entity) {
                debuff_plan(ascension_level)
            } else {
                attack_plan(ascension_level)
            }
        } else {
            debuff_plan(ascension_level)
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        if entity.is_dying || entity.half_dead {
            return entity.turn_plan();
        }
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        if entity.lagavulin.is_out {
            vec![set_next_move_action(entity, debuff_plan(_ascension_level))]
        } else {
            vec![
                gain_block_action(
                    entity,
                    &crate::semantics::combat::BlockStep {
                        target: MoveTarget::SelfTarget,
                        amount: ARMOR,
                    },
                ),
                apply_power_action(
                    entity,
                    &ApplyPowerStep {
                        target: MoveTarget::SelfTarget,
                        power_id: PowerId::Metallicize,
                        amount: ARMOR,
                        effect: PowerEffectKind::Buff,
                        visible_strength: EffectStrength::Normal,
                    },
                ),
            ]
        }
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let asc = state.meta.ascension_level;
        match decode_turn(plan) {
            LagavulinTurn::Debuff(dexterity, strength) => vec![
                lagavulin_runtime_update(entity, None, Some(0), None, None),
                apply_power_action(entity, dexterity),
                apply_power_action(entity, strength),
                Action::RollMonsterMove {
                    monster_id: entity.id,
                },
            ],
            LagavulinTurn::Attack(attack) => {
                let mut actions = vec![lagavulin_runtime_update(
                    entity,
                    None,
                    Some(entity.lagavulin.debuff_turn_count.saturating_add(1)),
                    None,
                    None,
                )];
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
            LagavulinTurn::Open => vec![Action::RollMonsterMove {
                monster_id: entity.id,
            }],
            LagavulinTurn::Idle => {
                let idle_count = entity.lagavulin.idle_count.saturating_add(1);
                if idle_count >= 3 {
                    let mut actions = vec![lagavulin_runtime_update(
                        entity,
                        Some(idle_count),
                        Some(0),
                        Some(true),
                        Some(true),
                    )];
                    actions.push(wake_effect_action(entity));
                    actions.push(set_next_move_action(entity, attack_plan(asc)));
                    actions
                } else {
                    vec![
                        lagavulin_runtime_update(entity, Some(idle_count), None, None, None),
                        // Native takeTurn sets Sleep immediately, then its
                        // queued RollMoveAction sets Sleep again. Both calls
                        // append moveHistory; only the roll consumes aiRng.
                        set_next_move_action(entity, idle_plan()),
                        Action::RollMonsterMove {
                            monster_id: entity.id,
                        },
                    ]
                }
            }
            LagavulinTurn::OpenNatural => {
                let mut actions = vec![lagavulin_runtime_update(
                    entity,
                    None,
                    Some(0),
                    Some(true),
                    Some(true),
                )];
                actions.push(wake_effect_action(entity));
                actions.push(set_next_move_action(entity, attack_plan(asc)));
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
        }
    }

    fn on_damaged(
        state: &mut CombatState,
        entity: &MonsterEntity,
        amount: i32,
    ) -> smallvec::SmallVec<[ActionInfo; 4]> {
        if amount <= 0 || entity.is_dying || entity.half_dead || entity.lagavulin.is_out_triggered {
            return smallvec::smallvec![];
        }

        if let Some(monster) = state
            .entities
            .monsters
            .iter_mut()
            .find(|monster| monster.id == entity.id)
        {
            monster.lagavulin.is_out_triggered = true;
        }
        // Native damage() sets STUN and appends moveHistory immediately.
        // ChangeStateAction(OPEN), including armor removal, remains queued:
        // Dagger Throw can open its discard choice before that action runs.
        crate::engine::action_handlers::execute_action(
            set_next_move_action(entity, open_plan()),
            state,
        );

        smallvec::smallvec![
            ActionInfo {
                action: lagavulin_runtime_update(entity, None, Some(0), Some(true), Some(true)),
                insertion_mode: AddTo::Bottom,
            },
            ActionInfo {
                action: wake_effect_action(entity),
                insertion_mode: AddTo::Bottom,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::open_natural_plan;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::PowerId;
    use crate::engine::action_handlers::damage::handle_fiend_fire;
    use crate::engine::core::tick_engine;
    use crate::runtime::action::{DamageInfo, DamageType};
    use crate::runtime::combat::{CombatCard, Power};
    use crate::runtime::monster_move::MonsterMoveSpec;
    use crate::state::core::EngineState;
    use crate::test_support::planned_monster;

    fn drain_to_stable(es: &mut EngineState, cs: &mut crate::runtime::combat::CombatState) -> bool {
        let mut iterations = 0;
        loop {
            match es {
                EngineState::CombatPlayerTurn => break,
                EngineState::CombatProcessing => {}
                EngineState::PendingChoice(_) => break,
                EngineState::GameOver(_) => return false,
                _ => break,
            }
            if !tick_engine(es, cs, None) {
                return false;
            }
            iterations += 1;
            assert!(iterations <= 1000, "tick loop exceeded 1000 iterations");
        }
        true
    }

    #[test]
    fn sleeping_turns_record_both_native_set_and_roll_then_wake_without_a_roll() {
        use crate::content::monsters::MonsterBehavior;
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = planned_monster(EnemyId::Lagavulin, super::IDLE);
        monster.lagavulin.is_out = false;
        monster.lagavulin.is_out_triggered = false;
        monster.move_history_mut().clear();
        monster.move_history_mut().push_back(super::IDLE);
        combat.entities.monsters.push(monster);
        let initial_ai = combat.rng.ai_rng.counter;
        for (turn, history, rolls) in [
            (1, vec![5, 5, 5], 1),
            (2, vec![5, 5, 5, 5, 5], 2),
            (3, vec![5, 5, 5, 5, 5, 3], 2),
        ] {
            let monster = combat.entities.monsters[0].clone();
            let plan = super::Lagavulin::turn_plan(&combat, &monster);
            for action in super::Lagavulin::take_turn_plan(&mut combat, &monster, &plan) {
                crate::engine::action_handlers::execute_action(action, &mut combat);
            }
            assert_eq!(combat.entities.monsters[0].lagavulin.idle_count, turn);
            assert_eq!(
                combat.entities.monsters[0]
                    .move_history()
                    .iter()
                    .copied()
                    .collect::<Vec<_>>(),
                history
            );
            assert_eq!(combat.rng.ai_rng.counter, initial_ai + rolls);
        }
        assert!(combat.entities.monsters[0].lagavulin.is_out);
        assert!(combat.entities.monsters[0].lagavulin.is_out_triggered);
    }

    #[test]
    fn natural_open_keeps_magic_execution_separate_from_unknown_visible_intent() {
        assert_eq!(
            open_natural_plan().visible_spec,
            Some(MonsterMoveSpec::Unknown)
        );
    }

    #[test]
    fn dagger_throw_wakes_before_discard_but_queues_the_armor_removal() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = planned_monster(EnemyId::Lagavulin, super::IDLE);
        monster.current_hp = 115;
        monster.max_hp = 115;
        monster.block = 8;
        monster.lagavulin.is_out = false;
        monster.lagavulin.is_out_triggered = false;
        monster.move_history_mut().clear();
        monster.move_history_mut().extend([super::IDLE; 5]);
        combat.entities.monsters.push(monster);
        combat.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::Metallicize,
                instance_id: None,
                amount: 8,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::Defend, 1));
        combat
            .zones
            .draw_pile
            .push(CombatCard::for_scenario(CardId::Strike, 2));
        let card = CombatCard::for_scenario(CardId::DaggerThrow, 3);
        let actions =
            crate::content::cards::resolve_card_play(CardId::DaggerThrow, &combat, &card, Some(1));
        combat.queue_actions(actions);
        let initial_ai = combat.rng.ai_rng.counter;
        let mut engine = EngineState::CombatProcessing;
        assert!(drain_to_stable(&mut engine, &mut combat));
        assert!(matches!(
            engine,
            EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect { .. })
        ));
        let monster = &combat.entities.monsters[0];
        assert_eq!(monster.planned_move_id(), super::OPEN);
        assert_eq!(
            monster.move_history().iter().copied().collect::<Vec<_>>(),
            [5, 5, 5, 5, 5, 4]
        );
        assert!(monster.lagavulin.is_out_triggered);
        assert!(!monster.lagavulin.is_out);
        assert_eq!(combat.entities.power_db[&1][0].amount, 8);
        assert_eq!(combat.rng.ai_rng.counter, initial_ai);
    }

    #[test]
    fn fiend_fire_wakes_lagavulin_only_once() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut lagavulin = planned_monster(EnemyId::Lagavulin, 5);
        lagavulin.current_hp = 111;
        lagavulin.max_hp = 111;
        lagavulin.block = 12;
        lagavulin.lagavulin.idle_count = 0;
        lagavulin.lagavulin.debuff_turn_count = 0;
        lagavulin.lagavulin.is_out = false;
        lagavulin.lagavulin.is_out_triggered = false;
        combat.entities.monsters.push(lagavulin);
        combat.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::Metallicize,
                instance_id: None,
                amount: 12,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            }],
        );
        combat.zones.hand.extend([
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::Bash, 3),
        ]);

        handle_fiend_fire(
            1,
            DamageInfo {
                source: 0,
                target: 1,
                base: 9,
                output: 9,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
            &mut combat,
        );

        let mut engine_state = EngineState::CombatProcessing;
        assert!(drain_to_stable(&mut engine_state, &mut combat));

        let metallicize = combat
            .entities
            .power_db
            .get(&1)
            .and_then(|powers| {
                powers
                    .iter()
                    .find(|power| power.power_type == PowerId::Metallicize)
            })
            .map(|power| power.amount);
        assert_eq!(metallicize, Some(4));
        assert!(combat.entities.monsters[0].lagavulin.is_out_triggered);
        assert!(combat.entities.monsters[0].lagavulin.is_out);
        assert_eq!(combat.entities.monsters[0].planned_move_id(), 4);
    }

    #[test]
    fn awake_lagavulin_initial_roll_and_prebattle_match_java_order() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut lagavulin = crate::test_support::test_monster(EnemyId::Lagavulin);
        lagavulin.lagavulin.is_out = true;
        lagavulin.lagavulin.is_out_triggered = true;

        let initial_roll =
            <super::Lagavulin as crate::content::monsters::MonsterBehavior>::roll_move_plan(
                &mut combat.rng.ai_rng,
                &lagavulin,
                0,
                0,
            );
        assert_eq!(
            initial_roll.move_id, super::STRONG_ATK,
            "Java init() rolls attack before non-asleep Lagavulin usePreBattleAction overrides to debuff"
        );

        lagavulin.set_planned_move_id(initial_roll.move_id);
        lagavulin.set_planned_steps(initial_roll.steps);
        lagavulin.set_planned_visible_spec(initial_roll.visible_spec);
        lagavulin.move_history_mut().push_back(initial_roll.move_id);

        let pre_battle =
            <super::Lagavulin as crate::content::monsters::MonsterBehavior>::use_pre_battle_actions(
                &mut combat,
                &lagavulin,
                crate::content::monsters::PreBattleLegacyRng::Misc,
            );
        assert!(matches!(
            pre_battle.as_slice(),
            [crate::runtime::action::Action::SetMonsterMove {
                next_move_byte: super::DEBUFF,
                ..
            }]
        ));
    }
}
