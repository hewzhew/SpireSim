use crate::content::monsters::exordium::{attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, AttackStep, BlockStep, DamageKind, DebuffSpec, DefendSpec,
    EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct SpireShield;

const BASH: u8 = 1;
const FORTIFY: u8 = 2;
const SMASH: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::{EnemyId, PreBattleLegacyRng};
    use crate::runtime::combat::{OrbEntity, OrbId, Power, PowerPayload};

    #[test]
    fn pre_battle_applies_surrounded_sentinel_then_artifact_like_java() {
        let mut shield = crate::test_support::test_monster(EnemyId::SpireShield);
        shield.id = 1;
        let mut state = crate::test_support::combat_with_monsters(vec![shield.clone()]);
        state.meta.ascension_level = 18;

        let actions =
            SpireShield::use_pre_battle_actions(&mut state, &shield, PreBattleLegacyRng::Misc);

        assert_eq!(
            actions,
            vec![
                Action::ApplyPower {
                    source: 1,
                    target: PLAYER,
                    power_id: PowerId::Surrounded,
                    amount: -1,
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Artifact,
                    amount: 2,
                },
            ]
        );
    }

    #[test]
    fn non_asc18_smash_block_uses_current_damage_output_like_java() {
        let mut shield = crate::test_support::test_monster(EnemyId::SpireShield);
        shield.id = 1;
        let mut state = crate::test_support::combat_with_monsters(vec![shield.clone()]);
        state.meta.ascension_level = 3;
        state.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: 5,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        let plan = smash_plan(3);

        let actions = SpireShield::take_turn_plan(&mut state, &shield, &plan);

        assert!(actions.iter().any(|action| matches!(
            action,
            Action::GainBlock {
                target: 1,
                amount: 43,
            }
        )));
    }

    #[test]
    fn bash_without_orbs_applies_strength_without_consuming_focus_rng() {
        let mut shield = crate::test_support::test_monster(EnemyId::SpireShield);
        shield.id = 1;
        let mut state = crate::test_support::combat_with_monsters(vec![shield.clone()]);
        state.rng.ai_rng = crate::runtime::rng::StsRng::new(0);

        let actions = SpireShield::take_turn_plan(&mut state, &shield, &bash_plan(3));

        assert_eq!(state.rng.ai_rng.counter, 0);
        assert!(actions.contains(&Action::ApplyPower {
            source: 1,
            target: PLAYER,
            power_id: PowerId::Strength,
            amount: -1,
        }));
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::ApplyPower {
                power_id: PowerId::Focus,
                ..
            }
        )));
    }

    #[test]
    fn bash_with_orbs_uses_take_turn_rng_for_focus_or_strength_choice() {
        let mut shield = crate::test_support::test_monster(EnemyId::SpireShield);
        shield.id = 1;
        let mut state = crate::test_support::combat_with_monsters(vec![shield.clone()]);
        state
            .entities
            .player
            .orbs
            .push(OrbEntity::new(OrbId::Lightning));
        state.rng.ai_rng = crate::runtime::rng::StsRng::new(0);

        let actions = SpireShield::take_turn_plan(&mut state, &shield, &bash_plan(3));

        assert_eq!(state.rng.ai_rng.counter, 1);
        assert!(actions.contains(&Action::ApplyPower {
            source: 1,
            target: PLAYER,
            power_id: PowerId::Focus,
            amount: -1,
        }));
    }

    #[test]
    fn fortify_blocks_all_monsters_in_group_without_hp_filter_like_java() {
        let mut shield = crate::test_support::test_monster(EnemyId::SpireShield);
        shield.id = 1;
        let mut defeated_ally = crate::test_support::test_monster(EnemyId::SpireSpear);
        defeated_ally.id = 2;
        defeated_ally.current_hp = 0;
        defeated_ally.is_dying = false;
        let mut state =
            crate::test_support::combat_with_monsters(vec![shield.clone(), defeated_ally]);

        let actions = SpireShield::take_turn_plan(&mut state, &shield, &fortify_plan());

        assert!(actions.contains(&Action::GainBlock {
            target: 1,
            amount: 30,
        }));
        assert!(actions.contains(&Action::GainBlock {
            target: 2,
            amount: 30,
        }));
    }

    #[test]
    fn roll_uses_private_move_count_not_truncated_move_history() {
        let mut shield = crate::test_support::test_monster(EnemyId::SpireShield);
        shield.spire_shield.move_count = 2;
        shield.move_history_mut().clear();

        let plan =
            SpireShield::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &shield, 20, 0);

        assert_eq!(
            plan.move_id, SMASH,
            "Java SpireShield.getMove branches on private moveCount, not recoverable moveHistory length"
        );
    }

    #[test]
    fn roll_updates_private_move_count_like_java_get_move() {
        let mut shield = crate::test_support::test_monster(EnemyId::SpireShield);
        shield.id = 63;
        shield.spire_shield.move_count = 2;

        let actions = SpireShield::on_roll_move(20, &shield, 0, &smash_plan(20));

        assert_eq!(
            actions,
            vec![Action::UpdateMonsterRuntime {
                monster_id: 63,
                patch: MonsterRuntimePatch::SpireShield {
                    move_count: Some(3),
                    protocol_seeded: Some(true),
                },
            }]
        );
    }
}

fn bash_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 3 {
        14
    } else {
        12
    }
}

fn smash_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 3 {
        38
    } else {
        34
    }
}

fn smash_block(ascension_level: u8) -> i32 {
    if ascension_level >= 18 {
        99
    } else {
        smash_damage(ascension_level)
    }
}

fn bash_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        BASH,
        smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: bash_damage(ascension_level),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Strength,
                amount: -1,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: bash_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Strength,
                amount: -1,
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn fortify_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        FORTIFY,
        smallvec![MoveStep::GainBlock(BlockStep {
            target: MoveTarget::AllMonsters,
            amount: 30,
        })],
        MonsterMoveSpec::Defend(DefendSpec { block: 30 }),
    )
}

fn smash_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SMASH,
        MonsterMoveSpec::AttackDefend(
            AttackSpec {
                base_damage: smash_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DefendSpec {
                block: smash_block(ascension_level),
            },
        ),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        BASH => bash_plan(ascension_level),
        FORTIFY => fortify_plan(),
        SMASH => smash_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![("Fortify", fortify_plan()), ("Smash", smash_plan(asc))],
        special_moves: vec![("Bash", (if asc >= 3 {
            "attack player for 14 x 1 base damage, then apply -1 Strength; with any player orbs, randomly apply -1 Focus instead with 50% probability"
        } else {
            "attack player for 12 x 1 base damage, then apply -1 Strength; with any player orbs, randomly apply -1 Focus instead with 50% probability"
        }).into())],
        notes: "Starts with 1 Artifact (2 at A18+) and applies Surrounded to the player. Repeats Bash and Fortify in random order, then Smash. While both enemies remain, targeted cards change which one the player faces; an enemy attacking from behind deals 50% more damage. Untargeted cards do not change facing.",
    }
}

fn current_move_count(entity: &MonsterEntity) -> u8 {
    assert!(
        entity.spire_shield.protocol_seeded,
        "spire shield runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.spire_shield.move_count
}

fn increment_move_count(entity: &MonsterEntity) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::SpireShield {
            move_count: Some(entity.spire_shield.move_count.saturating_add(1)),
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for SpireShield {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        vec![
            Action::ApplyPower {
                source: entity.id,
                target: PLAYER,
                power_id: PowerId::Surrounded,
                amount: -1,
            },
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Artifact,
                amount: if ascension_level >= 18 { 2 } else { 1 },
            },
        ]
    }

    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        match current_move_count(entity) % 3 {
            0 => {
                if rng.random_boolean() {
                    fortify_plan()
                } else {
                    bash_plan(ascension_level)
                }
            }
            1 => {
                if entity.move_history().back().copied() == Some(BASH) {
                    fortify_plan()
                } else {
                    bash_plan(ascension_level)
                }
            }
            _ => smash_plan(ascension_level),
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        vec![increment_move_count(entity)]
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
            (
                BASH,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                }), MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::Player,
                    effect: PowerEffectKind::Debuff,
                    ..
                })],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                let focus_roll =
                    !state.entities.player.orbs.is_empty() && state.rng.ai_rng.random_boolean();
                actions.push(Action::ApplyPower {
                    source: entity.id,
                    target: PLAYER,
                    power_id: if focus_roll {
                        PowerId::Focus
                    } else {
                        PowerId::Strength
                    },
                    amount: -1,
                });
                actions
            }
            (
                FORTIFY,
                [MoveStep::GainBlock(BlockStep {
                    target: MoveTarget::AllMonsters,
                    amount,
                })],
            ) => state
                .entities
                .monsters
                .iter()
                .map(|monster| Action::GainBlock {
                    target: monster.id,
                    amount: *amount,
                })
                .collect(),
            (
                SMASH,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                }), MoveStep::GainBlock(
                    block @ BlockStep {
                        target: MoveTarget::SelfTarget,
                        ..
                    },
                )],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                let block_amount = if state.meta.ascension_level >= 18 {
                    block.amount
                } else {
                    crate::content::powers::calculate_monster_damage(
                        attack.base_damage,
                        entity.id,
                        PLAYER,
                        state,
                    )
                };
                actions.push(Action::GainBlock {
                    target: entity.id,
                    amount: block_amount,
                });
                actions
            }
            (_, []) => panic!("spire shield plan missing locked truth"),
            (move_id, steps) => panic!("spire shield plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }

    fn on_death(state: &mut CombatState, _entity: &MonsterEntity) -> Vec<Action> {
        super::surrounded_cleanup_actions(state)
    }
}
