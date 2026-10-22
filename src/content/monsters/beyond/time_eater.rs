use crate::content::cards::CardId;
use crate::content::monsters::exordium::{
    add_card_action, apply_power_action, attack_actions, gain_block_action, remove_power_action,
    utility_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity, TimeEaterRuntimeState};
use crate::semantics::combat::{
    AddCardStep, ApplyPowerStep, AttackSpec, AttackStep, BlockStep, DamageKind, DebuffSpec,
    DefendSpec, EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget,
    PowerEffectKind, RemovePowerStep, UtilityStep,
};
use smallvec::smallvec;

pub struct TimeEater;

const REVERBERATE: u8 = 2;
const RIPPLE: u8 = 3;
const HEAD_SLAM: u8 = 4;
const HASTE: u8 = 5;

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.time_eater = TimeEaterRuntimeState {
        protocol_seeded: true,
        used_haste: false,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn haste_heal_amount_is_read_at_execution_time_like_java() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.id = 1;
        time_eater.max_hp = 456;
        time_eater.current_hp = 220;
        let plan = haste_plan(&time_eater, 19);

        time_eater.current_hp = 100;
        let mut state = crate::test_support::combat_with_monsters(vec![time_eater.clone()]);

        let actions = TimeEater::take_turn_plan(&mut state, &time_eater, &plan);

        assert!(matches!(
            actions
                .iter()
                .find(|action| matches!(action, Action::Heal { .. })),
            Some(Action::Heal {
                target: 1,
                amount: 128
            })
        ));
    }

    #[test]
    fn haste_has_javas_buff_intent_without_exposing_a_heal_amount() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.max_hp = 456;
        time_eater.current_hp = 220;

        let plan = haste_plan(&time_eater, 19);

        assert_eq!(plan.visible_spec, Some(MonsterMoveSpec::BuffIntent));
    }

    #[test]
    fn haste_selection_marks_private_used_haste_during_roll() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.max_hp = 456;
        time_eater.current_hp = 200;

        let plan = TimeEater::roll_move_plan(&mut StsRng::new(0), &time_eater, 0, 0);
        let actions = TimeEater::on_roll_move(0, &time_eater, 0, &plan);

        assert_eq!(plan.move_id, HASTE);
        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::TimeEater {
                    used_haste: Some(true),
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }

    #[test]
    fn imported_used_haste_true_with_empty_history_does_not_force_haste() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.max_hp = 456;
        time_eater.current_hp = 200;
        time_eater.time_eater.used_haste = true;
        time_eater.move_history_mut().clear();

        let plan = TimeEater::roll_move_plan(&mut StsRng::new(0), &time_eater, 0, 0);

        assert_eq!(
            plan.move_id, REVERBERATE,
            "Java gates Haste on private usedHaste, not move history"
        );
    }

    #[test]
    fn reverberate_blocked_by_last_two_moves_consumes_java_range_reroll() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.time_eater.used_haste = true;
        time_eater
            .move_history_mut()
            .extend([REVERBERATE, REVERBERATE]);
        let mut rng = StsRng::new(0);

        let plan = TimeEater::roll_move_plan(&mut rng, &time_eater, 0, 0);

        assert_ne!(plan.move_id, REVERBERATE);
        assert_eq!(
            rng.counter, 1,
            "Java TimeEater.getMove rerolls with aiRng.random(50, 99) when Reverberate is blocked by lastTwoMoves"
        );
    }

    #[test]
    fn head_slam_repeat_branch_consumes_java_boolean_chance() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.time_eater.used_haste = true;
        time_eater.move_history_mut().push_back(HEAD_SLAM);
        let mut rng = StsRng::new(0);

        let plan = TimeEater::roll_move_plan(&mut rng, &time_eater, 0, 50);

        assert_ne!(plan.move_id, HEAD_SLAM);
        assert_eq!(
            rng.counter, 1,
            "Java TimeEater.getMove consumes aiRng.randomBoolean(0.66f) when Head Slam is blocked by lastMove"
        );
    }

    #[test]
    fn ripple_repeat_branch_consumes_java_random_74_reroll() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.time_eater.used_haste = true;
        time_eater.move_history_mut().push_back(RIPPLE);
        let mut rng = StsRng::new(0);

        let plan = TimeEater::roll_move_plan(&mut rng, &time_eater, 0, 99);

        assert_eq!(
            rng.counter, 1,
            "Java TimeEater.getMove rerolls with aiRng.random(74) when Ripple is blocked by lastMove"
        );
        assert!(matches!(
            plan.move_id,
            REVERBERATE | RIPPLE | HEAD_SLAM | HASTE
        ));
    }

    #[test]
    fn ripple_a19_queues_block_vulnerable_weak_frail_then_roll_like_java() {
        let time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        let mut state = crate::test_support::combat_with_monsters(vec![time_eater.clone()]);

        let actions = TimeEater::take_turn_plan(&mut state, &time_eater, &ripple_plan(19));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::GainBlock {
                    target: 1,
                    amount: 20,
                },
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Vulnerable,
                    amount: 1,
                },
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Weak,
                    amount: 1,
                },
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Frail,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }

    #[test]
    fn head_slam_a19_queues_damage_draw_reduction_slimed_then_roll_like_java() {
        let time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        let mut state = crate::test_support::combat_with_monsters(vec![time_eater.clone()]);

        let actions = TimeEater::take_turn_plan(&mut state, &time_eater, &head_slam_plan(19));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 32,
                    ..
                },
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::DrawReduction,
                    amount: 1,
                },
                Action::MakeTempCardInDiscard {
                    card_id: CardId::Slimed,
                    amount: 2,
                    upgraded: false,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }

    #[test]
    fn haste_a19_queues_cleanse_shackled_heal_block_then_roll_like_java() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.max_hp = 480;
        time_eater.current_hp = 100;
        let mut state = crate::test_support::combat_with_monsters(vec![time_eater.clone()]);

        let actions =
            TimeEater::take_turn_plan(&mut state, &time_eater, &haste_plan(&time_eater, 19));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::RemoveAllDebuffs { target: 1 },
                Action::RemovePower {
                    target: 1,
                    power_id: PowerId::Shackled,
                },
                Action::Heal {
                    target: 1,
                    amount: 140,
                },
                Action::GainBlock {
                    target: 1,
                    amount: 32,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }

    #[test]
    fn haste_execution_restores_a0_time_eater_to_half_health_like_java() {
        let mut time_eater = crate::test_support::test_monster(EnemyId::TimeEater);
        time_eater.id = 1;
        time_eater.max_hp = 456;
        time_eater.current_hp = 105;
        let mut state = crate::test_support::combat_with_monsters(vec![time_eater.clone()]);
        let actions =
            TimeEater::take_turn_plan(&mut state, &time_eater, &haste_plan(&time_eater, 0));

        for action in actions {
            if matches!(action, Action::RollMonsterMove { .. }) {
                break;
            }
            crate::engine::action_handlers::execute_action(action, &mut state);
        }

        let restored = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == time_eater.id)
            .expect("time eater remains in combat");
        assert_eq!(restored.current_hp, 228);
        assert_eq!(restored.block, 0, "A0 Haste does not grant A19 block");
    }

    #[test]
    fn haste_cleanses_temporary_strength_without_returning_it_at_end_of_turn() {
        use crate::engine::core::tick_until_stable_turn;
        use crate::runtime::combat::{Power, PowerPayload};
        use crate::state::core::{ClientInput, EngineState};

        // GainStrengthPower (Shackled) returns Strength at monster end of turn,
        // not on removal. Haste must remove it before that hook is collected.
        for ascension in [0, 19] {
            for strength in [-6, 2] {
                let mut monster = crate::test_support::test_monster(EnemyId::TimeEater);
                monster.max_hp = if ascension == 0 { 456 } else { 480 };
                monster.current_hp = 100;
                monster.time_eater.used_haste = true;
                let plan = haste_plan(&monster, ascension);
                monster.set_planned_move_id(plan.move_id);
                monster.set_planned_steps(plan.steps);
                monster.set_planned_visible_spec(plan.visible_spec);
                monster.move_history_mut().push_back(HASTE);
                let mut state = crate::test_support::combat_with_monsters(vec![monster]);
                state.meta.ascension_level = ascension;
                state.entities.power_db.insert(
                    1,
                    [
                        (PowerId::TimeWarp, 4),
                        (PowerId::Strength, strength),
                        (PowerId::Shackled, 6),
                        (PowerId::Vulnerable, 4),
                        (PowerId::Weak, 1),
                    ]
                    .into_iter()
                    .map(|(power_type, amount)| Power {
                        power_type,
                        instance_id: None,
                        amount,
                        extra_data: 0,
                        payload: PowerPayload::None,
                        just_applied: false,
                    })
                    .collect(),
                );
                let mut engine = EngineState::CombatPlayerTurn;
                assert!(tick_until_stable_turn(
                    &mut engine,
                    &mut state,
                    ClientInput::EndTurn
                ));
                assert_eq!(engine, EngineState::CombatPlayerTurn);
                let monster = &state.entities.monsters[0];
                assert_eq!(monster.current_hp, monster.max_hp / 2);
                assert_eq!(monster.block, if ascension >= 19 { 32 } else { 0 });
                assert_ne!(monster.planned_move_id(), HASTE);
                assert!(monster.time_eater.used_haste);
                assert_eq!(state.get_power(1, PowerId::TimeWarp), 4);
                assert_eq!(state.get_power(1, PowerId::Strength), strength.max(0));
                for removed in [PowerId::Shackled, PowerId::Vulnerable, PowerId::Weak] {
                    assert!(!state.entities.power_db[&1]
                        .iter()
                        .any(|p| p.power_type == removed));
                }
            }
        }
    }
}

fn reverberate_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 4 {
        8
    } else {
        7
    }
}

fn head_slam_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 4 {
        32
    } else {
        26
    }
}

fn haste_heal_amount(entity: &MonsterEntity) -> i32 {
    (entity.max_hp / 2 - entity.current_hp).max(0)
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    let mut history = entity.move_history().iter().rev();
    matches!(
        (history.next().copied(), history.next().copied()),
        (Some(a), Some(b)) if a == move_id && b == move_id
    )
}

fn runtime(entity: &MonsterEntity) -> &TimeEaterRuntimeState {
    assert!(
        entity.time_eater.protocol_seeded,
        "time eater runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.time_eater
}

fn time_eater_runtime_update(entity: &MonsterEntity, used_haste: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::TimeEater {
            used_haste,
            protocol_seeded: Some(true),
        },
    }
}

fn reverberate_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        REVERBERATE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: reverberate_damage(ascension_level),
            hits: 3,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn ripple_plan(ascension_level: u8) -> MonsterTurnPlan {
    let mut steps = smallvec![
        MoveStep::GainBlock(BlockStep {
            target: MoveTarget::SelfTarget,
            amount: 20,
        }),
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Vulnerable,
            amount: 1,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }),
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Weak,
            amount: 1,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }),
    ];
    if ascension_level >= 19 {
        steps.push(MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Frail,
            amount: 1,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }));
    }
    MonsterTurnPlan::with_visible_spec(
        RIPPLE,
        steps,
        MonsterMoveSpec::DefendDebuff(
            DefendSpec { block: 20 },
            DebuffSpec {
                power_id: PowerId::Vulnerable,
                amount: 1,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn head_slam_plan(ascension_level: u8) -> MonsterTurnPlan {
    let mut steps = smallvec![
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: head_slam_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::DrawReduction,
            amount: 1,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }),
    ];
    if ascension_level >= 19 {
        steps.push(MoveStep::AddCard(AddCardStep {
            card_id: CardId::Slimed,
            amount: 2,
            upgraded: false,
            destination: crate::semantics::combat::CardDestination::Discard,
            visible_strength: EffectStrength::Normal,
        }));
    }
    MonsterTurnPlan::with_visible_spec(
        HEAD_SLAM,
        steps,
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: head_slam_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::DrawReduction,
                amount: 1,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn haste_plan(_entity: &MonsterEntity, ascension_level: u8) -> MonsterTurnPlan {
    let mut steps = smallvec![
        MoveStep::Utility(UtilityStep::RemoveAllDebuffs {
            target: MoveTarget::SelfTarget,
        }),
        MoveStep::RemovePower(RemovePowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Shackled,
        }),
    ];
    steps.push(MoveStep::Heal(crate::semantics::combat::HealStep {
        target: MoveTarget::SelfTarget,
        amount: 0,
    }));
    if ascension_level >= 19 {
        steps.push(MoveStep::GainBlock(BlockStep {
            target: MoveTarget::SelfTarget,
            amount: head_slam_damage(ascension_level),
        }));
    }
    MonsterTurnPlan::with_visible_spec(
        HASTE,
        steps,
        // getMove publishes BUFF; the heal and cleanse belong to execution.
        MonsterMoveSpec::BuffIntent,
    )
}

fn plan_for(entity: &MonsterEntity, ascension_level: u8, move_id: u8) -> MonsterTurnPlan {
    match move_id {
        REVERBERATE => reverberate_plan(ascension_level),
        RIPPLE => ripple_plan(ascension_level),
        HEAD_SLAM => head_slam_plan(ascension_level),
        HASTE => haste_plan(entity, ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn recursive_plan(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    ascension_level: u8,
    num: i32,
) -> MonsterTurnPlan {
    if num < 45 {
        if !last_two_moves(entity, REVERBERATE) {
            return reverberate_plan(ascension_level);
        }
        let reroll = rng.random_range(50, 99);
        return recursive_plan(rng, entity, ascension_level, reroll);
    }
    if num < 80 {
        if !last_move(entity, HEAD_SLAM) {
            return head_slam_plan(ascension_level);
        }
        if rng.random_boolean_chance(0.66) {
            return reverberate_plan(ascension_level);
        }
        return ripple_plan(ascension_level);
    }
    if !last_move(entity, RIPPLE) {
        return ripple_plan(ascension_level);
    }
    let reroll = rng.random(74);
    recursive_plan(rng, entity, ascension_level, reroll)
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("time_eater.reverberate"), reverberate_plan(asc)),
            (Text::new("time_eater.ripple"), ripple_plan(asc)),
            (Text::new("time_eater.headSlam"), head_slam_plan(asc)),
        ],
        special_moves: vec![(
            Text::new("time_eater.haste"),
            (if asc >= 19 {
                Text::new("time_eater.hasteHigh")
            } else {
                Text::new("time_eater.hasteLow")
            }),
        )],
        notes: Text::new("time_eater.notes"),
    }
}

impl MonsterBehavior for TimeEater {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::TimeWarp,
            amount: 0,
        }]
    }

    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        if entity.current_hp < entity.max_hp / 2 && !runtime(entity).used_haste {
            return haste_plan(entity, ascension_level);
        }
        recursive_plan(rng, entity, ascension_level, num)
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if plan.move_id == HASTE && !runtime(entity).used_haste {
            vec![time_eater_runtime_update(entity, Some(true))]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity, state.meta.ascension_level, entity.planned_move_id())
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (
                REVERBERATE,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                })],
            ) => attack_actions(entity.id, PLAYER, attack),
            (RIPPLE, steps) => {
                let mut actions = Vec::new();
                for step in steps {
                    match step {
                        MoveStep::GainBlock(
                            block @ BlockStep {
                                target: MoveTarget::SelfTarget,
                                ..
                            },
                        ) => actions.push(gain_block_action(entity, block)),
                        MoveStep::ApplyPower(
                            power @ ApplyPowerStep {
                                target: MoveTarget::Player,
                                effect: PowerEffectKind::Debuff,
                                ..
                            },
                        ) => actions.push(apply_power_action(entity, power)),
                        _ => panic!("time eater ripple step mismatch: {:?}", steps),
                    }
                }
                actions
            }
            (
                HEAD_SLAM,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                }), tail @ ..],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                for step in tail {
                    match step {
                        MoveStep::ApplyPower(
                            power @ ApplyPowerStep {
                                target: MoveTarget::Player,
                                power_id: PowerId::DrawReduction,
                                effect: PowerEffectKind::Debuff,
                                ..
                            },
                        ) => actions.push(apply_power_action(entity, power)),
                        MoveStep::AddCard(add_card) => actions.push(add_card_action(add_card)),
                        _ => panic!("time eater head slam step mismatch: {:?}", plan.steps),
                    }
                }
                actions
            }
            (HASTE, steps) => {
                let mut actions = Vec::new();
                for step in steps {
                    match step {
                        MoveStep::Utility(step) => actions.push(utility_action(entity, step)),
                        MoveStep::RemovePower(
                            step @ RemovePowerStep {
                                target: MoveTarget::SelfTarget,
                                power_id: PowerId::Shackled,
                            },
                        ) => actions.push(remove_power_action(entity, step)),
                        MoveStep::Heal(crate::semantics::combat::HealStep {
                            target: MoveTarget::SelfTarget,
                            ..
                        }) => {
                            let amount = haste_heal_amount(entity);
                            if amount > 0 {
                                actions.push(Action::Heal {
                                    target: entity.id,
                                    amount,
                                });
                            }
                        }
                        MoveStep::GainBlock(
                            block @ BlockStep {
                                target: MoveTarget::SelfTarget,
                                ..
                            },
                        ) => actions.push(gain_block_action(entity, block)),
                        _ => panic!("time eater haste step mismatch: {:?}", steps),
                    }
                }
                actions
            }
            (_, []) => panic!("time eater plan missing locked truth"),
            (move_id, steps) => panic!("time eater plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
