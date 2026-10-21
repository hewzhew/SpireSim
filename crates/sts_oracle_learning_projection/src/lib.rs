//! Typed adapters between privileged strategic owners and model-facing decisions.
//!
//! This crate owns projection only. It does not own policy selection, Python ABI,
//! environment mutation, training objectives, or persistence.

use sts_oracle_learning::eval::run_control::{
    LearningModelChoiceV1, LearningModelDecisionV1, LearningStrategicBoundaryV1, RunControlSession,
    RunDecisionAction,
};

/// Projects the current exact strategic owner action into one complete model-visible path.
///
/// Missing or ambiguous identities fail closed as unavailable. The returned ordinals are
/// meaningful only for the supplied unchanged boundary and must be revalidated before each
/// step by the execution adapter.
pub fn privileged_node_sl_oracle_path_v1(
    session: &RunControlSession,
    boundary: &LearningStrategicBoundaryV1,
) -> Result<Option<Vec<usize>>, String> {
    let sts_oracle_strategic_owner::PrivilegedStrategicOwnerPlannerActionV1::Choice {
        planner_candidate_id,
        action,
    } = sts_oracle_strategic_owner::current_privileged_strategic_owner_planner_action_v1(session)?
    else {
        return Ok(None);
    };
    let decision =
        LearningModelDecisionV1::from_strategic_boundary(boundary).map_err(|e| e.to_string())?;
    privileged_node_sl_oracle_choice_path_v1(&planner_candidate_id, action, &decision)
}

/// Project an owner choice against the caller's actual candidate surface.
pub fn privileged_node_sl_oracle_path_from_decision_v1(
    session: &RunControlSession,
    decision: &LearningModelDecisionV1<'_>,
) -> Result<Option<Vec<usize>>, String> {
    let sts_oracle_strategic_owner::PrivilegedStrategicOwnerPlannerActionV1::Choice {
        planner_candidate_id,
        action,
    } = sts_oracle_strategic_owner::current_privileged_strategic_owner_planner_action_v1(session)?
    else {
        return Ok(None);
    };
    privileged_node_sl_oracle_choice_path_v1(&planner_candidate_id, action, decision)
}

fn privileged_node_sl_oracle_choice_path_v1(
    planner_candidate_id: &str,
    action: RunDecisionAction,
    decision: &LearningModelDecisionV1<'_>,
) -> Result<Option<Vec<usize>>, String> {
    let Some(root_ordinal) =
        decision.strategic_ordinal_for_planner_candidate_id(planner_candidate_id)
    else {
        return Ok(None);
    };
    let choice = decision
        .choose(root_ordinal)
        .map_err(|error| error.to_string())?;
    match choice {
        LearningModelChoiceV1::Apply(_) => Ok(Some(vec![root_ordinal])),
        LearningModelChoiceV1::DecodeSelection(draft) => {
            let RunDecisionAction::Input(input) = action else {
                return Ok(None);
            };
            let Some((suffix, _)) = draft
                .ordinal_path_for_input_v1(&input)
                .map_err(|error| error.to_string())?
            else {
                return Ok(None);
            };
            let mut path = vec![root_ordinal];
            path.extend(suffix);
            Ok(Some(path))
        }
    }
}

#[cfg(test)]
mod tests {
    use sts_oracle_learning::content::cards::CardId;
    use sts_oracle_learning::eval::run_control::{
        LearningActionV1, LearningBoundaryV1, LearningEnvV3, LearningModelChoiceV1,
        LearningModelDecisionV1, LearningSelectionStepV1, RunControlConfig, RunControlSession,
        RunDecisionAction,
    };
    use sts_oracle_learning::runtime::combat::CombatCard;
    use sts_oracle_learning::state::core::{
        ClientInput, EngineState, RunPendingChoiceReason, RunPendingChoiceState,
    };
    use sts_oracle_learning::state::selection::{DomainEventSource, SelectionReason};

    use super::*;

    #[test]
    fn multi_card_owner_action_round_trips_through_one_complete_ordinal_path() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Defend, 22),
            CombatCard::for_scenario(CardId::Bash, 33),
        ];
        session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 2,
            max_choices: 2,
            reason: RunPendingChoiceReason::Transform,
            source: DomainEventSource::Selection(SelectionReason::Transform),
            return_state: Box::new(EngineState::MapNavigation),
        });
        let Some((_, RunDecisionAction::Input(ClientInput::SubmitSelection(expected)))) =
            sts_oracle_strategic_owner::current_privileged_node_sl_owner_action_v1(&session)
        else {
            panic!("transform owner must produce one typed selection");
        };
        let LearningBoundaryV1::Strategic { boundary } =
            LearningEnvV3::from_session(session.clone())
                .observe()
                .expect("capture strategic boundary")
        else {
            panic!("transform fixture must expose a strategic boundary");
        };

        let path = privileged_node_sl_oracle_path_v1(&session, &boundary)
            .expect("project privileged owner")
            .expect("transform owner must be represented");
        assert_eq!(path.len(), 4);

        let decision =
            LearningModelDecisionV1::from_strategic_boundary(&boundary).expect("root decision");
        let LearningModelChoiceV1::DecodeSelection(mut draft) =
            decision.choose(path[0]).expect("choose root family")
        else {
            panic!("transform root must start a symbolic selection");
        };
        for &ordinal in &path[1..path.len() - 1] {
            assert!(matches!(
                draft.choose(ordinal).expect("append selected card"),
                LearningSelectionStepV1::Continue
            ));
        }
        let LearningSelectionStepV1::Apply(LearningActionV1::RunSelection {
            resolution: actual,
            ..
        }) = draft
            .choose(*path.last().expect("submit ordinal"))
            .expect("submit selection")
        else {
            panic!("complete path must resolve to one run selection");
        };
        assert_eq!(actual, expected);
    }
}
