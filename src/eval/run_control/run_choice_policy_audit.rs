use serde::Serialize;

use crate::ai::deck_mutation_compiler_v1::{
    compile_deck_mutation_decision_v1, DeckMutationCompilerRequestV1, DeckMutationPlanCandidateV1,
};
use crate::content::cards::CardId;
use crate::state::core::EngineState;

use super::RunControlSession;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactRunChoicePolicyAuditV1 {
    pub reason: String,
    pub min_choices: usize,
    pub max_choices: usize,
    pub selected_plan_id: Option<String>,
    pub candidates: Vec<RunChoicePolicyAuditCandidateV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunChoicePolicyAuditCandidateV1 {
    pub owner_rank: usize,
    pub selected: bool,
    pub plan_id: String,
    pub label: String,
    pub deck_indices: Vec<usize>,
    pub cards: Vec<String>,
    pub target_classes: Vec<String>,
    pub removal_benefits: Vec<String>,
    pub target_losses: Vec<String>,
    pub role: String,
    pub score_hint: i32,
    pub confidence: f32,
    pub duplicate_targets: Vec<RunChoiceDuplicateTargetAuditV1>,
    pub allowed_consumers: Vec<String>,
    pub reasons: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunChoiceDuplicateTargetAuditV1 {
    pub card: CardId,
    pub upgrades: u8,
    pub priority: i32,
    pub premium: bool,
    pub role: String,
    pub stack_behavior: String,
    pub reasons: Vec<String>,
    pub risks: Vec<String>,
}

pub fn exact_run_choice_policy_audit_v1(
    session: &RunControlSession,
) -> Result<ExactRunChoicePolicyAuditV1, String> {
    let EngineState::RunPendingChoice(choice) = &session.engine_state else {
        return Err("exact run-choice audit requires a RunPendingChoice decision".to_string());
    };
    let decision = compile_deck_mutation_decision_v1(
        &session.run_state,
        choice,
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );
    let selected_plan_id = decision
        .selected_plan
        .as_ref()
        .map(|plan| plan.plan_id.clone());
    let candidates = decision
        .candidate_plans
        .iter()
        .enumerate()
        .map(|(owner_rank, plan)| audit_candidate_v1(owner_rank, plan, selected_plan_id.as_deref()))
        .collect();
    Ok(ExactRunChoicePolicyAuditV1 {
        reason: format!("{:?}", choice.reason),
        min_choices: choice.min_choices,
        max_choices: choice.max_choices,
        selected_plan_id,
        candidates,
    })
}

fn audit_candidate_v1(
    owner_rank: usize,
    plan: &DeckMutationPlanCandidateV1,
    selected_plan_id: Option<&str>,
) -> RunChoicePolicyAuditCandidateV1 {
    RunChoicePolicyAuditCandidateV1 {
        owner_rank,
        selected: selected_plan_id == Some(plan.plan_id.as_str()),
        plan_id: plan.plan_id.clone(),
        label: plan.step.effect_label.clone(),
        deck_indices: plan.step.deck_indices.clone(),
        cards: plan
            .step
            .cards
            .iter()
            .map(|card| card.label.clone())
            .collect(),
        target_classes: plan
            .step
            .cards
            .iter()
            .map(|card| format!("{:?}", card.target_class))
            .collect(),
        removal_benefits: plan
            .step
            .cards
            .iter()
            .map(|card| format!("{:?}", card.removal_benefit.tier))
            .collect(),
        target_losses: plan
            .step
            .cards
            .iter()
            .map(|card| format!("{:?}", card.target_loss.tier))
            .collect(),
        role: format!("{:?}", plan.role),
        score_hint: plan.score_hint,
        confidence: plan.confidence,
        duplicate_targets: plan
            .duplicate_targets
            .iter()
            .map(|target| RunChoiceDuplicateTargetAuditV1 {
                card: target.card,
                upgrades: target.upgrades,
                priority: target.priority,
                premium: target.premium,
                role: format!("{:?}", target.role),
                stack_behavior: format!("{:?}", target.stack_behavior),
                reasons: target.reasons.clone(),
                risks: target.risks.clone(),
            })
            .collect(),
        allowed_consumers: allowed_consumers_v1(plan),
        reasons: plan.reasons.clone(),
        risks: plan.risks.clone(),
    }
}

fn allowed_consumers_v1(plan: &DeckMutationPlanCandidateV1) -> Vec<String> {
    let allowed = &plan.allowed_consumers;
    let mut labels = Vec::new();
    if allowed.execute_autopilot {
        labels.push("execute".to_string());
    }
    if allowed.branch_active {
        labels.push("branch_active".to_string());
    }
    if allowed.branch_frozen {
        labels.push("branch_frozen".to_string());
    }
    if allowed.inspect {
        labels.push("inspect".to_string());
    }
    if allowed.replay {
        labels.push("replay".to_string());
    }
    if allowed.human_prompt {
        labels.push("human_prompt".to_string());
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{RunPendingChoiceReason, RunPendingChoiceState};
    use crate::state::events::EventId;
    use crate::state::selection::DomainEventSource;

    use super::super::RunControlConfig;

    #[test]
    fn duplicate_audit_projects_every_ranked_target_without_private_identity() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let upgraded = |card, uuid| {
            let mut card = CombatCard::for_scenario(card, uuid);
            card.upgrades = 1;
            card
        };
        session.run_state.master_deck = vec![
            upgraded(CardId::DarkEmbrace, 10),
            CombatCard::for_scenario(CardId::BurningPact, 11),
            upgraded(CardId::SecondWind, 12),
            CombatCard::for_scenario(CardId::Strike, 13),
        ];
        session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: RunPendingChoiceReason::Duplicate,
            source: DomainEventSource::Event(EventId::Duplicator),
            return_state: Box::new(EngineState::EventRoom),
        });

        let audit = exact_run_choice_policy_audit_v1(&session).expect("duplicate audit");

        assert_eq!(audit.candidates.len(), 4);
        let selected = audit
            .candidates
            .iter()
            .find(|candidate| candidate.selected)
            .expect("selected candidate");
        assert_eq!(selected.cards, vec!["Dark Embrace+"]);
        assert_eq!(selected.duplicate_targets[0].role, "EnginePayoff");
        assert_eq!(selected.duplicate_targets[0].stack_behavior, "Stackable");
        assert!(audit
            .candidates
            .iter()
            .all(|candidate| candidate.deck_indices.len() == 1));
    }
}
