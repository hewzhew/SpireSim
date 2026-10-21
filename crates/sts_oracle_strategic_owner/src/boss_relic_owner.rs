//! Boss-relic policy owned by the strategic-owner crate.

use std::collections::BTreeMap;

use sts_simulator::eval::run_control::{
    exact_boss_relic_policy_prior_v1, DecisionCandidateKey, DecisionSurface, RunControlSession,
    RunPolicyCandidateV1,
};
use sts_simulator::state::core::EngineState;

use super::owner_commands::executable_choices_including_cancel;
use super::owner_model::{OwnerChoice, OwnerChoiceExpansion};

pub(super) fn boss_relic_owner_choices(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> Result<Vec<OwnerChoice>, String> {
    if !matches!(session.engine_state, EngineState::BossRelicSelect(_)) {
        return Err("Boss relic owner requires BossRelicSelect engine state".to_string());
    }
    let mut choices = executable_choices_including_cancel(surface)
        .into_iter()
        .filter(|choice| choice.key.as_ref().is_some_and(is_boss_relic_key))
        .collect::<Vec<_>>();
    if choices.is_empty() {
        return Err("boss relic owner found no executable typed candidate".to_string());
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
        exact_boss_relic_policy_prior_v1(session, &legal)?
    };
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
    for choice in &mut choices {
        choice.expansion = OwnerChoiceExpansion::AutoAllowed;
    }
    Ok(choices)
}

fn is_boss_relic_key(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::BossRelicPick { .. } | DecisionCandidateKey::BossRelicSkip
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_simulator::content::relics::RelicId;
    use sts_simulator::eval::run_control::{build_decision_surface, RunControlConfig};
    use sts_simulator::state::rewards::BossRelicChoiceState;

    #[test]
    fn owner_keeps_every_boss_relic_and_skip_expandable() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::BossRelicSelect(BossRelicChoiceState::new(vec![
            RelicId::CoffeeDripper,
            RelicId::RunicPyramid,
            RelicId::PandorasBox,
        ]));
        let surface = build_decision_surface(&session);

        let choices =
            boss_relic_owner_choices(&session, &surface).expect("typed boss relic choices");

        assert_eq!(choices.len(), 4);
        assert!(choices.iter().all(OwnerChoice::auto_expand_allowed));
        assert!(choices
            .iter()
            .all(|choice| choice.key.as_ref().is_some_and(is_boss_relic_key)));
        assert!(choices
            .iter()
            .any(|choice| { matches!(choice.key, Some(DecisionCandidateKey::BossRelicSkip)) }));
    }
}
