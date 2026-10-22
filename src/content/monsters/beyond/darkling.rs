use crate::content::monsters::exordium::{
    apply_power_action, attack_actions, gain_block_action, set_next_move_action, PLAYER,
};
use crate::content::monsters::{MonsterBehavior, MonsterRollContext};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, DarklingRuntimeState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, BuffSpec, DamageKind, DefendSpec, EffectStrength, HealStep,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct Darkling;

const CHOMP: u8 = 1;
const HARDEN: u8 = 2;
const NIP: u8 = 3;
const COUNT: u8 = 4;
const REINCARNATE: u8 = 5;

pub fn roll_nip_damage(hp_rng: &mut crate::runtime::rng::StsRng, ascension_level: u8) -> i32 {
    let (min, max) = nip_damage_range(ascension_level);
    hp_rng.random_range(min, max)
}

pub fn nip_damage_range(asc: u8) -> (i32, i32) {
    if asc >= 2 {
        (9, 13)
    } else {
        (7, 11)
    }
}

pub const LIFE_LINK_MECHANICS: crate::content::powers::mechanics::PowerMechanics =
    crate::content::powers::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Regrow,
        round_duration: false,
    };

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    let (min, max) = nip_damage_range(asc);
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![("Chomp", chomp_plan(asc)), ("Harden", harden_plan(asc))],
        special_moves: vec![
            ("Nip", format!("Attack player once. Base damage is rolled {min}..{max} inclusive at spawn and remains fixed for that Darkling; see its visible intent for the current modified damage.")),
            ("Count", "Idle for one own turn while half-dead, then plan Reincarnate.".into()),
            ("Reincarnate", "Heal self for floor(Max HP / 2), revive, then reapply Life Link. New-monster relic hooks also run.".into()),
        ],
        notes: "Starts with Life Link. Opens with Harden or Nip. Only even-positioned Darklings can use Chomp; Chomp and Harden cannot repeat consecutively, and Nip cannot occur three times in a row. See info power Life Link for the coordinated half-death and revival rules.",
    }
}

pub fn initialize_runtime_state(
    entity: &mut MonsterEntity,
    hp_rng: &mut crate::runtime::rng::StsRng,
    ascension_level: u8,
) {
    if entity.enemy_id != crate::content::monsters::EnemyId::Darkling {
        return;
    }

    entity.darkling.first_move = true;
    entity.darkling.nip_dmg = roll_nip_damage(hp_rng, ascension_level);
    entity.darkling.protocol_seeded = true;
}

fn chomp_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        9
    } else {
        8
    }
}

fn is_even_position(entity: &MonsterEntity, monsters: &[MonsterEntity]) -> bool {
    let position = if monsters.len() <= entity.slot as usize {
        entity.slot as usize
    } else {
        monsters
            .iter()
            .rposition(|monster| monster.id == entity.id)
            .unwrap_or(entity.slot as usize)
    };
    position % 2 == 0
}

fn current_nip_damage(entity: &MonsterEntity, _ascension_level: u8) -> i32 {
    let runtime = runtime(entity);
    assert!(
        runtime.nip_dmg > 0,
        "darkling nip_dmg must be protocol-seeded or factory-seeded"
    );
    runtime.nip_dmg
}

fn runtime(entity: &MonsterEntity) -> &DarklingRuntimeState {
    assert!(
        entity.darkling.protocol_seeded,
        "darkling runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.darkling
}

fn darkling_runtime_update(
    entity: &MonsterEntity,
    first_move: Option<bool>,
    nip_dmg: Option<i32>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Darkling {
            first_move,
            nip_dmg,
            protocol_seeded: Some(true),
        },
    }
}

fn chomp_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        CHOMP,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: chomp_damage(ascension_level),
            hits: 2,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn harden_plan(ascension_level: u8) -> MonsterTurnPlan {
    if ascension_level >= 17 {
        MonsterTurnPlan::with_visible_spec(
            HARDEN,
            smallvec![
                MoveStep::GainBlock(crate::semantics::combat::BlockStep {
                    target: MoveTarget::SelfTarget,
                    amount: 12,
                }),
                MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::SelfTarget,
                    power_id: PowerId::Strength,
                    amount: 2,
                    effect: PowerEffectKind::Buff,
                    visible_strength: EffectStrength::Normal,
                }),
            ],
            MonsterMoveSpec::DefendBuff(
                DefendSpec { block: 12 },
                BuffSpec {
                    power_id: PowerId::Strength,
                    amount: 2,
                },
            ),
        )
    } else {
        MonsterTurnPlan::from_spec(HARDEN, MonsterMoveSpec::Defend(DefendSpec { block: 12 }))
    }
}

fn nip_plan(entity: &MonsterEntity, ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        NIP,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: current_nip_damage(entity, ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn count_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::unknown(COUNT)
}

fn reincarnate_plan(entity: &MonsterEntity) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        REINCARNATE,
        smallvec![
            MoveStep::Heal(HealStep {
                target: MoveTarget::SelfTarget,
                amount: entity.max_hp / 2,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Regrow,
                // RegrowPower inherits AbstractPower.amount = -1. The 1 in
                // Java ApplyPowerAction is a separate stackAmount; revival
                // installs a fresh instance after half-death cleared powers.
                amount: -1,
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::Buff(BuffSpec {
            power_id: PowerId::Regrow,
            amount: 1,
        }),
    )
}

fn plan_for(entity: &MonsterEntity, ascension_level: u8, move_id: u8) -> MonsterTurnPlan {
    match move_id {
        CHOMP => chomp_plan(ascension_level),
        HARDEN => harden_plan(ascension_level),
        NIP => nip_plan(entity, ascension_level),
        COUNT => count_plan(),
        REINCARNATE => reincarnate_plan(entity),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

fn roll_move_custom_plan(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    ascension_level: u8,
    num: i32,
    monsters: &[MonsterEntity],
) -> MonsterTurnPlan {
    if entity.half_dead {
        return reincarnate_plan(entity);
    }

    if entity.current_hp <= 0 {
        return count_plan();
    }

    if runtime(entity).first_move {
        return if num < 50 {
            harden_plan(ascension_level)
        } else {
            nip_plan(entity, ascension_level)
        };
    }

    let last_move = entity.move_history().back().copied().unwrap_or(0);
    let last_two_moves = |move_id| {
        entity.move_history().len() >= 2
            && entity.move_history()[entity.move_history().len() - 1] == move_id
            && entity.move_history()[entity.move_history().len() - 2] == move_id
    };

    if num < 40 {
        if last_move != CHOMP && is_even_position(entity, monsters) {
            chomp_plan(ascension_level)
        } else {
            let reroll = rng.random_range(40, 99);
            roll_move_custom_plan(rng, entity, ascension_level, reroll, monsters)
        }
    } else if num < 70 {
        if last_move != HARDEN {
            harden_plan(ascension_level)
        } else {
            nip_plan(entity, ascension_level)
        }
    } else if !last_two_moves(NIP) {
        nip_plan(entity, ascension_level)
    } else {
        let reroll = rng.random_range(0, 99);
        roll_move_custom_plan(rng, entity, ascension_level, reroll, monsters)
    }
}

impl MonsterBehavior for Darkling {
    fn roll_move_plan_with_context(
        rng: &mut crate::runtime::rng::StsRng,
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
        if runtime(entity).first_move && !entity.half_dead && entity.current_hp > 0 {
            vec![darkling_runtime_update(entity, Some(false), None)]
        } else {
            Vec::new()
        }
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
            power_id: PowerId::Regrow,
            amount: -1,
        }]
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity, state.meta.ascension_level, entity.planned_move_id())
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (CHOMP | NIP, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (HARDEN, [MoveStep::GainBlock(block)]) => vec![gain_block_action(entity, block)],
            (HARDEN, [MoveStep::GainBlock(block), MoveStep::ApplyPower(power)]) => vec![
                gain_block_action(entity, block),
                apply_power_action(entity, power),
            ],
            (COUNT, []) => Vec::new(),
            (REINCARNATE, [MoveStep::Heal(heal), MoveStep::ApplyPower(power)]) => {
                let actions = vec![
                    Action::Heal {
                        target: entity.id,
                        amount: heal.amount,
                    },
                    Action::ReviveMonster { target: entity.id },
                    apply_power_action(entity, power),
                ];
                crate::content::relics::hooks::on_spawn_monster(state, entity.id);
                actions
            }
            (move_id, steps) => panic!("darkling plan/steps mismatch: {} {:?}", move_id, steps),
        };

        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }

    fn on_death(state: &mut CombatState, entity: &MonsterEntity) -> Vec<Action> {
        let darkling_ids: Vec<_> = state
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.enemy_id == crate::content::monsters::EnemyId::Darkling)
            .map(|monster| monster.id)
            .collect();

        let all_dead = state
            .entities
            .monsters
            .iter()
            .filter(|monster| darkling_ids.contains(&monster.id))
            .all(|monster| monster.id == entity.id || monster.half_dead);

        if all_dead {
            for id in darkling_ids {
                if let Some(monster) = state.entities.monsters.iter_mut().find(|m| m.id == id) {
                    if monster.id == entity.id {
                        monster.half_dead = false;
                    }
                    monster.is_dying = true;
                    monster.current_hp = 0;
                }
            }
            return Vec::new();
        }

        let should_queue_count = entity.planned_move_id() != COUNT;
        if let Some(monster) = state
            .entities
            .monsters
            .iter_mut()
            .find(|m| m.id == entity.id)
        {
            monster.half_dead = true;
            monster.is_dying = false;
            monster.current_hp = 0;
            if should_queue_count {
                let plan = count_plan();
                monster.set_planned_move_id(plan.move_id);
                monster.set_planned_steps(plan.steps.clone());
                monster.set_planned_visible_spec(plan.visible_spec.clone());
                monster.move_history_mut().push_back(plan.move_id);
            }
        }

        if should_queue_count {
            vec![set_next_move_action(entity, count_plan())]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{current_nip_damage, reincarnate_plan, Darkling, COUNT, HARDEN, NIP, REINCARNATE};
    use crate::content::monsters::{EnemyId, MonsterBehavior, MonsterRollContext};
    use crate::content::powers::{store, PowerId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::action::{Action, MonsterRuntimePatch};
    use crate::runtime::combat::{Power, PowerPayload};

    fn power(power_type: PowerId, amount: i32) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    #[test]
    fn nip_damage_uses_seeded_private_runtime_truth() {
        let mut entity = crate::testing::support::test_monster(EnemyId::Darkling);
        entity.darkling.nip_dmg = 13;

        assert_eq!(current_nip_damage(&entity, 2), 13);
    }

    #[test]
    fn first_roll_uses_private_first_move_and_marks_it() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let entity = crate::testing::support::test_monster(EnemyId::Darkling);
        let monsters = vec![entity.clone()];
        let ctx = MonsterRollContext {
            monsters: &monsters,
            player_powers: &[],
        };
        let plan = Darkling::roll_move_plan_with_context(&mut rng, &entity, 0, 49, ctx);

        assert_eq!(plan.move_id, HARDEN);
        assert_eq!(
            Darkling::on_roll_move(0, &entity, 49, &plan),
            vec![Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::Darkling {
                    first_move: Some(false),
                    nip_dmg: None,
                    protocol_seeded: Some(true),
                },
            }]
        );
    }

    #[test]
    fn half_dead_reincarnate_roll_does_not_clear_first_move() {
        let mut rng = crate::runtime::rng::StsRng::new(1);
        let mut entity = crate::testing::support::test_monster(EnemyId::Darkling);
        entity.half_dead = true;
        let monsters = vec![entity.clone()];
        let ctx = MonsterRollContext {
            monsters: &monsters,
            player_powers: &[],
        };
        let plan = Darkling::roll_move_plan_with_context(&mut rng, &entity, 0, 49, ctx);

        assert_eq!(plan.move_id, REINCARNATE);
        assert!(Darkling::on_roll_move(0, &entity, 49, &plan).is_empty());
    }

    #[test]
    fn reincarnate_turn_queues_java_heal_revive_power_order() {
        let mut entity = crate::testing::support::test_monster(EnemyId::Darkling);
        entity.id = 7;
        entity.current_hp = 0;
        entity.max_hp = 58;
        entity.half_dead = true;
        let mut state = crate::testing::support::combat_with_monsters(vec![entity.clone()]);
        let plan = reincarnate_plan(&entity);

        let actions = Darkling::take_turn_plan(&mut state, &entity, &plan);

        assert_eq!(
            actions,
            vec![
                Action::Heal {
                    target: 7,
                    amount: 29,
                },
                Action::ReviveMonster { target: 7 },
                Action::ApplyPower {
                    source: 7,
                    target: 7,
                    power_id: PowerId::Regrow,
                    amount: -1,
                },
                Action::RollMonsterMove { monster_id: 7 },
            ],
            "Java Darkling queues HealAction, ChangeStateAction(REVIVE), ApplyPowerAction(Regrow, 1), then RollMoveAction"
        );
        for action in actions {
            crate::engine::action_handlers::execute_action(action, &mut state);
        }
        assert_eq!(state.entities.monsters[0].current_hp, 29);
        assert!(!state.entities.monsters[0].half_dead);
        assert_eq!(store::power_amount(&state, 7, PowerId::Regrow), -1);
    }

    #[test]
    fn first_half_death_immediately_records_count_then_queues_set_move() {
        let mut target = crate::testing::support::test_monster(EnemyId::Darkling);
        target.id = 7;
        target.set_planned_move_id(NIP);
        target.move_history_mut().push_back(NIP);
        let mut other = crate::testing::support::test_monster(EnemyId::Darkling);
        other.id = 8;
        other.slot = 1;
        let mut state = crate::testing::support::combat_with_monsters(vec![target.clone(), other]);

        let actions = Darkling::on_death(&mut state, &target);

        let darkling = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == 7)
            .unwrap();
        assert!(darkling.half_dead);
        assert!(!darkling.is_dying);
        assert_eq!(darkling.planned_move_id(), COUNT);
        assert_eq!(
            darkling.move_history().iter().filter(|&&m| m == COUNT).count(),
            1,
            "Java Darkling.damage() calls setMove(COUNT) immediately before queuing SetMoveAction(COUNT)"
        );
        assert!(matches!(
            actions.as_slice(),
            [Action::SetMonsterMove {
                monster_id: 7,
                next_move_byte: COUNT,
                ..
            }]
        ));

        crate::engine::action_handlers::execute_action(actions[0].clone(), &mut state);
        let darkling = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == 7)
            .unwrap();
        assert_eq!(
            darkling
                .move_history()
                .iter()
                .filter(|&&m| m == COUNT)
                .count(),
            2,
            "Java queued SetMoveAction(COUNT) records the duplicate move-history entry"
        );
    }

    #[test]
    fn half_death_already_on_count_does_not_queue_duplicate_set_move() {
        let mut target = crate::testing::support::test_monster(EnemyId::Darkling);
        target.id = 7;
        target.set_planned_move_id(COUNT);
        target.move_history_mut().push_back(COUNT);
        let mut other = crate::testing::support::test_monster(EnemyId::Darkling);
        other.id = 8;
        other.slot = 1;
        let mut state = crate::testing::support::combat_with_monsters(vec![target.clone(), other]);

        let actions = Darkling::on_death(&mut state, &target);

        assert!(actions.is_empty());
        let darkling = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == 7)
            .unwrap();
        assert_eq!(
            darkling
                .move_history()
                .iter()
                .filter(|&&m| m == COUNT)
                .count(),
            1,
            "Java guards the half-death SetMoveAction with nextMove != COUNT"
        );
    }

    #[test]
    fn central_darkling_death_keeps_powers_for_relic_hooks_then_clears_them() {
        let mut target = crate::testing::support::test_monster(EnemyId::Darkling);
        target.id = 7;
        target.current_hp = 1;
        target.set_planned_move_id(NIP);
        let mut other = crate::testing::support::test_monster(EnemyId::Darkling);
        other.id = 8;
        other.slot = 1;
        let mut state = crate::testing::support::combat_with_monsters(vec![target, other]);
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::TheSpecimen));
        store::set_powers_for(
            &mut state,
            7,
            vec![power(PowerId::Regrow, -1), power(PowerId::Poison, 6)],
        );

        crate::engine::action_handlers::damage::handle_damage(
            crate::runtime::action::DamageInfo {
                source: crate::content::monsters::exordium::PLAYER,
                target: 7,
                base: 1,
                output: 1,
                damage_type: crate::runtime::action::DamageType::Normal,
                is_modified: true,
            },
            &mut state,
        );

        let first = state
            .pop_next_action()
            .expect("The Specimen poison transfer");
        assert_eq!(
            first,
            Action::ApplyPower {
                source: 0,
                target: 8,
                power_id: PowerId::Poison,
                amount: 6,
            },
            "Java calls relic onMonsterDeath while Darkling powers are still present"
        );
        let second = state
            .pop_next_action()
            .expect("queued Darkling SetMoveAction");
        assert!(matches!(
            second,
            Action::SetMonsterMove {
                monster_id: 7,
                next_move_byte: COUNT,
                ..
            }
        ));
        assert!(
            store::powers_for(&state, 7).is_none(),
            "Java clears Darkling powers after relic onMonsterDeath hooks"
        );
    }
}
