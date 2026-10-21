use crate::ai::noncombat_strategy_v1::StrategyPlanSupportV1;
use crate::content::identity::gameplay_display_label_v1;
use crate::content::potions::{get_potion_definition, PotionId};
use crate::content::relics::RelicId;
use crate::state::rewards::{RewardItem, RewardState};
use crate::state::run::RunState;

use super::evaluator::low_agency_claim_action;
use super::types::{
    reward_candidate_id, RewardCandidateEvidenceV1, RewardDecisionContextV1, RewardDecisionV1,
    RewardPolicyActionV1, RewardPolicyClassV1, RewardPolicyConfigV1,
};

pub fn build_reward_decision_context_v1(
    run_state: &RunState,
    reward: &RewardState,
) -> RewardDecisionContextV1 {
    let has_empty_potion_slot = run_state.find_empty_potion_slot().is_some();
    let has_sozu = run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Sozu);
    let has_sapphire_key_reward = reward
        .items
        .iter()
        .any(|item| matches!(item, RewardItem::SapphireKey));
    let candidates = reward
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            candidate_evidence(
                index,
                item,
                has_empty_potion_slot,
                has_sozu,
                has_sapphire_key_reward,
            )
        })
        .collect();

    RewardDecisionContextV1 {
        pending_card_choice_open: reward.pending_card_choice.is_some(),
        has_empty_potion_slot,
        has_sozu,
        has_sapphire_key_reward,
        candidates,
    }
}

pub fn plan_reward_decision_v1(
    context: &RewardDecisionContextV1,
    config: &RewardPolicyConfigV1,
) -> RewardDecisionV1 {
    let claimable_fruit_juice = context.candidates.iter().any(|candidate| {
        candidate.class == RewardPolicyClassV1::PotionWithEmptySlot
            && candidate.potion_id == Some(PotionId::FruitJuice)
    });
    let action = if context.pending_card_choice_open {
        RewardPolicyActionV1::Stop {
            reason: "reward policy stopped because a card reward choice is open".to_string(),
        }
    } else {
        context
            .candidates
            .iter()
            .find_map(|candidate| {
                if claimable_fruit_juice
                    && candidate.class == RewardPolicyClassV1::PotionWithEmptySlot
                    && candidate.potion_id != Some(PotionId::FruitJuice)
                {
                    return None;
                }
                low_agency_claim_action(candidate, config)
            })
            .unwrap_or_else(|| RewardPolicyActionV1::Stop {
                reason: stop_reason(context),
            })
    };

    RewardDecisionV1 {
        action,
        label_role: "behavior_policy_not_teacher",
        context: context.clone(),
    }
}

fn candidate_evidence(
    index: usize,
    item: &RewardItem,
    has_empty_potion_slot: bool,
    has_sozu: bool,
    has_sapphire_key_reward: bool,
) -> RewardCandidateEvidenceV1 {
    let class = reward_class(
        item,
        has_empty_potion_slot,
        has_sozu,
        has_sapphire_key_reward,
    );
    let support_gate = support_gate_for_class(class);
    let mut evidence = vec![format!("reward item is {class:?}")];
    let mut risks = Vec::new();

    match class {
        RewardPolicyClassV1::Gold | RewardPolicyClassV1::StolenGold => {
            evidence.push("visible deterministic gold reward".to_string());
        }
        RewardPolicyClassV1::PotionWithEmptySlot => {
            evidence.push("empty potion slot is available".to_string());
        }
        RewardPolicyClassV1::PotionNoEmptySlot => {
            risks.push(
                "potion slots are full; claiming would require replacement or no-op behavior"
                    .to_string(),
            );
        }
        RewardPolicyClassV1::PotionBlockedBySozu => {
            risks.push("Sozu blocks potion gain".to_string());
        }
        RewardPolicyClassV1::RelicWithoutSapphireKeyConflict => {
            evidence.push("no Sapphire Key reward is present on this reward screen".to_string());
        }
        RewardPolicyClassV1::RelicWithSapphireKeyConflict => {
            risks.push("Sapphire Key competes with the visible relic reward".to_string());
        }
        RewardPolicyClassV1::CardReward => {
            risks.push("card reward selection is a separate strategy boundary".to_string());
        }
        RewardPolicyClassV1::EmeraldKey => {
            risks.push("Emerald Key timing is a route objective".to_string());
        }
        RewardPolicyClassV1::SapphireKey => {
            risks.push("Sapphire Key competes with the visible relic reward".to_string());
        }
    }

    RewardCandidateEvidenceV1 {
        index,
        candidate_id: reward_candidate_id(index, item),
        label: reward_item_label(item),
        potion_id: match item {
            RewardItem::Potion { potion_id } => Some(*potion_id),
            _ => None,
        },
        class,
        support_gate,
        evidence,
        risks,
    }
}

fn reward_class(
    item: &RewardItem,
    has_empty_potion_slot: bool,
    has_sozu: bool,
    has_sapphire_key_reward: bool,
) -> RewardPolicyClassV1 {
    match item {
        RewardItem::Gold { .. } => RewardPolicyClassV1::Gold,
        RewardItem::StolenGold { .. } => RewardPolicyClassV1::StolenGold,
        RewardItem::Potion { .. } if has_sozu => RewardPolicyClassV1::PotionBlockedBySozu,
        RewardItem::Potion { .. } if has_empty_potion_slot => {
            RewardPolicyClassV1::PotionWithEmptySlot
        }
        RewardItem::Potion { .. } => RewardPolicyClassV1::PotionNoEmptySlot,
        RewardItem::Relic { .. } if has_sapphire_key_reward => {
            RewardPolicyClassV1::RelicWithSapphireKeyConflict
        }
        RewardItem::Relic { .. } => RewardPolicyClassV1::RelicWithoutSapphireKeyConflict,
        RewardItem::Card { .. } => RewardPolicyClassV1::CardReward,
        RewardItem::EmeraldKey => RewardPolicyClassV1::EmeraldKey,
        RewardItem::SapphireKey => RewardPolicyClassV1::SapphireKey,
    }
}

fn support_gate_for_class(class: RewardPolicyClassV1) -> StrategyPlanSupportV1 {
    match class {
        RewardPolicyClassV1::Gold
        | RewardPolicyClassV1::StolenGold
        | RewardPolicyClassV1::PotionWithEmptySlot
        | RewardPolicyClassV1::RelicWithoutSapphireKeyConflict => StrategyPlanSupportV1::Strong,
        _ => StrategyPlanSupportV1::Blocked,
    }
}

fn reward_item_label(item: &RewardItem) -> String {
    match item {
        RewardItem::Gold { amount } => format!("{amount} gold"),
        RewardItem::StolenGold { amount } => format!("{amount} stolen gold"),
        RewardItem::Potion { potion_id } => {
            normalized_potion_reward_label(get_potion_definition(*potion_id).name)
        }
        RewardItem::Relic { relic_id } => {
            format!("Relic {}", gameplay_display_label_v1(*relic_id))
        }
        RewardItem::Card { .. } => "Card reward".to_string(),
        RewardItem::EmeraldKey => "Emerald key".to_string(),
        RewardItem::SapphireKey => "Sapphire key".to_string(),
    }
}

fn normalized_potion_reward_label(name: &str) -> String {
    if name.to_ascii_lowercase().ends_with("potion") {
        name.to_string()
    } else {
        format!("{name} potion")
    }
}

fn stop_reason(context: &RewardDecisionContextV1) -> String {
    if context.pending_card_choice_open {
        return "reward policy stopped because a card reward choice is open".to_string();
    }
    let classes = context
        .candidates
        .iter()
        .map(|candidate| format!("{}:{:?}", candidate.label, candidate.class))
        .collect::<Vec<_>>()
        .join(", ");
    format!("reward policy stopped because no low-agency claim action matched ({classes})")
}
