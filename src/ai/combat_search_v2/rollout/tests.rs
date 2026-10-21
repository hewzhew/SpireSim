use super::super::rollout_profile::RolloutPerformanceCounters;
use super::*;
use crate::content::monsters::EnemyId;
use crate::test_support::{blank_test_combat, test_monster};

#[derive(Clone, Copy)]
struct FirstActionWinsStepper;

impl CombatStepper for FirstActionWinsStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        vec![ClientInput::EndTurn]
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let engine = if matches!(input, ClientInput::EndTurn) {
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
struct PendingChoiceWinsStepper;

impl CombatStepper for PendingChoiceWinsStepper {
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
struct StalledEndTurnStepper;

impl CombatStepper for StalledEndTurnStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        vec![ClientInput::EndTurn]
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        _input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let position = CombatPosition::new(position.engine.clone(), position.combat.clone());
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
fn conservative_rollout_records_estimated_terminal_win() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode::root(EngineState::CombatPlayerTurn, combat);
    let config = CombatSearchV2Config::default();
    let mut performance = RolloutPerformanceCounters::default();

    let estimate = conservative_no_potion_rollout(
        &node,
        &FirstActionWinsStepper,
        &config,
        4,
        None,
        &mut performance,
    );

    assert!(estimate.evaluated);
    assert_eq!(estimate.terminal, SearchTerminalLabel::Win);
    assert!(!estimate.truncated);
}

#[test]
fn zero_action_budget_does_not_step_the_rollout() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode {
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
    };
    let mut performance = RolloutPerformanceCounters::default();

    let estimate = conservative_no_potion_rollout(
        &node,
        &StalledEndTurnStepper,
        &CombatSearchV2Config::default(),
        0,
        None,
        &mut performance,
    );

    assert_eq!(estimate.stop_reason, RolloutStopReason::MaxActions);
    assert_eq!(estimate.actions_simulated, 0);
    assert_eq!(estimate.total_actions, 0);
}

#[test]
fn rollout_cache_reuses_exact_state_estimate() {
    let mut cache = RolloutCache::new(
        CombatSearchV2RolloutPolicy::ConservativeNoPotion,
        4,
        4,
        3,
        0,
    );
    let node = SearchNode {
        engine: EngineState::CombatPlayerTurn,
        combat: blank_test_combat(),
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
    };
    let config = CombatSearchV2Config::default();

    let first = cache.estimate(&node, &FirstActionWinsStepper, &config, None, 0);
    let second = cache.estimate(&node, &FirstActionWinsStepper, &config, None, 0);

    assert_eq!(first, second);
    assert_eq!(cache.evaluations, 1);
    assert_eq!(cache.cache_hits, 1);
}

#[test]
fn rollout_report_includes_turn_beam_anchor_attribution() {
    let mut cache = RolloutCache::new(CombatSearchV2RolloutPolicy::TurnBeamNoPotion, 4, 4, 3, 0);
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode {
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
    };
    let config = CombatSearchV2Config {
        rollout_policy: CombatSearchV2RolloutPolicy::TurnBeamNoPotion,
        ..CombatSearchV2Config::default()
    };

    let estimate = cache.estimate(&node, &FirstActionWinsStepper, &config, None, 0);
    let report = cache.finish(None);

    assert_eq!(estimate.terminal, SearchTerminalLabel::Win);
    assert!(report.turn_beam_attribution.enabled);
    assert_eq!(report.turn_beam_attribution.calls, 1);
    assert_eq!(report.turn_beam_attribution.conservative_anchor_present, 1);
    assert_eq!(report.turn_beam_attribution.conservative_anchor_selected, 1);
    assert_eq!(
        report
            .turn_beam_attribution
            .conservative_anchor_terminal_wins,
        1
    );
    assert_eq!(report.turn_beam_attribution.extension_calls, 0);
}

#[test]
fn rollout_report_includes_turn_beam_extension_attribution() {
    let mut cache = RolloutCache::new(CombatSearchV2RolloutPolicy::TurnBeamNoPotion, 4, 2, 3, 0);
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode {
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
    };
    let config = CombatSearchV2Config {
        rollout_policy: CombatSearchV2RolloutPolicy::TurnBeamNoPotion,
        ..CombatSearchV2Config::default()
    };

    let estimate = cache.estimate(&node, &StalledEndTurnStepper, &config, None, 0);
    let report = cache.finish(None);

    assert_eq!(estimate.terminal, SearchTerminalLabel::Unresolved);
    assert_eq!(report.turn_beam_attribution.calls, 1);
    assert_eq!(report.turn_beam_attribution.conservative_anchor_present, 1);
    assert_eq!(report.turn_beam_attribution.extension_calls, 1);
    assert_eq!(report.turn_beam_attribution.turn_plan_calls, 1);
    assert_eq!(
        report.turn_beam_attribution.turn_plan_inner_nodes_generated,
        1
    );
    assert_eq!(report.turn_beam_attribution.best_pv_len, 0);
    assert_eq!(
        report.turn_beam_attribution.best_pv_terminal,
        Some(SearchTerminalLabel::Unresolved)
    );
}

#[test]
fn rollout_estimate_records_phase_adjusted_enemy_effort() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 1;
    guardian.current_hp = 180;
    guardian.max_hp = 240;
    guardian.block = 20;
    guardian.guardian.is_open = false;
    combat.entities.monsters = vec![guardian];
    let node = SearchNode {
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
    };

    let estimate = RolloutNodeEstimate::from_node(
        &node,
        0,
        RolloutStopReason::MaxActions,
        None,
        RolloutPendingChoiceProgress::default(),
    );

    assert_eq!(estimate.total_enemy_hp, 180);
    assert_eq!(estimate.total_enemy_block, 20);
    assert_eq!(estimate.phase_adjusted_enemy_effort, 200);
}

#[test]
fn conservative_rollout_stops_before_large_pending_choice_branch() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode {
        engine: EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
            cards: vec![crate::content::cards::CardId::Strike; 7],
            card_lineages: crate::test_support::scenario_lineages(0..7),
        }),
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
    };
    let config = CombatSearchV2Config::default();
    let mut performance = RolloutPerformanceCounters::default();

    let estimate = conservative_no_potion_rollout(
        &node,
        &FirstActionWinsStepper,
        &config,
        4,
        None,
        &mut performance,
    );

    assert!(estimate.evaluated);
    assert!(estimate.truncated);
    assert_eq!(
        estimate.stop_reason,
        RolloutStopReason::HighFanoutPendingChoice
    );
    assert!(estimate.high_fanout_pending_choice);
    assert_eq!(estimate.pending_choice_estimated_action_fanout, 128);
    assert_eq!(estimate.pending_choices_seen, 1);
    assert_eq!(estimate.pending_choice_actions_simulated, 0);
    assert_eq!(estimate.max_pending_choice_candidate_count, 7);
    assert_eq!(estimate.max_pending_choice_estimated_action_fanout, 128);
    assert_eq!(estimate.last_pending_choice_kind, Some("scry_select"));
    assert!(estimate.stopped_on_high_fanout_pending_choice);
    assert_eq!(estimate.actions_simulated, 0);
}

#[test]
fn engine_rollout_stops_at_nonlinear_structured_pending_choice() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![
        crate::runtime::combat::CombatCard::for_scenario(crate::content::cards::CardId::Strike, 10),
        crate::runtime::combat::CombatCard::for_scenario(crate::content::cards::CardId::Strike, 11),
    ];
    let node = SearchNode {
        engine: EngineState::PendingChoice(crate::state::core::PendingChoice::HandSelect {
            candidate_lineages: crate::test_support::scenario_lineages([10, 11]),
            min_cards: 1,
            max_cards: 2,
            can_cancel: false,
            reason: crate::state::core::HandSelectReason::Discard,
        }),
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
    };
    let config = CombatSearchV2Config::default();
    let mut performance = RolloutPerformanceCounters::default();

    let estimate = conservative_no_potion_rollout(
        &node,
        &EngineCombatStepper,
        &config,
        4,
        None,
        &mut performance,
    );

    assert_eq!(
        estimate.stop_reason,
        RolloutStopReason::StructuredPendingChoice
    );
    assert_eq!(estimate.actions_simulated, 0);
    assert_eq!(estimate.pending_choices_seen, 1);
    assert_eq!(estimate.pending_choice_actions_simulated, 0);
}

#[test]
fn conservative_rollout_tracks_small_pending_choice_resolution() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode {
        engine: EngineState::PendingChoice(crate::state::core::PendingChoice::StanceChoice),
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
    };
    let config = CombatSearchV2Config::default();
    let mut performance = RolloutPerformanceCounters::default();

    let estimate = conservative_no_potion_rollout(
        &node,
        &PendingChoiceWinsStepper,
        &config,
        4,
        None,
        &mut performance,
    );

    assert_eq!(estimate.terminal, SearchTerminalLabel::Win);
    assert!(!estimate.truncated);
    assert_eq!(estimate.pending_choices_seen, 1);
    assert_eq!(estimate.pending_choice_actions_simulated, 1);
    assert_eq!(estimate.max_pending_choice_candidate_count, 2);
    assert_eq!(estimate.max_pending_choice_estimated_action_fanout, 2);
    assert_eq!(estimate.last_pending_choice_kind, Some("stance_choice"));
    assert!(!estimate.stopped_on_high_fanout_pending_choice);
    assert_eq!(estimate.actions_simulated, 1);
}

#[test]
fn turn_beam_rollout_preserves_conservative_anchor_win() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode {
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
    };
    let config = CombatSearchV2Config {
        rollout_policy: CombatSearchV2RolloutPolicy::TurnBeamNoPotion,
        rollout_beam_width: 1,
        ..CombatSearchV2Config::default()
    };

    let estimate = turn_beam_no_potion_rollout(&node, &FirstActionWinsStepper, &config, 4, None);

    assert!(estimate.evaluated);
    assert_eq!(estimate.terminal, SearchTerminalLabel::Win);
    assert_eq!(estimate.actions_simulated, 1);
    assert_eq!(
        estimate.last_action_reason,
        Some("turn_beam_no_potion_conservative_anchor")
    );
}

#[test]
fn turn_beam_rollout_resolves_small_pending_choice_via_conservative_fallback() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode {
        engine: EngineState::PendingChoice(crate::state::core::PendingChoice::StanceChoice),
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
    };
    let config = CombatSearchV2Config {
        rollout_policy: CombatSearchV2RolloutPolicy::TurnBeamNoPotion,
        ..CombatSearchV2Config::default()
    };

    let estimate = turn_beam_no_potion_rollout(&node, &PendingChoiceWinsStepper, &config, 4, None);

    assert!(estimate.evaluated);
    assert_eq!(estimate.terminal, SearchTerminalLabel::Win);
    assert_eq!(estimate.stop_reason, RolloutStopReason::TerminalState);
    assert_eq!(estimate.pending_choices_seen, 1);
    assert_eq!(estimate.pending_choice_actions_simulated, 1);
    assert_eq!(estimate.actions_simulated, 1);
    assert_eq!(
        estimate.last_action_reason,
        Some("turn_beam_no_potion_conservative_boundary_fallback")
    );
}

#[test]
fn turn_beam_rollout_still_stops_before_large_pending_choice_branch() {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    let node = SearchNode {
        engine: EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
            cards: vec![crate::content::cards::CardId::Strike; 7],
            card_lineages: crate::test_support::scenario_lineages(0..7),
        }),
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
    };
    let config = CombatSearchV2Config {
        rollout_policy: CombatSearchV2RolloutPolicy::TurnBeamNoPotion,
        ..CombatSearchV2Config::default()
    };

    let estimate = turn_beam_no_potion_rollout(&node, &FirstActionWinsStepper, &config, 4, None);

    assert!(estimate.evaluated);
    assert_eq!(estimate.terminal, SearchTerminalLabel::Unresolved);
    assert_eq!(
        estimate.stop_reason,
        RolloutStopReason::HighFanoutPendingChoice
    );
    assert!(estimate.truncated);
    assert_eq!(estimate.pending_choices_seen, 1);
    assert_eq!(estimate.pending_choice_actions_simulated, 0);
    assert!(estimate.stopped_on_high_fanout_pending_choice);
    assert_eq!(estimate.last_action_reason, None);
}
