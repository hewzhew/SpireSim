use super::super::rollout_profile::RolloutPerformanceCounters;
use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::runtime::combat::{CombatCard, Power, PowerPayload};
use crate::sim::combat::{CombatPosition, CombatStepLimits};
use crate::test_support::{blank_test_combat, test_monster};

#[derive(Clone, Copy)]
struct ProbeStepper {
    damage_on_card_index: Option<usize>,
    player_hp_loss_on_damage: i32,
}

impl CombatStepper for ProbeStepper {
    fn atomic_actions(&self, _position: &CombatPosition) -> Vec<ClientInput> {
        Vec::new()
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> crate::sim::combat::CombatStepResult {
        let mut combat = position.combat.clone();
        if let ClientInput::PlayCard { card_index, .. } = input {
            if Some(card_index) == self.damage_on_card_index {
                if let Some(monster) = combat.entities.monsters.first_mut() {
                    monster.current_hp = monster.current_hp.saturating_sub(10);
                }
                combat.entities.player.current_hp = combat
                    .entities
                    .player
                    .current_hp
                    .saturating_sub(self.player_hp_loss_on_damage);
            }
        }
        let position = CombatPosition::new(position.engine.clone(), combat);
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
fn one_step_probe_can_choose_nonterminal_special_phase_value_upgrade() {
    let combat = split_debt_combat();
    let node = test_node(combat.clone());
    let ordered = vec![
        indexed_choice(&combat, 0, ClientInput::EndTurn),
        indexed_choice(
            &combat,
            1,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];

    let OneStepProbeSelection::Upgrade {
        choice: selection,
        reason,
        ..
    } = choose_by_one_step_probe(
        &node,
        &ProbeStepper {
            damage_on_card_index: Some(1),
            player_hp_loss_on_damage: 0,
        },
        &test_config(),
        None,
        &ordered,
        true,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    )
    else {
        panic!("phase progress without hp regression should be eligible");
    };

    assert_eq!(selection.original_action_id, 1);
    assert_eq!(
        reason,
        super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_PHASE_VALUE
    );
}

#[test]
fn one_step_probe_rejects_phase_upgrade_with_hp_regression() {
    let combat = split_debt_combat();
    let node = test_node(combat.clone());
    let ordered = vec![
        indexed_choice(&combat, 0, ClientInput::EndTurn),
        indexed_choice(
            &combat,
            1,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];

    let selection = choose_by_one_step_probe(
        &node,
        &ProbeStepper {
            damage_on_card_index: Some(1),
            player_hp_loss_on_damage: 1,
        },
        &test_config(),
        None,
        &ordered,
        true,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    );

    assert!(matches!(selection, OneStepProbeSelection::Fallback { .. }));
}

#[test]
fn one_step_probe_terminal_only_mode_rejects_nonterminal_phase_upgrade() {
    let combat = split_debt_combat();
    let node = test_node(combat.clone());
    let ordered = vec![
        indexed_choice(&combat, 0, ClientInput::EndTurn),
        indexed_choice(
            &combat,
            1,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];

    let selection = choose_by_one_step_probe(
        &node,
        &ProbeStepper {
            damage_on_card_index: Some(1),
            player_hp_loss_on_damage: 0,
        },
        &test_config(),
        None,
        &ordered,
        false,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    );

    assert!(matches!(selection, OneStepProbeSelection::Fallback { .. }));
}

#[test]
fn one_step_probe_can_choose_sustained_mitigation_from_action_facts() {
    let combat = attacking_guardian_with_hand(vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Disarm, 11),
    ]);
    let node = test_node(combat.clone());
    let ordered = vec![
        indexed_choice(
            &combat,
            0,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ),
        indexed_choice(
            &combat,
            1,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
        ),
    ];

    let OneStepProbeSelection::Upgrade {
        choice: selection,
        reason,
        ..
    } = choose_by_one_step_probe(
        &node,
        &ProbeStepper {
            damage_on_card_index: None,
            player_hp_loss_on_damage: 0,
        },
        &test_config(),
        None,
        &ordered,
        true,
        &mut RolloutPerformanceCounters::default(),
        OracleCombatRolloutContractV1::default(),
    )
    else {
        panic!("Disarm facts should be eligible when it does not regress survival");
    };

    assert_eq!(selection.original_action_id, 1);
    assert_eq!(
        reason,
        super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_ACTION_FACTS_VALUE
    );
}

#[test]
fn probe_upgrade_reason_accepts_hp_gain_as_survival_value() {
    let fallback = score_with_survival(30, 5, 0);
    let candidate = score_with_survival(31, 5, 0);

    assert_eq!(
        probe_upgrade_reason(candidate, fallback, true),
        Some(
            super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_SURVIVAL_VALUE
        )
    );
}

#[test]
fn probe_upgrade_reason_rejects_block_only_survival_value_without_visible_hp_loss_reduction() {
    let fallback = score_with_survival(30, 30, 0);
    let candidate = score_with_survival(30, 35, 0);

    assert_eq!(probe_upgrade_reason(candidate, fallback, true), None);
}

#[test]
fn probe_upgrade_reason_rejects_survival_gain_that_drops_resource_urgency() {
    let mut fallback = score_with_survival(30, 5, 8);
    fallback.recoverable_resource_urgency = 1;
    let candidate = score_with_survival(30, 10, 3);

    assert_eq!(probe_upgrade_reason(candidate, fallback, true), None);
}

#[test]
fn probe_upgrade_reason_rejects_nonterminal_end_turn_phase_upgrade() {
    let fallback = score_with_survival(30, 5, 0);
    let mut candidate = fallback;
    candidate.split_debt_stability = 10;
    candidate.nonterminal_upgrade_eligible = false;

    assert_eq!(probe_upgrade_reason(candidate, fallback, true), None);
}

#[test]
fn probe_upgrade_reason_accepts_reduced_visible_hp_loss() {
    let fallback = score_with_survival(30, 5, 6);
    let candidate = score_with_survival(30, 10, 1);

    assert_eq!(
        probe_upgrade_reason(candidate, fallback, true),
        Some(
            super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_SURVIVAL_VALUE
        )
    );
}

#[test]
fn probe_upgrade_reason_accepts_action_fact_setup_value() {
    let fallback = score_with_survival(30, 5, 0);
    let mut candidate = fallback;
    candidate.action_debuff_setup = 2;

    assert_eq!(
        probe_upgrade_reason(candidate, fallback, true),
        Some(
            super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_ACTION_FACTS_VALUE
        )
    );
}

#[test]
fn probe_upgrade_reason_rejects_fact_upgrade_with_reactive_risk_regression() {
    let fallback = score_with_survival(30, 5, 0);
    let mut candidate = fallback;
    candidate.action_debuff_setup = 2;
    candidate.action_reactive_safety = -1;

    assert_eq!(probe_upgrade_reason(candidate, fallback, true), None);
}

#[test]
fn probe_upgrade_reason_allows_survival_gain_before_reactive_risk_gate() {
    let fallback = score_with_survival(30, 5, 0);
    let mut candidate = score_with_survival(31, 5, 0);
    candidate.action_reactive_safety = -1;

    assert_eq!(
        probe_upgrade_reason(candidate, fallback, true),
        Some(
            super::super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_SURVIVAL_VALUE
        )
    );
}

#[test]
fn probe_score_prefers_lower_visible_hp_loss_before_late_tie_breakers() {
    let mut fallback = score_with_survival(30, 5, 8);
    fallback.action_progress_hint = 10;
    let candidate = score_with_survival(30, 5, 3);

    assert!(candidate > fallback);
}

fn indexed_choice(
    combat: &CombatState,
    original_action_id: usize,
    input: ClientInput,
) -> IndexedActionChoice {
    IndexedActionChoice {
        original_action_id,
        choice: CombatActionChoice::from_input(combat, input),
    }
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

fn split_debt_combat() -> CombatState {
    let mut combat = blank_test_combat();
    let mut monster = test_monster(EnemyId::AcidSlimeL);
    monster.id = 1;
    monster.current_hp = 30;
    monster.max_hp = 65;
    combat.entities.monsters = vec![monster];
    combat.entities.power_db.insert(
        1,
        vec![Power {
            power_type: PowerId::Split,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    combat
}

fn attacking_guardian_with_hand(hand: Vec<CombatCard>) -> CombatState {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.id = 1;
    guardian.set_planned_move_id(4);
    combat.entities.monsters = vec![guardian];
    combat.zones.hand = hand;
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

fn score_with_survival(
    final_hp: i32,
    survival_margin: i32,
    visible_hp_loss: i32,
) -> RolloutActionProbeScore {
    RolloutActionProbeScore {
        terminal_rank: terminal_rank(SearchTerminalLabel::Unresolved),
        recoverable_resource_urgency: 0,
        final_hp,
        survival_margin,
        visible_hp_loss,
        living_enemy_progress: -1,
        phase_adjusted_enemy_progress: -30,
        split_debt_stability: 0,
        mechanics_stability: 0,
        action_sustained_mitigation: 0,
        action_visible_mitigation: 0,
        action_debuff_setup: 0,
        action_resource_timing: 0,
        action_progress_hint: 0,
        action_access_gain: 0,
        action_reactive_safety: 0,
        pending_choice_fanout: 0,
        ordered_preference: 0,
        nonterminal_upgrade_eligible: true,
    }
}
