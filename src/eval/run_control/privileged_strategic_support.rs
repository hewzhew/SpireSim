use crate::agent::task::{
    TaskPrivilegedStrategicSupportOutcomeV1, TaskPrivilegedStrategicSupportUnavailableV1,
    TaskPrivilegedStrategicSupportV1, TaskPrivilegedSupportAccessV1,
    TaskPrivilegedSupportProvenanceV1, TaskPublicChoiceV1, TaskPublicDecisionV1,
};
use crate::ai::planner_core::PlannerAction;
use crate::state::core::ClientInput;

use super::{LearningBoundaryV1, LearningEnvV3, RunDecisionAction};

pub const PRIVILEGED_STRATEGIC_OWNER_SUPPORT_PRODUCER_V1: &str = "privileged_exact_strategic_owner";
pub const PRIVILEGED_STRATEGIC_OWNER_SUPPORT_REVISION_V1: u32 = 3;

pub(super) fn privileged_strategic_support_v1(
    env: &LearningEnvV3,
    public_root: &TaskPublicDecisionV1,
) -> Result<TaskPrivilegedStrategicSupportV1, String> {
    let provenance = TaskPrivilegedSupportProvenanceV1 {
        access: TaskPrivilegedSupportAccessV1::PrivilegedExactStrategicOwner,
        producer_name: PRIVILEGED_STRATEGIC_OWNER_SUPPORT_PRODUCER_V1.to_string(),
        producer_revision: PRIVILEGED_STRATEGIC_OWNER_SUPPORT_REVISION_V1,
    };
    let unavailable = |reason| TaskPrivilegedStrategicSupportV1 {
        root_public_snapshot_id: public_root.information.snapshot_id.clone(),
        provenance: provenance.clone(),
        outcome: TaskPrivilegedStrategicSupportOutcomeV1::Unavailable { reason },
    };
    let LearningBoundaryV1::Strategic { boundary } = env.observe()? else {
        return Err("strategic support requested outside a strategic boundary".to_string());
    };
    let (planner_candidate_id, exact_action) = match
        sts_oracle_strategic_owner::current_privileged_strategic_owner_planner_action_v1(
            env.session(),
        )?
    {
        sts_oracle_strategic_owner::PrivilegedStrategicOwnerPlannerActionV1::Choice {
            planner_candidate_id,
            action,
        } => (planner_candidate_id, action),
        sts_oracle_strategic_owner::PrivilegedStrategicOwnerPlannerActionV1::ExactOwnerUnavailable => {
            return Ok(unavailable(
                TaskPrivilegedStrategicSupportUnavailableV1::ExactOwnerUnavailable,
            ));
        }
        sts_oracle_strategic_owner::PrivilegedStrategicOwnerPlannerActionV1::OwnerCandidateNotRepresented => {
            return Ok(unavailable(
                TaskPrivilegedStrategicSupportUnavailableV1::OwnerCandidateNotRepresented,
            ));
        }
    };
    let mut matching = boundary
        .legal_candidates
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.candidate_id == planner_candidate_id);
    let Some((root_ordinal, planner_candidate)) = matching.next() else {
        return Ok(unavailable(
            TaskPrivilegedStrategicSupportUnavailableV1::PlannerCandidateMissing,
        ));
    };
    if matching.next().is_some() {
        return Ok(unavailable(
            TaskPrivilegedStrategicSupportUnavailableV1::PlannerCandidateAmbiguous,
        ));
    }
    let mut ordinal_path = vec![root_ordinal];
    match (&planner_candidate.action, exact_action) {
        (
            PlannerAction::BeginRunCardSelection {
                selectable_card_lineages,
                min_choices,
                max_choices,
                ..
            },
            RunDecisionAction::Input(ClientInput::SubmitSelection(resolution)),
        ) => {
            let selected = resolution.selected_card_lineages();
            if selected.len() < *min_choices || selected.len() > *max_choices {
                return Ok(unavailable(
                    TaskPrivilegedStrategicSupportUnavailableV1::SelectionPayloadUnavailable,
                ));
            }
            for selected_lineage in selected {
                let mut ordinals =
                    selectable_card_lineages
                        .iter()
                        .enumerate()
                        .filter_map(|(ordinal, lineage)| {
                            (*lineage == selected_lineage).then_some(ordinal)
                        });
                let Some(ordinal) = ordinals.next() else {
                    return Ok(unavailable(
                        TaskPrivilegedStrategicSupportUnavailableV1::SelectionPayloadUnavailable,
                    ));
                };
                if ordinals.next().is_some() {
                    return Ok(unavailable(
                        TaskPrivilegedStrategicSupportUnavailableV1::SelectionPayloadUnavailable,
                    ));
                }
                ordinal_path.push(ordinal);
            }
        }
        (
            PlannerAction::SubmitRunSelection {
                selected_card_lineages,
                ..
            },
            RunDecisionAction::Input(ClientInput::SubmitSelection(resolution)),
        ) if selected_card_lineages.as_slice()
            == resolution.selected_card_lineages().as_slice() => {}
        (PlannerAction::BeginRunCardSelection { .. }, _)
        | (PlannerAction::SubmitRunSelection { .. }, _)
        | (_, RunDecisionAction::Input(ClientInput::SubmitSelection(_))) => {
            return Ok(unavailable(
                TaskPrivilegedStrategicSupportUnavailableV1::SelectionPayloadUnavailable,
            ));
        }
        _ => {}
    }
    let support = TaskPrivilegedStrategicSupportV1 {
        root_public_snapshot_id: public_root.information.snapshot_id.clone(),
        provenance,
        outcome: TaskPrivilegedStrategicSupportOutcomeV1::Choice {
            choice: TaskPublicChoiceV1 { ordinal_path },
        },
    };
    Ok(support)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::{
        task_public_strategic_decision_v1, TaskPrivilegedStrategicSupportOutcomeV1,
        TaskPublicCandidateV1, TaskStrategicActionV1,
    };
    use crate::content::cards::CardId;
    use crate::eval::run_control::{LearningBoundaryV1, RunControlConfig, RunControlSession};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::EngineState;

    #[test]
    fn unresolved_reward_is_unavailable_support_but_invalid_exit_is_an_error() {
        use crate::state::rewards::{RewardItem, RewardState};
        for invalid_exit in [false, true] {
            let mut session = RunControlSession::new(RunControlConfig::default());
            session.run_state.event_state = None;
            session.reward_automation.claim_gold = false;
            let mut reward = RewardState::new();
            if invalid_exit {
                reward.skippable = false;
                // Keep an unrelated legal action so task projection succeeds;
                // the missing required reward exit is the actual defect.
                session.run_state.potions = vec![Some(crate::content::potions::Potion::new(
                    crate::content::potions::PotionId::WeakenPotion,
                    1,
                ))];
            } else {
                reward.items.push(RewardItem::Gold { amount: 12 });
            }
            session.engine_state = EngineState::RewardScreen(reward);
            let env = LearningEnvV3::from_session(session);
            let LearningBoundaryV1::Strategic { boundary } = env.observe().unwrap() else {
                panic!("reward boundary");
            };
            let root = task_public_strategic_decision_v1(
                &boundary.observation,
                &boundary.legal_candidates,
            )
            .unwrap();
            let result = privileged_strategic_support_v1(&env, &root);
            if invalid_exit {
                assert!(result.is_err());
            } else {
                assert!(matches!(
                    result.unwrap().outcome,
                    TaskPrivilegedStrategicSupportOutcomeV1::Unavailable {
                        reason: TaskPrivilegedStrategicSupportUnavailableV1::ExactOwnerUnavailable
                    }
                ));
            }
        }
    }

    #[test]
    fn exact_campfire_owner_projects_to_public_smith_instead_of_full_hp_rest() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::Campfire;
        session.run_state.current_hp = session.run_state.max_hp;
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 10_001),
            CombatCard::for_scenario(CardId::Defend, 10_002),
            CombatCard::for_scenario(CardId::Bash, 10_003),
        ];
        let env = LearningEnvV3::from_session(session);
        let LearningBoundaryV1::Strategic { boundary } = env.observe().expect("campfire boundary")
        else {
            panic!("campfire fixture must be strategic");
        };
        let public_root =
            task_public_strategic_decision_v1(&boundary.observation, &boundary.legal_candidates)
                .expect("public campfire root");

        let support = privileged_strategic_support_v1(&env, &public_root)
            .expect("project exact campfire owner");
        let TaskPrivilegedStrategicSupportOutcomeV1::Choice { choice } = support.outcome else {
            panic!("exact campfire owner must be represented");
        };
        let selected = &public_root.candidates[choice.ordinal_path[0]];

        assert!(matches!(
            selected,
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::Smith { .. }
            }
        ));
    }
}
