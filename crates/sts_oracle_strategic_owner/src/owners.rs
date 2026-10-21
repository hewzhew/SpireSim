//! Strategic-owner boundary dispatcher.

use sts_simulator::eval::run_control::{DecisionSurface, RunControlSession};

use super::boss_relic_owner::boss_relic_owner_choices;
use super::campfire_owner::campfire_owner_decision;
use super::card_reward_owner::card_reward_owner_choices;
use super::event_owner_bridge::event_owner_decision;
use super::owner_model::OwnerDecision;
use super::reward_tiny_owner::reward_tiny_owner_decision;
use super::run_choice_owner::run_choice_owner_decision;
use super::shop_tiny_owner::shop_tiny_owner_choices;
use super::Owner;

pub fn owner_decision(
    session: &RunControlSession,
    owner: Owner,
    surface: &DecisionSurface,
) -> OwnerDecision {
    match owner {
        Owner::CardReward => match card_reward_owner_choices(session, surface) {
            Ok(choices) => OwnerDecision::Candidates(choices),
            Err(err) => OwnerDecision::Gap(err),
        },
        Owner::BossRelic => match boss_relic_owner_choices(session, surface) {
            Ok(choices) => OwnerDecision::Candidates(choices),
            Err(err) => OwnerDecision::Gap(err),
        },
        Owner::ShopTiny => match shop_tiny_owner_choices(session, surface) {
            Ok(choices) => OwnerDecision::Candidates(choices),
            Err(err) => OwnerDecision::Gap(err),
        },
        Owner::Event(_) => event_owner_decision(session, surface),
        Owner::RewardTiny => reward_tiny_owner_decision(session, surface),
        Owner::Campfire => campfire_owner_decision(session, surface),
        Owner::RunChoice => run_choice_owner_decision(session, surface),
    }
}
