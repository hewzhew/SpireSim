use crate::content::cards::CardId;
use crate::content::monsters::exordium::{
    apply_power_action, attack_actions, gain_block_action, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AttackSpec, DamageKind, DebuffSpec, DefendSpec, EffectStrength, MonsterMoveSpec,
    MonsterTurnPlan, MoveStep, PowerEffectKind,
};
use smallvec::smallvec;

pub struct WrithingMass;

const BIG_HIT: u8 = 0;
const MULTI_HIT: u8 = 1;
const ATTACK_BLOCK: u8 = 2;
const ATTACK_DEBUFF: u8 = 3;
const MEGA_DEBUFF: u8 = 4;
pub(crate) const STARTING_POWERS: [(PowerId, i32); 2] =
    [(PowerId::Reactive, -1), (PowerId::Malleable, 3)];

fn big_hit_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        38
    } else {
        32
    }
}

fn multi_hit_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        9
    } else {
        7
    }
}

fn attack_block_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        16
    } else {
        15
    }
}

fn attack_debuff_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 2 {
        12
    } else {
        10
    }
}

fn normal_debuff_amt() -> i32 {
    2
}

fn big_hit_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BIG_HIT,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: big_hit_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn multi_hit_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        MULTI_HIT,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: multi_hit_damage(ascension_level),
            hits: 3,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn attack_block_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        ATTACK_BLOCK,
        MonsterMoveSpec::AttackDefend(
            AttackSpec {
                base_damage: attack_block_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DefendSpec {
                block: attack_block_damage(ascension_level),
            },
        ),
    )
}

fn attack_debuff_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        ATTACK_DEBUFF,
        smallvec![
            MoveStep::Attack(crate::semantics::combat::AttackStep {
                target: crate::semantics::combat::MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: attack_debuff_damage(ascension_level),
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::ApplyPower(crate::semantics::combat::ApplyPowerStep {
                target: crate::semantics::combat::MoveTarget::Player,
                power_id: PowerId::Weak,
                amount: normal_debuff_amt(),
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::ApplyPower(crate::semantics::combat::ApplyPowerStep {
                target: crate::semantics::combat::MoveTarget::Player,
                power_id: PowerId::Vulnerable,
                amount: normal_debuff_amt(),
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::AttackDebuff(
            AttackSpec {
                base_damage: attack_debuff_damage(ascension_level),
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            DebuffSpec {
                power_id: PowerId::Weak,
                amount: normal_debuff_amt(),
                strength: EffectStrength::Normal,
            },
        ),
    )
}

fn mega_debuff_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        MEGA_DEBUFF,
        smallvec![],
        MonsterMoveSpec::StrongDebuffIntent,
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        BIG_HIT => big_hit_plan(ascension_level),
        MULTI_HIT => multi_hit_plan(ascension_level),
        ATTACK_BLOCK => attack_block_plan(ascension_level),
        ATTACK_DEBUFF => attack_debuff_plan(ascension_level),
        MEGA_DEBUFF => mega_debuff_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> crate::content::monsters::mechanics::MonsterMechanics {
    use crate::content::monsters::mechanics::MonsterText as Text;
    crate::content::monsters::mechanics::MonsterMechanics {
        moves: vec![
            (Text::new("writhing_mass.strongStrike"), big_hit_plan(asc)),
            (Text::new("writhing_mass.multiStrike"), multi_hit_plan(asc)),
            (Text::new("writhing_mass.flail"), attack_block_plan(asc)),
            (Text::new("writhing_mass.wither"), attack_debuff_plan(asc)),
        ],
        special_moves: vec![(
            Text::new("writhing_mass.implant"),
            Text::new("writhing_mass.implantRule"),
        )],
        notes: Text::new("writhing_mass.notes"),
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn used_mega_debuff(entity: &MonsterEntity) -> bool {
    assert!(
        entity.writhing_mass.protocol_seeded,
        "writhing mass runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.writhing_mass.used_mega_debuff
}

fn first_move(entity: &MonsterEntity) -> bool {
    assert!(
        entity.writhing_mass.protocol_seeded,
        "writhing mass runtime truth must be protocol-seeded or factory-seeded"
    );
    entity.writhing_mass.first_move
}

fn writhing_mass_runtime_update(
    entity: &MonsterEntity,
    first_move: Option<bool>,
    used_mega_debuff: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::WrithingMass {
            first_move,
            used_mega_debuff,
            protocol_seeded: Some(true),
        },
    }
}

fn roll_move_recursive(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    ascension_level: u8,
    num: i32,
) -> MonsterTurnPlan {
    if first_move(entity) {
        return if num < 33 {
            multi_hit_plan(ascension_level)
        } else if num < 66 {
            attack_block_plan(ascension_level)
        } else {
            attack_debuff_plan(ascension_level)
        };
    }

    if num < 10 {
        if !last_move(entity, BIG_HIT) {
            big_hit_plan(ascension_level)
        } else {
            let reroll = rng.random_range(10, 99);
            roll_move_recursive(rng, entity, ascension_level, reroll)
        }
    } else if num < 20 {
        if !used_mega_debuff(entity) && !last_move(entity, MEGA_DEBUFF) {
            mega_debuff_plan()
        } else if rng.random_boolean_chance(0.1) {
            big_hit_plan(ascension_level)
        } else {
            let reroll = rng.random_range(20, 99);
            roll_move_recursive(rng, entity, ascension_level, reroll)
        }
    } else if num < 40 {
        if !last_move(entity, ATTACK_DEBUFF) {
            attack_debuff_plan(ascension_level)
        } else if rng.random_boolean_chance(0.4) {
            let reroll = rng.random(19);
            roll_move_recursive(rng, entity, ascension_level, reroll)
        } else {
            let reroll = rng.random_range(40, 99);
            roll_move_recursive(rng, entity, ascension_level, reroll)
        }
    } else if num < 70 {
        if !last_move(entity, MULTI_HIT) {
            multi_hit_plan(ascension_level)
        } else if rng.random_boolean_chance(0.3) {
            attack_block_plan(ascension_level)
        } else {
            let reroll = rng.random(39);
            roll_move_recursive(rng, entity, ascension_level, reroll)
        }
    } else if !last_move(entity, ATTACK_BLOCK) {
        attack_block_plan(ascension_level)
    } else {
        let reroll = rng.random(69);
        roll_move_recursive(rng, entity, ascension_level, reroll)
    }
}

impl MonsterBehavior for WrithingMass {
    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        roll_move_recursive(rng, entity, ascension_level, num)
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        if first_move(entity) {
            vec![writhing_mass_runtime_update(entity, Some(false), None)]
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
        STARTING_POWERS
            .into_iter()
            .map(|(power_id, amount)| Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id,
                amount,
            })
            .collect()
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (BIG_HIT | MULTI_HIT, [MoveStep::Attack(attack)]) => {
                attack_actions(entity.id, PLAYER, &attack.attack)
            }
            (ATTACK_BLOCK, [MoveStep::Attack(attack), MoveStep::GainBlock(block)]) => {
                let mut actions = attack_actions(entity.id, PLAYER, &attack.attack);
                actions.push(gain_block_action(entity, block));
                actions
            }
            (
                ATTACK_DEBUFF,
                [MoveStep::Attack(attack), MoveStep::ApplyPower(weak), MoveStep::ApplyPower(vulnerable)],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, &attack.attack);
                actions.push(apply_power_action(entity, weak));
                actions.push(apply_power_action(entity, vulnerable));
                actions
            }
            (MEGA_DEBUFF, []) => vec![
                writhing_mass_runtime_update(entity, None, Some(true)),
                Action::AddCardToMasterDeck {
                    card_id: CardId::Parasite,
                    source: entity.id,
                },
            ],
            (move_id, steps) => {
                panic!("writhing mass plan/steps mismatch: {} {:?}", move_id, steps)
            }
        };

        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::rng::StsRng;

    #[test]
    fn mega_debuff_eligibility_uses_execution_flag_not_move_history() {
        let mut mass = crate::test_support::test_monster(EnemyId::WrithingMass);
        mass.writhing_mass.first_move = false;
        mass.move_history_mut().push_back(MEGA_DEBUFF);
        mass.move_history_mut().push_back(BIG_HIT);
        mass.writhing_mass.used_mega_debuff = false;

        let plan =
            <WrithingMass as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &mass, 20, 15);

        assert_eq!(
            plan.move_id, MEGA_DEBUFF,
            "Java only sets usedMegaDebuff when Parasite actually executes; a Reactive reroll through move 4 must not consume it"
        );
        assert!(plan.steps.is_empty());
        assert_eq!(
            crate::runtime::combat::Intent::from_visible_move_spec(&plan.summary_spec()),
            crate::runtime::combat::Intent::StrongDebuff
        );
    }

    #[test]
    fn mega_debuff_execution_marks_runtime_state_before_parasite() {
        let mut state = crate::test_support::combat_with_monsters(vec![]);
        let mut mass = crate::test_support::test_monster(EnemyId::WrithingMass);
        mass.id = 42;

        let actions = <WrithingMass as MonsterBehavior>::take_turn_plan(
            &mut state,
            &mass,
            &mega_debuff_plan(),
        );

        assert_eq!(
            actions,
            vec![
                Action::UpdateMonsterRuntime {
                    monster_id: 42,
                    patch: MonsterRuntimePatch::WrithingMass {
                        first_move: None,
                        used_mega_debuff: Some(true),
                        protocol_seeded: Some(true),
                    },
                },
                Action::AddCardToMasterDeck {
                    card_id: CardId::Parasite,
                    source: 42,
                },
                Action::RollMonsterMove { monster_id: 42 },
            ],
            "Java sets usedMegaDebuff=true inside takeTurn before queuing AddCardToDeckAction"
        );
    }

    #[test]
    fn first_move_uses_runtime_flag_not_empty_history() {
        let mut mass = crate::test_support::test_monster(EnemyId::WrithingMass);
        mass.writhing_mass.first_move = false;
        mass.move_history_mut().clear();

        let plan =
            <WrithingMass as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &mass, 20, 5);

        assert_eq!(
            plan.move_id, BIG_HIT,
            "post-opening Writhing Mass must use Java firstMove, not empty history"
        );
    }

    #[test]
    fn opening_roll_clears_first_move_runtime_flag() {
        let mass = crate::test_support::test_monster(EnemyId::WrithingMass);
        let plan =
            <WrithingMass as MonsterBehavior>::roll_move_plan(&mut StsRng::new(0), &mass, 20, 50);

        let actions = WrithingMass::on_roll_move(20, &mass, 50, &plan);

        assert!(matches!(
            actions.as_slice(),
            [Action::UpdateMonsterRuntime {
                patch: MonsterRuntimePatch::WrithingMass {
                    first_move: Some(false),
                    used_mega_debuff: None,
                    protocol_seeded: Some(true),
                },
                ..
            }]
        ));
    }

    #[test]
    fn pre_battle_powers_use_java_amounts() {
        let mut state = crate::test_support::combat_with_monsters(vec![]);
        let mut mass = crate::test_support::test_monster(EnemyId::WrithingMass);
        mass.id = 42;

        let actions = <WrithingMass as MonsterBehavior>::use_pre_battle_actions(
            &mut state,
            &mass,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );

        assert_eq!(
            actions,
            vec![
                Action::ApplyPower {
                    source: 42,
                    target: 42,
                    power_id: PowerId::Reactive,
                    amount: -1,
                },
                Action::ApplyPower {
                    source: 42,
                    target: 42,
                    power_id: PowerId::Malleable,
                    amount: 3,
                },
            ],
            "Java ReactivePower inherits AbstractPower.amount == -1; MalleablePower(this) starts at 3"
        );
    }
}
