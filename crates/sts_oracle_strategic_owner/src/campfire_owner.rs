//! Campfire policy owned by the strategic-owner crate.

use std::collections::BTreeMap;

use sts_simulator::eval::run_control::{
    exact_campfire_policy_prior_v1, DecisionCandidateKey, DecisionSurface,
    RunForcedTransitionKindV1, RunPolicyCandidateV1,
};
use sts_simulator::eval::run_control::{RunControlSession, RunPolicyPriorV1};
use sts_simulator::state::core::EngineState;

use super::owner_commands::executable_choices;
use super::owner_model::{OwnerChoice, OwnerChoiceExpansion, OwnerDecision, OwnerRoutine};

pub(super) fn campfire_owner_decision(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> OwnerDecision {
    if !matches!(session.engine_state, EngineState::Campfire) {
        return OwnerDecision::Gap("Campfire owner requires Campfire engine state".to_string());
    }
    if sts_simulator::engine::campfire_handler::get_available_options(&session.run_state).is_empty()
    {
        return OwnerDecision::Routine(OwnerRoutine::ForcedTransition(
            RunForcedTransitionKindV1::EmptyCampfireExit,
        ));
    }
    match campfire_owner_choices(session, surface) {
        Ok(choices) => OwnerDecision::Candidates(choices),
        Err(err) => OwnerDecision::Gap(err),
    }
}

fn campfire_owner_choices(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> Result<Vec<OwnerChoice>, String> {
    let mut choices = executable_choices(surface)
        .into_iter()
        .filter(|choice| choice.key.as_ref().is_some_and(is_campfire_key))
        .collect::<Vec<_>>();
    if choices.is_empty() {
        return Err("campfire owner found no executable typed candidate".to_string());
    }

    let prior = {
        let legal = choices
            .iter()
            .map(|choice| RunPolicyCandidateV1 {
                candidate_id: &choice.candidate_id,
                label: &choice.label,
                action: &choice.action,
            })
            .collect::<Vec<_>>();
        exact_campfire_policy_prior_v1(session, &legal)?
    };
    sort_by_prior(&mut choices, &prior);
    for choice in &mut choices {
        choice.expansion = OwnerChoiceExpansion::AutoAllowed;
    }
    Ok(choices)
}

fn sort_by_prior(choices: &mut [OwnerChoice], prior: &RunPolicyPriorV1) {
    let ranks = prior
        .entries
        .iter()
        .enumerate()
        .map(|(rank, entry)| (entry.candidate_id.as_str(), rank))
        .collect::<BTreeMap<_, _>>();
    choices.sort_by_key(|choice| {
        ranks
            .get(choice.candidate_id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
}

fn is_campfire_key(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::CampfireRest
            | DecisionCandidateKey::CampfireSmith { .. }
            | DecisionCandidateKey::CampfireDig
            | DecisionCandidateKey::CampfireLift
            | DecisionCandidateKey::CampfireToke { .. }
            | DecisionCandidateKey::CampfireRecall
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_simulator::content::cards::CardId;
    use sts_simulator::content::relics::{RelicId, RelicState};
    use sts_simulator::eval::run_control::{build_decision_surface, RunControlConfig};
    use sts_simulator::runtime::combat::CombatCard;

    #[test]
    fn owner_keeps_every_exact_campfire_action_expandable() {
        let mut session = campfire_session();
        session.run_state.is_final_act_available = true;
        session.run_state.relics = [RelicId::Shovel, RelicId::Girya, RelicId::PeacePipe]
            .into_iter()
            .map(RelicState::new)
            .collect();
        let surface = build_decision_surface(&session);
        let expected = surface
            .view
            .candidates
            .iter()
            .filter(|candidate| candidate.action.executable_action_ref().is_some())
            .count();

        let OwnerDecision::Candidates(choices) = campfire_owner_decision(&session, &surface) else {
            panic!("expected complete campfire candidate surface");
        };
        assert_eq!(choices.len(), expected);
        assert!(choices.iter().all(OwnerChoice::auto_expand_allowed));
        assert!(choices
            .iter()
            .all(|choice| choice.key.as_ref().is_some_and(is_campfire_key)));
    }

    #[test]
    fn coffee_dripper_does_not_require_a_rest_or_smith_fallback() {
        let mut session = campfire_session();
        session.run_state.current_hp = session.run_state.max_hp;
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::CoffeeDripper));
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::Shovel));
        let surface = build_decision_surface(&session);

        let OwnerDecision::Candidates(choices) = campfire_owner_decision(&session, &surface) else {
            panic!("expected exact Coffee Dripper choices");
        };
        assert!(choices.iter().any(|choice| {
            matches!(choice.key, Some(DecisionCandidateKey::CampfireSmith { .. }))
        }));
        assert!(choices
            .iter()
            .any(|choice| { matches!(choice.key, Some(DecisionCandidateKey::CampfireDig)) }));
        assert!(choices.iter().all(OwnerChoice::auto_expand_allowed));
    }

    #[test]
    fn mechanically_empty_campfire_uses_the_forced_transition() {
        let mut session = campfire_session();
        session.run_state.relics = [RelicId::CoffeeDripper, RelicId::FusionHammer]
            .into_iter()
            .map(RelicState::new)
            .collect();
        let surface = build_decision_surface(&session);

        assert!(matches!(
            campfire_owner_decision(&session, &surface),
            OwnerDecision::Routine(OwnerRoutine::ForcedTransition(
                RunForcedTransitionKindV1::EmptyCampfireExit
            ))
        ));
    }

    fn campfire_session() -> RunControlSession {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::Campfire;
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Apparition, 10_000),
            CombatCard::for_scenario(CardId::Strike, 10_001),
            CombatCard::for_scenario(CardId::Defend, 10_002),
        ];
        session
    }
}
