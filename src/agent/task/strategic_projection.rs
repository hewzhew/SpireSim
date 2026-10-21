use crate::ai::planner_core::{PlannerAction, PlannerCardObservation};

use super::TaskStrategicActionV1;

impl TaskStrategicActionV1 {
    pub fn from_planner_action_v1(
        cards: &[PlannerCardObservation],
        action: &PlannerAction,
    ) -> Result<Self, String> {
        let card_ordinal = |lineage| {
            cards
                .iter()
                .position(|card| card.card_lineage == lineage)
                .ok_or_else(|| {
                    "strategic action references a card outside public deck order".to_string()
                })
        };
        Ok(match action {
            PlannerAction::ChooseRouteNode {
                x,
                y,
                flight,
                question_mark_outcomes,
            } => Self::ChooseRouteNode {
                x: *x,
                y: *y,
                flight: *flight,
                question_mark_outcomes: question_mark_outcomes.clone(),
            },
            PlannerAction::ChooseEventOption {
                event,
                screen,
                option_index,
                action,
                public_semantics,
            } => Self::ChooseEventOption {
                event: *event,
                screen: *screen,
                option_index: *option_index,
                action: *action,
                public_semantics: public_semantics.clone(),
            },
            PlannerAction::TakeCard {
                reward_item_index,
                option_index,
                card,
                upgrades,
            } => Self::TakeCard {
                reward_item_index: *reward_item_index,
                option_index: *option_index,
                card: *card,
                upgrades: *upgrades,
            },
            PlannerAction::OpenCardReward { reward_item_index } => Self::OpenCardReward {
                reward_item_index: *reward_item_index,
            },
            PlannerAction::SingingBowl { reward_item_index } => Self::SingingBowl {
                reward_item_index: *reward_item_index,
            },
            PlannerAction::SkipCardReward { reward_item_index } => Self::SkipCardReward {
                reward_item_index: *reward_item_index,
            },
            PlannerAction::ClaimReward {
                reward_item_index,
                reward,
            } => Self::ClaimReward {
                reward_item_index: *reward_item_index,
                reward: reward.clone(),
            },
            PlannerAction::Rest => Self::Rest,
            PlannerAction::Smith {
                card_lineage,
                card,
                upgrades,
            } => Self::Smith {
                card_ordinal: card_ordinal(*card_lineage)?,
                card: *card,
                upgrades: *upgrades,
            },
            PlannerAction::Dig => Self::Dig,
            PlannerAction::Lift => Self::Lift,
            PlannerAction::Toke {
                card_lineage,
                card,
                upgrades,
            } => Self::Toke {
                card_ordinal: card_ordinal(*card_lineage)?,
                card: *card,
                upgrades: *upgrades,
            },
            PlannerAction::Recall => Self::Recall,
            PlannerAction::BuyCard {
                shop_slot,
                card,
                upgrades,
                price,
            } => Self::BuyCard {
                shop_slot: *shop_slot,
                card: *card,
                upgrades: *upgrades,
                price: *price,
            },
            PlannerAction::BuyRelic {
                shop_slot,
                relic,
                price,
            } => Self::BuyRelic {
                shop_slot: *shop_slot,
                relic: *relic,
                price: *price,
            },
            PlannerAction::BuyPotion {
                shop_slot,
                potion,
                price,
            } => Self::BuyPotion {
                shop_slot: *shop_slot,
                potion: *potion,
                price: *price,
            },
            PlannerAction::UseRunPotion { slot, potion, .. } => Self::UseRunPotion {
                slot: *slot,
                potion: *potion,
            },
            PlannerAction::DiscardRunPotion { slot, potion, .. } => Self::DiscardRunPotion {
                slot: *slot,
                potion: *potion,
            },
            PlannerAction::RemoveCard {
                card_lineage,
                card,
                upgrades,
                price,
            } => Self::RemoveCard {
                card_ordinal: card_ordinal(*card_lineage)?,
                card: *card,
                upgrades: *upgrades,
                price: *price,
            },
            PlannerAction::OpenPendingRewards => Self::OpenPendingRewards,
            PlannerAction::LeaveShop => Self::LeaveShop,
            PlannerAction::TakeBossRelic {
                option_index,
                relic,
            } => Self::TakeBossRelic {
                option_index: *option_index,
                relic: *relic,
            },
            PlannerAction::SkipBossRelic => Self::SkipBossRelic,
            PlannerAction::BeginRunCardSelection {
                scope,
                reason,
                min_choices,
                max_choices,
                selectable_card_lineages,
            } => Self::BeginRunCardSelection {
                scope: *scope,
                reason: *reason,
                min_choices: *min_choices,
                max_choices: *max_choices,
                selectable_card_ordinals: selectable_card_lineages
                    .iter()
                    .map(|lineage| card_ordinal(*lineage))
                    .collect::<Result<_, _>>()?,
            },
            PlannerAction::SubmitRunSelection {
                scope,
                selected_card_lineages,
            } => Self::SubmitRunSelection {
                scope: *scope,
                selected_card_ordinals: selected_card_lineages
                    .iter()
                    .map(|lineage| card_ordinal(*lineage))
                    .collect::<Result<_, _>>()?,
            },
            PlannerAction::OpenChest => Self::OpenChest,
            PlannerAction::Proceed { site } => Self::Proceed { site: *site },
            PlannerAction::Cancel { site } => Self::Cancel { site: *site },
        })
    }
}
