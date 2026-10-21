use crate::content::cards::CardId;
use crate::runtime::combat::CardLineageId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatEngineKey {
    CombatPlayerTurn,
    CombatProcessing,
    PendingChoice(CombatPendingChoiceKey),
    RewardScreen(String),
    RewardOverlay(String),
    TreasureRoom(String),
    Campfire,
    Shop(String),
    MapNavigation,
    MapOverlay(String),
    EventRoom,
    CombatStart(String),
    RunPendingChoice(String),
    BossRelicSelect(String),
    GameOver(String),
    DoubleBossComplete,
    Act3VictoryComplete,
    TrueVictoryComplete,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatPendingChoiceKey {
    GridSelect {
        source_pile: CombatPileTypeKey,
        candidate_lineages: Vec<CardLineageId>,
        min_cards: u8,
        max_cards: u8,
        can_cancel: bool,
        reason: CombatGridSelectReasonKey,
    },
    HandSelect {
        candidate_lineages: Vec<CardLineageId>,
        min_cards: u8,
        max_cards: u8,
        can_cancel: bool,
        reason: CombatHandSelectReasonKey,
    },
    DiscoverySelect {
        cards: Vec<CardId>,
        colorless: bool,
        card_type: Option<CombatCardTypeKey>,
        amount: u8,
        can_skip: bool,
    },
    ScrySelect {
        cards: Vec<CardId>,
        card_lineages: Vec<CardLineageId>,
    },
    CardRewardSelect {
        cards: Vec<CardId>,
        destination: CombatCardDestinationKey,
        can_skip: bool,
    },
    ForeignInfluenceSelect {
        cards: Vec<CardId>,
        upgraded: bool,
    },
    ChooseOneSelect {
        choices: Vec<CombatChooseOneCardKey>,
    },
    StanceChoice,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatChooseOneCardKey {
    pub(crate) card_id: CardId,
    pub(crate) upgrades: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatPileTypeKey {
    Draw,
    Discard,
    Exhaust,
    Hand,
    Limbo,
    MasterDeck,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatHandSelectReasonKey {
    Exhaust,
    PotionExhaust { has_pending_use_effect: bool },
    Discard,
    Retain,
    PutOnDrawPile,
    PutToBottomOfDraw,
    Setup,
    Copy { amount: u8 },
    Nightmare { amount: u8 },
    Upgrade,
    GamblingChip,
    Recycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatGridSelectReasonKey {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatCardTypeKey {
    Attack,
    Skill,
    Power,
    Status,
    Curse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatCardDestinationKey {
    Hand,
    DrawPileRandom,
}
