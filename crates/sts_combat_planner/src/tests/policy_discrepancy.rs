use super::*;

struct DelayThirdApplyStepper {
    inner: TinyTurnStepper,
    calls: std::sync::atomic::AtomicUsize,
}

struct ImplicitPotionConsumptionStepper {
    inner: TinyTurnStepper,
    consumed_slot: usize,
}

impl CombatStepper for ImplicitPotionConsumptionStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        self.inner.atomic_actions(position)
    }

    fn legal_action_surface(&self, position: &CombatPosition) -> CombatLegalActionSurfaceV2 {
        self.inner.legal_action_surface(position)
    }

    fn supports_canonical_pending_choice_actions(&self) -> bool {
        self.inner.supports_canonical_pending_choice_actions()
    }

    fn is_legal_action(&self, position: &CombatPosition, input: &ClientInput) -> bool {
        self.inner.is_legal_action(position, input)
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        limits: CombatStepLimits,
    ) -> CombatStepResult {
        let mut result = self.inner.apply_to_stable(position, input, limits);
        if self.inner.terminal(&result.position) == CombatTerminal::Win {
            result.position.combat.entities.potions[self.consumed_slot] = None;
        }
        result
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        self.inner.terminal(position)
    }
}

impl DelayThirdApplyStepper {
    fn new() -> Self {
        Self {
            inner: TinyTurnStepper::plain(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl CombatStepper for DelayThirdApplyStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        self.inner.atomic_actions(position)
    }

    fn legal_action_surface(&self, position: &CombatPosition) -> CombatLegalActionSurfaceV2 {
        self.inner.legal_action_surface(position)
    }

    fn supports_canonical_pending_choice_actions(&self) -> bool {
        self.inner.supports_canonical_pending_choice_actions()
    }

    fn is_legal_action(&self, position: &CombatPosition, input: &ClientInput) -> bool {
        self.inner.is_legal_action(position, input)
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        limits: CombatStepLimits,
    ) -> CombatStepResult {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if call == 2 {
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        self.inner.apply_to_stable(position, input, limits)
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        self.inner.terminal(position)
    }
}

#[test]
fn exact_slot_mask_filters_use_and_discard_without_filtering_other_actions() {
    assert!(crate::witness::potion_input_uses_allowed_slot(
        &ClientInput::UsePotion {
            potion_index: 1,
            target: None,
        },
        Some(1_u64 << 1),
    ));
    assert!(!crate::witness::potion_input_uses_allowed_slot(
        &ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
        Some(1_u64 << 1),
    ));
    assert!(!crate::witness::potion_input_uses_allowed_slot(
        &ClientInput::DiscardPotion(0),
        Some(1_u64 << 1),
    ));
    assert!(crate::witness::potion_input_uses_allowed_slot(
        &ClientInput::EndTurn,
        Some(0),
    ));
}

#[test]
fn discrepancy_surface_can_disable_discard_without_changing_simulator_legality() {
    let mut combat = sts_core::test_support::blank_test_combat();
    let mut monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    monster.current_hp = 6;
    monster.max_hp = 6;
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![Some(Potion::new(PotionId::EnergyPotion, 7))];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
    let root = CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
        .expect("discard-policy root");
    let actions = [
        ClientInput::DiscardPotion(0),
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
    ];
    let config = PolicyDiscrepancyConfig {
        allow_potion_discard: false,
        ..PolicyDiscrepancyConfig::default()
    };
    let mut blocked =
        PolicyDiscrepancySession::with_policy(root.clone(), config, Arc::new(PreferPlayPolicy));

    assert!(blocked
        .audit_trajectory(&EngineCombatStepper, &actions)
        .is_err());

    let mut allowed = PolicyDiscrepancySession::with_policy(
        root,
        PolicyDiscrepancyConfig {
            allow_potion_discard: true,
            ..config
        },
        Arc::new(PreferPlayPolicy),
    );
    let audit = allowed
        .audit_trajectory(&EngineCombatStepper, &actions)
        .expect("all-legal discrepancy surface");
    assert_eq!(audit.terminal, CombatTerminal::Win);
}

#[test]
fn policy_discrepancy_search_follows_a_good_policy_to_terminal_truth() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            max_greedy_actions_per_dive: 8,
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );

    let report = search.advance(
        &TinyTurnStepper::lethal(),
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 128,
            additional_engine_steps: 512,
            deadline: None,
        },
    );

    assert_eq!(report.status, PolicyDiscrepancyStatus::WitnessFound);
    assert_eq!(report.after.policy_dives, 1);
    assert_eq!(report.witness.unwrap().actions.len(), 1);
}

#[test]
fn policy_discrepancy_rejects_implicit_potion_consumption_outside_identity_contract() {
    let mut position = root().position().clone();
    position.combat.entities.potions = vec![
        Some(Potion::new(PotionId::ColorlessPotion, 7)),
        Some(Potion::new(PotionId::FairyPotion, 8)),
    ];
    let root = CombatDecisionRoot::new(position).expect("implicit potion root");
    let mut search = PolicyDiscrepancySession::with_policy(
        root,
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            max_greedy_actions_per_dive: 8,
            max_potions_used: Some(1),
            allowed_potion_slots: Some(1 << 0),
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );
    let stepper = ImplicitPotionConsumptionStepper {
        inner: TinyTurnStepper::lethal(),
        consumed_slot: 1,
    };

    let report = search.advance(
        &stepper,
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 128,
            additional_engine_steps: 512,
            deadline: None,
        },
    );

    assert_ne!(report.status, PolicyDiscrepancyStatus::WitnessFound);
    assert!(report.witness.is_none());
}

#[test]
fn policy_discrepancy_trajectory_audit_uses_runtime_policy_probabilities() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferEndTurnPolicy),
    );

    let audit = search
        .audit_trajectory(&TinyTurnStepper::lethal(), &[PLAY])
        .expect("known legal winning trajectory should be auditable");

    assert_eq!(audit.source_action_count, 1);
    assert_eq!(audit.non_greedy_action_count, 1);
    assert_eq!(audit.terminal, CombatTerminal::Win);
    assert!(audit.total_weighted_discrepancy > 0.0);
    assert_eq!(audit.deviations[0].demonstrated_input, PLAY);
    assert_eq!(audit.deviations[0].greedy_input, ClientInput::EndTurn);
}

#[test]
fn policy_discrepancy_search_repairs_one_wrong_policy_action() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            max_greedy_actions_per_dive: 4,
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferEndTurnPolicy),
    );

    let report = search.advance(
        &TinyTurnStepper::lethal(),
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 128,
            additional_engine_steps: 512,
            deadline: None,
        },
    );

    assert_eq!(report.status, PolicyDiscrepancyStatus::WitnessFound);
    let witness = report
        .witness
        .expect("one discrepancy should repair policy");
    assert_eq!(
        witness.actions,
        exact_actions(&TinyTurnStepper::lethal(), &root(), [PLAY])
    );
    assert!(report.after.queued_discrepancies > 0);
}

#[test]
fn policy_discrepancy_search_split_quanta_match_one_shot_after_reroot() {
    let stepper = TinyTurnStepper::lethal_after_current_turn();
    let boundary = stepper.apply_to_stable(
        root().position(),
        ClientInput::EndTurn,
        CombatStepLimits {
            max_engine_steps: 4,
            deadline: None,
        },
    );
    let mut search = PolicyDiscrepancySession::with_policy(
        CombatDecisionRoot::new(boundary.position).expect("exact player-turn boundary"),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            max_greedy_actions_per_dive: 4,
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferEndTurnPolicy),
    );

    let mut last = None;
    for _ in 0..256 {
        let report = search.advance(
            &stepper,
            PolicyDiscrepancyQuantum {
                additional_applied_transitions: 8,
                additional_engine_steps: 32,
                deadline: None,
            },
        );
        if report.status == PolicyDiscrepancyStatus::WitnessFound {
            return;
        }
        last = Some(report);
    }

    let report = last.expect("at least one quantum");
    panic!(
        "split search failed: status={:?}, counters={:?}, frontier={}",
        report.status, report.after, report.frontier_entries
    );
}

#[test]
fn policy_discrepancy_search_resumes_the_same_deviation_after_budget_exhaustion() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );

    let interrupted = search.advance(
        &TinyTurnStepper::lethal(),
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 0,
            additional_engine_steps: 0,
            deadline: None,
        },
    );
    assert_eq!(
        interrupted.status,
        PolicyDiscrepancyStatus::Partial(PolicyDiscrepancyInterruption::AppliedTransitionBudget)
    );

    let resumed = search.advance(
        &TinyTurnStepper::lethal(),
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 1,
            additional_engine_steps: 4,
            deadline: None,
        },
    );
    assert_eq!(resumed.status, PolicyDiscrepancyStatus::WitnessFound);
    assert_eq!(resumed.after.applied_action_transitions, 1);
}

#[test]
fn policy_discrepancy_search_resumes_a_greedy_dive_after_its_depth_quantum() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            max_greedy_actions_per_dive: 1,
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferSelection22Policy),
    );

    let report = search.advance(
        &TinyTurnStepper::with_winning_single_selection(),
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 8,
            additional_engine_steps: 32,
            deadline: None,
        },
    );

    assert_eq!(report.status, PolicyDiscrepancyStatus::WitnessFound);
    assert_eq!(report.after.greedy_depth_limit_hits, 2);
    assert_eq!(report.witness.unwrap().actions.len(), 2);
}

#[test]
fn policy_discrepancy_search_uses_policy_order_for_single_card_selection() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferSelection22Policy),
    );

    let report = search.advance(
        &TinyTurnStepper::with_winning_single_selection(),
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 16,
            additional_engine_steps: 64,
            deadline: None,
        },
    );

    assert_eq!(report.status, PolicyDiscrepancyStatus::WitnessFound);
    assert_eq!(report.witness.unwrap().actions.len(), 2);
    assert_eq!(report.after.structured_inputs_materialized, 2);
}

#[test]
fn policy_discrepancy_search_keeps_turn_macros_lazy_when_policy_already_wins() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            turn_macro: Some(PolicyDiscrepancyTurnMacroConfig {
                max_applied_transitions: 8,
                proposals_per_view: 2,
                ..PolicyDiscrepancyTurnMacroConfig::default()
            }),
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );

    let report = search.advance(
        &TinyTurnStepper::lethal(),
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 16,
            additional_engine_steps: 64,
            deadline: None,
        },
    );

    assert_eq!(report.status, PolicyDiscrepancyStatus::WitnessFound);
    assert_eq!(report.after.applied_action_transitions, 1);
    assert_eq!(report.after.turn_macro_generations, 0);
}

#[test]
fn policy_discrepancy_search_can_use_a_complete_turn_macro_as_one_deviation() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            max_greedy_actions_per_dive: 4,
            turn_macro: Some(PolicyDiscrepancyTurnMacroConfig {
                max_applied_transitions: 8,
                proposals_per_view: 2,
                ..PolicyDiscrepancyTurnMacroConfig::default()
            }),
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferEndTurnPolicy),
    );

    let report = search.advance(
        &TinyTurnStepper::lethal(),
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 32,
            additional_engine_steps: 128,
            deadline: None,
        },
    );

    assert_eq!(report.status, PolicyDiscrepancyStatus::WitnessFound);
    assert_eq!(report.after.turn_macro_generations, 1);
    assert!(report.after.turn_macro_options_enqueued > 0);
    assert_eq!(report.witness.unwrap().actions.len(), 1);
}

#[test]
fn policy_discrepancy_turn_macro_waits_for_its_full_budget_across_quanta() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            max_greedy_actions_per_dive: 4,
            turn_macro: Some(PolicyDiscrepancyTurnMacroConfig {
                max_applied_transitions: 8,
                proposals_per_view: 1,
                ..PolicyDiscrepancyTurnMacroConfig::default()
            }),
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferEndTurnPolicy),
    );

    let mut last = None;
    for _ in 0..128 {
        let report = search.advance(
            &TinyTurnStepper::lethal(),
            PolicyDiscrepancyQuantum {
                additional_applied_transitions: 1,
                additional_engine_steps: 4,
                deadline: None,
            },
        );
        if report.status == PolicyDiscrepancyStatus::WitnessFound {
            assert!(report.after.turn_macro_generations > 0);
            return;
        }
        last = Some(report);
    }

    let report = last.expect("at least one quantum");
    panic!(
        "a split budget never completed the exact turn macro: status={:?}, counters={:?}, frontier={}",
        report.status, report.after, report.frontier_entries
    );
}

#[test]
fn policy_discrepancy_turn_macro_retries_after_an_internal_deadline() {
    let mut search = PolicyDiscrepancySession::with_policy(
        root(),
        PolicyDiscrepancyConfig {
            max_engine_steps_per_transition: 4,
            max_greedy_actions_per_dive: 1,
            turn_macro: Some(PolicyDiscrepancyTurnMacroConfig {
                max_applied_transitions: 8,
                proposals_per_view: 1,
                ..PolicyDiscrepancyTurnMacroConfig::default()
            }),
            ..PolicyDiscrepancyConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );
    let stepper = DelayThirdApplyStepper::new();

    let interrupted = search.advance(
        &stepper,
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 10,
            additional_engine_steps: 40,
            deadline: Some(std::time::Instant::now() + std::time::Duration::from_millis(10)),
        },
    );
    assert_eq!(
        interrupted.status,
        PolicyDiscrepancyStatus::Partial(PolicyDiscrepancyInterruption::Deadline)
    );
    assert_eq!(interrupted.after.turn_macro_generations, 1);
    assert_eq!(interrupted.after.turn_macro_deadline_retries, 1);

    let resumed = search.advance(
        &stepper,
        PolicyDiscrepancyQuantum {
            additional_applied_transitions: 8,
            additional_engine_steps: 32,
            deadline: None,
        },
    );
    assert!(
        resumed.after.turn_macro_generations >= 2,
        "the exact boundary macro was not returned to the frontier: {resumed:#?}"
    );
    assert_eq!(resumed.after.turn_macro_deadline_retries, 1);
}
