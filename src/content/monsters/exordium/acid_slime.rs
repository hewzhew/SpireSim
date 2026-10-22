use crate::content::cards::CardId;
use crate::content::monsters::exordium::{
    add_card_action, apply_power_action, attack_actions, set_next_move_action, spawn_action, PLAYER,
};
use crate::content::monsters::{EnemyId, MonsterBehavior};
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, ApplyPowerStep, AttackSpec, AttackStep, CardDestination, DamageKind,
    EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
    SpawnHpSpec, SpawnHpValue, SpawnMonsterStep,
};

const WOUND_TACKLE: u8 = 1;
const NORMAL_TACKLE: u8 = 2;
const SPLIT: u8 = 3;
const WEAK_LICK: u8 = 4;

pub struct AcidSlimeL;
pub struct AcidSlimeM;
pub struct AcidSlimeS;

fn large_wound_damage(asc: u8) -> i32 {
    if asc >= 2 {
        12
    } else {
        11
    }
}

fn large_normal_damage(asc: u8) -> i32 {
    if asc >= 2 {
        18
    } else {
        16
    }
}

fn medium_wound_damage(asc: u8) -> i32 {
    if asc >= 2 {
        8
    } else {
        7
    }
}

fn medium_normal_damage(asc: u8) -> i32 {
    if asc >= 2 {
        12
    } else {
        10
    }
}

fn small_tackle_damage(asc: u8) -> i32 {
    if asc >= 2 {
        4
    } else {
        3
    }
}

fn weak_plan(move_id: u8, amount: i32) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        move_id,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Weak,
            amount,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn wound_plan(move_id: u8, damage: i32, slimed_count: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        move_id,
        smallvec::smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: damage,
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Slimed,
                amount: slimed_count,
                upgraded: false,
                destination: CardDestination::Discard,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::AttackAddCard(
            AttackSpec {
                base_damage: damage,
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            AddCardStep {
                card_id: CardId::Slimed,
                amount: slimed_count,
                upgraded: false,
                destination: CardDestination::Discard,
                visible_strength: EffectStrength::Normal,
            },
        ),
    )
}

fn normal_attack_plan(move_id: u8, damage: i32) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        move_id,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: damage,
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

pub(crate) fn split_plan(child_id: EnemyId) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        SPLIT,
        smallvec::smallvec![
            MoveStep::Suicide,
            MoveStep::SpawnMonster(SpawnMonsterStep {
                enemy_id: child_id,
                logical_position_offset: -1,
                protocol_draw_x_offset: Some(-134),
                hp: SpawnHpSpec {
                    current: SpawnHpValue::SourceCurrentHp,
                    max: SpawnHpValue::SourceCurrentHp,
                },
                is_minion: false,
            }),
            MoveStep::SpawnMonster(SpawnMonsterStep {
                enemy_id: child_id,
                logical_position_offset: 1,
                protocol_draw_x_offset: Some(134),
                hp: SpawnHpSpec {
                    current: SpawnHpValue::SourceCurrentHp,
                    max: SpawnHpValue::SourceCurrentHp,
                },
                is_minion: false,
            }),
        ],
        MonsterMoveSpec::Unknown,
    )
}

fn large_plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        WOUND_TACKLE => wound_plan(WOUND_TACKLE, large_wound_damage(asc), 2),
        NORMAL_TACKLE => normal_attack_plan(NORMAL_TACKLE, large_normal_damage(asc)),
        SPLIT => split_plan(EnemyId::AcidSlimeM),
        WEAK_LICK => weak_plan(WEAK_LICK, 2),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(enemy: EnemyId, asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    let (moves, notes) = match enemy {
        EnemyId::AcidSlimeL => (
            vec![
                (
                    Text::new("acid_slime.corrosiveSpit"),
                    large_plan_for(WOUND_TACKLE, asc),
                ),
                (
                    Text::new("acid_slime.tackle"),
                    large_plan_for(NORMAL_TACKLE, asc),
                ),
                (Text::new("acid_slime.lick"), large_plan_for(WEAK_LICK, asc)),
                (
                    Text::new("acid_slime.split"),
                    split_plan(EnemyId::AcidSlimeM),
                ),
            ],
            Text::new("acid_slime.notesLarge"),
        ),
        EnemyId::AcidSlimeM => (
            vec![
                (
                    Text::new("acid_slime.corrosiveSpit"),
                    medium_plan_for(WOUND_TACKLE, asc),
                ),
                (
                    Text::new("acid_slime.tackle"),
                    medium_plan_for(NORMAL_TACKLE, asc),
                ),
                (
                    Text::new("acid_slime.lick"),
                    medium_plan_for(WEAK_LICK, asc),
                ),
            ],
            Text::new("acid_slime.notesMedium"),
        ),
        EnemyId::AcidSlimeS => (
            vec![
                (Text::new("acid_slime.tackle"), small_tackle_plan(asc)),
                (Text::new("acid_slime.lick"), small_debuff_plan()),
            ],
            Text::new("acid_slime.notesSmall"),
        ),
        _ => unreachable!("not an Acid Slime"),
    };
    super::super::mechanics::MonsterMechanics {
        moves,
        special_moves: vec![],
        notes,
    }
}

fn medium_plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        WOUND_TACKLE => wound_plan(WOUND_TACKLE, medium_wound_damage(asc), 1),
        NORMAL_TACKLE => normal_attack_plan(NORMAL_TACKLE, medium_normal_damage(asc)),
        WEAK_LICK => weak_plan(WEAK_LICK, 1),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn small_tackle_plan(asc: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        WOUND_TACKLE,
        MoveStep::Attack(AttackStep {
            target: MoveTarget::Player,
            attack: AttackSpec {
                base_damage: small_tackle_damage(asc),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
        }),
    )
}

fn small_debuff_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::single(
        NORMAL_TACKLE,
        MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::Player,
            power_id: PowerId::Weak,
            amount: 1,
            effect: PowerEffectKind::Debuff,
            visible_strength: EffectStrength::Normal,
        }),
    )
}

fn small_plan_for(move_id: u8, asc: u8) -> MonsterTurnPlan {
    match move_id {
        WOUND_TACKLE => small_tackle_plan(asc),
        NORMAL_TACKLE => small_debuff_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn large_roll_plan(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    asc: u8,
    num: i32,
) -> MonsterTurnPlan {
    if num < 0 {
        panic!("acid slime L roll num invalid");
    }
    if asc >= 17 {
        if num < 40 {
            if last_two_moves(entity, WOUND_TACKLE) {
                if roll_chance(rng, 60) {
                    normal_attack_plan(NORMAL_TACKLE, large_normal_damage(asc))
                } else {
                    weak_plan(WEAK_LICK, 2)
                }
            } else {
                wound_plan(WOUND_TACKLE, large_wound_damage(asc), 2)
            }
        } else if num < 70 {
            if last_two_moves(entity, NORMAL_TACKLE) {
                if roll_chance(rng, 60) {
                    wound_plan(WOUND_TACKLE, large_wound_damage(asc), 2)
                } else {
                    weak_plan(WEAK_LICK, 2)
                }
            } else {
                normal_attack_plan(NORMAL_TACKLE, large_normal_damage(asc))
            }
        } else if last_move(entity, WEAK_LICK) {
            if roll_chance(rng, 40) {
                wound_plan(WOUND_TACKLE, large_wound_damage(asc), 2)
            } else {
                normal_attack_plan(NORMAL_TACKLE, large_normal_damage(asc))
            }
        } else {
            weak_plan(WEAK_LICK, 2)
        }
    } else if num < 30 {
        if last_two_moves(entity, WOUND_TACKLE) {
            if rng.random_boolean() {
                normal_attack_plan(NORMAL_TACKLE, large_normal_damage(asc))
            } else {
                weak_plan(WEAK_LICK, 2)
            }
        } else {
            wound_plan(WOUND_TACKLE, large_wound_damage(asc), 2)
        }
    } else if num < 70 {
        if last_move(entity, NORMAL_TACKLE) {
            if roll_chance(rng, 40) {
                wound_plan(WOUND_TACKLE, large_wound_damage(asc), 2)
            } else {
                weak_plan(WEAK_LICK, 2)
            }
        } else {
            normal_attack_plan(NORMAL_TACKLE, large_normal_damage(asc))
        }
    } else if last_two_moves(entity, WEAK_LICK) {
        if roll_chance(rng, 40) {
            wound_plan(WOUND_TACKLE, large_wound_damage(asc), 2)
        } else {
            normal_attack_plan(NORMAL_TACKLE, large_normal_damage(asc))
        }
    } else {
        weak_plan(WEAK_LICK, 2)
    }
}

fn medium_roll_plan(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    asc: u8,
    num: i32,
) -> MonsterTurnPlan {
    if asc >= 17 {
        if num < 40 {
            if last_two_moves(entity, WOUND_TACKLE) {
                if rng.random_boolean() {
                    normal_attack_plan(NORMAL_TACKLE, medium_normal_damage(asc))
                } else {
                    weak_plan(WEAK_LICK, 1)
                }
            } else {
                wound_plan(WOUND_TACKLE, medium_wound_damage(asc), 1)
            }
        } else if num < 80 {
            if last_two_moves(entity, NORMAL_TACKLE) {
                if roll_chance(rng, 50) {
                    wound_plan(WOUND_TACKLE, medium_wound_damage(asc), 1)
                } else {
                    weak_plan(WEAK_LICK, 1)
                }
            } else {
                normal_attack_plan(NORMAL_TACKLE, medium_normal_damage(asc))
            }
        } else if last_move(entity, WEAK_LICK) {
            if roll_chance(rng, 40) {
                wound_plan(WOUND_TACKLE, medium_wound_damage(asc), 1)
            } else {
                normal_attack_plan(NORMAL_TACKLE, medium_normal_damage(asc))
            }
        } else {
            weak_plan(WEAK_LICK, 1)
        }
    } else if num < 30 {
        if last_two_moves(entity, WOUND_TACKLE) {
            if rng.random_boolean() {
                normal_attack_plan(NORMAL_TACKLE, medium_normal_damage(asc))
            } else {
                weak_plan(WEAK_LICK, 1)
            }
        } else {
            wound_plan(WOUND_TACKLE, medium_wound_damage(asc), 1)
        }
    } else if num < 70 {
        if last_move(entity, NORMAL_TACKLE) {
            if roll_chance(rng, 40) {
                wound_plan(WOUND_TACKLE, medium_wound_damage(asc), 1)
            } else {
                weak_plan(WEAK_LICK, 1)
            }
        } else {
            normal_attack_plan(NORMAL_TACKLE, medium_normal_damage(asc))
        }
    } else if last_two_moves(entity, WEAK_LICK) {
        if roll_chance(rng, 40) {
            wound_plan(WOUND_TACKLE, medium_wound_damage(asc), 1)
        } else {
            normal_attack_plan(NORMAL_TACKLE, medium_normal_damage(asc))
        }
    } else {
        weak_plan(WEAK_LICK, 1)
    }
}

fn small_roll_plan(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    asc: u8,
) -> MonsterTurnPlan {
    if asc >= 17 {
        if last_two_moves(entity, WOUND_TACKLE) {
            small_tackle_plan(asc)
        } else {
            small_debuff_plan()
        }
    } else if rng.random_boolean() {
        small_tackle_plan(asc)
    } else {
        small_debuff_plan()
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn last_two_moves(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().len() >= 2
        && entity.move_history()[entity.move_history().len() - 1] == move_id
        && entity.move_history()[entity.move_history().len() - 2] == move_id
}

fn roll_chance(rng: &mut crate::runtime::rng::StsRng, percent: i32) -> bool {
    rng.random_boolean_chance(percent as f32 / 100.0)
}

fn execute_steps(
    state: &CombatState,
    entity: &MonsterEntity,
    plan: &MonsterTurnPlan,
) -> Vec<Action> {
    let mut actions = Vec::new();
    for step in &plan.steps {
        match step {
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            }) => actions.extend(attack_actions(entity.id, PLAYER, attack)),
            MoveStep::ApplyPower(apply_power)
                if apply_power.target == MoveTarget::Player
                    && apply_power.power_id == PowerId::Weak
                    && apply_power.effect == PowerEffectKind::Debuff =>
            {
                actions.push(apply_power_action(entity, apply_power))
            }
            MoveStep::AddCard(add_card) => actions.push(add_card_action(add_card)),
            MoveStep::Suicide => actions.push(Action::Suicide {
                target: entity.id,
                trigger_relics: false,
            }),
            MoveStep::SpawnMonster(step) => actions.push(spawn_action(state, entity, step)),
            other => panic!("acid slime step unsupported in execution: {:?}", other),
        }
    }
    actions
}

impl MonsterBehavior for AcidSlimeL {
    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        large_roll_plan(rng, entity, ascension_level, num)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        large_plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_hp_rng, _ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![Action::ApplyPower {
            source: entity.id,
            target: entity.id,
            power_id: PowerId::Split,
            amount: -1,
        }]
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = execute_steps(state, entity, plan);
        if plan.move_id == SPLIT {
            // Java takeTurn calls setMove(Split) again before its queued
            // Suicide/Spawn actions execute, preserving a repeated history entry.
            actions.insert(0, set_next_move_action(entity, plan.clone()));
        } else {
            actions.push(Action::RollMonsterMove {
                monster_id: entity.id,
            });
        }
        actions
    }
}

impl MonsterBehavior for AcidSlimeM {
    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        medium_roll_plan(rng, entity, ascension_level, num)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        medium_plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = execute_steps(state, entity, plan);
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

impl MonsterBehavior for AcidSlimeS {
    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        small_roll_plan(rng, entity, ascension_level)
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        small_plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match plan.steps.as_slice() {
            [MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack,
            })] => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(set_next_move_action(entity, small_debuff_plan()));
                actions
            }
            [MoveStep::ApplyPower(apply_power)]
                if apply_power.target == MoveTarget::Player
                    && apply_power.power_id == PowerId::Weak
                    && apply_power.amount == 1
                    && apply_power.effect == PowerEffectKind::Debuff =>
            {
                vec![
                    apply_power_action(entity, apply_power),
                    set_next_move_action(entity, small_tackle_plan(state.meta.ascension_level)),
                ]
            }
            [] => panic!("acid slime S plan missing locked truth"),
            steps => panic!("acid slime S plan/steps mismatch: {:?}", steps),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        roll_chance, AcidSlimeL, AcidSlimeM, AcidSlimeS, NORMAL_TACKLE, WEAK_LICK, WOUND_TACKLE,
    };
    use crate::content::cards::CardId;
    use crate::content::monsters::{EnemyId, MonsterBehavior, PreBattleLegacyRng};
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;
    use crate::runtime::rng::StsRng;

    #[test]
    fn percent_roll_uses_java_random_boolean_chance_not_integer_roll() {
        for seed in 0..128 {
            for percent in [40, 50, 60] {
                let mut actual = StsRng::new(seed);
                let mut expected = actual.clone();

                let actual_result = roll_chance(&mut actual, percent);
                let expected_result = expected.random_boolean_chance(percent as f32 / 100.0);

                assert_eq!(
                    actual_result, expected_result,
                    "seed {seed} percent {percent} should follow Java randomBoolean(float)"
                );
                assert_eq!(
                    actual, expected,
                    "seed {seed} percent {percent} should leave identical RNG state"
                );
            }
        }
    }

    #[test]
    fn successive_boss_splits_keep_dead_siblings_before_the_acid_children() {
        use crate::engine::action_handlers::execute_action;

        fn split<T: MonsterBehavior>(
            state: &mut crate::runtime::combat::CombatState,
            id: usize,
            hp: i32,
        ) {
            let parent = state
                .entities
                .monsters
                .iter_mut()
                .find(|monster| monster.id == id)
                .unwrap();
            parent.current_hp = hp;
            parent.set_planned_move_id(super::SPLIT);
            let parent = parent.clone();
            let plan = T::turn_plan(state, &parent);
            for action in T::take_turn_plan(state, &parent, &plan) {
                state.queue_action_back(action);
            }
            while let Some(action) = state.pop_next_action() {
                execute_action(action, state);
            }
        }

        let mut boss = crate::test_support::test_monster(EnemyId::SlimeBoss);
        boss.max_hp = 150;
        let mut state = crate::test_support::combat_with_monsters(vec![boss]);
        state.meta.ascension_level = 20;
        let hp_rng = state.rng.monster_hp_rng.clone();
        split::<crate::content::monsters::exordium::slime_boss::SlimeBoss>(&mut state, 1, 57);
        split::<crate::content::monsters::exordium::spike_slime::SpikeSlimeL>(&mut state, 2, 21);
        let dead_sibling = state
            .entities
            .monsters
            .iter_mut()
            .find(|m| m.id == 5)
            .unwrap();
        dead_sibling.current_hp = 0;
        dead_sibling.is_dying = true;
        split::<AcidSlimeL>(&mut state, 3, 14);

        // Java SpawnMonsterAction counts every smaller drawX, including dead
        // parents/siblings. The Acid child at 120 - 134 lies AFTER the Spike
        // child at -385 + 134; coarse logical offsets make both positions zero.
        assert_eq!(
            state
                .entities
                .monsters
                .iter()
                .map(|m| (m.id, m.enemy_id, m.current_hp))
                .collect::<Vec<_>>(),
            vec![
                (4, EnemyId::SpikeSlimeM, 21),
                (2, EnemyId::SpikeSlimeL, 0),
                (5, EnemyId::SpikeSlimeM, 0),
                (6, EnemyId::AcidSlimeM, 14),
                (1, EnemyId::SlimeBoss, 0),
                (3, EnemyId::AcidSlimeL, 0),
                (7, EnemyId::AcidSlimeM, 14),
            ]
        );
        assert_eq!(state.rng.monster_hp_rng, hp_rng);
        for (index, monster) in state.entities.monsters.iter().enumerate() {
            assert_eq!(monster.slot as usize, index);
            assert_eq!(
                state
                    .monster_protocol_identity(monster.id)
                    .unwrap()
                    .group_index,
                Some(index)
            );
        }
    }

    #[test]
    fn split_turn_reasserts_the_parent_move_before_suicide_without_consuming_rng() {
        assert_split_reassertion::<AcidSlimeL>(EnemyId::AcidSlimeL);
        assert_split_reassertion::<crate::content::monsters::exordium::spike_slime::SpikeSlimeL>(
            EnemyId::SpikeSlimeL,
        );
        assert_split_reassertion::<crate::content::monsters::exordium::slime_boss::SlimeBoss>(
            EnemyId::SlimeBoss,
        );
    }

    fn assert_split_reassertion<T: MonsterBehavior>(enemy_id: EnemyId) {
        let mut parent = crate::test_support::test_monster(enemy_id);
        parent.current_hp = 21;
        parent.max_hp = 68;
        parent.set_planned_move_id(super::SPLIT);
        parent.move_history_mut().clear();
        parent.move_history_mut().extend([1, 2, 3, 3]);
        let mut state = crate::test_support::combat_with_monsters(vec![parent.clone()]);
        let ai_before = state.rng.ai_rng.clone();
        let plan = T::turn_plan(&state, &parent);
        let actions = T::take_turn_plan(&mut state, &parent, &plan);
        crate::engine::action_handlers::execute_action(actions[0].clone(), &mut state);
        let current = &state.entities.monsters[0];
        assert_eq!(
            current.move_history().iter().copied().collect::<Vec<_>>(),
            [1, 2, 3, 3, 3]
        );
        assert_eq!(current.planned_move_id(), super::SPLIT);
        assert_eq!(current.current_hp, 21);
        assert!(!current.is_dying);
        assert_eq!(state.rng.ai_rng, ai_before);
        assert!(matches!(
            actions[1],
            Action::Suicide {
                trigger_relics: false,
                ..
            }
        ));
        assert_eq!(
            actions
                .iter()
                .any(|action| matches!(action, Action::RollMonsterMove { .. })),
            enemy_id == EnemyId::SpikeSlimeL,
            "Java only SpikeSlime_L queues a subsequent RollMove after splitting"
        );
    }

    #[test]
    fn split_power_prebattle_uses_java_sentinel_amount() {
        for (enemy_id, amount) in [
            (
                EnemyId::AcidSlimeL,
                split_power_amount::<AcidSlimeL>(EnemyId::AcidSlimeL),
            ),
            (
                EnemyId::SpikeSlimeL,
                split_power_amount::<crate::content::monsters::exordium::spike_slime::SpikeSlimeL>(
                    EnemyId::SpikeSlimeL,
                ),
            ),
            (
                EnemyId::SlimeBoss,
                split_power_amount::<crate::content::monsters::exordium::slime_boss::SlimeBoss>(
                    EnemyId::SlimeBoss,
                ),
            ),
        ] {
            assert_eq!(
                amount, -1,
                "{enemy_id:?} SplitPower should match Java SplitPower.amount = -1"
            );
        }
    }

    fn split_power_amount<T: MonsterBehavior>(enemy_id: EnemyId) -> i32 {
        let mut state = crate::test_support::blank_test_combat();
        let monster = crate::test_support::test_monster(enemy_id);
        let actions = T::use_pre_battle_actions(&mut state, &monster, PreBattleLegacyRng::Misc);
        match actions.as_slice() {
            [Action::ApplyPower {
                power_id: PowerId::Split,
                amount,
                ..
            }] => *amount,
            other => panic!("expected one Split ApplyPower action, got {other:?}"),
        }
    }

    #[test]
    fn medium_acid_slime_roll_logic_matches_java_a17_branches_and_rng() {
        let mut slime = crate::test_support::test_monster(EnemyId::AcidSlimeM);
        let mut rng = StsRng::new(101);

        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut rng, &slime, 17, 39).move_id,
            WOUND_TACKLE,
            "Java A17+ num < 40 opens Wound Tackle unless lastTwoMoves(WOUND_TACKLE)"
        );
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut rng, &slime, 17, 79).move_id,
            NORMAL_TACKLE,
            "Java A17+ 40 <= num < 80 opens Normal Tackle unless lastTwoMoves(NORMAL_TACKLE)"
        );
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut rng, &slime, 17, 80).move_id,
            WEAK_LICK,
            "Java A17+ high rolls Weak Lick unless lastMove(WEAK_LICK)"
        );

        slime
            .move_history_mut()
            .extend([WOUND_TACKLE, WOUND_TACKLE]);
        let mut actual = StsRng::new(202);
        let mut expected_rng = actual.clone();
        let expected = if expected_rng.random_boolean() {
            NORMAL_TACKLE
        } else {
            WEAK_LICK
        };
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut actual, &slime, 17, 39).move_id,
            expected,
            "Java A17+ Wound repeat guard consumes aiRng.randomBoolean()"
        );
        assert_eq!(actual, expected_rng);

        slime.move_history_mut().clear();
        slime
            .move_history_mut()
            .extend([NORMAL_TACKLE, NORMAL_TACKLE]);
        let mut actual = StsRng::new(303);
        let mut expected_rng = actual.clone();
        let expected = if expected_rng.random_boolean_chance(0.5) {
            WOUND_TACKLE
        } else {
            WEAK_LICK
        };
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut actual, &slime, 17, 79).move_id,
            expected,
            "Java A17+ Normal repeat guard consumes aiRng.randomBoolean(0.5f)"
        );
        assert_eq!(actual, expected_rng);

        slime.move_history_mut().clear();
        slime.move_history_mut().push_back(WEAK_LICK);
        let mut actual = StsRng::new(404);
        let mut expected_rng = actual.clone();
        let expected = if expected_rng.random_boolean_chance(0.4) {
            WOUND_TACKLE
        } else {
            NORMAL_TACKLE
        };
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut actual, &slime, 17, 80).move_id,
            expected,
            "Java A17+ Weak repeat guard consumes aiRng.randomBoolean(0.4f)"
        );
        assert_eq!(actual, expected_rng);
    }

    #[test]
    fn medium_acid_slime_roll_logic_matches_java_pre_a17_branches_and_rng() {
        let mut slime = crate::test_support::test_monster(EnemyId::AcidSlimeM);
        let mut rng = StsRng::new(111);

        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut rng, &slime, 16, 29).move_id,
            WOUND_TACKLE
        );
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut rng, &slime, 16, 69).move_id,
            NORMAL_TACKLE
        );
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut rng, &slime, 16, 70).move_id,
            WEAK_LICK
        );

        slime
            .move_history_mut()
            .extend([WOUND_TACKLE, WOUND_TACKLE]);
        let mut actual = StsRng::new(222);
        let mut expected_rng = actual.clone();
        let expected = if expected_rng.random_boolean() {
            NORMAL_TACKLE
        } else {
            WEAK_LICK
        };
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut actual, &slime, 16, 29).move_id,
            expected
        );
        assert_eq!(actual, expected_rng);

        slime.move_history_mut().clear();
        slime.move_history_mut().push_back(NORMAL_TACKLE);
        let mut actual = StsRng::new(333);
        let mut expected_rng = actual.clone();
        let expected = if expected_rng.random_boolean_chance(0.4) {
            WOUND_TACKLE
        } else {
            WEAK_LICK
        };
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut actual, &slime, 16, 69).move_id,
            expected
        );
        assert_eq!(actual, expected_rng);

        slime.move_history_mut().clear();
        slime.move_history_mut().extend([WEAK_LICK, WEAK_LICK]);
        let mut actual = StsRng::new(444);
        let mut expected_rng = actual.clone();
        let expected = if expected_rng.random_boolean_chance(0.4) {
            WOUND_TACKLE
        } else {
            NORMAL_TACKLE
        };
        assert_eq!(
            AcidSlimeM::roll_move_plan(&mut actual, &slime, 16, 70).move_id,
            expected
        );
        assert_eq!(actual, expected_rng);
    }

    #[test]
    fn medium_acid_slime_take_turn_actions_match_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 2;
        let entity = crate::test_support::test_monster(EnemyId::AcidSlimeM);

        let wound = AcidSlimeM::take_turn_plan(
            &mut state,
            &entity,
            &super::medium_plan_for(WOUND_TACKLE, 2),
        );
        assert!(matches!(
            wound.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 8,
                    ..
                },
                Action::MakeTempCardInDiscard {
                    card_id: CardId::Slimed,
                    amount: 1,
                    upgraded: false,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));

        let normal = AcidSlimeM::take_turn_plan(
            &mut state,
            &entity,
            &super::medium_plan_for(NORMAL_TACKLE, 2),
        );
        assert!(matches!(
            normal.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 12,
                    ..
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));

        let weak =
            AcidSlimeM::take_turn_plan(&mut state, &entity, &super::medium_plan_for(WEAK_LICK, 2));
        assert!(matches!(
            weak.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: 0,
                    power_id: PowerId::Weak,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        ));
    }

    #[test]
    fn small_acid_slime_roll_logic_matches_java_a17_and_random_boolean() {
        let mut slime = crate::test_support::test_monster(EnemyId::AcidSlimeS);
        let mut rng = StsRng::new(12);

        assert_eq!(
            AcidSlimeS::roll_move_plan(&mut rng, &slime, 17, 99).move_id,
            NORMAL_TACKLE,
            "Java A17+ Small Acid Slime opens with Debuff unless lastTwoMoves(Tackle)"
        );

        slime
            .move_history_mut()
            .extend([WOUND_TACKLE, WOUND_TACKLE]);
        assert_eq!(
            AcidSlimeS::roll_move_plan(&mut rng, &slime, 17, 99).move_id,
            WOUND_TACKLE
        );

        let mut random_actual = StsRng::new(44);
        let mut random_expected = random_actual.clone();
        slime.move_history_mut().clear();
        let expected = if random_expected.random_boolean() {
            WOUND_TACKLE
        } else {
            NORMAL_TACKLE
        };
        assert_eq!(
            AcidSlimeS::roll_move_plan(&mut random_actual, &slime, 16, 0).move_id,
            expected,
            "Below A17 Java uses aiRng.randomBoolean() between Tackle and Debuff"
        );
        assert_eq!(random_actual, random_expected);
    }

    #[test]
    fn small_acid_slime_take_turn_switches_move_without_rollmove_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 2;
        let entity = crate::test_support::test_monster(EnemyId::AcidSlimeS);

        let tackle = AcidSlimeS::take_turn_plan(&mut state, &entity, &super::small_tackle_plan(2));
        assert!(matches!(
            tackle.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: 0,
                    base_damage: 4,
                    ..
                },
                Action::SetMonsterMove {
                    monster_id: 1,
                    next_move_byte: NORMAL_TACKLE,
                    ..
                },
            ]
        ));

        let debuff = AcidSlimeS::take_turn_plan(&mut state, &entity, &super::small_debuff_plan());
        assert!(matches!(
            debuff.as_slice(),
            [
                Action::ApplyPower {
                    source: 1,
                    target: 0,
                    power_id: PowerId::Weak,
                    amount: 1,
                },
                Action::SetMonsterMove {
                    monster_id: 1,
                    next_move_byte: WOUND_TACKLE,
                    ..
                },
            ]
        ));
    }
}
