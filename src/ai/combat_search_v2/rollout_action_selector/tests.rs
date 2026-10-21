use super::*;
use crate::ai::combat_search_v2::rollout_profile::RolloutPerformanceCounters;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::runtime::combat::{CombatCard, Power, PowerPayload};
use crate::sim::combat::{CombatPosition, CombatStepLimits, EngineCombatStepper};
use crate::test_support::{blank_test_combat, planned_monster, test_monster};

#[derive(Clone, Copy)]
struct ProbeWinStepper;

impl CombatStepper for ProbeWinStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        Vec::new()
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let engine = if matches!(input, ClientInput::PlayCard { .. }) {
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

    fn terminal(&self, position: &CombatPosition) -> crate::sim::combat::CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[derive(Clone, Copy)]
struct EscapeWinStepper;

impl CombatStepper for EscapeWinStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        Vec::new()
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

    fn terminal(&self, position: &CombatPosition) -> crate::sim::combat::CombatTerminal {
        combat_terminal(&position.engine, &position.combat)
    }
}

#[test]
fn conservative_rollout_policy_filters_potion_actions() {
    let combat = blank_test_combat();
    let legal = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        ),
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
    ];

    let filtered = filtered_rollout_legal_actions(
        CombatSearchRolloutPluginId::ConservativeNoPotion,
        legal,
        &combat,
    );

    assert_eq!(filtered.len(), 1);
    assert!(matches!(filtered[0].input, ClientInput::EndTurn));
}

#[test]
fn conservative_rollout_policy_reports_selection_reason() {
    let combat = non_terminal_combat();
    let legal = vec![CombatActionChoice::from_input(
        &combat,
        ClientInput::EndTurn,
    )];

    let selection = choose_rollout_action(
        CombatSearchRolloutPluginId::ConservativeNoPotion,
        &test_node(combat.clone()),
        &ProbeWinStepper,
        &test_config(),
        None,
        &EngineState::CombatPlayerTurn,
        &combat,
        legal,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    )
    .expect("single legal action should be selected");

    assert_eq!(
        selection.reason,
        ROLLOUT_ACTION_REASON_CONSERVATIVE_ORDERING_FIRST
    );
    assert!(matches!(
        selection.choice.choice.input,
        ClientInput::EndTurn
    ));
}

#[test]
fn conservative_rollout_probe_can_select_non_first_terminal_win() {
    let combat = non_terminal_combat();
    let legal = vec![
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 99,
                target: None,
            },
        ),
    ];

    let selection = choose_rollout_action(
        CombatSearchRolloutPluginId::ConservativeNoPotion,
        &test_node(combat.clone()),
        &ProbeWinStepper,
        &test_config(),
        None,
        &EngineState::CombatPlayerTurn,
        &combat,
        legal,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    )
    .expect("probe should select an action");

    assert_eq!(
        selection.reason,
        ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_PROBE
    );
    assert!(matches!(
        selection.choice.choice.input,
        ClientInput::PlayCard { .. }
    ));
}

#[test]
fn contract_aware_rollout_does_not_upgrade_to_a_thief_escape_win() {
    let mut combat = blank_test_combat();
    let mut mugger = test_monster(EnemyId::Mugger);
    mugger.id = 1;
    mugger.current_hp = 30;
    mugger.max_hp = 30;
    mugger.thief.stolen_gold = 30;
    combat.entities.monsters = vec![mugger];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    let legal = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
    ];

    let unconstrained = choose_rollout_action(
        CombatSearchRolloutPluginId::PhaseAwareNoPotion,
        &test_node(combat.clone()),
        &EscapeWinStepper,
        &test_config(),
        None,
        &EngineState::CombatPlayerTurn,
        &combat,
        legal.clone(),
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    )
    .expect("unconstrained rollout should select an action");
    assert!(matches!(
        unconstrained.choice.choice.input,
        ClientInput::EndTurn
    ));

    let constrained = choose_rollout_action(
        CombatSearchRolloutPluginId::PhaseAwareNoPotion,
        &test_node(combat.clone()),
        &EscapeWinStepper,
        &test_config(),
        None,
        &EngineState::CombatPlayerTurn,
        &combat,
        legal,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1 {
            minimum_final_hp: None,
            require_no_unrecovered_stolen_gold: true,
        },
    )
    .expect("contract-aware rollout should select an action");
    assert!(matches!(
        constrained.choice.choice.input,
        ClientInput::PlayCard { .. }
    ));
}

#[test]
fn conservative_rollout_probe_does_not_rescore_fallback_candidate() {
    let combat = non_terminal_combat();
    let legal = vec![
        CombatActionChoice::from_input(&combat, ClientInput::EndTurn),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 99,
                target: None,
            },
        ),
    ];
    let mut performance = RolloutPerformanceCounters::default();

    let selection = choose_rollout_action(
        CombatSearchRolloutPluginId::ConservativeNoPotion,
        &test_node(combat.clone()),
        &ProbeWinStepper,
        &test_config(),
        None,
        &EngineState::CombatPlayerTurn,
        &combat,
        legal,
        &mut performance,
        OracleCombatRolloutContractV1::default(),
    )
    .expect("probe should select an action");

    assert_eq!(
        selection.reason,
        ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_PROBE
    );
    assert!(selection.cached_step.is_some());
    assert_eq!(performance.no_potion_probe_score_calls, 2);
    assert_eq!(performance.no_potion_probe_actions_evaluated, 2);
}

#[test]
fn conservative_rollout_reuses_timed_threat_ordering() {
    let mut combat = blank_test_combat();
    let mut neutral = test_monster(EnemyId::Repulsor);
    neutral.id = 1;
    neutral.current_hp = 40;
    neutral.max_hp = 40;
    let mut timed = test_monster(EnemyId::Exploder);
    timed.id = 2;
    timed.current_hp = 40;
    timed.max_hp = 40;
    combat.entities.monsters = vec![neutral, timed];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
    combat.entities.power_db.insert(
        2,
        vec![Power {
            power_type: PowerId::Explosive,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let legal = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(2),
            },
        ),
    ];

    let selection = choose_rollout_action(
        CombatSearchRolloutPluginId::ConservativeNoPotion,
        &test_node(combat.clone()),
        &ProbeWinStepper,
        &test_config(),
        None,
        &EngineState::CombatPlayerTurn,
        &combat,
        legal,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    )
    .expect("rollout should select an action");

    assert!(matches!(
        selection.choice.choice.input,
        ClientInput::PlayCard {
            target: Some(2),
            ..
        }
    ));
}

#[test]
fn conservative_rollout_reuses_attack_retaliation_ordering() {
    let mut combat = blank_test_combat();
    let mut spiker = test_monster(EnemyId::Spiker);
    spiker.id = 1;
    spiker.current_hp = 40;
    spiker.max_hp = 40;
    combat.entities.monsters = vec![spiker];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::TwinStrike, 10),
        CombatCard::for_scenario(CardId::Strike, 11),
    ];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Thorns,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let legal = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];

    let selection = choose_rollout_action(
        CombatSearchRolloutPluginId::ConservativeNoPotion,
        &test_node(combat.clone()),
        &ProbeWinStepper,
        &test_config(),
        None,
        &EngineState::CombatPlayerTurn,
        &combat,
        legal,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    )
    .expect("rollout should select an action");

    assert!(matches!(
        selection.choice.choice.input,
        ClientInput::PlayCard { card_index: 1, .. }
    ));
}

#[test]
fn phase_aware_probe_cannot_replace_thief_damage_setup_with_nonlethal_block() {
    let mut combat = blank_test_combat();
    combat.entities.player.current_hp = 85;
    combat.turn.energy = 3;
    let mut looter = planned_monster(EnemyId::Looter, 1);
    looter.id = 1;
    looter.current_hp = 47;
    looter.max_hp = 47;
    let mut mugger = planned_monster(EnemyId::Mugger, 1);
    mugger.id = 2;
    mugger.current_hp = 48;
    mugger.max_hp = 48;
    combat.entities.monsters = vec![looter, mugger];
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 10),
        CombatCard::for_scenario(CardId::Defend, 11),
        CombatCard::for_scenario(CardId::FiendFire, 12),
        CombatCard::for_scenario(CardId::ThunderClap, 13),
    ];
    let legal = vec![
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 2,
                target: Some(1),
            },
        ),
        CombatActionChoice::from_input(
            &combat,
            ClientInput::PlayCard {
                card_index: 3,
                target: None,
            },
        ),
    ];

    let selection = choose_rollout_action(
        CombatSearchRolloutPluginId::PhaseAwareNoPotion,
        &test_node(combat.clone()),
        &EngineCombatStepper,
        &test_config(),
        None,
        &EngineState::CombatPlayerTurn,
        &combat,
        legal,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1 {
            minimum_final_hp: Some(66),
            require_no_unrecovered_stolen_gold: true,
        },
    )
    .expect("rollout should select one legal action");

    assert!(matches!(
        selection.choice.choice.input,
        ClientInput::PlayCard { card_index: 3, .. }
    ));
}

fn test_node(combat: CombatState) -> SearchNode {
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

fn non_terminal_combat() -> CombatState {
    let mut combat = blank_test_combat();
    combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
    combat
}

fn test_config() -> CombatSearchV2Config {
    CombatSearchV2Config {
        max_nodes: 100,
        max_actions_per_line: 100,
        max_engine_steps_per_action: 10,
        wall_time: None,
        satisfaction: CombatSearchV2Satisfaction::ZeroLossOrBudget,
        input_label: None,
        potion_policy: CombatSearchV2PotionPolicy::Never,
        max_potions_used: None,
        allowed_potion_slots: None,
        allow_potion_discard: None,
        rollout_policy: CombatSearchV2RolloutPolicy::ConservativeNoPotion,
        child_rollout_policy: CombatSearchV2ChildRolloutPolicy::Immediate,
        rollout_max_evaluations: 10,
        rollout_max_actions: 10,
        rollout_beam_width: 3,
        expansion_policy: CombatSearchV2ExpansionPolicy::AtomicActions,
        turn_plan_policy: CombatSearchV2TurnPlanPolicy::DiagnosticOnly,
        phase_guard_policy: CombatSearchV2PhaseGuardPolicy::Default,
        setup_bias_policy: CombatSearchV2SetupBiasPolicy::Default,
        turn_plan_probe_max_inner_nodes: None,
        turn_plan_probe_max_partial_frontier_states: None,
        turn_plan_probe_max_end_states: None,
        turn_plan_probe_per_bucket_limit: None,
        root_action_prior: None,
        turn_plan_prior: None,
    }
}
