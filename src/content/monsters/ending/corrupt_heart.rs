use crate::content::cards::CardId;
use crate::content::monsters::exordium::{
    add_card_action, apply_power_action, attack_actions, PLAYER,
};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, ApplyPowerStep, AttackSpec, BuffSpec, DamageKind, DebuffSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind,
};
use smallvec::smallvec;

pub struct CorruptHeart;

const BLOOD_SHOTS: u8 = 1;
const ECHO_ATTACK: u8 = 2;
const DEBILITATE: u8 = 3;
const GAIN_ONE_STRENGTH: u8 = 4;
const DEBUFF_AMOUNT: i32 = 2;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::{EnemyId, PreBattleLegacyRng};
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
    fn pre_battle_invincible_and_beat_of_death_use_java_asc19_gate() {
        let mut heart = crate::test_support::test_monster(EnemyId::CorruptHeart);
        heart.id = 1;
        let mut low = crate::test_support::combat_with_monsters(vec![heart.clone()]);
        low.meta.ascension_level = 18;

        let low_actions =
            CorruptHeart::use_pre_battle_actions(&mut low, &heart, PreBattleLegacyRng::Misc);

        assert_eq!(
            low_actions,
            vec![
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Invincible,
                    amount: 300,
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::BeatOfDeath,
                    amount: 1,
                },
            ]
        );

        let mut high = crate::test_support::combat_with_monsters(vec![heart.clone()]);
        high.meta.ascension_level = 19;

        let high_actions =
            CorruptHeart::use_pre_battle_actions(&mut high, &heart, PreBattleLegacyRng::Misc);

        assert_eq!(
            high_actions,
            vec![
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Invincible,
                    amount: 200,
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::BeatOfDeath,
                    amount: 2,
                },
            ]
        );
    }

    #[test]
    fn first_roll_sets_debilitate_and_only_clears_first_move_like_java() {
        let mut heart = crate::test_support::test_monster(EnemyId::CorruptHeart);
        heart.id = 1;
        heart.corrupt_heart.first_move = true;
        heart.corrupt_heart.move_count = 7;

        let plan =
            CorruptHeart::roll_move_plan(&mut crate::runtime::rng::StsRng::new(0), &heart, 20, 0);
        let actions = CorruptHeart::on_roll_move(20, &heart, 0, &plan);

        assert_eq!(plan.move_id, DEBILITATE);
        assert_eq!(
            actions,
            vec![Action::UpdateMonsterRuntime {
                monster_id: 1,
                patch: MonsterRuntimePatch::CorruptHeart {
                    first_move: Some(false),
                    move_count: None,
                    buff_count: None,
                    blood_hit_count: None,
                    protocol_seeded: Some(true),
                },
            }]
        );
    }

    #[test]
    fn painful_stabs_followup_uses_java_sentinel_amount() {
        let mut heart = crate::test_support::test_monster(EnemyId::CorruptHeart);
        heart.id = 1;
        heart.corrupt_heart.buff_count = 2;
        let plan = buff_plan(&heart, 2);
        let mut state = crate::test_support::combat_with_monsters(vec![heart.clone()]);

        let actions = CorruptHeart::take_turn_plan(&mut state, &heart, &plan);

        assert!(actions.iter().any(|action| matches!(
            action,
            Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::PainfulStabs,
                amount: -1,
            }
        )));
    }

    #[test]
    fn buff_turn_updates_private_buff_count_before_queued_powers_execute() {
        let mut heart = crate::test_support::test_monster(EnemyId::CorruptHeart);
        heart.id = 1;
        heart.corrupt_heart.buff_count = 1;
        heart.set_planned_move_id(GAIN_ONE_STRENGTH);
        let mut state = crate::test_support::combat_with_monsters(vec![heart.clone()]);
        crate::content::powers::store::set_powers_for(
            &mut state,
            1,
            vec![power(PowerId::Strength, -3)],
        );

        let plan = CorruptHeart::turn_plan(&state, &heart);
        let actions = CorruptHeart::take_turn_plan(&mut state, &heart, &plan);

        assert_eq!(
            actions,
            vec![
                Action::UpdateMonsterRuntime {
                    monster_id: 1,
                    patch: MonsterRuntimePatch::CorruptHeart {
                        first_move: None,
                        move_count: None,
                        buff_count: Some(2),
                        blood_hit_count: None,
                        protocol_seeded: Some(true),
                    },
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::Strength,
                    amount: 5,
                },
                Action::ApplyPower {
                    source: 1,
                    target: 1,
                    power_id: PowerId::BeatOfDeath,
                    amount: 1,
                },
                Action::RollMonsterMove { monster_id: 1 },
            ]
        );
    }
}

fn echo_attack_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 4 {
        45
    } else {
        40
    }
}

fn initial_blood_hit_count(ascension_level: u8) -> u8 {
    if ascension_level >= 4 {
        15
    } else {
        12
    }
}

pub fn initialize_runtime_state(entity: &mut MonsterEntity, ascension_level: u8) {
    entity.corrupt_heart.protocol_seeded = true;
    entity.corrupt_heart.first_move = true;
    entity.corrupt_heart.move_count = 0;
    entity.corrupt_heart.buff_count = 0;
    entity.corrupt_heart.blood_hit_count = initial_blood_hit_count(ascension_level);
}

fn current_runtime(entity: &MonsterEntity) -> (bool, u8, u8, u8) {
    assert!(
        entity.corrupt_heart.protocol_seeded,
        "CorruptHeart runtime must be factory/protocol seeded before semantic use"
    );
    (
        entity.corrupt_heart.first_move,
        entity.corrupt_heart.move_count,
        entity.corrupt_heart.buff_count,
        entity.corrupt_heart.blood_hit_count,
    )
}

fn corrupt_heart_runtime_update(
    entity: &MonsterEntity,
    first_move: Option<bool>,
    move_count: Option<u8>,
    buff_count: Option<u8>,
    blood_hit_count: Option<u8>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::CorruptHeart {
            first_move,
            move_count,
            buff_count,
            blood_hit_count,
            protocol_seeded: Some(true),
        },
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn debilitate_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        DEBILITATE,
        smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Vulnerable,
                amount: DEBUFF_AMOUNT,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Strong,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Weak,
                amount: DEBUFF_AMOUNT,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Strong,
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::Player,
                power_id: PowerId::Frail,
                amount: DEBUFF_AMOUNT,
                effect: PowerEffectKind::Debuff,
                visible_strength: EffectStrength::Strong,
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Dazed,
                amount: 1,
                upgraded: false,
                destination: crate::semantics::combat::CardDestination::DrawPileRandom,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Slimed,
                amount: 1,
                upgraded: false,
                destination: crate::semantics::combat::CardDestination::DrawPileRandom,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Wound,
                amount: 1,
                upgraded: false,
                destination: crate::semantics::combat::CardDestination::DrawPileRandom,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Burn,
                amount: 1,
                upgraded: false,
                destination: crate::semantics::combat::CardDestination::DrawPileRandom,
                visible_strength: EffectStrength::Normal,
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Void,
                amount: 1,
                upgraded: false,
                destination: crate::semantics::combat::CardDestination::DrawPileRandom,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::StrongDebuff(DebuffSpec {
            power_id: PowerId::Vulnerable,
            amount: DEBUFF_AMOUNT,
            strength: EffectStrength::Strong,
        }),
    )
}

fn blood_shots_plan(hit_count: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        BLOOD_SHOTS,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 2,
            hits: hit_count,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn echo_attack_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        ECHO_ATTACK,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: echo_attack_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn strength_cleanse_bonus(state: &CombatState, entity: &MonsterEntity) -> i32 {
    (-state.get_power(entity.id, PowerId::Strength)).max(0)
}

fn buff_followup_step(buff_count: u8) -> MoveStep {
    match buff_count {
        0 => MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Artifact,
            amount: 2,
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
        1 => MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::BeatOfDeath,
            amount: 1,
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
        2 => MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::PainfulStabs,
            amount: -1,
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
        3 => MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Strength,
            amount: 10,
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
        _ => MoveStep::ApplyPower(ApplyPowerStep {
            target: MoveTarget::SelfTarget,
            power_id: PowerId::Strength,
            amount: 50,
            effect: PowerEffectKind::Buff,
            visible_strength: EffectStrength::Normal,
        }),
    }
}

fn buff_plan(entity: &MonsterEntity, strength_amount: i32) -> MonsterTurnPlan {
    let (_, _, buff_count, _) = current_runtime(entity);
    MonsterTurnPlan::with_visible_spec(
        GAIN_ONE_STRENGTH,
        smallvec![
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Strength,
                amount: strength_amount,
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            }),
            buff_followup_step(buff_count),
        ],
        MonsterMoveSpec::Buff(BuffSpec {
            power_id: PowerId::Strength,
            amount: strength_amount,
        }),
    )
}

fn roll_buff_plan(entity: &MonsterEntity) -> MonsterTurnPlan {
    buff_plan(entity, 2)
}

fn turn_buff_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
    buff_plan(entity, strength_cleanse_bonus(state, entity) + 2)
}

fn plan_for(state: &CombatState, entity: &MonsterEntity, move_id: u8) -> MonsterTurnPlan {
    match move_id {
        DEBILITATE => debilitate_plan(),
        BLOOD_SHOTS => {
            let (_, _, _, blood_hit_count) = current_runtime(entity);
            blood_shots_plan(blood_hit_count)
        }
        ECHO_ATTACK => echo_attack_plan(state.meta.ascension_level),
        GAIN_ONE_STRENGTH => turn_buff_plan(state, entity),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(asc: u8) -> super::super::mechanics::MonsterMechanics {
    let followups = (0..=4)
        .map(|count| {
            let MoveStep::ApplyPower(step) = buff_followup_step(count) else {
                unreachable!("Heart Buff followup must apply a power");
            };
            let name = crate::content::powers::get_power_definition(step.power_id).name;
            let effect = if step.power_id == PowerId::PainfulStabs {
                format!("gain {name}")
            } else {
                format!("gain {} {name}", step.amount)
            };
            let later = if count == 4 {
                " and every later Buff"
            } else {
                ""
            };
            format!("Buff {}{later}: {effect}", count + 1)
        })
        .collect::<Vec<_>>()
        .join("; ");
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            ("Debilitate", debilitate_plan()),
            ("Blood Shots", blood_shots_plan(initial_blood_hit_count(asc))),
            ("Echo", echo_attack_plan(asc)),
        ],
        special_moves: vec![("Buff", format!("Every Buff removes negative Strength by restoring it to 0, then gains 2 Strength. Additional effects from the first Buff onward: {followups}."))],
        notes: "Opens with Debilitate, then repeats two attacks (one Blood Shots and one Echo, random order) followed by Buff. Starting power amounts above use this encounter's Ascension. Invincible resets its damage allowance at the Heart's turn start. Beat of Death damages the player for every counted card after that card's own effects but before its final exhaust triggers Feel No Pain.",
    }
}

pub fn starting_powers(ascension_level: u8) -> [(PowerId, i32); 2] {
    [
        (
            PowerId::Invincible,
            if ascension_level >= 19 { 200 } else { 300 },
        ),
        (
            PowerId::BeatOfDeath,
            if ascension_level >= 19 { 2 } else { 1 },
        ),
    ]
}

impl MonsterBehavior for CorruptHeart {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        starting_powers(ascension_level)
            .into_iter()
            .map(|(power_id, amount)| Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id,
                amount,
            })
            .collect()
    }

    fn roll_move_plan(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        let (first_move, move_count, _, blood_hit_count) = current_runtime(entity);
        if first_move {
            return debilitate_plan();
        }

        match move_count % 3 {
            0 => {
                if rng.random_boolean() {
                    blood_shots_plan(blood_hit_count)
                } else {
                    echo_attack_plan(ascension_level)
                }
            }
            1 => {
                if !last_move(entity, ECHO_ATTACK) {
                    echo_attack_plan(ascension_level)
                } else {
                    blood_shots_plan(blood_hit_count)
                }
            }
            _ => roll_buff_plan(entity),
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let (first_move, move_count, buff_count, _) = current_runtime(entity);
        vec![if first_move {
            corrupt_heart_runtime_update(entity, Some(false), None, None, None)
        } else {
            corrupt_heart_runtime_update(
                entity,
                None,
                Some(move_count.saturating_add(1)),
                Some(buff_count),
                None,
            )
        }]
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(state, entity, entity.planned_move_id())
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (
                DEBILITATE,
                [MoveStep::ApplyPower(vulnerable), MoveStep::ApplyPower(weak), MoveStep::ApplyPower(frail), MoveStep::AddCard(dazed), MoveStep::AddCard(slimed), MoveStep::AddCard(wound), MoveStep::AddCard(burn), MoveStep::AddCard(void)],
            ) => vec![
                apply_power_action(entity, vulnerable),
                apply_power_action(entity, weak),
                apply_power_action(entity, frail),
                add_card_action(dazed),
                add_card_action(slimed),
                add_card_action(wound),
                add_card_action(burn),
                add_card_action(void),
            ],
            (
                BLOOD_SHOTS | ECHO_ATTACK,
                [MoveStep::Attack(crate::semantics::combat::AttackStep {
                    target: MoveTarget::Player,
                    attack,
                })],
            ) => attack_actions(entity.id, PLAYER, attack),
            (
                GAIN_ONE_STRENGTH,
                [MoveStep::ApplyPower(strength_step), MoveStep::ApplyPower(followup_step)],
            ) => {
                let (_, _, buff_count, _) = current_runtime(entity);
                vec![
                    corrupt_heart_runtime_update(
                        entity,
                        None,
                        None,
                        Some(buff_count.saturating_add(1)),
                        None,
                    ),
                    apply_power_action(entity, strength_step),
                    apply_power_action(entity, followup_step),
                ]
            }
            (_, []) => panic!("corrupt heart plan missing locked truth: {}", plan.move_id),
            (move_id, steps) => {
                panic!("corrupt heart plan/steps mismatch: {} {:?}", move_id, steps)
            }
        };

        actions.push(Action::RollMonsterMove {
            monster_id: entity.id,
        });
        actions
    }

    fn on_death(_state: &mut CombatState, _entity: &MonsterEntity) -> Vec<Action> {
        Vec::new()
    }
}
