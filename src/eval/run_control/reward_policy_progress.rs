//! Policy work remaining behind reward navigation. This is recomputed from the
//! current exact root; it never consumes rewards or changes legal navigation.

use crate::ai::reward_policy_v1::{build_reward_decision_context_v1, RewardPolicyClassV1};
use crate::state::core::EngineState;
use crate::state::rewards::RewardItem;

use super::{
    build_decision_surface, exact_card_reward_policy_prior_v1, DecisionCandidateKey,
    RunControlSession, RunPolicyCandidateV1,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RewardPolicyProgressV1 {
    /// Executable candidates at this unchanged root, in reward-item order.
    pub preferred_card_candidate_ids: Vec<String>,
    pub pending_card_selection: bool,
    pub pending_other_rewards: bool,
}

impl RewardPolicyProgressV1 {
    pub fn is_complete(&self) -> bool {
        self.preferred_card_candidate_ids.is_empty()
            && !self.pending_card_selection
            && !self.pending_other_rewards
    }
}

/// Opening a card reward has value only when the existing card policy would
/// acquire something there. Skip closes the page without consuming its item,
/// so treating every remaining item as new work creates an open/skip cycle.
/// Re-evaluation deliberately allows a later deck or relic change to change
/// the answer. Unsupported projections remain errors, never completed work.
pub fn reward_policy_progress_v1(
    session: &RunControlSession,
) -> Result<RewardPolicyProgressV1, String> {
    let reward = match &session.engine_state {
        EngineState::RewardScreen(reward) => reward,
        EngineState::RewardOverlay { reward_state, .. } => reward_state,
        _ => return Err("reward progress requires a reward boundary".to_string()),
    };
    if reward.pending_card_choice.is_some() {
        return Ok(RewardPolicyProgressV1 {
            pending_card_selection: true,
            ..Default::default()
        });
    }
    let context = build_reward_decision_context_v1(&session.run_state, reward);
    let mut progress = RewardPolicyProgressV1 {
        pending_other_rewards: context.candidates.iter().any(|candidate| {
            !matches!(
                candidate.class,
                RewardPolicyClassV1::CardReward
                    | RewardPolicyClassV1::PotionNoEmptySlot
                    | RewardPolicyClassV1::PotionBlockedBySozu
            )
        }) || super::reward_auto::reward_policy_has_potion_space_step(
            session,
        ),
        ..Default::default()
    };
    let surface = build_decision_surface(session);
    for (index, item) in reward.items.iter().enumerate() {
        if !matches!(item, RewardItem::Card { .. }) {
            continue;
        }
        let open = surface
            .view
            .candidates
            .iter()
            .find(|candidate| {
                candidate.key
                    == Some(DecisionCandidateKey::CardRewardOpen {
                        reward_item_index: index,
                    })
            })
            .ok_or_else(|| format!("reward item {index} has no typed open candidate"))?;
        let mut opened = session.clone();
        opened.apply_candidate_id(&open.id)?;
        if opened_card_policy_acquires(&opened)? {
            progress.preferred_card_candidate_ids.push(open.id.clone());
        }
    }
    Ok(progress)
}

fn opened_card_policy_acquires(session: &RunControlSession) -> Result<bool, String> {
    let surface = build_decision_surface(session);
    let legal = surface
        .view
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.key,
                Some(
                    DecisionCandidateKey::CardRewardPick { .. }
                        | DecisionCandidateKey::CardRewardSkip { .. }
                        | DecisionCandidateKey::CardRewardSingingBowl { .. }
                )
            )
        })
        .filter_map(|candidate| {
            candidate
                .action
                .executable_action_ref()
                .map(|action| RunPolicyCandidateV1 {
                    candidate_id: &candidate.id,
                    label: &candidate.label,
                    action,
                })
        })
        .collect::<Vec<_>>();
    let prior = exact_card_reward_policy_prior_v1(session, &legal)?;
    let selected = prior.entries.first().and_then(|entry| {
        surface
            .view
            .candidates
            .iter()
            .find(|candidate| candidate.id == entry.candidate_id)
    });
    match selected.and_then(|candidate| candidate.key.as_ref()) {
        Some(DecisionCandidateKey::CardRewardSkip { .. }) => Ok(false),
        Some(
            DecisionCandidateKey::CardRewardPick { .. }
            | DecisionCandidateKey::CardRewardSingingBowl { .. },
        ) => Ok(true),
        _ => Err("opened card policy has no supported acquisition or skip".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{RunControlConfig, RunDecisionAction};
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::state::core::ClientInput;
    use crate::state::rewards::{RewardCard, RewardState};

    fn session(items: Vec<RewardItem>) -> RunControlSession {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        let mut reward = RewardState::new();
        reward.items = items;
        session.engine_state = EngineState::RewardScreen(reward);
        session
    }

    fn card(id: CardId) -> RewardItem {
        RewardItem::Card {
            cards: vec![RewardCard::new(id, 0)],
        }
    }

    #[test]
    fn completion_distinguishes_empty_declined_and_claimable_rewards() {
        for items in [vec![], vec![card(CardId::Injury)]] {
            let session = session(items);
            assert!(reward_policy_progress_v1(&session).unwrap().is_complete());
        }
        for item in [
            RewardItem::Gold { amount: 12 },
            RewardItem::EmeraldKey,
            card(CardId::ShrugItOff),
        ] {
            assert!(!reward_policy_progress_v1(&session(vec![item]))
                .unwrap()
                .is_complete());
        }
    }

    #[test]
    fn skip_leaves_the_reward_legal_but_does_not_create_new_policy_work() {
        let mut session = session(vec![card(CardId::Injury)]);
        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::ClaimReward(0)))
            .unwrap();
        assert!(
            reward_policy_progress_v1(&session)
                .unwrap()
                .pending_card_selection
        );
        session
            .apply_decision_action(RunDecisionAction::SkipCardReward {
                reward_item_index: 0,
            })
            .unwrap();
        assert!(reward_policy_progress_v1(&session).unwrap().is_complete());
        assert!(build_decision_surface(&session)
            .view
            .candidates
            .iter()
            .any(|c| c.key
                == Some(DecisionCandidateKey::CardRewardOpen {
                    reward_item_index: 0
                })));
        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::ClaimReward(0)))
            .unwrap();
        assert!(
            reward_policy_progress_v1(&session)
                .unwrap()
                .pending_card_selection
        );
    }

    #[test]
    fn useful_reward_after_a_declined_item_tracks_the_current_item_index() {
        let mut session = session(vec![
            RewardItem::Gold { amount: 9 },
            card(CardId::Injury),
            card(CardId::ShrugItOff),
        ]);
        for expected_index in [2, 1] {
            if expected_index == 1 {
                session
                    .apply_decision_action(RunDecisionAction::Input(ClientInput::ClaimReward(0)))
                    .unwrap();
            }
            let progress = reward_policy_progress_v1(&session).unwrap();
            assert_eq!(progress.preferred_card_candidate_ids.len(), 1);
            let surface = build_decision_surface(&session);
            let selected = surface
                .view
                .candidates
                .iter()
                .find(|c| progress.preferred_card_candidate_ids.contains(&c.id))
                .unwrap();
            assert_eq!(
                selected.key,
                Some(DecisionCandidateKey::CardRewardOpen {
                    reward_item_index: expected_index
                })
            );
        }
    }

    #[test]
    fn new_relic_reopens_a_declined_reward_as_a_singing_bowl_acquisition() {
        let mut session = session(vec![card(CardId::Injury)]);
        assert!(reward_policy_progress_v1(&session).unwrap().is_complete());
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::SingingBowl));
        let progress = reward_policy_progress_v1(&session).unwrap();
        assert!(!progress.is_complete());
        let surface = build_decision_surface(&session);
        let selected = surface
            .view
            .candidates
            .iter()
            .find(|c| progress.preferred_card_candidate_ids.contains(&c.id))
            .unwrap();
        let candidate_id = selected.id.clone();
        let hp_before = session.run_state.max_hp;
        session.apply_candidate_id(&candidate_id).unwrap();
        let surface = build_decision_surface(&session);
        let legal = surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect::<Vec<_>>();
        let prior = exact_card_reward_policy_prior_v1(&session, &legal).unwrap();
        session
            .apply_candidate_id(&prior.entries[0].candidate_id)
            .unwrap();
        assert_eq!(session.run_state.max_hp, hp_before + 2);
        assert!(reward_policy_progress_v1(&session).unwrap().is_complete());
    }

    #[test]
    fn full_potion_belt_preserves_work_when_the_policy_can_free_a_slot() {
        let mut session = session(vec![RewardItem::Potion {
            potion_id: PotionId::FirePotion,
        }]);
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::FruitJuice, 1)),
            Some(Potion::new(PotionId::WeakenPotion, 2)),
            Some(Potion::new(PotionId::FearPotion, 3)),
        ];
        assert!(super::super::reward_surface_has_only_unclaimable_potions(
            &session
        ));
        assert!(
            reward_policy_progress_v1(&session)
                .unwrap()
                .pending_other_rewards
        );
    }
}
