use crate::content::cards::{CardId, CardType};
use crate::runtime::combat::CardLineageId;
use crate::state::selection::{
    SelectionConstraint, SelectionReason, SelectionRequest, SelectionScope, SelectionTargetRef,
};
use serde::{Deserialize, Serialize};

/// Combat-local pending selections, such as choosing cards from hand, draw/discard,
/// or generated combat choices.
///
/// This shares the UI-facing `SelectionRequest` / `SelectionResolution` protocol
/// with run-level choices, but its lifecycle and effects are combat-local.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum PendingChoice {
    GridSelect {
        source_pile: PileType,
        candidate_lineages: Vec<CardLineageId>,
        min_cards: u8,
        max_cards: u8,
        can_cancel: bool,
        reason: GridSelectReason,
    },
    HandSelect {
        candidate_lineages: Vec<CardLineageId>,
        min_cards: u8,
        max_cards: u8,
        can_cancel: bool,
        reason: HandSelectReason,
    },
    DiscoverySelect(DiscoveryChoiceState),
    ScrySelect {
        cards: Vec<CardId>,
        card_lineages: Vec<CardLineageId>,
    },
    /// Unified card reward selection (NilrysCodex, Toolbox, etc.)
    /// Player picks 1 card from `cards`. Card goes to `destination`.
    /// If `can_skip`, player can Cancel to skip without picking.
    CardRewardSelect {
        cards: Vec<CardId>,
        destination: crate::runtime::action::CardDestination,
        can_skip: bool,
    },
    /// Java Watcher `ForeignInfluenceAction`: pick one generated any-color
    /// attack. This is not `DiscoveryAction`; it has different RNG and resume
    /// semantics.
    ForeignInfluenceSelect {
        cards: Vec<CardId>,
        upgraded: bool,
    },
    ChooseOneSelect {
        choices: Vec<ChooseOneCardChoice>,
    },
    /// StancePotion: player chooses Wrath or Calm. Index 0 = Wrath, 1 = Calm.
    StanceChoice,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum PileType {
    Draw,
    Discard,
    Exhaust,
    Hand,
    Limbo,
    MasterDeck,
}

impl PileType {
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::Draw => "draw",
            Self::Discard => "discard",
            Self::Exhaust => "exhaust",
            Self::Hand => "hand",
            Self::Limbo => "limbo",
            Self::MasterDeck => "master_deck",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum HandSelectReason {
    Exhaust,
    /// Exhaust selection opened by a potion whose slot has already been
    /// consumed.  `has_pending_use_effect` records whether finishing the
    /// selection can still realize a non-selection effect such as Toy
    /// Ornithopter healing, so automatic planners can distinguish a truly
    /// empty potion use from an intentional on-use trigger without changing
    /// the simulator's legal zero-card/cancel surface.
    PotionExhaust {
        has_pending_use_effect: bool,
    },
    Discard,
    Retain,
    PutOnDrawPile,
    PutToBottomOfDraw,
    Setup,
    Copy {
        amount: u8,
    },
    Nightmare {
        amount: u8,
    },
    Upgrade,
    GamblingChip,
    Recycle,
}

impl HandSelectReason {
    pub fn vocabulary_token_v1(self) -> String {
        match self {
            Self::Exhaust => "exhaust".to_string(),
            Self::PotionExhaust {
                has_pending_use_effect,
            } => format!("potion_exhaust/pending_use_effect:{has_pending_use_effect}"),
            Self::Discard => "discard".to_string(),
            Self::Retain => "retain".to_string(),
            Self::PutOnDrawPile => "put_on_draw_pile".to_string(),
            Self::PutToBottomOfDraw => "put_to_bottom_of_draw".to_string(),
            Self::Setup => "setup".to_string(),
            Self::Copy { amount } => format!("copy/{amount}"),
            Self::Nightmare { amount } => format!("nightmare/{amount}"),
            Self::Upgrade => "upgrade".to_string(),
            Self::GamblingChip => "gambling_chip".to_string(),
            Self::Recycle => "recycle".to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DiscoveryChoiceState {
    pub cards: Vec<CardId>,
    pub colorless: bool,
    pub card_type: Option<CardType>,
    pub amount: u8,
    pub can_skip: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChooseOneCardChoice {
    pub card_id: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum HandSelectFilter {
    Any,
    Upgradeable,
    AttackOrPower,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum GridSelectReason {
    MoveToDrawPile,
    Exhume { upgrade: bool },
    DrawPileToHand,
    SkillFromDeckToHand,
    AttackFromDeckToHand,
    DiscardToHand,
    DiscardToHandNoCostChange,
    DiscardToHandRetain,
    Omniscience { play_amount: u8 },
}

impl GridSelectReason {
    pub fn vocabulary_token_v1(self) -> String {
        match self {
            Self::MoveToDrawPile => "move_to_draw_pile".to_string(),
            Self::Exhume { upgrade } => format!("exhume/upgrade:{upgrade}"),
            Self::DrawPileToHand => "draw_pile_to_hand".to_string(),
            Self::SkillFromDeckToHand => "skill_from_deck_to_hand".to_string(),
            Self::AttackFromDeckToHand => "attack_from_deck_to_hand".to_string(),
            Self::DiscardToHand => "discard_to_hand".to_string(),
            Self::DiscardToHandNoCostChange => "discard_to_hand_no_cost_change".to_string(),
            Self::DiscardToHandRetain => "discard_to_hand_retain".to_string(),
            Self::Omniscience { play_amount } => format!("omniscience/{play_amount}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum GridSelectFilter {
    Any,
    NonExhume,
    Skill,
    Attack,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum TargetValidation {
    AnyEnemy,
    AnyMonster,
}

impl From<HandSelectReason> for SelectionReason {
    fn from(value: HandSelectReason) -> Self {
        match value {
            HandSelectReason::Exhaust | HandSelectReason::PotionExhaust { .. } => {
                SelectionReason::Exhaust
            }
            HandSelectReason::Discard => SelectionReason::Discard,
            HandSelectReason::Retain => SelectionReason::Retain,
            HandSelectReason::PutOnDrawPile => SelectionReason::PutOnDrawPile,
            HandSelectReason::PutToBottomOfDraw => SelectionReason::PutToBottomOfDraw,
            HandSelectReason::Setup => SelectionReason::Setup,
            HandSelectReason::Copy { .. } => SelectionReason::Copy,
            HandSelectReason::Nightmare { .. } => SelectionReason::Nightmare,
            HandSelectReason::Upgrade => SelectionReason::Upgrade,
            HandSelectReason::GamblingChip => SelectionReason::GamblingChip,
            HandSelectReason::Recycle => SelectionReason::Recycle,
        }
    }
}

impl From<GridSelectReason> for SelectionReason {
    fn from(value: GridSelectReason) -> Self {
        match value {
            GridSelectReason::MoveToDrawPile => SelectionReason::MoveToDrawPile,
            GridSelectReason::Exhume { .. } => SelectionReason::Exhume,
            GridSelectReason::DrawPileToHand => SelectionReason::DrawPileToHand,
            GridSelectReason::SkillFromDeckToHand => SelectionReason::SkillFromDeckToHand,
            GridSelectReason::AttackFromDeckToHand => SelectionReason::AttackFromDeckToHand,
            GridSelectReason::DiscardToHand => SelectionReason::DiscardToHand,
            GridSelectReason::DiscardToHandNoCostChange => {
                SelectionReason::DiscardToHandNoCostChange
            }
            GridSelectReason::DiscardToHandRetain => SelectionReason::DiscardToHandRetain,
            GridSelectReason::Omniscience { .. } => SelectionReason::Omniscience,
        }
    }
}

impl PendingChoice {
    pub const fn kind_v1(&self) -> &'static str {
        match self {
            Self::GridSelect { .. } => "grid_select",
            Self::HandSelect { .. } => "hand_select",
            Self::DiscoverySelect(_) => "discovery_select",
            Self::ScrySelect { .. } => "scry_select",
            Self::CardRewardSelect { .. } => "card_reward_select",
            Self::ForeignInfluenceSelect { .. } => "foreign_influence_select",
            Self::ChooseOneSelect { .. } => "choose_one_select",
            Self::StanceChoice => "stance_choice",
        }
    }

    pub fn selection_request(&self) -> Option<SelectionRequest> {
        match self {
            PendingChoice::HandSelect {
                candidate_lineages,
                min_cards,
                max_cards,
                can_cancel,
                reason,
            } => Some(SelectionRequest {
                scope: SelectionScope::Hand,
                reason: (*reason).into(),
                constraint: SelectionConstraint::from_bounds(
                    *min_cards as usize,
                    *max_cards as usize,
                    candidate_lineages.len(),
                ),
                can_cancel: *can_cancel,
                targets: candidate_lineages
                    .iter()
                    .copied()
                    .map(SelectionTargetRef::CardLineage)
                    .collect(),
            }),
            PendingChoice::GridSelect {
                candidate_lineages,
                min_cards,
                max_cards,
                can_cancel,
                reason,
                ..
            } => Some(SelectionRequest {
                scope: SelectionScope::Grid,
                reason: (*reason).into(),
                constraint: SelectionConstraint::from_bounds(
                    *min_cards as usize,
                    *max_cards as usize,
                    candidate_lineages.len(),
                ),
                can_cancel: *can_cancel,
                targets: candidate_lineages
                    .iter()
                    .copied()
                    .map(SelectionTargetRef::CardLineage)
                    .collect(),
            }),
            _ => None,
        }
    }
}
