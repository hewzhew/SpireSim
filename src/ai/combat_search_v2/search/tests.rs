use super::*;
use crate::ai::combat_search_v2::{
    COMBAT_SEARCH_V2_REPORT_SCHEMA_NAME, COMBAT_SEARCH_V2_REPORT_SCHEMA_VERSION,
};
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::content::relics::{RelicId, RelicState};
use crate::runtime::combat::{CombatCard, Power, PowerPayload};
use crate::test_support::{blank_test_combat, test_monster};

#[derive(Clone, Copy)]
struct PotionWinStepper;

#[derive(Clone, Copy)]
struct TwoPotionWinStepper;

#[test]
fn production_search_exposes_both_exact_lethal_potion_reserves() {
    use crate::content::potions::{Potion, PotionId};
    use sts_combat_outcome::combat_witness_materialization::materialize_combat_witness_outcome_v1;

    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::LouseNormal);
    monster.current_hp = 5;
    monster.louse.bite_damage = Some(7);
    monster.set_planned_move_id(3);
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 101)),
        Some(Potion::new(PotionId::ExplosivePotion, 102)),
    ];
    let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
    let config = CombatSearchV2Config {
        max_nodes: 20,
        max_actions_per_line: 1,
        satisfaction: CombatSearchV2Satisfaction::BudgetOrExhaustion,
        potion_policy: CombatSearchV2PotionPolicy::All,
        allow_potion_discard: Some(false),
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };
    let report = run_combat_search_v2(&root.engine, &root.combat, config);
    let outcomes = report
        .win_candidate_trajectories
        .iter()
        .map(|candidate| {
            assert_eq!(candidate.actions.len(), 1);
            assert_eq!(candidate.terminal, SearchTerminalLabel::Win);
            materialize_combat_witness_outcome_v1(
                &root,
                &candidate
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect::<Vec<_>>(),
                1_000,
            )
            .expect("every published choice must still replay to a complete victory")
        })
        .collect::<Vec<_>>();
    let mut spent_slots = outcomes
        .iter()
        .map(|outcome| {
            assert_eq!(outcome.final_hp, root.combat.entities.player.current_hp);
            assert_eq!(outcome.total_potion_expenditures, 1);
            outcome.starting_potion_expenditure_slots
        })
        .collect::<Vec<_>>();
    spent_slots.sort();
    assert_eq!(
        spent_slots,
        [1, 2],
        "both potion identities must reach the consumer frontier"
    );
    assert!(!outcomes[0].strictly_dominates(&outcomes[1]));
    assert!(!outcomes[1].strictly_dominates(&outcomes[0]));
}

#[test]
fn production_search_factors_scry_without_materializing_the_power_set() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.draw_pile = ((0..13)
        .map(|index| CombatCard::for_scenario(CardId::Strike, 1_000 + index))
        .collect::<Vec<_>>())
    .into();
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
        cards: vec![CardId::Strike; 13],
        card_lineages: crate::test_support::scenario_lineages(1_000..1_013),
    });
    let config = CombatSearchV2Config {
        max_nodes: 8,
        max_actions_per_line: 1,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };

    let report = run_combat_search_v2(&engine, &combat, config);

    assert!(report.stats.action_prefix_budget_hit);
    assert_eq!(
        report.outcome.coverage_status,
        SearchCoverageStatus::ActionPrefixBudgetLimited
    );
    assert_eq!(report.performance.pending_choice_prefixes_expanded, 8);
    assert_eq!(report.performance.engine_step_calls, 0);
    assert!(report.frontier.remaining_work_items > 0);
    assert_eq!(report.frontier.remaining_work_items, 1);
    assert_eq!(report.frontier.pending_choice_work_items, 1);
    assert!(report.best_frontier_trajectory.is_some());
}

#[test]
fn production_search_steps_only_completed_scry_prefixes() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.draw_pile = ((0..3)
        .map(|index| CombatCard::for_scenario(CardId::Strike, 2_000 + index))
        .collect::<Vec<_>>())
    .into();
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
        cards: vec![CardId::Strike; 3],
        card_lineages: crate::test_support::scenario_lineages(2_000..2_003),
    });
    let config = CombatSearchV2Config {
        max_nodes: 100,
        max_actions_per_line: 1,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };

    let report = run_combat_search_v2(&engine, &combat, config);

    assert_eq!(
        report.performance.pending_choice_complete_actions_submitted,
        8
    );
    assert_eq!(report.performance.engine_step_calls, 8);
    assert_eq!(report.stats.nodes_generated, 8);
    assert!(!report.stats.action_prefix_budget_hit);
    assert!(report.stats.action_surface_incomplete);
    assert_eq!(
        report.final_root_evidence.closure_status,
        CombatSearchV2RootClosureStatus::NotProven
    );
    assert!(report
        .final_root_evidence
        .closure_blockers
        .contains(&CombatSearchV2RootClosureBlocker::PendingChoiceOrderedVariantsOmitted));
    assert!(!report.outcome.exhaustive);
    assert_eq!(
        report.outcome.coverage_status,
        SearchCoverageStatus::ActionSurfaceIncomplete
    );
}

#[test]
fn production_search_covers_canonical_hand_choices_and_cancel() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = (0..10)
        .map(|index| CombatCard::for_scenario(CardId::Strike, 3_000 + index))
        .collect();
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages(3_000..3_010),
        min_cards: 1,
        max_cards: 1,
        can_cancel: true,
        reason: crate::state::core::HandSelectReason::Discard,
    });
    let config = CombatSearchV2Config {
        max_nodes: 100,
        max_actions_per_line: 1,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };

    let report = run_combat_search_v2(&engine, &combat, config);

    assert_eq!(
        report.performance.pending_choice_complete_actions_submitted, 11,
        "ten canonical single-card selections plus the independent Cancel leaf must be reachable"
    );
    assert_eq!(report.performance.engine_step_calls, 11);
    assert_eq!(
        report.search_policy.pending_choice_action_surface,
        "canonical_member_set_prefix_with_explicit_order_variant_gap_v2"
    );
}

#[test]
fn production_search_submits_cancel_before_a_deep_selection_residual() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = (0..10)
        .map(|index| CombatCard::for_scenario(CardId::Strike, 4_000 + index))
        .collect();
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages(4_000..4_010),
        min_cards: 0,
        max_cards: 10,
        can_cancel: true,
        reason: crate::state::core::HandSelectReason::GamblingChip,
    });
    let config = CombatSearchV2Config {
        max_nodes: 1,
        max_actions_per_line: 1,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };

    let report = run_combat_search_v2(&engine, &combat, config);

    assert!(report.stats.node_budget_hit || report.stats.action_prefix_budget_hit);
    assert_eq!(report.performance.pending_choice_prefixes_expanded, 1);
    assert_eq!(
        report.performance.pending_choice_complete_actions_submitted,
        1
    );
    assert_eq!(report.performance.engine_step_calls, 1);
    assert!(report.frontier.remaining_work_items > 0);
}

#[test]
fn production_search_records_one_unresolved_parent_for_an_infeasible_selection() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Strike, 5_100)].into();
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::GridSelect {
        source_pile: crate::state::core::PileType::Discard,
        candidate_lineages: crate::test_support::scenario_lineages([5_100]),
        min_cards: 2,
        max_cards: 2,
        can_cancel: false,
        reason: crate::state::core::GridSelectReason::MoveToDrawPile,
    });

    let report = run_combat_search_v2(
        &engine,
        &combat,
        CombatSearchV2Config {
            max_nodes: 20,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            ..CombatSearchV2Config::default()
        },
    );

    assert_eq!(report.frontier.unresolved_leaf_count, 1);
    assert_eq!(report.performance.engine_step_calls, 0);
    assert!(!report.outcome.exhaustive);
}

#[test]
fn rejected_complete_prefixes_do_not_inflate_concrete_unresolved_leaves() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([6_000, 6_001, 6_002, 6_003]),
        min_cards: 1,
        max_cards: 4,
        can_cancel: false,
        reason: crate::state::core::HandSelectReason::Discard,
    });

    let report = run_combat_search_v2(
        &engine,
        &combat,
        CombatSearchV2Config {
            max_nodes: 100,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            ..CombatSearchV2Config::default()
        },
    );

    assert!(report.performance.pending_choice_complete_prefixes_rejected > 0);
    assert_eq!(report.frontier.unresolved_leaf_count, 1);
    assert_eq!(report.performance.engine_step_calls, 0);
}

#[test]
fn production_search_submits_duplicate_uuid_selection_once() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 5_000)];
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([5_000, 5_000]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: crate::state::core::HandSelectReason::Discard,
    });
    let config = CombatSearchV2Config {
        max_nodes: 20,
        max_actions_per_line: 1,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };

    let report = run_combat_search_v2(&engine, &combat, config);

    assert_eq!(
        report.performance.pending_choice_complete_actions_submitted,
        1
    );
    assert_eq!(report.performance.engine_step_calls, 1);
}

#[test]
fn node_ordering_carries_only_the_best_retaliation_protection_into_frontier() {
    let mut combat = blank_test_combat();
    combat.turn.energy = 4;
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 10),
        CombatCard::for_scenario(CardId::HeavyBlade, 11),
        {
            let mut flame = CombatCard::for_scenario(CardId::FlameBarrier, 12);
            flame.upgrades = 1;
            flame
        },
    ];
    let mut spiker = test_monster(EnemyId::Spiker);
    spiker.id = 1;
    spiker.current_hp = 100;
    spiker.max_hp = 100;
    combat.entities.monsters = vec![spiker];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Thorns,
            instance_id: None,
            amount: 13,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let node = SearchNode::root(EngineState::CombatPlayerTurn, combat.clone());
    let position = CombatPosition::new(node.engine.clone(), combat);
    let legal = EngineCombatStepper.atomic_action_choices(&position);
    let config = CombatSearchV2Config::default();
    let mut loop_state = SearchLoopState::new(
        &config,
        false,
        CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat()),
    );

    let ordered = super::node_action_ordering::order_node_actions(
        &mut loop_state,
        &node,
        legal,
        None,
        &config,
    );

    assert!(matches!(
        ordered.ordered_choices[0].choice.choice.input,
        ClientInput::PlayCard {
            card_index: 2,
            target: None
        }
    ));
    assert_eq!(ordered.ordered_choices[0].action_ordering_frontier_hint, 1);
    assert!(ordered
        .ordered_choices
        .iter()
        .skip(1)
        .all(|choice| choice.action_ordering_frontier_hint == 0));
}

impl CombatStepper for PotionWinStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        vec![
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
            ClientInput::EndTurn,
        ]
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let engine = if matches!(input, ClientInput::UsePotion { .. }) {
            EngineState::GameOver(crate::state::core::RunResult::Victory)
        } else {
            position.engine.clone()
        };
        let position = CombatPosition::new(engine, position.combat.clone());
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

impl CombatStepper for TwoPotionWinStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        vec![
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
            ClientInput::EndTurn,
        ]
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let mut next = position.clone();
        if matches!(input, ClientInput::UsePotion { .. }) {
            if next.combat.entities.player.block == 0 {
                next.combat.entities.player.block = 1;
            } else {
                next.engine = EngineState::GameOver(crate::state::core::RunResult::Victory);
            }
        } else {
            next.combat.turn.turn_count = next.combat.turn.turn_count.saturating_add(1);
        }
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&next.engine, &next.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position: next,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct ReversePotionWinStepper;

impl CombatStepper for ReversePotionWinStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        vec![
            ClientInput::EndTurn,
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        ]
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let engine = if matches!(input, ClientInput::UsePotion { .. }) {
            EngineState::GameOver(crate::state::core::RunResult::Victory)
        } else {
            position.engine.clone()
        };
        let position = CombatPosition::new(engine, position.combat.clone());
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct PendingChoiceResolveStepper;

impl CombatStepper for PendingChoiceResolveStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if matches!(position.engine, EngineState::PendingChoice(_)) {
            vec![
                ClientInput::SubmitDiscoverChoice(0),
                ClientInput::SubmitDiscoverChoice(1),
            ]
        } else {
            Vec::new()
        }
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let engine = if matches!(input, ClientInput::SubmitDiscoverChoice(_)) {
            EngineState::CombatPlayerTurn
        } else {
            position.engine.clone()
        };
        let position = CombatPosition::new(engine, position.combat.clone());
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct TimedOutPendingChoiceStepper {
    engine_steps: usize,
    mutate_partial_position: bool,
}

#[derive(Clone, Copy)]
struct TerminalPendingChoiceStepper;

impl CombatStepper for TerminalPendingChoiceStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        Vec::new()
    }

    fn supports_canonical_pending_choice_actions(&self) -> bool {
        true
    }

    fn is_legal_action(&self, _position: &CombatPosition, input: &ClientInput) -> bool {
        matches!(input, ClientInput::SubmitSelection(_))
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        _input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let terminal_position = CombatPosition::new(
            EngineState::GameOver(crate::state::core::RunResult::Defeat),
            position.combat.clone(),
        );
        crate::sim::combat::CombatStepResult {
            terminal: CombatTerminal::Loss,
            alive: false,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position: terminal_position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

impl CombatStepper for TimedOutPendingChoiceStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        Vec::new()
    }

    fn supports_canonical_pending_choice_actions(&self) -> bool {
        true
    }

    fn is_legal_action(&self, _position: &CombatPosition, input: &ClientInput) -> bool {
        matches!(input, ClientInput::SubmitSelection(_))
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        _input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let mut partial = position.clone();
        if self.mutate_partial_position {
            partial.engine = EngineState::CombatProcessing;
            partial.combat.entities.player.current_hp = 1;
        }
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&partial.engine, &partial.combat),
            alive: true,
            truncated: true,
            timed_out: true,
            engine_steps: self.engine_steps,
            position: partial,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[test]
fn timed_out_pending_choice_steps_requeue_the_atomic_leaf_without_a_fake_child() {
    for stepper in [
        TimedOutPendingChoiceStepper {
            engine_steps: 0,
            mutate_partial_position: false,
        },
        TimedOutPendingChoiceStepper {
            engine_steps: 1,
            mutate_partial_position: true,
        },
    ] {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 6_100)];
        let initial_hp = combat.entities.player.current_hp;
        let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
            candidate_lineages: crate::test_support::scenario_lineages([6_100]),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::core::HandSelectReason::Discard,
        });

        let report = run_combat_search_v2_with_stepper(
            &engine,
            &combat,
            CombatSearchV2Config {
                max_nodes: 20,
                rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
                ..CombatSearchV2Config::default()
            },
            &stepper,
        );

        assert_eq!(
            report.outcome.coverage_status,
            SearchCoverageStatus::TimeBudgetLimited
        );
        assert_eq!(report.performance.engine_step_calls, 1);
        assert_eq!(
            report.performance.pending_choice_complete_actions_submitted,
            0
        );
        assert_eq!(report.stats.nodes_generated, 0);
        assert_eq!(report.frontier.remaining_work_items, 1);
        assert_eq!(report.frontier.pending_choice_work_items, 1);
        let frontier = report
            .best_frontier_trajectory
            .expect("the concrete parent must remain recoverable");
        assert!(frontier.actions.is_empty());
        assert_eq!(frontier.final_hp, initial_hp);
    }
}

#[test]
fn single_card_pending_choice_surface_can_still_be_exhaustive() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 6_200),
        CombatCard::for_scenario(CardId::Defend, 6_201),
    ];
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([6_200, 6_201]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: crate::state::core::HandSelectReason::Discard,
    });

    let report = run_combat_search_v2_with_stepper(
        &engine,
        &combat,
        CombatSearchV2Config {
            max_nodes: 20,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            ..CombatSearchV2Config::default()
        },
        &TerminalPendingChoiceStepper,
    );

    assert!(!report.stats.action_surface_incomplete);
    assert!(report.outcome.exhaustive);
    assert_eq!(
        report.outcome.coverage_status,
        SearchCoverageStatus::Exhaustive
    );
}

#[derive(Clone, Copy)]
struct OneCardWinStepper;

impl CombatStepper for OneCardWinStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if !matches!(position.engine, EngineState::CombatPlayerTurn)
            || position.combat.zones.hand.is_empty()
        {
            return Vec::new();
        }
        vec![ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        }]
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let mut combat = position.combat.clone();
        let mut engine = position.engine.clone();
        if matches!(input, ClientInput::PlayCard { .. }) {
            if let Some(monster) = combat.entities.monsters.first_mut() {
                monster.current_hp = 0;
            }
        }
        if combat
            .entities
            .monsters
            .iter()
            .all(|monster| !monster.is_alive_for_action())
        {
            engine = EngineState::GameOver(crate::state::core::RunResult::Victory);
        }
        let position = CombatPosition::new(engine, combat);
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct SecondActionWinStepper;

impl CombatStepper for SecondActionWinStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if !matches!(position.engine, EngineState::CombatPlayerTurn)
            || position.combat.zones.hand.is_empty()
        {
            return Vec::new();
        }
        vec![
            ClientInput::EndTurn,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ]
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        OneCardWinStepper.apply_to_stable(position, input, limits)
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct DuplicateTerminalWinStepper;

impl CombatStepper for DuplicateTerminalWinStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if matches!(position.engine, EngineState::CombatPlayerTurn) {
            vec![ClientInput::EndTurn, ClientInput::Proceed]
        } else {
            Vec::new()
        }
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        _input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let mut combat = position.combat.clone();
        for monster in &mut combat.entities.monsters {
            monster.current_hp = 0;
        }
        let position = CombatPosition::new(
            EngineState::GameOver(crate::state::core::RunResult::Victory),
            combat,
        );
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct CleanOrDirtyTerminalWinStepper;

impl CombatStepper for CleanOrDirtyTerminalWinStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if matches!(position.engine, EngineState::CombatPlayerTurn) {
            vec![ClientInput::EndTurn, ClientInput::Proceed]
        } else {
            Vec::new()
        }
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        if limits
            .deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return crate::sim::combat::CombatStepResult {
                terminal: combat_terminal(&position.engine, &position.combat),
                alive: true,
                truncated: true,
                timed_out: true,
                engine_steps: 0,
                position: position.clone(),
            };
        }

        let mut combat = position.combat.clone();
        let monster = combat
            .entities
            .monsters
            .first_mut()
            .expect("terminal witness test monster");
        monster.current_hp = 0;
        if input == ClientInput::Proceed {
            combat
                .meta
                .meta_changes
                .push(crate::runtime::combat::MetaChange::AddCardToMasterDeck(
                    crate::test_support::test_master_deck_acquisition(CardId::Parasite, 999),
                ));
            combat.entities.player.current_hp = 80;
        } else {
            combat.entities.player.current_hp = 20;
        }
        let position = CombatPosition::new(
            EngineState::GameOver(crate::state::core::RunResult::Victory),
            combat,
        );
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct DuplicateUnresolvedChildStepper;

impl CombatStepper for DuplicateUnresolvedChildStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if matches!(position.engine, EngineState::CombatPlayerTurn) {
            vec![ClientInput::EndTurn, ClientInput::Proceed]
        } else {
            Vec::new()
        }
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        _input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position: position.clone(),
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct TwoTurnWinStepper;

impl CombatStepper for TwoTurnWinStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if !matches!(position.engine, EngineState::CombatPlayerTurn) {
            return Vec::new();
        }
        if position.combat.turn.turn_count == 0 {
            vec![ClientInput::EndTurn]
        } else {
            vec![ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            }]
        }
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let mut combat = position.combat.clone();
        let mut engine = position.engine.clone();
        match input {
            ClientInput::EndTurn => {
                combat.turn.turn_count = combat.turn.turn_count.saturating_add(1);
                combat.entities.player.current_hp =
                    combat.entities.player.current_hp.saturating_sub(1);
            }
            ClientInput::PlayCard { .. } => {
                if let Some(monster) = combat.entities.monsters.first_mut() {
                    monster.current_hp = 0;
                }
            }
            _ => {}
        }
        if combat
            .entities
            .monsters
            .iter()
            .all(|monster| !monster.is_alive_for_action())
        {
            engine = EngineState::GameOver(crate::state::core::RunResult::Victory);
        }
        let position = CombatPosition::new(engine, combat);
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct WinOrDelayStepper;

impl CombatStepper for WinOrDelayStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if !matches!(position.engine, EngineState::CombatPlayerTurn) {
            return Vec::new();
        }
        vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
            ClientInput::EndTurn,
        ]
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let mut combat = position.combat.clone();
        let mut engine = position.engine.clone();
        match input {
            ClientInput::PlayCard { .. } => {
                if let Some(monster) = combat.entities.monsters.first_mut() {
                    monster.current_hp = 0;
                }
                engine = EngineState::GameOver(crate::state::core::RunResult::Victory);
            }
            ClientInput::EndTurn => {
                combat.turn.turn_count = combat.turn.turn_count.saturating_add(1);
                combat.entities.player.current_hp =
                    combat.entities.player.current_hp.saturating_sub(1);
            }
            _ => {}
        }
        let position = CombatPosition::new(engine, combat);
        crate::sim::combat::CombatStepResult {
            terminal: combat_terminal(&position.engine, &position.combat),
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
            position,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[test]
fn max_potions_used_cuts_potion_branches_without_disabling_policy_all() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let mut config = CombatSearchV2Config {
        potion_policy: CombatSearchV2PotionPolicy::All,
        max_potions_used: Some(0),
        max_nodes: 8,
        ..CombatSearchV2Config::default()
    };

    let blocked = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        config.clone(),
        &PotionWinStepper,
    );

    assert!(!blocked.outcome.complete_trajectory_found);
    assert!(blocked.frontier.potion_budget_cut_count > 0);
    assert!(blocked
        .diagnostics
        .diagnosis
        .contains(&"potion_budget_cutoffs"));

    config.max_potions_used = Some(1);
    let allowed = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        config,
        &PotionWinStepper,
    );

    assert!(allowed.outcome.complete_trajectory_found);
    assert_eq!(
        allowed
            .best_complete_trajectory
            .as_ref()
            .map(|trajectory| trajectory.potions_used),
        Some(1)
    );
}

#[test]
fn search_report_declares_privileged_policy_evidence_boundary() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.draw_pile = (vec![
        CombatCard::for_scenario(CardId::Strike, 11),
        CombatCard::for_scenario(CardId::Defend, 12),
    ])
    .into();
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RunicDome));

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 1,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert_eq!(report.schema_name, COMBAT_SEARCH_V2_REPORT_SCHEMA_NAME);
    assert_eq!(COMBAT_SEARCH_V2_REPORT_SCHEMA_VERSION, 21);
    assert_eq!(
        report.schema_version,
        COMBAT_SEARCH_V2_REPORT_SCHEMA_VERSION
    );
    assert_eq!(report.quantum_history.len(), 1);
    assert_eq!(report.quantum_history[0].requested_additional_nodes, 1);
    assert_eq!(
        report.quantum_history[0].after.nodes_expanded,
        report.stats.nodes_expanded
    );
    assert!(
        report.performance.total_elapsed_us >= report.performance.report_finalization_elapsed_us
    );
    assert_eq!(
        report.stats.elapsed_ms,
        report.performance.total_elapsed_us / 1_000
    );
    assert_eq!(
        report.policy_evidence.information_access,
        CombatSearchV2InformationAccess::PrivilegedSimulator
    );
    assert!(!report.policy_evidence.public_safe);
    assert!(report
        .policy_evidence
        .hidden_information_risks
        .contains(&CombatSearchV2HiddenInformationRisk::ExactDrawPileOrderWithoutFrozenEye));
    assert!(report
        .policy_evidence
        .hidden_information_risks
        .contains(&CombatSearchV2HiddenInformationRisk::ExactMonsterIntentUnderRunicDome));
}

#[test]
fn hp_loss_acceptance_threshold_stops_after_good_enough_complete_win() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        100,
    )];
    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 20,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            satisfaction: CombatSearchV2Satisfaction::HpLossAtMost(0),
            ..CombatSearchV2Config::default()
        },
        &WinOrDelayStepper,
    );

    assert_eq!(
        report.outcome.coverage_status,
        SearchCoverageStatus::AcceptedCompleteCandidate
    );
    assert!(report.outcome.complete_trajectory_found);
    assert!(
        report.stats.nodes_expanded < 20,
        "search should stop before exhausting node budget after an accepted complete win"
    );
    assert_eq!(
        report
            .best_complete_trajectory
            .as_ref()
            .map(|trajectory| trajectory.hp_loss),
        Some(0)
    );
}

#[test]
fn zero_hp_loss_complete_win_stops_without_explicit_hp_gate() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        100,
    )];
    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 20,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            satisfaction: CombatSearchV2Satisfaction::ZeroLossOrBudget,
            ..CombatSearchV2Config::default()
        },
        &WinOrDelayStepper,
    );

    assert_eq!(
        report.outcome.coverage_status,
        SearchCoverageStatus::AcceptedCompleteCandidate
    );
    assert_eq!(
        report
            .best_complete_trajectory
            .as_ref()
            .map(|trajectory| trajectory.hp_loss),
        Some(0)
    );
    assert!(
        report.stats.nodes_expanded < 20,
        "zero-loss complete win cannot be improved on hp loss, so search should stop early"
    );
}

#[test]
fn zero_hp_loss_complete_win_waits_when_external_payoff_remains_possible() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::Feed,
        100,
    )];
    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 20,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            satisfaction: CombatSearchV2Satisfaction::ZeroLossOrBudget,
            ..CombatSearchV2Config::default()
        },
        &WinOrDelayStepper,
    );

    assert_ne!(
        report.outcome.coverage_status,
        SearchCoverageStatus::AcceptedCompleteCandidate,
        "default zero-loss early accept should not close search when combat-external payoff cards remain relevant"
    );
    assert!(report.outcome.complete_trajectory_found);
}

#[test]
fn split_work_quanta_reuse_the_same_search_state() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];
    let config = CombatSearchV2Config {
        max_nodes: 99,
        max_actions_per_line: 20,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        satisfaction: CombatSearchV2Satisfaction::BudgetOrExhaustion,
        ..CombatSearchV2Config::default()
    };
    let quantum = |additional_nodes| CombatSearchV2WorkQuantum {
        additional_nodes,
        soft_wall_time: Some(std::time::Duration::from_secs(1)),
    };

    let mut split = CombatSearchV2Session::new_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        config.clone(),
        &WinOrDelayStepper,
    );
    assert_eq!(
        split.advance_with_stepper(quantum(2), &WinOrDelayStepper),
        CombatSearchV2AdvanceStop::QuantumNodeBudget
    );
    let after_first = split.snapshot();
    assert_eq!(after_first.nodes_expanded, 2);
    assert!(after_first.frontier_work_items > 0);
    assert_eq!(
        after_first.root_evidence.materialization,
        CombatSearchV2RootMaterializationStatus::Complete
    );
    assert_eq!(after_first.root_evidence.contenders.len(), 2);
    assert_eq!(
        after_first
            .root_evidence
            .leader
            .as_ref()
            .map(|leader| leader.action_key.as_str()),
        after_first
            .best_win
            .as_ref()
            .and_then(|win| win.actions.first())
            .map(|action| action.action_key.as_str())
    );
    assert_eq!(
        after_first
            .root_evidence
            .contenders
            .iter()
            .map(|contender| contender.work.generated_concrete_nodes)
            .sum::<u64>(),
        split.loop_state.stats.nodes_generated
    );
    assert_eq!(
        after_first
            .root_evidence
            .unattributed
            .expanded_concrete_nodes,
        1,
        "the concrete root expansion has no root action lineage yet"
    );
    assert_eq!(
        split.advance_with_stepper(quantum(2), &WinOrDelayStepper),
        CombatSearchV2AdvanceStop::QuantumNodeBudget
    );
    let split_snapshot = split.snapshot();

    let mut combined = CombatSearchV2Session::new_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        config,
        &WinOrDelayStepper,
    );
    assert_eq!(
        combined.advance_with_stepper(quantum(4), &WinOrDelayStepper),
        CombatSearchV2AdvanceStop::QuantumNodeBudget
    );
    let combined_snapshot = combined.snapshot();

    assert_eq!(
        split_snapshot.nodes_expanded,
        combined_snapshot.nodes_expanded
    );
    assert_eq!(
        split_snapshot.exact_state_keys,
        combined_snapshot.exact_state_keys
    );
    assert_eq!(
        split_snapshot.candidate_frontier_revision,
        combined_snapshot.candidate_frontier_revision
    );
    assert_eq!(
        split_snapshot
            .candidate_frontier
            .iter()
            .map(|candidate| (&candidate.outcome_order_key, candidate.potions_used))
            .collect::<Vec<_>>(),
        combined_snapshot
            .candidate_frontier
            .iter()
            .map(|candidate| (&candidate.outcome_order_key, candidate.potions_used))
            .collect::<Vec<_>>()
    );
    let split_report = split.finish_with_stepper(&WinOrDelayStepper);
    assert_eq!(split_report.quantum_history.len(), 2);
    assert_eq!(
        split_report.quantum_history[0].after,
        split_report.quantum_history[1].before
    );
    assert_eq!(
        split_report.quantum_history[1].root_evidence,
        split_snapshot.root_evidence
    );
    assert_eq!(split_report.budget.max_nodes, 4);
    assert_eq!(split_report.budget.wall_time_ms, Some(2_000));
}

#[test]
fn split_pending_choice_quanta_advance_the_same_virtual_prefix_work() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.draw_pile = ((0..13)
        .map(|index| CombatCard::for_scenario(CardId::Strike, 8_000 + index))
        .collect::<Vec<_>>())
    .into();
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
        cards: vec![CardId::Strike; 13],
        card_lineages: crate::test_support::scenario_lineages(8_000..8_013),
    });
    let mut session = CombatSearchV2Session::new(
        &engine,
        &combat,
        CombatSearchV2Config {
            max_nodes: 4,
            max_actions_per_line: 1,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            satisfaction: CombatSearchV2Satisfaction::BudgetOrExhaustion,
            ..CombatSearchV2Config::default()
        },
    );
    let quantum = CombatSearchV2WorkQuantum {
        additional_nodes: 2,
        soft_wall_time: None,
    };

    assert_eq!(
        session.advance(quantum),
        CombatSearchV2AdvanceStop::QuantumNodeBudget
    );
    let first_prefixes = session
        .loop_state
        .performance
        .pending_choice_prefixes_expanded;
    let first = session.snapshot();
    assert_eq!(first_prefixes, 2);
    assert_eq!(
        first.root_evidence.materialization,
        CombatSearchV2RootMaterializationStatus::Partial
    );
    assert!(first.root_evidence.contenders.is_empty());
    assert_eq!(
        first
            .root_evidence
            .unattributed
            .open_pending_choice_work_items,
        1
    );
    assert_eq!(first.root_evidence.unattributed.open_work_items, 1);

    assert_eq!(
        session.advance(quantum),
        CombatSearchV2AdvanceStop::QuantumNodeBudget
    );
    assert_eq!(
        session
            .loop_state
            .performance
            .pending_choice_prefixes_expanded,
        4,
        "the second quantum must extend the cumulative virtual-work ceiling"
    );
}

#[test]
fn historical_pending_prefix_work_does_not_raise_the_concrete_node_limit() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];
    let mut session = CombatSearchV2Session::new_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 10,
            max_actions_per_line: 100,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            satisfaction: CombatSearchV2Satisfaction::BudgetOrExhaustion,
            ..CombatSearchV2Config::default()
        },
        &WinOrDelayStepper,
    );
    session.loop_state.stats.nodes_expanded = 1;
    session
        .loop_state
        .performance
        .pending_choice_prefixes_expanded = 100;

    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 10,
                soft_wall_time: None,
            },
            &WinOrDelayStepper,
        ),
        CombatSearchV2AdvanceStop::QuantumNodeBudget
    );
    assert_eq!(session.loop_state.stats.nodes_expanded, 11);
    assert_eq!(
        session
            .loop_state
            .performance
            .pending_choice_prefixes_expanded,
        100
    );
}

#[test]
fn replayable_rollout_win_is_promoted_at_the_quantum_boundary() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];
    let mut session = CombatSearchV2Session::new_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 0,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            satisfaction: CombatSearchV2Satisfaction::HpLossAtMost(0),
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 0,
                soft_wall_time: None,
            },
            &OneCardWinStepper,
        ),
        CombatSearchV2AdvanceStop::CandidateSatisfied
    );
    let snapshot = session.snapshot();
    assert!(snapshot.best_win.is_some());
    assert!(snapshot
        .root_evidence
        .contenders
        .iter()
        .any(|contender| contender.work.best_exact_win.is_some()));
}

fn terminal_witness_for_input(
    engine: &EngineState,
    combat: &crate::runtime::combat::CombatState,
    input: ClientInput,
    stepper: &impl CombatStepper,
) -> RolloutNodeEstimate {
    let root = SearchNode::root(engine.clone(), combat.clone());
    let position = CombatPosition::new(engine.clone(), combat.clone());
    let choices = stepper.atomic_action_choices(&position);
    let action_id = choices
        .iter()
        .position(|choice| choice.input == input)
        .expect("test witness input must be legal");
    let choice = choices[action_id].clone();
    let step = stepper.apply_to_stable(
        &position,
        choice.input.clone(),
        CombatStepLimits {
            max_engine_steps: 100,
            deadline: None,
        },
    );
    assert!(!step.truncated && !step.timed_out);

    let mut child = root.clone_for_child(step.position.engine, step.position.combat);
    let transition = classify_turn_branch_transition(
        &root.engine,
        &root.combat,
        &choice.input,
        &child.engine,
        &child.combat,
    );
    child.note_turn_prefix(&root.combat, &choice.input, transition);
    child.note_input(&choice.input);
    child.note_turn_branch_priority(transition.frontier_priority_hint());
    child.push_action(CombatSearchV2ActionTrace {
        step_index: 0,
        action_id,
        action_key: choice.action_key,
        action_debug: choice.action_debug,
        input: choice.input,
    });
    RolloutNodeEstimate::from_node(
        &child,
        1,
        RolloutStopReason::TerminalState,
        None,
        crate::ai::combat_search_v2::rollout_pending_choice::RolloutPendingChoiceProgress::default(
        ),
    )
}

#[test]
fn clean_satisfaction_promotes_the_preserved_clean_rollout_witness() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::WrithingMass)];
    let engine = EngineState::CombatPlayerTurn;
    let clean = terminal_witness_for_input(
        &engine,
        &combat,
        ClientInput::EndTurn,
        &CleanOrDirtyTerminalWinStepper,
    );
    let dirty = terminal_witness_for_input(
        &engine,
        &combat,
        ClientInput::Proceed,
        &CleanOrDirtyTerminalWinStepper,
    );
    assert!(clean.is_replayable_terminal_win_without_new_external_burden(0));
    assert_eq!(dirty.external_burden_count, 1);
    assert!(dirty.final_hp > clean.final_hp);

    let mut session = CombatSearchV2Session::new_with_stepper(
        &engine,
        &combat,
        CombatSearchV2Config {
            max_nodes: 0,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            satisfaction: CombatSearchV2Satisfaction::FirstCompleteWinWithoutNewExternalBurden,
            ..CombatSearchV2Config::default()
        },
        &CleanOrDirtyTerminalWinStepper,
    );
    session
        .loop_state
        .rollout_cache
        .best_replayable_terminal_win = Some(
        crate::ai::combat_search_v2::rollout_cache::ReplayableTerminalWinWitness {
            estimate: dirty,
            nodes_generated_at_discovery: 9,
        },
    );
    session
        .loop_state
        .rollout_cache
        .best_replayable_terminal_win_without_new_external_burden = Some(
        crate::ai::combat_search_v2::rollout_cache::ReplayableTerminalWinWitness {
            estimate: clean,
            nodes_generated_at_discovery: 3,
        },
    );

    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 0,
                soft_wall_time: None,
            },
            &CleanOrDirtyTerminalWinStepper,
        ),
        CombatSearchV2AdvanceStop::CandidateSatisfied
    );
    let best = session
        .loop_state
        .trajectories
        .best_win
        .as_ref()
        .expect("preserved clean witness should replay exactly");
    assert_eq!(best.combat.entities.player.current_hp, 20);
    assert_eq!(outcome_score::external_burden_count(&best.combat), 0);
}

#[test]
fn expired_soft_quantum_still_promotes_a_bounded_exact_witness() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];
    let engine = EngineState::CombatPlayerTurn;
    let witness = terminal_witness_for_input(
        &engine,
        &combat,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        &OneCardWinStepper,
    );
    let mut session = CombatSearchV2Session::new_with_stepper(
        &engine,
        &combat,
        CombatSearchV2Config {
            max_nodes: 0,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            satisfaction: CombatSearchV2Satisfaction::HpLossAtMost(0),
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );
    session
        .loop_state
        .rollout_cache
        .best_replayable_terminal_win = Some(
        crate::ai::combat_search_v2::rollout_cache::ReplayableTerminalWinWitness {
            estimate: witness.clone(),
            nodes_generated_at_discovery: 0,
        },
    );

    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 0,
                soft_wall_time: Some(std::time::Duration::ZERO),
            },
            &OneCardWinStepper,
        ),
        CombatSearchV2AdvanceStop::CandidateSatisfied
    );
    assert!(session.snapshot().best_win.is_some());
    assert_eq!(
        session.loop_state.last_promoted_rollout_witness,
        Some(witness.clone())
    );
    assert_eq!(
        session
            .loop_state
            .performance
            .rollout_promotion_actions_replayed,
        1,
        "exact witness verification remains bounded even after exploration time expires"
    );
    assert_eq!(
        session
            .loop_state
            .rollout_cache
            .best_replayable_terminal_win
            .as_ref()
            .map(|candidate| &candidate.estimate),
        Some(&witness),
        "the rollout cache retains the verified witness as evidence"
    );
}

#[test]
fn rollout_promotion_preserves_the_original_root_action_id() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];
    let mut session = CombatSearchV2Session::new_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 0,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            satisfaction: CombatSearchV2Satisfaction::HpLossAtMost(0),
            ..CombatSearchV2Config::default()
        },
        &SecondActionWinStepper,
    );

    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 0,
                soft_wall_time: None,
            },
            &SecondActionWinStepper,
        ),
        CombatSearchV2AdvanceStop::CandidateSatisfied
    );
    let snapshot = session.snapshot();
    assert_eq!(
        snapshot
            .best_win
            .as_ref()
            .and_then(|win| win.actions.first())
            .map(|action| action.action_id),
        Some(1)
    );
    assert_eq!(
        snapshot
            .root_evidence
            .leader
            .as_ref()
            .map(|leader| leader.action_id),
        Some(1)
    );
}

#[test]
fn dominated_terminal_children_still_record_exact_evidence_for_each_root() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let mut session = CombatSearchV2Session::new_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 1,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            satisfaction: CombatSearchV2Satisfaction::BudgetOrExhaustion,
            ..CombatSearchV2Config::default()
        },
        &DuplicateTerminalWinStepper,
    );

    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 1,
                soft_wall_time: None,
            },
            &DuplicateTerminalWinStepper,
        ),
        CombatSearchV2AdvanceStop::QuantumNodeBudget
    );
    let snapshot = session.snapshot();
    assert_eq!(snapshot.root_evidence.contenders.len(), 2);
    assert!(snapshot
        .root_evidence
        .contenders
        .iter()
        .all(|contender| contender.work.best_exact_win.is_some()));
}

#[test]
fn root_rollout_waits_for_the_first_quantum_and_uses_its_deadline() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];
    let mut session = CombatSearchV2Session::new_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 0,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            satisfaction: CombatSearchV2Satisfaction::HpLossAtMost(0),
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert_eq!(session.loop_state.rollout_cache.evaluations, 0);
    assert_eq!(
        session.loop_state.performance.root_rollout_estimate_calls,
        0
    );
    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 0,
                soft_wall_time: Some(std::time::Duration::ZERO),
            },
            &OneCardWinStepper,
        ),
        CombatSearchV2AdvanceStop::QuantumWallTime
    );
    assert_eq!(session.loop_state.rollout_cache.evaluations, 0);
    assert_eq!(session.loop_state.rollout_cache.deadline_budget_skips, 1);
    assert_eq!(
        session.loop_state.performance.root_rollout_estimate_calls,
        1
    );
    assert!(session.snapshot().best_win.is_none());
    assert!(session.root_rollout_pending);

    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 0,
                soft_wall_time: Some(std::time::Duration::from_secs(1)),
            },
            &OneCardWinStepper,
        ),
        CombatSearchV2AdvanceStop::CandidateSatisfied
    );
    assert_eq!(session.loop_state.rollout_cache.evaluations, 1);
    assert_eq!(
        session.loop_state.performance.root_rollout_estimate_calls,
        2
    );
    assert!(session.snapshot().best_win.is_some());
    assert!(!session.root_rollout_pending);
}

#[test]
fn replayable_rollout_win_is_not_masked_by_an_existing_exact_win() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];
    let config = CombatSearchV2Config {
        max_nodes: 0,
        rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
        satisfaction: CombatSearchV2Satisfaction::HpLossAtMost(0),
        ..CombatSearchV2Config::default()
    };
    let mut session = CombatSearchV2Session::new_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        config.clone(),
        &OneCardWinStepper,
    );
    let mut inferior_win = combat;
    inferior_win.entities.monsters.clear();
    inferior_win.entities.player.current_hp = 1;
    session.loop_state.remember_win(
        SearchNode::root(EngineState::CombatPlayerTurn, inferior_win),
        &config,
    );

    assert_eq!(
        session.advance_with_stepper(
            CombatSearchV2WorkQuantum {
                additional_nodes: 0,
                soft_wall_time: None,
            },
            &OneCardWinStepper,
        ),
        CombatSearchV2AdvanceStop::CandidateSatisfied
    );
    assert_eq!(
        session
            .loop_state
            .trajectories
            .best_win
            .as_ref()
            .map(|node| node.combat.entities.player.current_hp),
        Some(80)
    );
    assert!(session
        .snapshot()
        .root_evidence
        .contenders
        .iter()
        .any(|contender| contender.work.best_exact_win.is_some()));
}

#[test]
fn action_ordering_preserves_original_action_id_in_trace() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let config = CombatSearchV2Config {
        potion_policy: CombatSearchV2PotionPolicy::All,
        max_nodes: 8,
        ..CombatSearchV2Config::default()
    };

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        config,
        &ReversePotionWinStepper,
    );

    let first_action_id = report
        .best_complete_trajectory
        .as_ref()
        .and_then(|trajectory| trajectory.actions.first())
        .map(|action| action.action_id);

    assert_eq!(first_action_id, Some(1));
}

#[test]
fn pending_choice_contract_counts_exact_child_resolution() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let config = CombatSearchV2Config {
        max_nodes: 1,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };

    let report = run_combat_search_v2_with_stepper(
        &EngineState::PendingChoice(crate::state::core::PendingChoice::StanceChoice),
        &combat,
        config,
        &PendingChoiceResolveStepper,
    );

    assert_eq!(report.diagnostics.pending_choice.pending_choice_states, 1);
    assert_eq!(
        report
            .diagnostics
            .pending_choice
            .expanded_pending_choice_states,
        1
    );
    assert_eq!(
        report
            .diagnostics
            .pending_choice
            .legal_actions_from_pending_choice,
        2
    );
    assert_eq!(report.diagnostics.pending_choice.resolved_children, 2);
    assert_eq!(report.diagnostics.pending_choice.still_pending_children, 0);
    assert!(report
        .diagnostics
        .pending_choice
        .ordering_role_counts
        .iter()
        .any(|role| role.role == "pending_choice_neutral_selection"
            && role.actions == 2
            && role.first_actions == 1));
    assert!(report
        .diagnostics
        .diagnosis
        .contains(&"pending_choice_contract_observed"));
}

#[test]
fn custom_stepper_keeps_hand_pending_choice_ownership_without_opt_in() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 7_000)];
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
        candidate_lineages: crate::test_support::scenario_lineages([7_000]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: crate::state::core::HandSelectReason::Discard,
    });

    let report = run_combat_search_v2_with_stepper(
        &engine,
        &combat,
        CombatSearchV2Config {
            max_nodes: 1,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            ..CombatSearchV2Config::default()
        },
        &PendingChoiceResolveStepper,
    );

    assert_eq!(report.performance.pending_choice_prefixes_expanded, 0);
    assert_eq!(
        report.performance.pending_choice_complete_actions_submitted,
        0
    );
    assert_eq!(report.performance.engine_step_calls, 2);
    assert_eq!(
        report
            .diagnostics
            .pending_choice
            .legal_actions_from_pending_choice,
        2
    );
    assert_eq!(report.diagnostics.pending_choice.resolved_children, 2);
}

#[test]
fn root_turn_plan_frontier_seed_is_explicit_but_does_not_claim_terminal() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        100,
    )];
    let base_config = CombatSearchV2Config {
        max_nodes: 1,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };

    let baseline = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        base_config.clone(),
        &OneCardWinStepper,
    );
    assert!(!baseline.outcome.complete_trajectory_found);
    assert_eq!(baseline.search_policy.turn_plan_policy, "disabled");
    assert_eq!(baseline.diagnostics.turn_plan.frontier_seeded_nodes, 0);
    assert_eq!(baseline.diagnostics.turn_plan.root_states_observed, 0);

    let seeded = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::RootFrontierSeed,
            ..base_config
        },
        &OneCardWinStepper,
    );

    assert!(
        !seeded.outcome.complete_trajectory_found,
        "a generated turn-plan seed is evidence and cannot bypass root materialization or budget"
    );
    assert_eq!(seeded.stats.nodes_to_first_win, Some(1));
    assert_eq!(seeded.diagnostics.turn_plan.frontier_seeded_nodes, 1);
    assert_eq!(
        seeded.diagnostics.turn_plan.behavioral_effect,
        "turn_plan_frontier_seed_exact_end_states_no_prune_no_terminal_claim"
    );
    assert!(seeded
        .diagnostics
        .diagnosis
        .contains(&"turn_plan_frontier_seeded"));
    assert_eq!(seeded.search_policy.turn_plan_policy, "root_frontier_seed");
}

#[test]
fn hierarchical_turn_boundary_owns_the_source_and_charges_shared_budget() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 2,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            expansion_policy: CombatSearchV2ExpansionPolicy::HierarchicalTurnBoundary,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::RootFrontierSeed,
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert!(report.outcome.complete_win_found);
    assert_eq!(
        report.search_policy.expansion_policy,
        "hierarchical_turn_boundary"
    );
    assert_eq!(report.performance.turn_boundary_macro_calls, 1);
    assert_eq!(report.performance.turn_boundary_macro_candidates, 1);
    assert_eq!(
        report.stats.nodes_expanded,
        report.performance.turn_boundary_macro_inner_nodes_expanded
    );
    assert!(report.stats.nodes_expanded <= report.budget.max_nodes as u64);
    assert_eq!(
        report.diagnostics.branching.states_queried, 0,
        "the source node must not also be expanded by the atomic action owner"
    );
    assert_eq!(
        report.performance.turn_plan_frontier_seed_calls, 0,
        "legacy frontier seeding must not run beside hierarchical ownership"
    );
}

#[test]
fn hierarchical_turn_boundary_preserves_the_whole_line_potion_budget() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let base = CombatSearchV2Config {
        max_nodes: 32,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        expansion_policy: CombatSearchV2ExpansionPolicy::HierarchicalTurnBoundary,
        potion_policy: CombatSearchV2PotionPolicy::All,
        ..CombatSearchV2Config::default()
    };

    let blocked = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_potions_used: Some(1),
            ..base.clone()
        },
        &TwoPotionWinStepper,
    );
    assert!(
        !blocked.outcome.complete_win_found,
        "one complete-turn macro must not spend two potions through a one-potion contract"
    );

    let allowed = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_potions_used: Some(2),
            ..base
        },
        &TwoPotionWinStepper,
    );
    assert!(allowed.outcome.complete_win_found);
    assert_eq!(
        allowed
            .best_complete_trajectory
            .as_ref()
            .map(|trajectory| trajectory.potions_used),
        Some(2)
    );
}

#[test]
fn hierarchical_turn_boundary_records_atomic_fallback_for_an_uncovered_gap() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 4,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            expansion_policy: CombatSearchV2ExpansionPolicy::HierarchicalTurnBoundary,
            ..CombatSearchV2Config::default()
        },
        &PendingChoiceResolveStepper,
    );

    assert_eq!(report.performance.turn_boundary_macro_calls, 1);
    assert_eq!(report.performance.turn_boundary_macro_candidates, 0);
    assert_eq!(
        report.performance.turn_boundary_macro_atomic_fallbacks, 1,
        "an empty supported portfolio is a typed refinement gap, not a silent stop"
    );
    assert_eq!(report.diagnostics.branching.states_queried, 1);
}

#[test]
fn report_includes_search_performance_attribution_counts() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        100,
    )];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 2,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::RootFrontierSeed,
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert!(report.performance.rollout_estimate_calls >= 1);
    assert_eq!(report.performance.root_rollout_estimate_calls, 1);
    assert_eq!(
        report.performance.rollout_estimate_calls,
        report
            .performance
            .root_rollout_estimate_calls
            .saturating_add(report.performance.child_rollout_estimate_calls)
            .saturating_add(report.performance.deferred_child_rollout_estimate_calls)
            .saturating_add(report.performance.turn_plan_seed_rollout_estimate_calls)
    );
    assert!(report.performance.frontier_pop_calls >= 1);
    assert_eq!(report.performance.turn_plan_frontier_seed_calls, 1);
    assert!(report.performance.total_elapsed_us >= report.performance.engine_step_elapsed_us);
    assert!(report.performance.total_elapsed_us >= report.performance.rollout_estimate_elapsed_us);
    assert!(report.performance.total_elapsed_us >= report.performance.shadow_audit_elapsed_us);
    assert!(
        report.performance.total_elapsed_us
            >= report.performance.root_turn_plan_diagnostics_elapsed_us
    );

    let exact_report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 2,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::DiagnosticOnly,
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert!(exact_report.performance.engine_step_calls >= 1);
}

#[test]
fn terminal_children_skip_rollout_estimate() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 2,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::DiagnosticOnly,
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert_eq!(
        report.performance.rollout_estimate_calls, 1,
        "only the root needs a rollout estimate; the generated terminal child should use terminal value"
    );
    assert_eq!(report.performance.root_rollout_estimate_calls, 1);
    assert_eq!(report.performance.child_rollout_estimate_calls, 0);
    assert_eq!(report.performance.terminal_child_rollout_skips, 1);
    assert_eq!(report.stats.terminal_wins, 1);
}

#[test]
fn terminal_rollout_is_promoted_only_after_exact_replay() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 0,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert_eq!(report.stats.nodes_expanded, 0);
    assert_eq!(report.rollout.terminal_wins, 1);
    assert!(
        report.outcome.complete_trajectory_found,
        "a complete terminal rollout should become evidence only after exact replay"
    );
    let trajectory = report
        .best_complete_trajectory
        .expect("exactly replayed rollout win");
    assert_eq!(trajectory.terminal, SearchTerminalLabel::Win);
    assert_eq!(trajectory.actions.len(), 1);
    assert_eq!(trajectory.actions[0].action_id, 0);
    assert_eq!(
        trajectory.actions[0].input,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        }
    );
}

#[test]
fn exact_replayed_terminal_rollout_honors_hp_loss_acceptance_threshold() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 0,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            satisfaction: CombatSearchV2Satisfaction::HpLossAtMost(0),
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert_eq!(report.stats.nodes_expanded, 0);
    assert_eq!(
        report
            .best_complete_trajectory
            .as_ref()
            .map(|trajectory| trajectory.hp_loss),
        Some(0)
    );
    assert_eq!(
        report.outcome.coverage_status,
        SearchCoverageStatus::AcceptedCompleteCandidate,
        "an exact-replayed rollout win within the configured threshold must carry the same acceptance status as a main-frontier win"
    );
}

#[test]
fn exact_replayed_rollout_reports_when_its_witness_was_discovered() {
    let mut combat = blank_test_combat();
    combat.turn.turn_count = 0;
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 1,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            ..CombatSearchV2Config::default()
        },
        &TwoTurnWinStepper,
    );

    assert_eq!(report.stats.nodes_expanded, 1);
    assert_eq!(report.stats.nodes_generated, 1);
    assert!(report.stats.node_budget_hit);
    assert!(report.outcome.complete_win_found);
    assert_eq!(
        report.stats.nodes_to_first_win,
        Some(0),
        "the exact-replayed root rollout witness was discovered before the main search generated a node"
    );
}

#[test]
fn terminal_turn_plan_seeds_skip_rollout_estimate() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 1,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::RootFrontierSeed,
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert_eq!(report.performance.root_rollout_estimate_calls, 1);
    assert_eq!(report.performance.turn_plan_seed_rollout_estimate_calls, 0);
    assert!(
        report.performance.terminal_turn_plan_seed_rollout_skips >= 1,
        "terminal turn-plan seed nodes should use terminal value instead of rollout"
    );
}

#[test]
fn turn_local_dominance_pruned_children_skip_rollout_estimate() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 1,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            child_rollout_policy: CombatSearchV2ChildRolloutPolicy::Immediate,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::DiagnosticOnly,
            ..CombatSearchV2Config::default()
        },
        &DuplicateUnresolvedChildStepper,
    );

    assert_eq!(report.stats.turn_local_dominance_prunes, 1);
    assert_eq!(report.performance.child_rollout_estimate_calls, 1);
    assert_eq!(report.performance.turn_local_dominance_rollout_skips, 1);
}

#[test]
fn lazy_child_rollout_is_completed_from_frontier_pop() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 2,
            rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
            child_rollout_policy: CombatSearchV2ChildRolloutPolicy::LazyOnPop,
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::DiagnosticOnly,
            ..CombatSearchV2Config::default()
        },
        &DuplicateUnresolvedChildStepper,
    );

    assert_eq!(report.performance.child_rollout_estimate_calls, 0);
    assert!(report.performance.deferred_child_rollout_nodes >= 1);
    assert!(report.performance.deferred_child_rollout_estimate_calls >= 1);
    assert!(report.performance.deferred_child_rollout_requeues >= 1);
}

#[test]
fn config_and_turn_plan_policy_defaults_match() {
    assert_eq!(
        CombatSearchV2Config::default().expansion_policy,
        CombatSearchV2ExpansionPolicy::AtomicActions
    );
    assert_eq!(
        CombatSearchV2Config::default().turn_plan_policy,
        CombatSearchV2TurnPlanPolicy::default()
    );
    assert_eq!(CombatSearchV2TurnPlanPolicy::default().label(), "disabled");
}

#[test]
fn default_turn_plan_policy_does_not_run_root_diagnostics() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 100)];

    let report = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            max_nodes: 2,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            ..CombatSearchV2Config::default()
        },
        &OneCardWinStepper,
    );

    assert_eq!(report.search_policy.turn_plan_policy, "disabled");
    assert_eq!(report.diagnostics.turn_plan.root_states_observed, 0);
    assert_eq!(report.performance.root_turn_plan_diagnostics_elapsed_us, 0);
}

#[test]
fn tactical_enemy_turn_plan_seed_gate_requires_healer_pair() {
    let mut combat = blank_test_combat();
    let mut healer = test_monster(EnemyId::Healer);
    healer.id = 2;
    combat.entities.monsters = vec![healer];

    assert!(!tactical_enemy_turn_plan_seed_gate(&test_search_node(
        combat.clone()
    )));

    let mut centurion = test_monster(EnemyId::Centurion);
    centurion.id = 1;
    combat.entities.monsters.push(centurion);

    assert!(tactical_enemy_turn_plan_seed_gate(&test_search_node(
        combat
    )));
}

#[test]
fn tactical_enemy_turn_plan_seed_gate_allows_fungi_swarm() {
    let mut combat = blank_test_combat();
    let mut first = test_monster(EnemyId::FungiBeast);
    first.id = 1;
    let mut second = test_monster(EnemyId::FungiBeast);
    second.id = 2;
    combat.entities.monsters = vec![first, second];

    assert!(!tactical_enemy_turn_plan_seed_gate(&test_search_node(
        combat.clone()
    )));

    let mut third = test_monster(EnemyId::FungiBeast);
    third.id = 3;
    combat.entities.monsters.push(third);

    assert!(tactical_enemy_turn_plan_seed_gate(&test_search_node(
        combat
    )));
}

#[test]
fn tactical_enemy_turn_plan_seed_gate_allows_boss_and_elite_boundaries() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];

    assert!(!tactical_enemy_turn_plan_seed_gate(&test_search_node(
        combat.clone()
    )));

    combat.meta.is_boss_fight = true;
    assert!(tactical_enemy_turn_plan_seed_gate(&test_search_node(
        combat.clone()
    )));

    combat.meta.is_boss_fight = false;
    combat.meta.is_elite_fight = true;
    assert!(tactical_enemy_turn_plan_seed_gate(&test_search_node(
        combat
    )));
}

#[test]
fn tactical_enemy_turn_plan_seed_gate_allows_visible_high_pressure() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 10;
    combat.entities.player.block = 0;
    let mut jaw_worm = test_monster(EnemyId::JawWorm);
    jaw_worm.set_planned_move_id(1);
    combat.entities.monsters = vec![jaw_worm];

    assert!(tactical_enemy_turn_plan_seed_gate(&test_search_node(
        combat
    )));
}

#[test]
fn tactical_turn_plan_policy_seeds_exact_states_with_turn_plan_prior_hints() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 80;
    combat.entities.player.block = 0;
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = test_search_node(combat);
    assert!(
        !tactical_enemy_turn_plan_seed_gate(&node),
        "fixture should be an ordinary low-pressure fight"
    );
    let state_hash = combat_exact_state_hash_v2(&node.engine, &node.combat);
    let config = CombatSearchV2Config {
        turn_plan_policy: CombatSearchV2TurnPlanPolicy::TacticalEnemyTurnBoundaryFrontierSeed,
        turn_plan_prior: Some(CombatSearchV2TurnPlanPrior::from_plan_scores([(
            state_hash,
            [(vec!["combat/end_turn".to_string()], 1.0)],
        )])),
        ..CombatSearchV2Config::default()
    };

    assert!(
        should_seed_turn_plan_at_node(
            &node,
            &config,
            &CombatSearchPluginStack::from_config(&config)
        ),
        "exact-state turn-plan hints should make tactical policy enumerate known prior states"
    );
}

fn test_search_node(combat: CombatState) -> SearchNode {
    SearchNode {
        engine: EngineState::CombatPlayerTurn,
        combat,
        actions: Vec::new(),
        turn_prefix: TurnPrefixState::default(),
        initial_hp: 80,
        potions_used: 0,
        potions_discarded: 0,
        cards_played: 0,
        potion_tactical_priority: 0,
        last_turn_branch_priority: 0,
        action_prior_score: None,
        action_ordering_frontier_hint: 0,
        rollout_estimate: RolloutNodeEstimate::unevaluated(),
        root_lineage: Default::default(),
    }
}

#[test]
fn turn_boundary_frontier_seed_extends_beyond_root_when_explicitly_enabled() {
    let mut combat = blank_test_combat();
    combat.turn.turn_count = 0;
    let mut monster = test_monster(EnemyId::JawWorm);
    monster.id = 1;
    monster.current_hp = 200;
    monster.max_hp = 200;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        100,
    )];
    let base_config = CombatSearchV2Config {
        max_nodes: 4,
        rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
        ..CombatSearchV2Config::default()
    };

    let root_only = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::RootFrontierSeed,
            ..base_config.clone()
        },
        &TwoTurnWinStepper,
    );

    let turn_boundary = run_combat_search_v2_with_stepper(
        &EngineState::CombatPlayerTurn,
        &combat,
        CombatSearchV2Config {
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::TurnBoundaryFrontierSeed,
            ..base_config
        },
        &TwoTurnWinStepper,
    );

    assert_eq!(root_only.diagnostics.turn_plan.frontier_seeded_nodes, 0);
    assert!(
        turn_boundary.diagnostics.turn_plan.frontier_seeded_nodes
            > root_only.diagnostics.turn_plan.frontier_seeded_nodes
    );
    assert_eq!(
        turn_boundary.search_policy.turn_plan_policy,
        "turn_boundary_frontier_seed"
    );
}
