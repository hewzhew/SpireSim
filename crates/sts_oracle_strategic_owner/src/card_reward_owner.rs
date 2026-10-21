//! Card-reward policy owned by the strategic-owner crate.

use std::collections::BTreeMap;

use sts_simulator::eval::run_control::{
    exact_card_reward_policy_prior_v1, DecisionCandidateKey, DecisionSurface, RunControlSession,
    RunPolicyCandidateV1,
};

use super::owner_commands::executable_choices;
use super::owner_model::{OwnerChoice, OwnerChoiceExpansion};

pub(super) fn card_reward_owner_choices(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> Result<Vec<OwnerChoice>, String> {
    let mut choices = executable_choices(surface)
        .into_iter()
        .filter(|choice| choice.key.as_ref().is_some_and(is_card_reward_key))
        .collect::<Vec<_>>();
    if choices.is_empty() {
        return Err("card reward owner found no executable typed candidate".to_string());
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
        exact_card_reward_policy_prior_v1(session, &legal)?
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

fn is_card_reward_key(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::CardRewardPick { .. }
            | DecisionCandidateKey::CardRewardOpen { .. }
            | DecisionCandidateKey::CardRewardSingingBowl { .. }
            | DecisionCandidateKey::CardRewardSkip { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_simulator::content::cards::CardId;
    use sts_simulator::content::monsters::factory::EncounterId;
    use sts_simulator::eval::run_control::{build_decision_surface, RunControlConfig};
    use sts_simulator::state::core::EngineState;
    use sts_simulator::state::rewards::{RewardCard, RewardItem, RewardState};

    fn reward_session(cards: &[(CardId, u8)]) -> RunControlSession {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let reward_cards = cards
            .iter()
            .map(|(card, upgrades)| RewardCard::new(*card, *upgrades))
            .collect::<Vec<_>>();
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Card {
            cards: reward_cards.clone(),
        }];
        reward.pending_card_choice = Some(reward_cards);
        reward.pending_card_reward_index = Some(0);
        session.engine_state = EngineState::RewardScreen(reward);
        session
    }

    #[test]
    fn production_owner_recognizes_battle_trance_access() {
        let session = reward_session(&[(CardId::BattleTrance, 1)]);
        let surface = build_decision_surface(&session);
        let choices =
            card_reward_owner_choices(&session, &surface).expect("exact card reward owner");

        assert!(matches!(
            choices.first().and_then(|choice| choice.key.as_ref()),
            Some(DecisionCandidateKey::CardRewardPick {
                card: CardId::BattleTrance,
                ..
            })
        ));
    }

    #[test]
    fn production_owner_uses_known_champ_obligation() {
        let mut session = reward_session(&[(CardId::ThunderClap, 0), (CardId::Inflame, 1)]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 30;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        let surface = build_decision_surface(&session);
        let choices =
            card_reward_owner_choices(&session, &surface).expect("exact card reward owner");

        assert!(matches!(
            choices.first().and_then(|choice| choice.key.as_ref()),
            Some(DecisionCandidateKey::CardRewardPick {
                card: CardId::Inflame,
                ..
            })
        ));
    }

    #[test]
    fn every_typed_card_reward_action_remains_expandable() {
        let session = reward_session(&[(CardId::WildStrike, 0), (CardId::BattleTrance, 1)]);
        let surface = build_decision_surface(&session);
        let choices =
            card_reward_owner_choices(&session, &surface).expect("exact card reward owner");
        let expected = surface
            .view
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.key.as_ref().is_some_and(is_card_reward_key)
                    && candidate.action.executable_action_ref().is_some()
            })
            .count();

        assert_eq!(choices.len(), expected);
        assert!(choices.iter().all(OwnerChoice::auto_expand_allowed));
    }

    #[test]
    fn card_owner_preserves_the_typed_card_reward_skip_action() {
        let session = reward_session(&[(CardId::BattleTrance, 1)]);
        let surface = build_decision_surface(&session);
        assert!(surface.view.candidates.iter().any(|candidate| matches!(
            candidate.key,
            Some(DecisionCandidateKey::CardRewardSkip {
                reward_item_index: 0
            })
        ) && candidate
            .action
            .executable_action_ref()
            .is_some()));

        let choices =
            card_reward_owner_choices(&session, &surface).expect("exact card reward owner");
        assert!(choices.iter().any(|choice| matches!(
            choice.key,
            Some(DecisionCandidateKey::CardRewardSkip {
                reward_item_index: 0
            })
        )));
    }
}
