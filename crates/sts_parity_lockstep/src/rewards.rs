use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{SemanticChoiceV1, SemanticScalarV1};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalRunRewardsV2 {
    Inactive,
    Combat {
        items: Vec<NaturalRunRewardItemV1>,
        skippable: bool,
    },
    Treasure {
        items: Vec<NaturalRunRewardItemV1>,
        skippable: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalRunRewardItemV1 {
    Gold { amount: i32 },
    StolenGold { amount: i32 },
    Card { cards: Vec<NaturalRewardCardV1> },
    Relic { relic_id: String },
    Potion { potion_id: String },
    EmeraldKey,
    SapphireKey,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRewardCardV1 {
    pub card_id: String,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalBossRelicChoiceV1 {
    pub relics: Vec<String>,
    pub can_skip: bool,
}

pub fn natural_boss_relic_choice_v1(relic_id: &str, occurrence: u32) -> SemanticChoiceV1 {
    SemanticChoiceV1::new("boss_relic", relic_id, occurrence)
}

/// Suspended Discovery action at its real player-choice boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatDiscoveryV1 {
    pub cards: Vec<NaturalRewardCardV1>,
    pub amount: u8,
    pub colorless: bool,
    pub card_type: Option<String>,
    pub cost_for_turn: u8,
    pub can_skip: bool,
}

/// Nilry's Codex offers one temporary card, inserted at a random draw-pile
/// position at its printed cost, or skipped. This is not Discovery's free hand copy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatCodexV1 {
    pub cards: Vec<NaturalRewardCardV1>,
    pub can_skip: bool,
}

pub fn natural_codex_choice_v1(card: &NaturalRewardCardV1, occurrence: u32) -> SemanticChoiceV1 {
    let mut choice = natural_card_reward_choice_v1(card, occurrence);
    choice.kind = "codex_card".to_owned();
    choice
}

pub fn natural_discovery_choice_v1(
    card: &NaturalRewardCardV1,
    occurrence: u32,
) -> SemanticChoiceV1 {
    let mut choice = natural_card_reward_choice_v1(card, occurrence);
    choice.kind = "discovery_card".to_owned();
    choice
}

/// One offered card on an ordinary run card-reward screen.
///
/// The card's stable game ID and upgrade count identify its visible semantics;
/// `occurrence` distinguishes duplicate offers without exposing Java UUIDs or
/// endpoint-local collection positions.
pub fn natural_card_reward_choice_v1(
    card: &NaturalRewardCardV1,
    occurrence: u32,
) -> SemanticChoiceV1 {
    let mut choice = SemanticChoiceV1::new("reward_card", card.card_id.clone(), occurrence);
    choice.attributes.insert(
        "upgrades".to_owned(),
        SemanticScalarV1::Integer(i64::from(card.upgrades)),
    );
    choice
}

pub fn natural_singing_bowl_choice_v1() -> SemanticChoiceV1 {
    SemanticChoiceV1::new("card_reward_option", "singing_bowl", 0)
}

pub fn natural_reward_choice_v1(
    item: &NaturalRunRewardItemV1,
    occurrence: u32,
) -> SemanticChoiceV1 {
    let mut attributes = BTreeMap::new();
    let id = match item {
        NaturalRunRewardItemV1::Gold { amount } => {
            attributes.insert(
                "amount".to_owned(),
                SemanticScalarV1::Integer(i64::from(*amount)),
            );
            "gold"
        }
        NaturalRunRewardItemV1::StolenGold { amount } => {
            attributes.insert(
                "amount".to_owned(),
                SemanticScalarV1::Integer(i64::from(*amount)),
            );
            "stolen_gold"
        }
        NaturalRunRewardItemV1::Card { cards } => {
            for (index, card) in cards.iter().enumerate() {
                attributes.insert(
                    format!("card_{index}"),
                    SemanticScalarV1::Text(card.card_id.clone()),
                );
                attributes.insert(
                    format!("upgrade_{index}"),
                    SemanticScalarV1::Integer(i64::from(card.upgrades)),
                );
            }
            "card"
        }
        NaturalRunRewardItemV1::Relic { relic_id } => {
            attributes.insert("relic".to_owned(), SemanticScalarV1::Text(relic_id.clone()));
            "relic"
        }
        NaturalRunRewardItemV1::Potion { potion_id } => {
            attributes.insert(
                "potion".to_owned(),
                SemanticScalarV1::Text(potion_id.clone()),
            );
            "potion"
        }
        NaturalRunRewardItemV1::EmeraldKey => "emerald_key",
        NaturalRunRewardItemV1::SapphireKey => "sapphire_key",
    };
    SemanticChoiceV1 {
        kind: "run_reward".to_owned(),
        id: id.to_owned(),
        occurrence,
        attributes,
    }
}
