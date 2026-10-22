use crate::content::cards::CardId;
use crate::content::monsters::exordium::{add_card_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, DecaRuntimeState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, ApplyPowerStep, AttackSpec, AttackStep, CardDestination, DamageKind, DefendSpec,
    EffectStrength, MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct Deca;

const BEAM: u8 = 0;
const SQUARE_OF_PROTECTION: u8 = 2;

pub fn initialize_runtime_state(entity: &mut MonsterEntity) {
    entity.deca = DecaRuntimeState {
        protocol_seeded: true,
        is_attacking: true,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn pre_battle_artifact_amount_matches_java_ascension_gate() {
        let mut state = crate::test_support::blank_test_combat();
        let deca = crate::test_support::test_monster(EnemyId::Deca);

        let normal = Deca::use_pre_battle_actions(
            &mut state,
            &deca,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );
        state.meta.ascension_level = 19;
        let asc19 = Deca::use_pre_battle_actions(
            &mut state,
            &deca,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );

        assert!(matches!(
            normal.as_slice(),
            [Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::Artifact,
                amount: 2
            }]
        ));
        assert!(matches!(
            asc19.as_slice(),
            [Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::Artifact,
                amount: 3
            }]
        ));
    }

    #[test]
    fn imported_is_attacking_false_with_empty_history_rolls_square() {
        let mut deca = crate::test_support::test_monster(EnemyId::Deca);
        deca.deca.is_attacking = false;
        deca.move_history_mut().clear();

        let plan = Deca::roll_move_plan(&mut StsRng::new(0), &deca, 0, 0);

        assert_eq!(
            plan.move_id, SQUARE_OF_PROTECTION,
            "Java Deca.getMove gates on private isAttacking, not move history"
        );
    }

    #[test]
    fn beam_turn_sets_private_is_attacking_false_before_roll() {
        let mut state =
            crate::test_support::combat_with_monsters(vec![crate::test_support::test_monster(
                EnemyId::Deca,
            )]);
        let deca = state.entities.monsters[0].clone();
        let plan = beam_plan(4);

        let actions = Deca::take_turn_plan(&mut state, &deca, &plan);

        assert!(matches!(
            actions.as_slice(),
            [
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 12,
                    damage_kind: DamageKind::Normal
                },
                Action::MonsterAttack {
                    source: 1,
                    target: PLAYER,
                    base_damage: 12,
                    damage_kind: DamageKind::Normal
                },
                Action::MakeTempCardInDiscard {
                    card_id: CardId::Dazed,
                    amount: 2,
                    upgraded: false
                },
                Action::UpdateMonsterRuntime {
                    patch: MonsterRuntimePatch::Deca {
                        is_attacking: Some(false),
                        protocol_seeded: Some(true),
                    },
                    ..
                },
                Action::RollMonsterMove { .. },
            ]
        ));
    }

    #[test]
    fn asc19_square_turn_blocks_and_plates_each_monster_before_roll() {
        let mut deca_entity = crate::test_support::test_monster(EnemyId::Deca);
        deca_entity.id = 1;
        let mut donu_entity = crate::test_support::test_monster(EnemyId::Donu);
        donu_entity.id = 2;
        donu_entity.current_hp = 0;
        donu_entity.is_dying = false;
        donu_entity.is_escaped = false;
        let mut state = crate::test_support::combat_with_monsters(vec![
            deca_entity.clone(),
            donu_entity.clone(),
        ]);
        let mut deca = state.entities.monsters[0].clone();
        deca.deca.is_attacking = false;
        let plan = square_plan(19);

        let actions = Deca::take_turn_plan(&mut state, &deca, &plan);

        assert!(matches!(
            actions.as_slice(),
            [
                Action::GainBlock {
                    target: 1,
                    amount: 16
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::PlatedArmor,
                    amount: 3
                },
                Action::GainBlock {
                    target: 2,
                    amount: 16
                },
                Action::ApplyPower {
                    source: 1,
                    target: 2,
                    power_id: PowerId::PlatedArmor,
                    amount: 3
                },
                Action::UpdateMonsterRuntime {
                    patch: MonsterRuntimePatch::Deca {
                        is_attacking: Some(true),
                        protocol_seeded: Some(true),
                    },
                    ..
                },
                Action::RollMonsterMove { .. },
            ]
        ));
    }
}

fn beam_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 4 {
        12
    } else {
        10
    }
}

fn beam_plan(ascension_level: u8) -> MonsterTurnPlan {
    let dazed = AddCardStep {
        card_id: CardId::Dazed,
        amount: 2,
        upgraded: false,
        destination: CardDestination::Discard,
        visible_strength: EffectStrength::Normal,
    };
    MonsterTurnPlan::with_visible_spec(
        BEAM,
        smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: beam_damage(ascension_level),
                    hits: 2,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::AddCard(dazed.clone()),
        ],
        MonsterMoveSpec::AttackAddCard(
            AttackSpec {
                base_damage: beam_damage(ascension_level),
                hits: 2,
                damage_kind: DamageKind::Normal,
            },
            dazed,
        ),
    )
}

fn square_plan(ascension_level: u8) -> MonsterTurnPlan {
    let steps = if ascension_level >= 19 {
        smallvec![
            MoveStep::GainBlock(crate::semantics::combat::BlockStep {
                target: MoveTarget::AllMonsters,
                amount: 16,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::AllMonsters,
                power_id: PowerId::PlatedArmor,
                amount: 3,
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            }),
        ]
    } else {
        smallvec![MoveStep::GainBlock(crate::semantics::combat::BlockStep {
            target: MoveTarget::AllMonsters,
            amount: 16,
        })]
    };

    let visible = if ascension_level >= 19 {
        MonsterMoveSpec::DefendBuff(
            DefendSpec { block: 16 },
            crate::semantics::combat::BuffSpec {
                power_id: PowerId::PlatedArmor,
                amount: 3,
            },
        )
    } else {
        MonsterMoveSpec::Defend(DefendSpec { block: 16 })
    };

    MonsterTurnPlan::with_visible_spec(SQUARE_OF_PROTECTION, steps, visible)
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        BEAM => beam_plan(ascension_level),
        SQUARE_OF_PROTECTION => square_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("deca.beam"), beam_plan(asc)),
            (Text::new("deca.squareOfProtection"), square_plan(asc)),
        ],
        special_moves: vec![],
        notes: Text::new("deca.notes"),
    }
}

fn runtime(entity: &MonsterEntity) -> &DecaRuntimeState {
    assert!(
        entity.deca.protocol_seeded,
        "deca runtime truth must be protocol-seeded or factory-seeded"
    );
    &entity.deca
}

fn deca_runtime_update(entity: &MonsterEntity, is_attacking: bool) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::Deca {
            is_attacking: Some(is_attacking),
            protocol_seeded: Some(true),
        },
    }
}

impl MonsterBehavior for Deca {
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
            power_id: PowerId::Artifact,
            amount: if ascension_level >= 19 { 3 } else { 2 },
        }]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        if runtime(entity).is_attacking {
            beam_plan(ascension_level)
        } else {
            square_plan(ascension_level)
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
            (
                BEAM,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                }), MoveStep::AddCard(add_card)],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(add_card_action(add_card));
                actions
            }
            (
                SQUARE_OF_PROTECTION,
                [MoveStep::GainBlock(crate::semantics::combat::BlockStep {
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
                SQUARE_OF_PROTECTION,
                [MoveStep::GainBlock(crate::semantics::combat::BlockStep {
                    target: MoveTarget::AllMonsters,
                    amount,
                }), MoveStep::ApplyPower(ApplyPowerStep {
                    target: MoveTarget::AllMonsters,
                    power_id: PowerId::PlatedArmor,
                    amount: plated_amount,
                    effect: PowerEffectKind::Buff,
                    ..
                })],
            ) => {
                let mut actions = Vec::new();
                for monster in &state.entities.monsters {
                    actions.push(Action::GainBlock {
                        target: monster.id,
                        amount: *amount,
                    });
                    actions.push(Action::ApplyPower {
                        source: entity.id,
                        target: monster.id,
                        power_id: PowerId::PlatedArmor,
                        amount: *plated_amount,
                    });
                }
                actions
            }
            (_, []) => panic!("deca plan missing locked truth"),
            (move_id, steps) => panic!("deca plan/steps mismatch: {} {:?}", move_id, steps),
        };
        actions.push(deca_runtime_update(
            entity,
            plan.move_id == SQUARE_OF_PROTECTION,
        ));
        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}
