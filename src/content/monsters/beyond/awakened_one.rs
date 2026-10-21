use crate::content::cards::CardId;
use crate::content::monsters::exordium::{add_card_action, attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    AddCardStep, AttackSpec, AttackStep, CardDestination, DamageKind, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget,
};
use smallvec::smallvec;

pub struct AwakenedOne;

const SLASH: u8 = 1;
const SOUL_STRIKE: u8 = 2;
const REBIRTH: u8 = 3;
const DARK_ECHO: u8 = 5;
const SLUDGE: u8 = 6;
const TACKLE: u8 = 8;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::store;
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
    fn pre_battle_powers_match_java_amounts_and_order() {
        let awakened = crate::test_support::test_monster(EnemyId::AwakenedOne);
        let mut state = crate::test_support::combat_with_monsters(vec![awakened.clone()]);
        state.meta.ascension_level = 19;

        let actions = AwakenedOne::use_pre_battle_actions(
            &mut state,
            &awakened,
            crate::content::monsters::PreBattleLegacyRng::Misc,
        );

        assert_eq!(
            actions,
            vec![
                Action::ApplyPower {
                    source: awakened.id,
                    target: awakened.id,
                    power_id: PowerId::Regen,
                    amount: 15,
                },
                Action::ApplyPower {
                    source: awakened.id,
                    target: awakened.id,
                    power_id: PowerId::Curiosity,
                    amount: 2,
                },
                Action::ApplyPower {
                    source: awakened.id,
                    target: awakened.id,
                    power_id: PowerId::Unawakened,
                    amount: -1,
                },
                Action::ApplyPower {
                    source: awakened.id,
                    target: awakened.id,
                    power_id: PowerId::Strength,
                    amount: 2,
                },
            ],
            "Java queues Regenerate, Curiosity, Unawakened(sentinel -1), then A4+ Strength"
        );
    }

    #[test]
    fn final_death_escapes_zero_hp_or_escaped_cultists_like_java() {
        let awakened = crate::test_support::test_monster(EnemyId::AwakenedOne);
        let mut cultist = crate::test_support::test_monster(EnemyId::Cultist);
        cultist.id = 2;
        cultist.current_hp = 0;
        cultist.is_dying = false;
        cultist.is_escaped = true;
        let mut state = crate::test_support::combat_with_monsters(vec![awakened.clone(), cultist]);

        let actions = AwakenedOne::on_death(&mut state, &awakened);

        assert!(matches!(actions.as_slice(), [Action::Escape { target: 2 }]));
    }

    #[test]
    fn first_phase_death_immediately_sets_rebirth_state_and_queues_java_set_move() {
        let mut awakened = crate::test_support::test_monster(EnemyId::AwakenedOne);
        awakened.id = 7;
        awakened.current_hp = 1;
        awakened.max_hp = 300;
        awakened.set_planned_move_id(SLASH);
        awakened.move_history_mut().push_back(SLASH);
        let mut state = crate::test_support::combat_with_monsters(vec![awakened]);
        // S=10 before Dark Shackles, D=8 => the state immediately before lethal
        // contains Strength(2) plus Shackled(8). The purge removes Shackled and
        // preserves the positive Strength(2).
        store::set_powers_for(
            &mut state,
            7,
            vec![
                power(PowerId::Regen, 10),
                power(PowerId::Curiosity, 1),
                power(PowerId::Unawakened, -1),
                power(PowerId::Shackled, 8),
                power(PowerId::Weak, 2),
                power(PowerId::Strength, 2),
            ],
        );
        state.queue_action_back(Action::GainBlock {
            target: 0,
            amount: 1,
        });

        crate::engine::action_handlers::damage::handle_damage(
            crate::runtime::action::DamageInfo {
                source: PLAYER,
                target: 7,
                base: 1,
                output: 1,
                damage_type: crate::runtime::action::DamageType::Normal,
                is_modified: true,
            },
            &mut state,
        );

        let reborn = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == 7)
            .unwrap();
        assert_eq!(reborn.current_hp, 0);
        assert!(!reborn.is_dying);
        assert!(reborn.half_dead);
        assert!(!reborn.awakened_one.form1);
        assert!(reborn.awakened_one.first_turn);
        assert_eq!(reborn.planned_move_id(), REBIRTH);
        assert_eq!(
            reborn
                .move_history()
                .iter()
                .filter(|&&m| m == REBIRTH)
                .count(),
            1
        );

        let remaining_power_ids = store::powers_for(&state, 7)
            .unwrap()
            .iter()
            .map(|power| power.power_type)
            .collect::<Vec<_>>();
        assert_eq!(
            remaining_power_ids,
            vec![PowerId::Regen, PowerId::Strength],
            "Java removes debuffs, Curiosity, Unawakened, and Shackled immediately during first-phase death"
        );
        assert_eq!(store::power_amount(&state, 7, PowerId::Strength), 2);
        assert!(!store::has_power(&state, 7, PowerId::Shackled));

        assert_eq!(state.pop_next_action(), Some(Action::ClearCardQueue));
        assert_eq!(
            state.pop_next_action(),
            Some(Action::GainBlock {
                target: 0,
                amount: 1,
            }),
            "Java addToTop(ClearCardQueueAction) runs before pre-existing queued bottom actions"
        );
        let set_move = state.pop_next_action().expect("queued SetMoveAction");
        assert!(matches!(
            set_move,
            Action::SetMonsterMove {
                monster_id: 7,
                next_move_byte: REBIRTH,
                ..
            }
        ));
        crate::engine::action_handlers::execute_action(set_move, &mut state);
        let reborn = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == 7)
            .unwrap();
        assert_eq!(
            reborn.move_history().iter().filter(|&&m| m == REBIRTH).count(),
            2,
            "Java damage() calls setMove(REBIRTH) immediately and also queues SetMoveAction(REBIRTH)"
        );
    }

    #[test]
    fn first_phase_purge_removes_negative_strength_when_temporary_loss_exceeds_strength() {
        let mut awakened = crate::test_support::test_monster(EnemyId::AwakenedOne);
        awakened.id = 7;
        awakened.current_hp = 1;
        awakened.max_hp = 300;
        awakened.set_planned_move_id(SLASH);
        let mut state = crate::test_support::combat_with_monsters(vec![awakened]);
        store::set_powers_for(
            &mut state,
            7,
            vec![
                power(PowerId::Regen, 10),
                power(PowerId::Unawakened, -1),
                power(PowerId::Strength, -6),
                power(PowerId::Shackled, 8),
            ],
        );

        crate::engine::action_handlers::damage::handle_damage(
            crate::runtime::action::DamageInfo {
                source: PLAYER,
                target: 7,
                base: 1,
                output: 1,
                damage_type: crate::runtime::action::DamageType::Normal,
                is_modified: true,
            },
            &mut state,
        );

        assert_eq!(store::power_amount(&state, 7, PowerId::Strength), 0);
        assert!(!store::has_power(&state, 7, PowerId::Strength));
        assert!(!store::has_power(&state, 7, PowerId::Shackled));
    }
}

fn current_runtime(entity: &MonsterEntity) -> (bool, bool) {
    assert!(
        entity.awakened_one.protocol_seeded,
        "awakened one runtime truth must be protocol-seeded or factory-seeded"
    );
    (entity.awakened_one.form1, entity.awakened_one.first_turn)
}

fn awakened_one_runtime_update(
    entity: &MonsterEntity,
    form1: Option<bool>,
    first_turn: Option<bool>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::AwakenedOne {
            form1,
            first_turn,
            protocol_seeded: Some(true),
        },
    }
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

fn slash_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SLASH,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 20,
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn soul_strike_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        SOUL_STRIKE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 6,
            hits: 4,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn rebirth_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::unknown(REBIRTH)
}

fn dark_echo_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        DARK_ECHO,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 40,
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn sludge_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        SLUDGE,
        smallvec![
            MoveStep::Attack(AttackStep {
                target: MoveTarget::Player,
                attack: AttackSpec {
                    base_damage: 18,
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                },
            }),
            MoveStep::AddCard(AddCardStep {
                card_id: CardId::Void,
                amount: 1,
                upgraded: false,
                destination: CardDestination::DrawPileRandom,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::AttackAddCard(
            AttackSpec {
                base_damage: 18,
                hits: 1,
                damage_kind: DamageKind::Normal,
            },
            AddCardStep {
                card_id: CardId::Void,
                amount: 1,
                upgraded: false,
                destination: CardDestination::DrawPileRandom,
                visible_strength: EffectStrength::Normal,
            },
        ),
    )
}

fn tackle_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        TACKLE,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 10,
            hits: 3,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn plan_for(move_id: u8) -> MonsterTurnPlan {
    match move_id {
        SLASH => slash_plan(),
        SOUL_STRIKE => soul_strike_plan(),
        REBIRTH => rebirth_plan(),
        DARK_ECHO => dark_echo_plan(),
        SLUDGE => sludge_plan(),
        TACKLE => tackle_plan(),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics() -> super::super::mechanics::MonsterMechanics {
    super::super::mechanics::MonsterMechanics {
        moves: vec![
            ("Slash (phase 1)", slash_plan()),
            ("Soul Strike (phase 1)", soul_strike_plan()),
            ("Dark Echo (phase 2 opening)", dark_echo_plan()),
            ("Sludge (phase 2)", sludge_plan()),
            ("Tackle (phase 2)", tackle_plan()),
        ],
        special_moves: vec![("Rebirth", "revive self and heal to full Max HP; no attack on this enemy turn".into())],
        notes: "Starts with Regenerate 10 (15 at A19+), Curiosity 1 (2 at A19+), Unawakened, and 2 Strength at A4+. Phase 1 opens with Slash; Soul Strike cannot repeat immediately and Slash cannot occur three times consecutively. Each Power played while Curiosity is present grants this enemy permanent Strength equal to Curiosity. First lethal damage immediately removes debuffs, Curiosity, Unawakened and Shackled; positive Strength and Regenerate remain. It becomes untargetable until Rebirth, then opens phase 2 with Dark Echo. Later Sludge and Tackle each cannot occur three times consecutively. Final death makes surviving Cultists escape.",
    }
}

impl MonsterBehavior for AwakenedOne {
    fn use_pre_battle_actions(
        state: &mut CombatState,
        entity: &MonsterEntity,
        legacy_rng: crate::content::monsters::PreBattleLegacyRng,
    ) -> Vec<Action> {
        let (_rng, ascension_level) =
            crate::content::monsters::legacy_pre_battle_rng(state, legacy_rng);
        let regen_amount = if ascension_level >= 19 { 15 } else { 10 };
        let curiosity_amount = if ascension_level >= 19 { 2 } else { 1 };
        let mut actions = vec![
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Regen,
                amount: regen_amount,
            },
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Curiosity,
                amount: curiosity_amount,
            },
            Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Unawakened,
                amount: -1,
            },
        ];
        if ascension_level >= 4 {
            actions.push(Action::ApplyPower {
                source: entity.id,
                target: entity.id,
                power_id: PowerId::Strength,
                amount: 2,
            });
        }
        actions
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        _ascension_level: u8,
        num: i32,
    ) -> MonsterTurnPlan {
        let (form1, first_turn) = current_runtime(entity);
        if form1 {
            if first_turn {
                return slash_plan();
            }
            if num < 25 {
                if !last_move(entity, SOUL_STRIKE) {
                    soul_strike_plan()
                } else {
                    slash_plan()
                }
            } else if !last_two_moves(entity, SLASH) {
                slash_plan()
            } else {
                soul_strike_plan()
            }
        } else if first_turn {
            dark_echo_plan()
        } else if num < 50 {
            if !last_two_moves(entity, SLUDGE) {
                sludge_plan()
            } else {
                tackle_plan()
            }
        } else if !last_two_moves(entity, TACKLE) {
            tackle_plan()
        } else {
            sludge_plan()
        }
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let (form1, first_turn) = current_runtime(entity);
        if form1 && first_turn && plan.move_id == SLASH {
            vec![awakened_one_runtime_update(entity, None, Some(false))]
        } else {
            Vec::new()
        }
    }

    fn turn_plan(_state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id())
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let mut actions = match (plan.move_id, plan.steps.as_slice()) {
            (
                SLASH | SOUL_STRIKE | DARK_ECHO | TACKLE,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                })],
            ) => {
                let mut actions = Vec::new();
                if plan.move_id == DARK_ECHO {
                    actions.push(awakened_one_runtime_update(entity, None, Some(false)));
                }
                actions.extend(attack_actions(entity.id, PLAYER, attack));
                actions
            }
            (
                SLUDGE,
                [MoveStep::Attack(AttackStep {
                    target: MoveTarget::Player,
                    attack,
                }), MoveStep::AddCard(add_card)],
            ) => {
                let mut actions = attack_actions(entity.id, PLAYER, attack);
                actions.push(add_card_action(add_card));
                actions
            }
            (REBIRTH, []) => vec![
                Action::ReviveMonster { target: entity.id },
                Action::Heal {
                    target: entity.id,
                    amount: entity.max_hp,
                },
            ],
            (_, []) => panic!("awakened one plan missing locked truth: {}", plan.move_id),
            (move_id, steps) => panic!("awakened one plan/steps mismatch: {} {:?}", move_id, steps),
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
            .filter(|monster| {
                monster.id != entity.id
                    && !monster.is_dying
                    && monster.enemy_id == crate::content::monsters::EnemyId::Cultist
            })
            .map(|monster| Action::Escape { target: monster.id })
            .collect()
    }
}
