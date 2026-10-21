use super::*;
use crate::runtime::combat::CardLineageId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct StableRunPendingChoiceKey {
    pub selected: Vec<CardLineageId>,
    pub min_choices: usize,
    pub max_choices: usize,
    pub reason: String,
    pub return_state: StableRunPendingReturnKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) enum StableRunPendingReturnKey {
    Reward(StableRewardKey),
    TreasureRoom(StableTreasureChestKey),
    Campfire,
    Shop(StableShopKey),
    MapNavigation,
    MapOverlay(Box<StableRunPendingReturnKey>),
    EventRoom,
    BossRelic(StableBossRelicKey),
    RunPendingChoice(Box<StableRunPendingChoiceKey>),
    GameOver(&'static str),
    Combat,
    RewardOverlay {
        reward: StableRewardKey,
        return_state: Box<StableRunPendingReturnKey>,
    },
    DoubleBossComplete,
    Act3VictoryComplete,
    TrueVictoryComplete,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct StableTreasureChestKey {
    pub size: String,
    pub base_relic_tier: String,
    pub gold_reward: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct StableBossRelicKey {
    pub relics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum StablePendingChoiceKey {
    GridSelect {
        source_pile: &'static str,
        min_cards: u8,
        max_cards: u8,
        can_cancel: bool,
        reason: String,
        candidates: Vec<StableChoiceCandidateKey>,
    },
    HandSelect {
        min_cards: u8,
        max_cards: u8,
        can_cancel: bool,
        reason: String,
        candidates: Vec<StableChoiceCandidateKey>,
    },
    Discovery(Vec<String>),
    Scry(Vec<StableChoiceCandidateKey>),
    CardRewardSelect {
        destination: String,
        can_skip: bool,
        cards: Vec<String>,
    },
    ForeignInfluence {
        upgraded: bool,
        cards: Vec<String>,
    },
    ChooseOne(Vec<String>),
    StanceChoice,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub(crate) enum StableChoiceCandidateKey {
    Card(String),
    Ref {
        prefix: &'static str,
        lineage: CardLineageId,
    },
}

impl StableRunPendingChoiceKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "min{}:max{}:reason{}:return{}",
            self.min_choices,
            self.max_choices,
            self.reason,
            self.return_state.diagnostic_string(),
        )
    }
}

impl StableRunPendingReturnKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        match self {
            StableRunPendingReturnKey::DoubleBossComplete => "double_boss_complete".to_owned(),
            StableRunPendingReturnKey::Act3VictoryComplete => "act3_victory_complete".to_owned(),
            StableRunPendingReturnKey::TrueVictoryComplete => "true_victory_complete".to_owned(),
            StableRunPendingReturnKey::Reward(value) => {
                format!("reward:{}", value.diagnostic_string())
            }
            StableRunPendingReturnKey::RewardOverlay {
                reward,
                return_state,
            } => format!(
                "reward_overlay:{}:return{}",
                reward.diagnostic_string(),
                return_state.diagnostic_string()
            ),
            StableRunPendingReturnKey::TreasureRoom(value) => {
                format!("treasure:{}", value.diagnostic_string())
            }
            StableRunPendingReturnKey::Campfire => "campfire".to_string(),
            StableRunPendingReturnKey::Shop(value) => {
                format!("shop:{}", value.diagnostic_string())
            }
            StableRunPendingReturnKey::MapNavigation => "map_navigation".to_string(),
            StableRunPendingReturnKey::MapOverlay(value) => {
                format!("map_overlay:return{}", value.diagnostic_string())
            }
            StableRunPendingReturnKey::EventRoom => "event_room".to_string(),
            StableRunPendingReturnKey::BossRelic(value) => {
                format!("boss_relic:{}", value.diagnostic_string())
            }
            StableRunPendingReturnKey::RunPendingChoice(value) => {
                format!("run_pending_choice:{}", value.diagnostic_string())
            }
            StableRunPendingReturnKey::GameOver(value) => format!("game_over:{value}"),
            StableRunPendingReturnKey::Combat => "combat".to_string(),
        }
    }
}

impl StableBossRelicKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        self.relics.join("|")
    }
}

impl StablePendingChoiceKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        match self {
            StablePendingChoiceKey::GridSelect {
                source_pile,
                min_cards,
                max_cards,
                can_cancel,
                reason,
                candidates,
            } => format!(
                "grid:{source_pile}:{min_cards}:{max_cards}:{can_cancel}:{reason}:{}",
                join_diagnostic_strings(candidates)
            ),
            StablePendingChoiceKey::HandSelect {
                min_cards,
                max_cards,
                can_cancel,
                reason,
                candidates,
            } => format!(
                "hand:{min_cards}:{max_cards}:{can_cancel}:{reason}:{}",
                join_diagnostic_strings(candidates)
            ),
            StablePendingChoiceKey::Discovery(cards) => format!("discovery:{}", cards.join("|")),
            StablePendingChoiceKey::Scry(cards) => {
                format!("scry:{}", join_diagnostic_strings(cards))
            }
            StablePendingChoiceKey::CardRewardSelect {
                destination,
                can_skip,
                cards,
            } => format!("card_reward:{destination}:{can_skip}:{}", cards.join("|")),
            StablePendingChoiceKey::ForeignInfluence { upgraded, cards } => {
                format!("foreign_influence:{upgraded}:{}", cards.join("|"))
            }
            StablePendingChoiceKey::ChooseOne(cards) => format!("choose_one:{}", cards.join("|")),
            StablePendingChoiceKey::StanceChoice => "stance_choice".to_string(),
        }
    }
}

impl StableTreasureChestKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "size{}:tier{}:gold{}",
            self.size,
            self.base_relic_tier,
            self.gold_reward
                .map(|amount| amount.to_string())
                .unwrap_or_else(|| "_".to_string())
        )
    }
}

impl StableChoiceCandidateKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        match self {
            StableChoiceCandidateKey::Card(value) => value.clone(),
            StableChoiceCandidateKey::Ref { prefix, lineage } => format!("{prefix}:{lineage}"),
        }
    }
}
