use crate::content::monsters::exordium::{attack_actions, PLAYER};
use crate::content::monsters::MonsterBehavior;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, MonsterRuntimePatch};
use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::semantics::combat::{
    ApplyPowerStep, AttackSpec, BlockStep, BuffSpec, DamageKind, DefendSpec, EffectStrength,
    MonsterMoveSpec, MonsterTurnPlan, MoveStep, MoveTarget, PowerEffectKind, SpawnHpSpec,
    SpawnHpValue,
};
use smallvec::smallvec;

pub struct BronzeAutomaton;

/// Exact, encounter-owned cadence facts for one public player-turn boundary.
///
/// This projection translates Bronze Automaton's deterministic move protocol;
/// it does not choose a target, card, or strategy. `player_turns_until_beam`
/// counts future player decisions after the current one, so zero means Hyper
/// Beam is the currently visible intent. A positive value is the number of
/// end-turn transitions before Hyper Beam becomes the visible intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BronzeAutomatonCadenceV1 {
    pub player_turns_until_beam: Option<u8>,
    pub recovery_window: bool,
}

const FLAIL: u8 = 1;
const HYPER_BEAM: u8 = 2;
const STUNNED: u8 = 3;
const SPAWN_ORBS: u8 = 4;
const BOOST: u8 = 5;
const LEFT_ORB_DRAW_X: i32 = -300;
const RIGHT_ORB_DRAW_X: i32 = 200;
pub const STARTING_ARTIFACT: i32 = 3;

pub fn bronze_automaton_cadence_v1(
    combat: &CombatState,
    entity: &MonsterEntity,
) -> Option<BronzeAutomatonCadenceV1> {
    if !entity.bronze_automaton.protocol_seeded {
        return None;
    }
    if entity.planned_move_id() == HYPER_BEAM {
        return Some(BronzeAutomatonCadenceV1 {
            player_turns_until_beam: Some(0),
            recovery_window: false,
        });
    }
    // `MonsterEntity::move_history` records newly rolled plans, so at the
    // A19+ post-Beam boundary its tail is already BOOST. Recovery is a fact
    // about the move that actually resolved, which the combat protocol keeps
    // separately from the currently visible intent.
    if entity.planned_move_id() == STUNNED
        || combat
            .monster_protocol_executed_move_history(entity.id)
            .last()
            .copied()
            == Some(HYPER_BEAM)
    {
        return Some(BronzeAutomatonCadenceV1 {
            player_turns_until_beam: None,
            recovery_window: true,
        });
    }
    Some(BronzeAutomatonCadenceV1 {
        player_turns_until_beam: Some(
            5_u8.saturating_sub(entity.bronze_automaton.num_turns.min(4)),
        ),
        recovery_window: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::Intent;

    #[test]
    fn spawn_orbs_uses_the_original_unknown_intent() {
        let plan = spawn_orbs_plan();

        assert!(plan.steps.is_empty());
        assert_eq!(plan.summary_spec(), MonsterMoveSpec::Unknown);
        assert_eq!(
            Intent::from_visible_move_spec(&plan.summary_spec()),
            Intent::Unknown
        );
    }

    #[test]
    fn cadence_projects_countdown_beam_and_recovery_without_replaying_moves() {
        let mut automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        automaton.bronze_automaton.protocol_seeded = true;
        automaton.bronze_automaton.first_turn = false;
        automaton.bronze_automaton.num_turns = 3;
        automaton.set_planned_move_id(FLAIL);
        assert_eq!(
            bronze_automaton_cadence_v1(&crate::test_support::blank_test_combat(), &automaton),
            Some(BronzeAutomatonCadenceV1 {
                player_turns_until_beam: Some(2),
                recovery_window: false,
            })
        );

        automaton.set_planned_move_id(HYPER_BEAM);
        assert_eq!(
            bronze_automaton_cadence_v1(&crate::test_support::blank_test_combat(), &automaton)
                .expect("beam cadence")
                .player_turns_until_beam,
            Some(0)
        );

        automaton.set_planned_move_id(STUNNED);
        assert!(
            bronze_automaton_cadence_v1(&crate::test_support::blank_test_combat(), &automaton)
                .expect("recovery cadence")
                .recovery_window
        );
    }

    #[test]
    fn cadence_follows_exact_spawn_to_beam_move_progression_without_zero_aliasing() {
        let mut state = crate::test_support::blank_test_combat();
        let mut automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        automaton.bronze_automaton.protocol_seeded = true;
        automaton.bronze_automaton.first_turn = false;
        automaton.bronze_automaton.num_turns = 0;
        automaton.set_planned_move_id(SPAWN_ORBS);
        automaton.move_history_mut().push_back(SPAWN_ORBS);
        state.entities.monsters = vec![automaton];

        let mut observed = Vec::new();
        for _ in 0..6 {
            let automaton = &state.entities.monsters[0];
            observed.push((
                automaton.planned_move_id(),
                bronze_automaton_cadence_v1(&state, automaton)
                    .expect("seeded Bronze Automaton cadence")
                    .player_turns_until_beam,
            ));
            crate::engine::action_handlers::execute_action(
                Action::RollMonsterMove {
                    monster_id: automaton.id,
                },
                &mut state,
            );
        }

        assert_eq!(
            observed,
            vec![
                (SPAWN_ORBS, Some(5)),
                (FLAIL, Some(4)),
                (BOOST, Some(3)),
                (FLAIL, Some(2)),
                (BOOST, Some(1)),
                (HYPER_BEAM, Some(0)),
            ],
            "only a currently visible Hyper Beam may project a zero countdown"
        );
    }

    #[test]
    fn a19_post_beam_boost_is_the_public_recovery_window() {
        let mut state = crate::test_support::blank_test_combat();
        state.meta.ascension_level = 20;
        let mut automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        automaton.id = 10;
        automaton.bronze_automaton.protocol_seeded = true;
        automaton.bronze_automaton.first_turn = false;
        automaton.bronze_automaton.num_turns = 0;
        automaton.set_planned_move_id(BOOST);
        automaton.move_history_mut().push_back(BOOST);
        state.entities.monsters = vec![automaton];
        state.record_monster_protocol_executed_move(10, HYPER_BEAM);

        assert_eq!(
            bronze_automaton_cadence_v1(&state, &state.entities.monsters[0]),
            Some(BronzeAutomatonCadenceV1 {
                player_turns_until_beam: None,
                recovery_window: true,
            }),
            "A19+ replaces Stunned with Boost, but that player turn is still the post-Beam recovery window"
        );
    }

    #[test]
    fn spawn_orbs_uses_java_absolute_draw_positions() {
        let mut state = crate::test_support::blank_test_combat();
        let automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        let actions = BronzeAutomaton::take_turn_plan(&mut state, &automaton, &spawn_orbs_plan());

        assert!(matches!(
            actions.as_slice(),
            [
                Action::SpawnMonsterSmart {
                    monster_id: EnemyId::BronzeOrb,
                    logical_position: LEFT_ORB_DRAW_X,
                    protocol_draw_x: Some(LEFT_ORB_DRAW_X),
                    ..
                },
                Action::SpawnMonsterSmart {
                    monster_id: EnemyId::BronzeOrb,
                    logical_position: RIGHT_ORB_DRAW_X,
                    protocol_draw_x: Some(RIGHT_ORB_DRAW_X),
                    ..
                },
                Action::RollMonsterMove { monster_id: 1 }
            ]
        ));
    }

    #[test]
    fn spawn_constructs_both_orbs_before_rolling_their_first_moves() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};

        for ascension in [0, 8, 9, 20] {
            let automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
            let mut state = crate::test_support::combat_with_monsters(vec![automaton.clone()]);
            state.meta.ascension_level = ascension;
            let trace = begin_sts_rng_trace().unwrap();
            let actions =
                BronzeAutomaton::take_turn_plan(&mut state, &automaton, &spawn_orbs_plan());
            let constructed = trace.finish().unwrap();
            assert_eq!(constructed.len(), 4);
            for (index, call) in constructed.iter().enumerate() {
                assert_eq!(call.owner, Some("floor.monster_hp"));
                let expected = if index % 2 == 0 || ascension < 9 {
                    (52, 58)
                } else {
                    (54, 60)
                };
                assert!(
                    matches!(call.call, StsRngCall::RandomIntRange { start, end, .. }
                    if (start, end) == expected)
                );
            }
            let trace = begin_sts_rng_trace().unwrap();
            for action in actions {
                state.queue_action_back(action);
            }
            while let Some(action) = state.pop_next_action() {
                crate::engine::action_handlers::execute_action(action, &mut state);
            }
            let initialized = trace.finish().unwrap();
            assert_eq!(initialized.len(), 3);
            assert!(initialized.iter().all(|call| call.owner == Some("floor.ai")
                && matches!(call.call, StsRngCall::RandomInt { range: 99, .. })));
            let orbs: Vec<_> = state
                .entities
                .monsters
                .iter()
                .filter(|monster| monster.enemy_id == EnemyId::BronzeOrb)
                .collect();
            assert_eq!(orbs.len(), 2);
            for (index, orb) in orbs.into_iter().enumerate() {
                let StsRngCall::RandomIntRange { result, .. } = constructed[index * 2 + 1].call
                else {
                    unreachable!()
                };
                assert_eq!((orb.current_hp, orb.max_hp), (result, result));
                assert_eq!(
                    orb.logical_position,
                    [LEFT_ORB_DRAW_X, RIGHT_ORB_DRAW_X][index]
                );
                assert_eq!(orb.move_history().len(), 1);
            }
        }
    }

    #[test]
    fn death_cleanup_suicides_zero_hp_or_escaped_non_dying_minions_like_java() {
        let automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        let mut orb = crate::test_support::test_monster(EnemyId::BronzeOrb);
        orb.id = 2;
        orb.current_hp = 0;
        orb.is_dying = false;
        orb.is_escaped = true;
        let mut state = crate::test_support::combat_with_monsters(vec![automaton.clone(), orb]);

        let actions = BronzeAutomaton::on_death(&mut state, &automaton);

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
        let automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        let mut first_orb = crate::test_support::test_monster(EnemyId::BronzeOrb);
        first_orb.id = 2;
        let mut second_orb = crate::test_support::test_monster(EnemyId::BronzeOrb);
        second_orb.id = 3;
        let mut state = crate::test_support::combat_with_monsters(vec![
            automaton.clone(),
            first_orb,
            second_orb,
        ]);

        let actions = BronzeAutomaton::on_death(&mut state, &automaton);

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
            "Java BronzeAutomaton.die adds VFX/Suicide/Hide cleanup with addToTop while iterating forward, so later minions' SuicideAction executes first"
        );
    }

    #[test]
    fn roll_move_first_turn_and_hyper_beam_counter_match_java_get_move() {
        let mut automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);

        let first_plan = BronzeAutomaton::roll_move_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &automaton,
            0,
            99,
        );
        assert_eq!(first_plan.move_id, SPAWN_ORBS);
        assert_eq!(
            BronzeAutomaton::on_roll_move(0, &automaton, 99, &first_plan),
            vec![automaton_runtime_update(&automaton, Some(false), Some(0))],
            "Java firstTurn branch clears firstTurn but does not increment numTurns"
        );

        automaton.bronze_automaton.first_turn = false;
        automaton.bronze_automaton.num_turns = 4;
        let beam_plan = BronzeAutomaton::roll_move_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &automaton,
            4,
            0,
        );
        assert_eq!(beam_plan.move_id, HYPER_BEAM);
        assert_eq!(
            BronzeAutomaton::on_roll_move(4, &automaton, 0, &beam_plan),
            vec![automaton_runtime_update(&automaton, Some(false), Some(0))],
            "Java numTurns == 4 branch sets Hyper Beam and immediately resets numTurns to 0"
        );
    }

    #[test]
    fn post_hyper_beam_branch_does_not_increment_counter_like_java() {
        let mut automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        automaton.bronze_automaton.first_turn = false;
        automaton.bronze_automaton.num_turns = 2;
        automaton.move_history_mut().push_back(HYPER_BEAM);

        let low_ascension_plan = BronzeAutomaton::roll_move_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &automaton,
            18,
            0,
        );
        assert_eq!(low_ascension_plan.move_id, STUNNED);
        assert_eq!(
            BronzeAutomaton::on_roll_move(18, &automaton, 0, &low_ascension_plan),
            vec![automaton_runtime_update(&automaton, Some(false), Some(2))]
        );

        let a19_plan = BronzeAutomaton::roll_move_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &automaton,
            19,
            0,
        );
        assert_eq!(a19_plan.move_id, BOOST);
        assert_eq!(
            BronzeAutomaton::on_roll_move(19, &automaton, 0, &a19_plan),
            vec![automaton_runtime_update(&automaton, Some(false), Some(2))],
            "Java lastMove(HYPER_BEAM) returns before ++numTurns on both STUN and A19 Boost branches"
        );
    }

    #[test]
    fn flail_and_normal_boost_increment_hyper_counter_like_java() {
        let mut automaton = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        automaton.bronze_automaton.first_turn = false;
        automaton.bronze_automaton.num_turns = 1;
        automaton.move_history_mut().push_back(SPAWN_ORBS);

        let flail_plan = BronzeAutomaton::roll_move_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &automaton,
            0,
            0,
        );
        assert_eq!(flail_plan.move_id, FLAIL);
        assert_eq!(
            BronzeAutomaton::on_roll_move(0, &automaton, 0, &flail_plan),
            vec![automaton_runtime_update(&automaton, Some(false), Some(2))]
        );

        automaton.move_history_mut().clear();
        let boost_plan = BronzeAutomaton::roll_move_plan(
            &mut crate::runtime::rng::StsRng::new(0),
            &automaton,
            0,
            0,
        );
        assert_eq!(boost_plan.move_id, BOOST);
        assert_eq!(
            BronzeAutomaton::on_roll_move(0, &automaton, 0, &boost_plan),
            vec![automaton_runtime_update(&automaton, Some(false), Some(2))]
        );
    }
}

fn flail_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 4 {
        8
    } else {
        7
    }
}

fn beam_damage(ascension_level: u8) -> i32 {
    if ascension_level >= 4 {
        50
    } else {
        45
    }
}

fn block_amount(ascension_level: u8) -> i32 {
    if ascension_level >= 9 {
        12
    } else {
        9
    }
}

fn strength_amount(ascension_level: u8) -> i32 {
    if ascension_level >= 4 {
        4
    } else {
        3
    }
}

fn current_runtime(entity: &MonsterEntity) -> (bool, u8) {
    assert!(
        entity.bronze_automaton.protocol_seeded,
        "bronze automaton runtime truth must be protocol-seeded or factory-seeded"
    );
    (
        entity.bronze_automaton.first_turn,
        entity.bronze_automaton.num_turns,
    )
}

fn automaton_runtime_update(
    entity: &MonsterEntity,
    first_turn: Option<bool>,
    num_turns: Option<u8>,
) -> Action {
    Action::UpdateMonsterRuntime {
        monster_id: entity.id,
        patch: MonsterRuntimePatch::BronzeAutomaton {
            first_turn,
            num_turns,
            protocol_seeded: Some(true),
        },
    }
}

fn flail_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        FLAIL,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: flail_damage(ascension_level),
            hits: 2,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn hyper_beam_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::from_spec(
        HYPER_BEAM,
        MonsterMoveSpec::Attack(AttackSpec {
            base_damage: beam_damage(ascension_level),
            hits: 1,
            damage_kind: DamageKind::Normal,
        }),
    )
}

fn stunned_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(STUNNED, smallvec![], MonsterMoveSpec::Stun)
}

fn spawn_orbs_plan() -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(SPAWN_ORBS, smallvec![], MonsterMoveSpec::Unknown)
}

fn boost_plan(ascension_level: u8) -> MonsterTurnPlan {
    MonsterTurnPlan::with_visible_spec(
        BOOST,
        smallvec![
            MoveStep::GainBlock(BlockStep {
                target: MoveTarget::SelfTarget,
                amount: block_amount(ascension_level),
            }),
            MoveStep::ApplyPower(ApplyPowerStep {
                target: MoveTarget::SelfTarget,
                power_id: PowerId::Strength,
                amount: strength_amount(ascension_level),
                effect: PowerEffectKind::Buff,
                visible_strength: EffectStrength::Normal,
            }),
        ],
        MonsterMoveSpec::DefendBuff(
            DefendSpec {
                block: block_amount(ascension_level),
            },
            BuffSpec {
                power_id: PowerId::Strength,
                amount: strength_amount(ascension_level),
            },
        ),
    )
}

fn plan_for(move_id: u8, ascension_level: u8) -> MonsterTurnPlan {
    match move_id {
        FLAIL => flail_plan(ascension_level),
        HYPER_BEAM => hyper_beam_plan(ascension_level),
        STUNNED => stunned_plan(),
        SPAWN_ORBS => spawn_orbs_plan(),
        BOOST => boost_plan(ascension_level),
        _ => MonsterTurnPlan::unknown(move_id),
    }
}

pub fn mechanics(ascension: u8) -> super::super::mechanics::MonsterMechanics {
    let mut moves = vec![
        ("Flail", flail_plan(ascension)),
        ("Boost", boost_plan(ascension)),
        ("Hyper Beam", hyper_beam_plan(ascension)),
    ];
    if ascension < 19 {
        moves.push(("Stunned", stunned_plan()));
    }
    super::super::mechanics::MonsterMechanics {
        moves,
        special_moves: vec![("Spawn Orbs", "Summon two Bronze Orbs as Minions. They die when Bronze Automaton dies.".into())],
        notes: "Opens by summoning two Bronze Orbs, then alternates Flail and Boost. The first Hyper Beam occurs on enemy turn 6. Below A19 the next move is Stunned; at A19+ it is Boost, then the attack/buff cycle continues toward another Hyper Beam. Starting Artifact blocks debuff applications individually.",
    }
}

fn last_move(entity: &MonsterEntity, move_id: u8) -> bool {
    entity.move_history().back().copied() == Some(move_id)
}

fn spawn_orb_action(state: &mut CombatState, draw_x: i32) -> Action {
    // takeTurn constructs both BronzeOrbs before either SpawnMonsterAction
    // updates. HP is consumed here; init and its AI roll wait for entry.
    let hp = super::bronze_orb::roll_constructor_hp(
        &mut state.rng.monster_hp_rng,
        state.meta.ascension_level,
    );
    Action::SpawnMonsterSmart {
        monster_id: crate::content::monsters::EnemyId::BronzeOrb,
        logical_position: draw_x,
        hp: SpawnHpSpec {
            current: SpawnHpValue::Fixed(hp),
            max: SpawnHpValue::Fixed(hp),
        },
        protocol_draw_x: Some(draw_x),
        is_minion: true,
    }
}

impl MonsterBehavior for BronzeAutomaton {
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
            power_id: PowerId::Artifact,
            amount: STARTING_ARTIFACT,
        }]
    }

    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        let (first_turn, num_turns) = current_runtime(entity);
        if first_turn {
            return spawn_orbs_plan();
        }
        if num_turns == 4 {
            return hyper_beam_plan(ascension_level);
        }
        if last_move(entity, HYPER_BEAM) {
            if ascension_level >= 19 {
                return boost_plan(ascension_level);
            }
            return stunned_plan();
        }
        if last_move(entity, STUNNED) || last_move(entity, BOOST) || last_move(entity, SPAWN_ORBS) {
            return flail_plan(ascension_level);
        }
        boost_plan(ascension_level)
    }

    fn on_roll_move(
        _ascension_level: u8,
        entity: &MonsterEntity,
        _num: i32,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        let (first_turn, num_turns) = current_runtime(entity);
        let next_first_turn = Some(false);
        let next_num_turns = match plan.move_id {
            HYPER_BEAM => Some(0),
            FLAIL | BOOST if !first_turn && !last_move(entity, HYPER_BEAM) => {
                Some(num_turns.saturating_add(1))
            }
            _ => Some(num_turns),
        };
        vec![automaton_runtime_update(
            entity,
            next_first_turn,
            next_num_turns,
        )]
    }

    fn turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        plan_for(entity.planned_move_id(), state.meta.ascension_level)
    }

    fn take_turn_plan(
        state: &mut CombatState,
        entity: &MonsterEntity,
        plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        match plan.move_id {
            SPAWN_ORBS => {
                vec![
                    spawn_orb_action(state, LEFT_ORB_DRAW_X),
                    spawn_orb_action(state, RIGHT_ORB_DRAW_X),
                    Action::RollMonsterMove {
                        monster_id: entity.id,
                    },
                ]
            }
            FLAIL => {
                let mut actions = attack_actions(
                    entity.id,
                    PLAYER,
                    &AttackSpec {
                        base_damage: flail_damage(state.meta.ascension_level),
                        hits: 2,
                        damage_kind: DamageKind::Normal,
                    },
                );
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
            BOOST => vec![
                Action::GainBlock {
                    target: entity.id,
                    amount: block_amount(state.meta.ascension_level),
                },
                Action::ApplyPower {
                    source: entity.id,
                    target: entity.id,
                    power_id: PowerId::Strength,
                    amount: strength_amount(state.meta.ascension_level),
                },
                Action::RollMonsterMove {
                    monster_id: entity.id,
                },
            ],
            HYPER_BEAM => {
                let mut actions = attack_actions(
                    entity.id,
                    PLAYER,
                    &AttackSpec {
                        base_damage: beam_damage(state.meta.ascension_level),
                        hits: 1,
                        damage_kind: DamageKind::Normal,
                    },
                );
                actions.push(Action::RollMonsterMove {
                    monster_id: entity.id,
                });
                actions
            }
            STUNNED => vec![Action::RollMonsterMove {
                monster_id: entity.id,
            }],
            _ => panic!(
                "bronze automaton take_turn received unsupported move {}",
                plan.move_id
            ),
        }
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
