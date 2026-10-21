use super::*;

#[test]
fn depth_beam_keeps_long_partial_after_a_short_turn_option_finishes() {
    let stepper = TinyTurnStepper::plain();
    let report = generate_depth_beam_turn_options(
        root(),
        DepthBeamTurnConfig {
            generator: config(),
            max_potion_expenditures: None,
            partial_beam_width: 1,
            retained_per_view: 1,
            max_atomic_depth: 4,
            max_structured_members_per_family: 8,
        },
        DepthBeamTurnBudget {
            max_applied_transitions: 8,
            max_engine_steps: 32,
            deadline: None,
        },
        Arc::new(PreferPlayPolicy),
        &stepper,
    );

    assert_eq!(report.status, DepthBeamTurnStatus::Complete);
    assert!(report.options.iter().any(|option| {
        option.actions().len() == 1 && option.actions()[0].input == ClientInput::EndTurn
    }));
    assert!(report.options.iter().any(|option| {
        option.actions().len() == 2
            && option.actions()[0].input == PLAY
            && option.actions()[1].input == ClientInput::EndTurn
    }));
}

#[test]
fn depth_beam_crosses_a_singleton_structured_selection() {
    let stepper = TinyTurnStepper::with_single_selection();
    let report = generate_depth_beam_turn_options(
        root(),
        DepthBeamTurnConfig {
            generator: config(),
            max_potion_expenditures: None,
            partial_beam_width: 1,
            retained_per_view: 1,
            max_atomic_depth: 5,
            max_structured_members_per_family: 8,
        },
        DepthBeamTurnBudget {
            max_applied_transitions: 12,
            max_engine_steps: 48,
            deadline: None,
        },
        Arc::new(PreferSelection22Policy),
        &stepper,
    );

    let selected = report
        .options
        .iter()
        .find(|option| option.actions().len() == 3)
        .expect("play, singleton choice, and end turn should form one proposal");
    assert_eq!(
        report.status,
        DepthBeamTurnStatus::Partial(DepthBeamTurnInterruption::BeamPruned)
    );
    assert_eq!(report.counters.pruned_partial_states, 1);
    assert!(matches!(
        &selected.actions()[1].input,
        ClientInput::SubmitSelection(resolution)
            if resolution.selected_card_lineages()
                == [sts_core::test_support::scenario_lineage(22)]
    ));
}

#[test]
fn depth_beam_reports_a_truncated_structured_family_as_partial() {
    let report = generate_depth_beam_turn_options(
        root(),
        DepthBeamTurnConfig {
            generator: config(),
            max_potion_expenditures: None,
            partial_beam_width: 8,
            retained_per_view: 1,
            max_atomic_depth: 5,
            max_structured_members_per_family: 1,
        },
        DepthBeamTurnBudget {
            max_applied_transitions: 12,
            max_engine_steps: 48,
            deadline: None,
        },
        Arc::new(PreferSelection22Policy),
        &TinyTurnStepper::with_single_selection(),
    );

    assert_eq!(
        report.status,
        DepthBeamTurnStatus::Partial(DepthBeamTurnInterruption::StructuredFamilyLimit)
    );
    assert_eq!(report.counters.truncated_structured_families, 1);
}
