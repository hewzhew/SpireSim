use crate::sim::combat::{CombatPosition, CombatStepper, EngineCombatStepper};

use super::pending_choice_action_prefix::canonical_pending_choice_inputs;
use super::pending_choice_fanout::pending_choice_fanout;
use super::*;

mod candidate_probe;
mod report;
mod types;
use report::{config_report, selected_first_action, trajectory_summary};
pub use types::{
    CombatSearchV2ActionFactsReport, CombatSearchV2DecisionCandidateReport,
    CombatSearchV2DecisionContext, CombatSearchV2DecisionMicroscopeConfigReport,
    CombatSearchV2DecisionMicroscopeReport, CombatSearchV2DecisionOneStepReport,
    CombatSearchV2DecisionSelectedAction, CombatSearchV2DecisionTrajectorySummary,
};

const CANDIDATE_REPORT_LIMIT: usize = 24;

pub fn explain_combat_search_v2_initial_decision(
    engine: &EngineState,
    combat: &CombatState,
    config: CombatSearchV2Config,
) -> CombatSearchV2DecisionMicroscopeReport {
    explain_combat_search_v2_initial_decision_with_stepper(
        engine,
        combat,
        config,
        &EngineCombatStepper,
    )
}

fn explain_combat_search_v2_initial_decision_with_stepper(
    engine: &EngineState,
    combat: &CombatState,
    config: CombatSearchV2Config,
    stepper: &impl CombatStepper,
) -> CombatSearchV2DecisionMicroscopeReport {
    let plugin_stack = CombatSearchPluginStack::from_config(&config);
    let action_ordering_plugins = CombatSearchActionOrderingPlugins::from_stack(
        config.root_action_prior.as_ref(),
        &plugin_stack,
    );
    let search_report = run_combat_search_v2_with_stepper(engine, combat, config.clone(), stepper);
    let selected_trace = selected_first_trace(&search_report);
    let selected_first_action =
        selected_first_action(engine, combat, action_ordering_plugins, &search_report);
    let selected_action_key = selected_first_action
        .as_ref()
        .map(|action| action.action_key.as_str());
    let initial_node = SearchNode::root(engine.clone(), combat.clone());
    let position = CombatPosition::new(engine.clone(), combat.clone());
    let (candidate_count, ordered_choices) = microscope_candidate_sample(
        engine,
        combat,
        &position,
        stepper,
        plugin_stack.potion.policy,
        action_ordering_plugins,
        selected_trace,
    );
    let candidates = ordered_choices
        .iter()
        .take(CANDIDATE_REPORT_LIMIT)
        .enumerate()
        .map(|(ordered_index, choice)| {
            candidate_probe::candidate_report(
                &initial_node,
                stepper,
                &config,
                action_ordering_plugins,
                choice,
                ordered_index,
                selected_action_key,
            )
        })
        .collect();

    CombatSearchV2DecisionMicroscopeReport {
        schema_name: "CombatSearchV2DecisionMicroscopeReport",
        schema_version: 2,
        question: "why_was_this_action_selected_and_where_might_it_be_wrong",
        behavioral_scope: "diagnostic_only_no_prune_no_policy_change_no_artifact_promotion",
        input_label: config.input_label.clone(),
        config: config_report(&config),
        search_outcome: search_report.outcome.clone(),
        best_complete_summary: search_report
            .best_complete_trajectory
            .as_ref()
            .map(trajectory_summary),
        best_win_summary: search_report
            .best_win_trajectory
            .as_ref()
            .map(trajectory_summary),
        selected_first_action,
        initial_context: CombatSearchV2DecisionContext {
            state: summarize_state(engine, combat),
            phase_profile: combat_search_phase_profile_report(combat_search_phase_profile(
                engine, combat,
            )),
            frontier_value: combat_search_frontier_value_report(&initial_node),
        },
        candidate_count,
        reported_candidate_limit: CANDIDATE_REPORT_LIMIT,
        candidates,
        notes: vec![
            "this report explains the initial decision boundary only",
            "selected_first_action comes from the best complete trajectory found under the current budget",
            "candidate one-step probes are exact simulator transitions to the next stable boundary",
            "one-step values explain local consequences, not whole-combat outcome ranking",
            "combinatorial pending choices report closed-form canonical fanout and probe only a bounded lazy sample that tries to include the selected action",
            "use this before changing global frontier ordering; if the failure is only a vague ordering preference, do not patch blindly",
        ],
    }
}

fn selected_first_trace(report: &CombatSearchV2Report) -> Option<&CombatSearchV2ActionTrace> {
    report
        .best_win_trajectory
        .as_ref()
        .and_then(|trajectory| trajectory.actions.first())
        .or_else(|| {
            report
                .best_complete_trajectory
                .as_ref()
                .and_then(|trajectory| trajectory.actions.first())
        })
}

fn microscope_candidate_sample(
    engine: &EngineState,
    combat: &CombatState,
    position: &CombatPosition,
    stepper: &impl CombatStepper,
    potion_policy: CombatSearchV2PotionPolicy,
    plugins: CombatSearchActionOrderingPlugins<'_>,
    selected_trace: Option<&CombatSearchV2ActionTrace>,
) -> (usize, Vec<IndexedActionChoice>) {
    if stepper.supports_canonical_pending_choice_actions() {
        if let EngineState::PendingChoice(choice) = engine {
            if let Some(inputs) = canonical_pending_choice_inputs(choice) {
                let mut sample = inputs
                    .take(CANDIDATE_REPORT_LIMIT)
                    .filter_map(|input| stepper.choice_for_legal_input(position, &input))
                    .collect::<Vec<_>>();
                if let Some(trace) = selected_trace {
                    if !sample
                        .iter()
                        .any(|choice| choice.action_key == trace.action_key)
                    {
                        if let Some(selected) =
                            stepper.choice_for_legal_input(position, &trace.input)
                        {
                            if sample.len() == CANDIDATE_REPORT_LIMIT {
                                sample.pop();
                            }
                            sample.push(selected);
                        }
                    }
                }
                let equivalence = compress_equivalent_actions(engine, combat, sample);
                let ordered = order_indexed_action_choices_with_plugins(
                    engine,
                    combat,
                    equivalence.choices,
                    plugins,
                );
                return (
                    pending_choice_fanout(choice).estimated_action_fanout,
                    ordered.choices,
                );
            }
        }
    }

    let legal = filtered_legal_actions(
        stepper.atomic_action_choices(position),
        potion_policy,
        None,
        None,
        combat,
    );
    let equivalence = compress_equivalent_actions(engine, combat, legal);
    let ordered =
        order_indexed_action_choices_with_plugins(engine, combat, equivalence.choices, plugins);
    (ordered.choices.len(), ordered.choices)
}

#[cfg(test)]
mod tests {
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::CombatCard;
    use crate::test_support::{blank_test_combat, planned_monster};

    use super::*;

    #[test]
    fn microscope_reports_selected_action_and_one_step_candidates() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        let config = CombatSearchV2Config {
            max_nodes: 200,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            input_label: Some("microscope_test".to_string()),
            ..CombatSearchV2Config::default()
        };

        let report = explain_combat_search_v2_initial_decision(
            &EngineState::CombatPlayerTurn,
            &combat,
            config,
        );

        assert_eq!(
            report.question,
            "why_was_this_action_selected_and_where_might_it_be_wrong"
        );
        assert!(report.candidate_count >= 2);
        assert!(report
            .candidates
            .iter()
            .any(|candidate| candidate.one_step.status == "stable"));
        assert!(report.selected_first_action.is_some());
    }

    #[test]
    fn microscope_candidate_order_respects_key_card_setup_bias() {
        let mut combat = blank_test_combat();
        let mut monster = planned_monster(EnemyId::JawWorm, 1);
        monster.current_hp = 50;
        monster.max_hp = 50;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::DemonForm, 2),
        ];
        let config = CombatSearchV2Config {
            max_nodes: 20,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            setup_bias_policy: CombatSearchV2SetupBiasPolicy::KeyCardOnline,
            input_label: Some("microscope_key_card_bias_test".to_string()),
            ..CombatSearchV2Config::default()
        };

        let report = explain_combat_search_v2_initial_decision(
            &EngineState::CombatPlayerTurn,
            &combat,
            config,
        );

        let demon_form = report
            .candidates
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.input,
                    ClientInput::PlayCard {
                        card_index: 1,
                        target: None
                    }
                )
            })
            .expect("Demon Form candidate should be reported");
        let strike = report
            .candidates
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.input,
                    ClientInput::PlayCard {
                        card_index: 0,
                        target: Some(_)
                    }
                )
            })
            .expect("Strike candidate should be reported");

        assert_eq!(demon_form.action_role, "key_setup_card");
        assert!(demon_form.ordered_index < strike.ordered_index);
    }

    #[test]
    fn microscope_samples_large_scry_without_materializing_its_power_set() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.draw_pile = ((0..13)
            .map(|index| CombatCard::for_scenario(CardId::Strike, 10_000 + index))
            .collect::<Vec<_>>())
        .into();
        let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
            cards: vec![CardId::Strike; 13],
            card_lineages: crate::test_support::scenario_lineages(10_000..10_013),
        });
        let config = CombatSearchV2Config {
            max_nodes: 8,
            max_actions_per_line: 1,
            rollout_policy: CombatSearchV2RolloutPolicy::Disabled,
            ..CombatSearchV2Config::default()
        };

        let report = explain_combat_search_v2_initial_decision(&engine, &combat, config);

        assert_eq!(report.candidate_count, 1 << 13);
        assert!(report.candidates.len() <= CANDIDATE_REPORT_LIMIT);
        assert!(!report.candidates.is_empty());
    }
}
