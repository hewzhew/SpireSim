use crate::content::monsters::exordium::{apply_power_action, attack_actions, PLAYER};
use crate::content::monsters::{EnemyId, MonsterBehavior, MonsterRollContext, PreBattleLegacyRng};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::runtime::rng::StsRng;
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, DamageKind, DebuffSpec, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind, SpawnHpSpec, SpawnHpValue,
};
use smallvec::smallvec;

pub struct Reptomancer;

impl Reptomancer {
    pub const DAGGER_DRAW_X: [i32; 4] = [210, -220, 180, -250];
}

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    if entity.enemy_id != EnemyId::Reptomancer {
        return;
    }
    entity.reptomancer.protocol_seeded = true;
    entity.reptomancer.first_move = true;
    entity.reptomancer.dagger_slots = [None, None, None, None];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_dagger_reuses_escaped_dagger_slot_like_java_is_dead_or_escaped() {
        let reptomancer = crate::test_support::test_monster(EnemyId::Reptomancer);
        let mut escaped_dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        escaped_dagger.id = 2;
        escaped_dagger.logical_position = Reptomancer::DAGGER_DRAW_X[0];
        escaped_dagger.is_escaped = true;
        escaped_dagger.is_dying = false;
        let mut state =
            crate::test_support::combat_with_monsters(vec![reptomancer.clone(), escaped_dagger]);

        let actions = Reptomancer::take_turn_plan(&mut state, &reptomancer, &spawn_dagger_plan());

        assert!(
            actions.iter().any(|action| matches!(
                action,
                Action::SpawnReptomancerDagger {
                    reptomancer_id: 1,
                    slot: 0,
                    logical_position: x,
                    ..
                } if *x == Reptomancer::DAGGER_DRAW_X[0]
            )),
            "Java Reptomancer checks dagger slot availability with isDeadOrEscaped()"
        );
    }

    #[test]
    fn death_cleanup_suicides_zero_hp_or_escaped_non_dying_minions_like_java() {
        let reptomancer = crate::test_support::test_monster(EnemyId::Reptomancer);
        let mut dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        dagger.id = 2;
        dagger.current_hp = 0;
        dagger.is_dying = false;
        dagger.is_escaped = true;
        let mut state =
            crate::test_support::combat_with_monsters(vec![reptomancer.clone(), dagger]);

        let actions = Reptomancer::on_death(&mut state, &reptomancer);

        assert!(matches!(
            actions.as_slice(),
            [Action::Suicide {
                target: 2,
                trigger_relics: true,
            }]
        ));
    }

    #[test]
    fn death_cleanup_suicides_minions_in_java_add_to_top_order() {
        let reptomancer = crate::test_support::test_monster(EnemyId::Reptomancer);
        let mut first_dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        first_dagger.id = 2;
        let mut second_dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        second_dagger.id = 3;
        let mut state = crate::test_support::combat_with_monsters(vec![
            reptomancer.clone(),
            first_dagger,
            second_dagger,
        ]);

        let actions = Reptomancer::on_death(&mut state, &reptomancer);

        assert_eq!(
            actions,
            vec![
                Action::Suicide {
                    target: 3,
                    trigger_relics: true,
                },
                Action::Suicide {
                    target: 2,
                    trigger_relics: true,
                },
            ],
            "Java Reptomancer.die adds cleanup actions with addToTop while iterating forward, so later minions' SuicideAction executes first"
        );
    }

    #[test]
    fn pre_battle_applies_minion_sentinel_amount_like_java() {
        let reptomancer = crate::test_support::test_monster(EnemyId::Reptomancer);
        let mut dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        dagger.id = 2;
        let mut state =
            crate::test_support::combat_with_monsters(vec![dagger, reptomancer.clone()]);

        let actions = Reptomancer::use_pre_battle_actions(
            &mut state,
            &reptomancer,
            PreBattleLegacyRng::MonsterHp,
        );

        assert!(actions.iter().any(|action| matches!(
            action,
            Action::ApplyPower {
                source: 2,
                target: 2,
                power_id: PowerId::Minion,
                amount: -1,
            }
        )));
    }

    #[test]
    fn pre_battle_maps_initial_dagger_slots_by_java_group_index() {
        let mut left_dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        left_dagger.id = 2;
        let mut reptomancer = crate::test_support::test_monster(EnemyId::Reptomancer);
        reptomancer.id = 10;
        let mut right_dagger = crate::test_support::test_monster(EnemyId::SnakeDagger);
        right_dagger.id = 3;
        let mut state = crate::test_support::combat_with_monsters(vec![
            left_dagger,
            reptomancer.clone(),
            right_dagger,
        ]);

        let _ = Reptomancer::use_pre_battle_actions(
            &mut state,
            &reptomancer,
            PreBattleLegacyRng::MonsterHp,
        );

        let stored = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == reptomancer.id)
            .unwrap();
        assert_eq!(
            stored.reptomancer.dagger_slots,
            [Some(3), Some(2), None, None],
            "Java usePreBattleAction stores daggers after Reptomancer in daggers[0] and before it in daggers[1]"
        );
    }

    #[test]
    fn a18_spawn_fills_first_available_java_dagger_slots_before_roll() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 18;
        let mut reptomancer = crate::test_support::test_monster(EnemyId::Reptomancer);
        reptomancer.id = 10;
        reptomancer.reptomancer.protocol_seeded = true;
        reptomancer.reptomancer.first_move = false;
        reptomancer.reptomancer.dagger_slots = [None, None, None, None];
        state.entities.monsters = vec![reptomancer.clone()];

        let trace = begin_sts_rng_trace().unwrap();
        let actions = Reptomancer::take_turn_plan(&mut state, &reptomancer, &spawn_dagger_plan());
        let constructed = trace.finish().unwrap();
        assert_eq!(constructed.len(), 2);
        let hp: Vec<_> = constructed
            .iter()
            .map(|entry| {
                assert_eq!(entry.owner, Some("floor.monster_hp"));
                match entry.call {
                    StsRngCall::RandomIntRange {
                        start: 20,
                        end: 25,
                        result,
                    } => result,
                    ref other => panic!("unexpected dagger constructor RNG: {other:?}"),
                }
            })
            .collect();

        assert_eq!(
            actions,
            vec![
                Action::SpawnReptomancerDagger {
                    reptomancer_id: 10,
                    slot: 0,
                    hp: SpawnHpSpec {
                        current: SpawnHpValue::Fixed(hp[0]),
                        max: SpawnHpValue::Fixed(hp[0]),
                    },
                    logical_position: Reptomancer::DAGGER_DRAW_X[0],
                    protocol_draw_x: Some(Reptomancer::DAGGER_DRAW_X[0]),
                },
                Action::SpawnReptomancerDagger {
                    reptomancer_id: 10,
                    slot: 1,
                    hp: SpawnHpSpec {
                        current: SpawnHpValue::Fixed(hp[1]),
                        max: SpawnHpValue::Fixed(hp[1]),
                    },
                    logical_position: Reptomancer::DAGGER_DRAW_X[1],
                    protocol_draw_x: Some(Reptomancer::DAGGER_DRAW_X[1]),
                },
                Action::RollMonsterMove { monster_id: 10 },
            ],
            "Java loops daggers[0..] and at A18 spawns two daggers before the queued RollMoveAction"
        );
        let hp_counter = state.rng.monster_hp_rng.counter;
        let trace = begin_sts_rng_trace().unwrap();
        for action in actions {
            crate::engine::action_handlers::execute_action(action, &mut state);
        }
        let initialized = trace.finish().unwrap();
        assert_eq!(initialized.len(), 3);
        assert!(initialized
            .iter()
            .all(|entry| entry.owner == Some("floor.ai")));
        assert_eq!(state.rng.monster_hp_rng.counter, hp_counter);
        let slots = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == reptomancer.id)
            .unwrap()
            .reptomancer
            .dagger_slots;
        for slot in 0..2 {
            let dagger = state
                .entities
                .monsters
                .iter()
                .find(|monster| Some(monster.id) == slots[slot])
                .unwrap();
            assert_eq!((dagger.current_hp, dagger.max_hp), (hp[slot], hp[slot]));
        }
    }

    #[test]
    fn can_spawn_counts_zero_hp_and_escaped_non_dying_monsters_like_java() {
        let mut reptomancer = crate::test_support::test_monster(EnemyId::Reptomancer);
        reptomancer.id = 10;
        reptomancer.reptomancer.protocol_seeded = true;
        reptomancer.reptomancer.first_move = false;

        let mut zero_hp = crate::test_support::test_monster(EnemyId::SnakeDagger);
        zero_hp.id = 2;
        zero_hp.current_hp = 0;
        zero_hp.is_dying = false;
        let mut escaped = crate::test_support::test_monster(EnemyId::SnakeDagger);
        escaped.id = 3;
        escaped.is_escaped = true;
        escaped.is_dying = false;
        let mut ordinary = crate::test_support::test_monster(EnemyId::SnakeDagger);
        ordinary.id = 4;
        let mut fourth = crate::test_support::test_monster(EnemyId::SnakeDagger);
        fourth.id = 5;
        let state = crate::test_support::combat_with_monsters(vec![
            reptomancer.clone(),
            zero_hp,
            escaped,
            ordinary,
            fourth,
        ]);

        let plan = roll_move_custom_plan(
            &mut StsRng::new(0),
            &reptomancer,
            0,
            50,
            &state.entities.monsters,
        );

        assert_eq!(
            plan.move_id, SNAKE_STRIKE,
            "Java canSpawn skips only self and isDying, so zero-HP or escaped non-dying monsters still count toward the aliveCount cap"
        );
    }

    #[test]
    fn snake_strike_queues_two_damage_hits_then_weak_then_roll() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 3;
        let mut reptomancer = crate::test_support::test_monster(EnemyId::Reptomancer);
        reptomancer.id = 10;

        let actions = Reptomancer::take_turn_plan(&mut state, &reptomancer, &snake_strike_plan(3));

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack { base_damage, .. },
                Action::MonsterAttack {
                    base_damage: base_damage2,
                    ..
                },
                Action::ApplyPower {
                    source: 10,
                    target: PLAYER,
                    power_id: PowerId::Weak,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 10 },
            ] if *base_damage == 16 && *base_damage2 == 16
        ));
    }
}

const SNAKE_STRIKE: u8 = 1;
const SPAWN_DAGGER: u8 = 2;
const BIG_BITE: u8 = 3;

fn snake_strike_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 3 {
        16
    } else {
        13
    }
}

fn big_bite_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 3 {
        34
    } else {
        30
    }
}

fn daggers_per_spawn(ascension_level: u8) -> usize {
    if ascension_level >= 18 {
        2
    } else {
        1
    }
}

fn snake_strike_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        SNAKE_STRIKE,
        smallvec![
            MoveStep::Attack(crate::semantics::combat::AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: snake_strike_damage(ascension_level),
                    hits: 2,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                amount: 1,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: snake_strike_damage(ascension_level),
                hits: 2,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Weak,
                amount: 1,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn spawn_dagger_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(SPAWN_DAGGER, smallvec![], MonsterMoveSpec::Unknown)
}

fn big_bite_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BIG_BITE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: big_bite_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        SNAKE_STRIKE => snake_strike_plan(ascension_level),
        SPAWN_DAGGER => spawn_dagger_plan(),
        BIG_BITE => big_bite_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(ascension: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            (
                Text::new("reptomancer.snakeStrike"),
                snake_strike_plan(ascension),
            ),
            (Text::new("reptomancer.bigBite"), big_bite_plan(ascension)),
        ],
        special_moves: vec![(
            Text::new("reptomancer.summon"),
            Text::new("reptomancer.summonRule").with("count", daggers_per_spawn(ascension) as i32),
        )],
        notes: Text::new("reptomancer.notes"),
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

fn can_spawn(monsters: &[MonsterEntity], reptomancer_id: usize) -> bool {
    monsters
        .iter()
        .filter(|monster| monster.id != reptomancer_id && !monster.is_dying)
        .count()
        <= 3
}

fn runtime(entity: &MonsterEntity) -> &crate::runtime::combat::ReptomancerRuntimeState {
    assert!(
        entity.reptomancer.protocol_seeded,
        "reptomancer runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.reptomancer
}

fn reptomancer_runtime_update(entity: &MonsterEntity, first_move: Option<bool>) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Reptomancer {
            first_move,
            dagger_slots: None,
            protocol_seeded: Some(true),
        },
    }
}

fn occupied_dagger_slots(state: &CombatState, entity: &MonsterEntity) -> [bool; 4] {
    let mut occupied = [false; 4];
    for (slot, monster_id) in runtime(entity).dagger_slots.iter().enumerate() {
        if let Some(monster_id) = monster_id {
            occupied[slot] = state
                .entities
                .monsters
                .iter()
                .find(|monster| monster.id == *monster_id)
                .is_some_and(|monster| !monster.is_dead_or_escaped());
        }
    }
    occupied
}

fn roll_move_custom_plan(
    rng: &mut StsRng,
    entity: &MonsterEntity,
    ascension_level: u8,
    num: i32,
    monsters: &[MonsterEntity],
) -> MonsterTurnPlan {
    if runtime(entity).first_move {
        return spawn_dagger_plan();
    }

    if num < 33 {
        if !last_move(entity, SNAKE_STRIKE) {
            snake_strike_plan(ascension_level)
        } else {
            let reroll = rng.random_range(33, 99);
            roll_move_custom_plan(rng, entity, ascension_level, reroll, monsters)
        }
    } else if num < 66 {
        if !last_two_moves(entity, SPAWN_DAGGER) {
            if can_spawn(monsters, entity.id) {
                spawn_dagger_plan()
            } else {
                snake_strike_plan(ascension_level)
            }
        } else {
            snake_strike_plan(ascension_level)
        }
    } else if !last_move(entity, BIG_BITE) {
        big_bite_plan(ascension_level)
    } else {
        let reroll = rng.random(65);
        roll_move_custom_plan(rng, entity, ascension_level, reroll, monsters)
    }
}

impl MonsterBehavior for Reptomancer {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        _legacy_rng: PreBattleLegacyRng,
    ) -> Vec<Action> {
        let Some(reptomancer_index) = state
            .entities
            .monsters
            .iter()
            .position(|monster| monster.id == entity.id)
        else {
            return Vec::new();
        };

        let mut dagger_slots = [None, None, None, None];
        for (index, monster) in state.entities.monsters.iter().enumerate() {
            if monster.enemy_id == EnemyId::SnakeDagger {
                if index > reptomancer_index {
                    dagger_slots[0] = Some(monster.id);
                } else {
                    dagger_slots[1] = Some(monster.id);
                }
            }
        }
        if let Some(reptomancer) = state
            .entities
            .monsters
            .iter_mut()
            .find(|monster| monster.id == entity.id)
        {
            reptomancer.reptomancer.dagger_slots = dagger_slots;
            reptomancer.reptomancer.protocol_seeded = true;
        }

        state
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.id != entity.id)
            .map(|monster| Action::ApplyPower {
                source: monster.id,
                target: monster.id,
                power_id: PowerId::Minion,
                amount: -1,
            })
            .collect()
    }

    fn roll_move_plan_with_context(
        rng: &mut StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        ctx: MonsterRollContext<'_>,
    ) -> MonsterTurnPlan {
        roll_move_custom_plan(rng, entity, ascension_level, num, ctx.monsters)
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if runtime(entity).first_move {
            vec![reptomancer_runtime_update(entity, Some(false))]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (SNAKE_STRIKE, [MoveStep::Attack(attack), MoveStep::ApplyPower(power)]) => {
                let mut actions = attack_actions(entity.id, PLAYER, &attack.attack);
                actions.push(apply_power_action(entity, power));
                actions
            }
            (SPAWN_DAGGER, []) => {
                let occupied = occupied_dagger_slots(state, entity);
                let mut spawned = 0usize;
                let mut actions = Vec::new();
                for (slot, draw_x) in Reptomancer::DAGGER_DRAW_X.iter().enumerate() {
                    if spawned >= daggers_per_spawn(state.meta.ascension_level) || occupied[slot] {
                        continue;
                    }
                    spawned += 1;
                    // Java constructs every SnakeDagger in takeTurn, before
                    // queued SpawnMonsterAction updates initialize their AI.
                    let (hp_min, hp_max) = crate::content::monsters::get_hp_range(
                        EnemyId::SnakeDagger,
                        state.meta.ascension_level,
                    );
                    let hp = state.rng.monster_hp_rng.random_range(hp_min, hp_max);
                    actions.push(Action::SpawnReptomancerDagger {
                        reptomancer_id: entity.id,
                        slot: slot as u8,
                        hp: SpawnHpSpec {
                            current: SpawnHpValue::Fixed(hp),
                            max: SpawnHpValue::Fixed(hp),
                        },
                        logical_position: *draw_x,
                        protocol_draw_x: Some(*draw_x),
                    });
                }
                actions
            }
            (BIG_BITE, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (move_id, steps) => panic!("reptomancer plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }

    fn on_death(state: &mut CombatState, entity: &MonsterEntity) -> Vec<Action> {
        state
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.id != entity.id && !monster.is_dying)
            .rev()
            .map(|monster| Action::Suicide {
                target: monster.id,
                trigger_relics: true,
            })
            .collect()
    }
}
