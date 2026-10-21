use crate::state::selection::SelectionResolution;
use crate::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum ClientInput {
    PlayCard {
        card_index: usize,
        target: Option<EntityId>,
    },
    UsePotion {
        potion_index: usize,
        target: Option<EntityId>,
    },
    DiscardPotion(usize),
    EndTurn,
    SubmitCardChoice(Vec<usize>),
    SubmitDiscoverChoice(usize),
    SelectMapNode(usize),
    FlyToNode(usize, usize),
    SelectEventOption(usize),
    CampfireOption(CampfireChoice),
    EventChoice(usize),
    SubmitScryDiscard(Vec<usize>),
    SubmitSelection(SelectionResolution),
    ClaimReward(usize),
    OpenRewardOverlay,
    OpenChest,
    SelectCard(usize),
    BuyCard(usize),
    BuyRelic(usize),
    BuyPotion(usize),
    PurgeCard(usize),
    SubmitRelicChoice(usize),
    Proceed,
    Cancel,
}

impl ClientInput {
    /// Payload-free protocol token. Adding an input requires choosing this
    /// token explicitly; Rust variant names are never an action identity.
    pub const fn kind_v1(&self) -> &'static str {
        match self {
            Self::PlayCard { .. } => "play_card",
            Self::UsePotion { .. } => "use_potion",
            Self::DiscardPotion(_) => "discard_potion",
            Self::EndTurn => "end_turn",
            Self::SubmitCardChoice(_) => "submit_card_choice",
            Self::SubmitDiscoverChoice(_) => "submit_discover_choice",
            Self::SelectMapNode(_) => "select_map_node",
            Self::FlyToNode(_, _) => "fly_to_node",
            Self::SelectEventOption(_) => "select_event_option",
            Self::CampfireOption(_) => "campfire_option",
            Self::EventChoice(_) => "event_choice",
            Self::SubmitScryDiscard(_) => "submit_scry_discard",
            Self::SubmitSelection(_) => "submit_selection",
            Self::ClaimReward(_) => "claim_reward",
            Self::OpenRewardOverlay => "open_reward_overlay",
            Self::OpenChest => "open_chest",
            Self::SelectCard(_) => "select_card",
            Self::BuyCard(_) => "buy_card",
            Self::BuyRelic(_) => "buy_relic",
            Self::BuyPotion(_) => "buy_potion",
            Self::PurgeCard(_) => "purge_card",
            Self::SubmitRelicChoice(_) => "submit_relic_choice",
            Self::Proceed => "proceed",
            Self::Cancel => "cancel",
        }
    }

    /// Stable runtime-handle payload for action keys and fingerprints.
    /// Gameplay semantics such as the card or potion identity are enriched by
    /// the owning combat surface rather than inferred here.
    pub fn payload_key_v1(&self) -> String {
        match self {
            Self::PlayCard { card_index, target } => format!(
                "play_card/hand:{card_index}/target:{}",
                optional_index(*target)
            ),
            Self::UsePotion {
                potion_index,
                target,
            } => format!(
                "use_potion/slot:{potion_index}/target:{}",
                optional_index(*target)
            ),
            Self::DiscardPotion(index) => format!("discard_potion/{index}"),
            Self::EndTurn => "end_turn".to_string(),
            Self::SubmitCardChoice(indices) => {
                format!("submit_card_choice/{}", decimal_list(indices))
            }
            Self::SubmitDiscoverChoice(index) => format!("submit_discover_choice/{index}"),
            Self::SelectMapNode(index) => format!("select_map_node/{index}"),
            Self::FlyToNode(row, column) => format!("fly_to_node/{row}/{column}"),
            Self::SelectEventOption(index) => format!("select_event_option/{index}"),
            Self::CampfireOption(choice) => choice.payload_key_v1(),
            Self::EventChoice(index) => format!("event_choice/{index}"),
            Self::SubmitScryDiscard(indices) => {
                format!("submit_scry_discard/{}", decimal_list(indices))
            }
            Self::SubmitSelection(resolution) => format!(
                "submit_selection/{}/{}",
                resolution.scope.vocabulary_token_v1(),
                decimal_list(&resolution.selected_card_lineages())
            ),
            Self::ClaimReward(index) => format!("claim_reward/{index}"),
            Self::OpenRewardOverlay => "open_reward_overlay".to_string(),
            Self::OpenChest => "open_chest".to_string(),
            Self::SelectCard(index) => format!("select_card/{index}"),
            Self::BuyCard(index) => format!("buy_card/{index}"),
            Self::BuyRelic(index) => format!("buy_relic/{index}"),
            Self::BuyPotion(index) => format!("buy_potion/{index}"),
            Self::PurgeCard(index) => format!("purge_card/{index}"),
            Self::SubmitRelicChoice(index) => format!("submit_relic_choice/{index}"),
            Self::Proceed => "proceed".to_string(),
            Self::Cancel => "cancel".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum CampfireChoice {
    Rest,
    Smith(usize),
    Dig,
    Lift,
    Toke(usize),
    Recall,
}

impl CampfireChoice {
    pub fn payload_key_v1(self) -> String {
        match self {
            Self::Rest => "campfire_option/rest".to_string(),
            Self::Smith(index) => format!("campfire_option/smith/{index}"),
            Self::Dig => "campfire_option/dig".to_string(),
            Self::Lift => "campfire_option/lift".to_string(),
            Self::Toke(index) => format!("campfire_option/toke/{index}"),
            Self::Recall => "campfire_option/recall".to_string(),
        }
    }
}

fn decimal_list<T>(values: &[T]) -> String
where
    T: std::fmt::Display,
{
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn optional_index(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}
