//! Residual reward policy owned by the strategic-owner crate.

use sts_simulator::eval::run_control::{
    reward_policy_has_claimable_step, reward_policy_progress_v1, DecisionSurface, RunControlSession,
};

use super::owner_model::{OwnerDecision, OwnerRoutine};

pub(super) fn reward_tiny_owner_decision(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> OwnerDecision {
    match sapphire_key_policy_candidate(session, surface) {
        Ok(Some((candidate_id, action))) => {
            return OwnerDecision::Routine(OwnerRoutine::Candidate {
                candidate_id,
                action,
            })
        }
        Ok(None) => {}
        Err(error) => return OwnerDecision::Gap(error),
    }
    match reward_policy_has_claimable_step(session) {
        Ok(true) => return OwnerDecision::Routine(OwnerRoutine::RewardPolicyStep),
        Ok(false) => {}
        Err(error) => return OwnerDecision::Gap(error),
    }
    let progress = match reward_policy_progress_v1(session) {
        Ok(progress) => progress,
        Err(error) => return OwnerDecision::Gap(error),
    };
    if let Some((candidate_id, action)) = surface
        .view
        .candidates
        .iter()
        .find(|candidate| progress.preferred_card_candidate_ids.first() == Some(&candidate.id))
        .and_then(|candidate| {
            candidate
                .action
                .executable_action()
                .map(|action| (candidate.id.clone(), action))
        })
    {
        return OwnerDecision::Routine(OwnerRoutine::Candidate {
            candidate_id,
            action,
        });
    }
    OwnerDecision::Routine(OwnerRoutine::RewardPolicyStep)
}

fn sapphire_key_policy_candidate(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> Result<Option<(String, sts_simulator::eval::run_control::RunDecisionAction)>, String> {
    use sts_simulator::content::relics::{get_relic_tier, RelicTier};
    use sts_simulator::eval::run_control::RunDecisionAction;
    use sts_simulator::state::core::{ClientInput, EngineState};
    use sts_simulator::state::rewards::RewardItem;

    if !session.run_state.is_final_act_available || session.run_state.keys[1] {
        return Ok(None);
    }
    let reward = match &session.engine_state {
        EngineState::RewardScreen(reward) => reward,
        EngineState::RewardOverlay { reward_state, .. } => reward_state,
        _ => return Ok(None),
    };
    if reward.pending_card_choice.is_some() {
        return Ok(None);
    }
    let Some(key_index) = reward
        .items
        .iter()
        .position(|item| matches!(item, RewardItem::SapphireKey))
    else {
        return Ok(None);
    };
    // The engine links the key to the immediately preceding relic. Do not
    // infer a link to a different relic elsewhere on the reward screen.
    let linked_relic = key_index
        .checked_sub(1)
        .and_then(|index| match reward.items.get(index) {
            Some(RewardItem::Relic { relic_id }) => Some((index, *relic_id)),
            _ => None,
        });
    let reward_item_index = match (session.run_state.act_num, linked_relic) {
        // Reference policy: realize an ordinary relic in Acts 1/2 while
        // later Acts still offer a guaranteed chest. This is a deferral
        // preference, not a valuation of unseen future relics or a proof
        // that every ordinary relic improves the current deck.
        (1 | 2, Some((index, relic)))
            if matches!(
                get_relic_tier(relic),
                RelicTier::Common | RelicTier::Uncommon | RelicTier::Rare
            ) =>
        {
            index
        }
        (1..=3, None) | (3, _) => key_index,
        _ => return Ok(None),
    };
    let expected = RunDecisionAction::Input(ClientInput::ClaimReward(reward_item_index));
    let candidate_id = super::owner_commands::owner_candidate_id_for_action(surface, &expected)?;
    Ok(Some((candidate_id, expected)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_simulator::content::cards::CardId;
    use sts_simulator::content::relics::RelicId;
    use sts_simulator::eval::run_control::{
        build_decision_surface, DecisionCandidateKey, RunControlConfig, RunControlSession,
        RunDecisionAction,
    };
    use sts_simulator::state::core::{ClientInput, EngineState};
    use sts_simulator::state::rewards::{RewardCard, RewardItem, RewardState};

    #[test]
    fn sapphire_key_is_deferred_for_ordinary_relics_until_act_three() {
        for act in 1..=3 {
            for overlay in [false, true] {
                for relic_id in [
                    RelicId::RegalPillow,
                    RelicId::Omamori,
                    RelicId::MummifiedHand,
                ] {
                    let mut session = RunControlSession::new(RunControlConfig {
                        final_act: true,
                        ..Default::default()
                    });
                    session.run_state.event_state = None;
                    session.run_state.act_num = act;
                    let mut reward = RewardState::new();
                    reward.items = vec![
                        RewardItem::Gold { amount: 12 },
                        RewardItem::Relic { relic_id },
                        RewardItem::SapphireKey,
                    ];
                    session.engine_state = if overlay {
                        EngineState::reward_overlay(reward, EngineState::MapNavigation)
                    } else {
                        EngineState::RewardScreen(reward)
                    };
                    let before = session.clone();
                    let (_, action) =
                        crate::current_privileged_strategic_owner_action_result_v1(&session)
                            .unwrap()
                            .unwrap();
                    assert_eq!(
                        action,
                        RunDecisionAction::Input(ClientInput::ClaimReward(if act == 3 {
                            2
                        } else {
                            1
                        }))
                    );
                    assert_eq!(session.engine_state, before.engine_state);
                    assert_eq!(session.run_state.relics, before.run_state.relics);
                    let surface = build_decision_surface(&session);
                    let order = crate::current_oracle_candidate_order_v1(&session);
                    // Both sides of the tradeoff remain available to search.
                    for index in [1, 2] {
                        assert!(surface.view.candidates.iter().any(|candidate| candidate
                            .action
                            .executable_action()
                            == Some(RunDecisionAction::Input(ClientInput::ClaimReward(index)))
                            && order.contains(&candidate.id)));
                    }
                    session.apply_decision_action(action).unwrap();
                    assert_eq!(session.run_state.keys[1], act == 3);
                    assert_eq!(
                        session.run_state.relics.iter().any(|r| r.id == relic_id),
                        act != 3
                    );
                    let reward = match &session.engine_state {
                        EngineState::RewardScreen(reward)
                        | EngineState::RewardOverlay {
                            reward_state: reward,
                            ..
                        } => reward,
                        _ => panic!("claim remains on rewards"),
                    };
                    assert_eq!(reward.items, vec![RewardItem::Gold { amount: 12 }]);
                }
            }
        }
    }

    #[test]
    fn key_policy_uses_only_the_linked_relic_and_requires_a_visible_action() {
        let mut session = RunControlSession::new(RunControlConfig {
            final_act: true,
            ..Default::default()
        });
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::SapphireKey,
            RewardItem::Relic {
                relic_id: RelicId::Omamori,
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let mut surface = build_decision_surface(&session);
        assert_eq!(
            sapphire_key_policy_candidate(&session, &surface)
                .unwrap()
                .unwrap()
                .1,
            RunDecisionAction::Input(ClientInput::ClaimReward(0))
        );
        surface.view.candidates.clear();
        assert!(sapphire_key_policy_candidate(&session, &surface).is_err());
        session.run_state.keys[1] = true;
        assert!(sapphire_key_policy_candidate(&session, &surface)
            .unwrap()
            .is_none());
        session.run_state.keys[1] = false;
        session.run_state.is_final_act_available = false;
        assert!(sapphire_key_policy_candidate(&session, &surface)
            .unwrap()
            .is_none());
    }

    #[test]
    fn unavailable_policy_and_invalid_reward_exit_are_distinct() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        session.reward_automation.claim_gold = false;
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Gold { amount: 12 }];
        session.engine_state = EngineState::RewardScreen(reward);
        let before = session.engine_state.clone();
        assert!(
            crate::current_privileged_strategic_owner_action_result_v1(&session)
                .unwrap()
                .is_none()
        );
        assert_eq!(session.engine_state, before);
        let mut reward = RewardState::new();
        reward.skippable = false;
        session.engine_state = EngineState::RewardScreen(reward);
        // No work is pending, but the required exit cannot be bound. This
        // must propagate as an error rather than unavailable policy support.
        assert!(crate::current_privileged_strategic_owner_action_result_v1(&session).is_err());
    }

    #[test]
    fn declined_reward_and_empty_reward_both_continue_to_a_real_route() {
        for decline_card in [false, true] {
            let mut session = RunControlSession::new(RunControlConfig::default());
            session.run_state.event_state = None;
            let mut reward = RewardState::new();
            if decline_card {
                reward.items.push(RewardItem::Card {
                    cards: vec![RewardCard::new(CardId::Injury, 0)],
                });
            }
            session.engine_state = EngineState::RewardScreen(reward);
            if decline_card {
                session
                    .apply_decision_action(RunDecisionAction::Input(ClientInput::ClaimReward(0)))
                    .unwrap();
                let (_, action) =
                    crate::current_privileged_strategic_owner_action_result_v1(&session)
                        .unwrap()
                        .unwrap();
                assert!(matches!(action, RunDecisionAction::SkipCardReward { .. }));
                session.apply_decision_action(action).unwrap();
            }
            let (_, action) = crate::current_privileged_strategic_owner_action_result_v1(&session)
                .unwrap()
                .unwrap();
            assert_eq!(action, RunDecisionAction::Input(ClientInput::Proceed));
            session.apply_decision_action(action).unwrap();
            assert!(build_decision_surface(&session)
                .view
                .candidates
                .iter()
                .any(|c| c.key == Some(DecisionCandidateKey::RouteCancel)));
            let (_, action) = crate::current_privileged_strategic_owner_action_result_v1(&session)
                .unwrap()
                .unwrap();
            assert!(matches!(
                action,
                RunDecisionAction::Input(ClientInput::SelectMapNode(_))
            ));
            let floor_before = session.run_state.floor_num;
            session.apply_decision_action(action).unwrap();
            assert!(session.run_state.floor_num > floor_before);
        }
    }

    #[test]
    fn reward_owner_processes_a_useful_reward_beyond_a_declined_one() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::Injury, 0)],
            },
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::ShrugItOff, 0)],
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let surface = build_decision_surface(&session);
        let OwnerDecision::Routine(OwnerRoutine::Candidate { action, .. }) =
            reward_tiny_owner_decision(&session, &surface)
        else {
            panic!("pending reward");
        };
        assert_eq!(
            action,
            RunDecisionAction::Input(ClientInput::ClaimReward(1))
        );
        for _ in 0..2 {
            let (_, action) = crate::current_privileged_strategic_owner_action_result_v1(&session)
                .unwrap()
                .unwrap();
            session.apply_decision_action(action).unwrap();
        }
        assert!(session
            .run_state
            .master_deck
            .iter()
            .any(|c| c.id == CardId::ShrugItOff));
        let (_, action) = crate::current_privileged_strategic_owner_action_result_v1(&session)
            .unwrap()
            .unwrap();
        assert_eq!(action, RunDecisionAction::Input(ClientInput::Proceed));
    }

    #[test]
    fn declined_shop_reward_returns_once_and_does_not_reopen() {
        use sts_simulator::state::shop::ShopState;
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        session.run_state.gold = 0;
        let mut shop = ShopState::new();
        shop.purge_available = false;
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Card {
            cards: vec![RewardCard::new(CardId::Injury, 0)],
        }];
        session.engine_state = EngineState::reward_overlay(reward, EngineState::Shop(shop));
        let (_, action) = crate::current_privileged_strategic_owner_action_result_v1(&session)
            .unwrap()
            .unwrap();
        assert_eq!(action, RunDecisionAction::Input(ClientInput::Cancel));
        session.apply_decision_action(action).unwrap();
        assert!(build_decision_surface(&session)
            .view
            .candidates
            .iter()
            .any(|c| c.key == Some(DecisionCandidateKey::ShopOpenRewards)));
        let (_, action) = crate::current_privileged_strategic_owner_action_result_v1(&session)
            .unwrap()
            .unwrap();
        assert_ne!(
            action,
            RunDecisionAction::Input(ClientInput::OpenRewardOverlay)
        );
        session.apply_decision_action(action).unwrap();
        assert!(!matches!(
            session.engine_state,
            EngineState::RewardOverlay { .. }
        ));
    }

    #[test]
    fn reward_owner_claims_safe_relic_before_opening_card_boundary() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::ShrugItOff, 0)],
            },
            RewardItem::Relic {
                relic_id: RelicId::MummifiedHand,
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);

        let surface = build_decision_surface(&session);
        assert!(matches!(
            reward_tiny_owner_decision(&session, &surface),
            OwnerDecision::Routine(OwnerRoutine::RewardPolicyStep)
        ));
        let current_order = crate::current_oracle_candidate_order_v1(&session);
        let current_first = surface
            .view
            .candidates
            .iter()
            .find(|candidate| Some(&candidate.id) == current_order.first())
            .expect("production prior should select one exact candidate");
        assert!(matches!(
            current_first.action.executable_action_ref(),
            Some(RunDecisionAction::Input(ClientInput::ClaimReward(1)))
        ));
        crate::apply_owner_routine(&mut session, OwnerRoutine::RewardPolicyStep)
            .expect("safe relic claim should execute as one owner step");
        assert!(session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::MummifiedHand));

        let surface = build_decision_surface(&session);
        let OwnerDecision::Routine(OwnerRoutine::Candidate { candidate_id, .. }) =
            reward_tiny_owner_decision(&session, &surface)
        else {
            panic!("card reward should become the next boundary");
        };
        assert!(surface.view.candidates.iter().any(|candidate| {
            candidate.id == candidate_id
                && matches!(
                    candidate.key,
                    Some(DecisionCandidateKey::CardRewardOpen { .. })
                )
        }));
    }

    #[test]
    fn act_three_heart_reward_claims_sapphire_key_before_linked_relic() {
        let mut session = RunControlSession::new(RunControlConfig {
            final_act: true,
            ..RunControlConfig::default()
        });
        session.run_state.act_num = 3;
        session.run_state.keys[1] = false;
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Relic {
                relic_id: RelicId::MummifiedHand,
            },
            RewardItem::SapphireKey,
        ];
        session.engine_state = EngineState::RewardScreen(reward);

        let surface = build_decision_surface(&session);
        let OwnerDecision::Routine(OwnerRoutine::Candidate { action, .. }) =
            reward_tiny_owner_decision(&session, &surface)
        else {
            panic!("Act 3 Heart reward must select the Sapphire Key candidate");
        };
        assert_eq!(
            action,
            RunDecisionAction::Input(ClientInput::ClaimReward(1))
        );
    }
}
