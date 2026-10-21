//! Run-choice policy owned by the strategic-owner crate.

use sts_simulator::ai::deck_mutation_compiler_v1::{
    compile_deck_mutation_decision_v1, DeckMutationCompilerRequestV1,
};
use sts_simulator::content::cards::get_card_definition;
use sts_simulator::eval::run_control::{DecisionSurface, RunControlSession, RunDecisionAction};
use sts_simulator::state::core::{
    ClientInput, EngineState, RunPendingChoiceReason, RunPendingChoiceState,
};
use sts_simulator::state::selection::{SelectionResolution, SelectionScope};

use super::owner_commands::owner_candidate_id_for_action;
use super::owner_model::{ChoiceAnnotation, OwnerChoice, OwnerChoiceExpansion, OwnerDecision};

pub(super) fn can_handle(reason: RunPendingChoiceReason) -> bool {
    matches!(
        reason,
        RunPendingChoiceReason::Purge
            | RunPendingChoiceReason::PurgeNonBottled
            | RunPendingChoiceReason::Upgrade
            | RunPendingChoiceReason::Transform
            | RunPendingChoiceReason::TransformNonBottled
            | RunPendingChoiceReason::TransformUpgraded
            | RunPendingChoiceReason::Duplicate
            | RunPendingChoiceReason::BottleFlame
            | RunPendingChoiceReason::BottleLightning
            | RunPendingChoiceReason::BottleTornado
    )
}

pub(super) fn run_choice_owner_decision(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> OwnerDecision {
    let EngineState::RunPendingChoice(choice) = &session.engine_state else {
        return OwnerDecision::Gap("RunChoice owner requires RunPendingChoice state".to_string());
    };
    if !can_handle(choice.reason) {
        return OwnerDecision::Gap(format!(
            "RunChoice owner has no policy for {:?}",
            choice.reason
        ));
    }
    deck_mutation_owner_decision(session, surface, choice)
}

fn deck_mutation_owner_decision(
    session: &RunControlSession,
    surface: &DecisionSurface,
    choice: &RunPendingChoiceState,
) -> OwnerDecision {
    if choice.min_choices != choice.max_choices {
        return OwnerDecision::Gap(format!(
            "{:?} requires fixed-count committed selection, got {}-{}",
            choice.reason, choice.min_choices, choice.max_choices
        ));
    }

    let decision = compile_deck_mutation_decision_v1(
        &session.run_state,
        choice,
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );
    let Some(selected_plan) = decision.selected_plan else {
        return OwnerDecision::Gap(format!(
            "{:?} has no legal committed deck mutation target",
            choice.reason
        ));
    };
    if selected_plan.step.deck_indices.len() != choice.max_choices {
        return OwnerDecision::Gap(format!(
            "{:?} selected {} target(s), expected {}",
            choice.reason,
            selected_plan.step.deck_indices.len(),
            choice.max_choices
        ));
    }

    let lineages = selected_plan
        .step
        .deck_indices
        .iter()
        .filter_map(|idx| {
            session
                .run_state
                .master_deck
                .get(*idx)
                .map(|card| card.lineage)
        })
        .collect::<Vec<_>>();
    if lineages.len() != choice.max_choices {
        return OwnerDecision::Gap(format!(
            "{:?} selected target index outside master deck",
            choice.reason
        ));
    }

    let labels = selected_plan
        .step
        .cards
        .iter()
        .map(|card| {
            let name = get_card_definition(card.card).name;
            if card.upgrades == 0 {
                name.to_string()
            } else {
                format!("{name}+{}", card.upgrades)
            }
        })
        .collect::<Vec<_>>();
    let risk_note = if selected_plan.risks.is_empty() {
        String::new()
    } else {
        format!(" risks={}", selected_plan.risks.join("; "))
    };

    let action = RunDecisionAction::Input(ClientInput::SubmitSelection(
        SelectionResolution::card_lineages(SelectionScope::Deck, lineages),
    ));
    let candidate_id = match owner_candidate_id_for_action(surface, &action) {
        Ok(candidate_id) => candidate_id,
        Err(err) => return OwnerDecision::Gap(err),
    };

    OwnerDecision::Candidates(vec![OwnerChoice {
        candidate_id,
        key: None,
        action,
        label: format!(
            "{:?} {} role={:?} confidence={:.2}{}",
            choice.reason,
            labels.join(", "),
            selected_plan.role,
            selected_plan.confidence,
            risk_note
        ),
        annotation: ChoiceAnnotation::None,
        expansion: OwnerChoiceExpansion::AutoAllowed,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_simulator::content::cards::CardId;
    use sts_simulator::eval::run_control::{
        RunControlConfig, RunDecisionAction, RunDecisionSelectionSourceV1,
    };
    use sts_simulator::state::events::EventId;
    use sts_simulator::state::selection::{DomainEventSource, SelectionScope};

    #[test]
    fn event_origin_upgrade_produces_one_typed_run_choice_target() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.add_card_to_deck(CardId::Bash);
        session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: RunPendingChoiceReason::Upgrade,
            source: DomainEventSource::Event(EventId::UpgradeShrine),
            return_state: Box::new(EngineState::EventRoom),
        });

        let surface = sts_simulator::eval::run_control::build_decision_surface(&session);
        let OwnerDecision::Candidates(choices) = run_choice_owner_decision(&session, &surface)
        else {
            panic!("event-origin upgrade must be owned by RunChoice");
        };
        let [choice] = choices.as_slice() else {
            panic!("RunChoice must produce one committed candidate");
        };
        let RunDecisionAction::Input(ClientInput::SubmitSelection(resolution)) = &choice.action
        else {
            panic!("RunChoice candidate must submit a typed selection");
        };
        assert_eq!(resolution.scope, SelectionScope::Deck);
        assert_eq!(resolution.selected_card_lineages().len(), 1);

        let outcome = session
            .apply_owner_candidate(&choice.candidate_id, choice.action.clone())
            .expect("RunChoice owner candidate should execute through its public id");
        let Some(transaction) = outcome.single_decision_transaction() else {
            panic!("owner candidate should preserve exactly one decision transaction");
        };
        assert_eq!(
            transaction.selection.source,
            RunDecisionSelectionSourceV1::OwnerPolicy
        );
    }
}
