use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::runtime::combat::CardLineageId;
use crate::state::events::EventId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum SelectionScope {
    Hand,
    Deck,
    Grid,
}

impl SelectionScope {
    /// Stable non-entity token for protocol and model vocabularies.
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::Hand => "hand",
            Self::Deck => "deck",
            Self::Grid => "grid",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum SelectionReason {
    Upgrade,
    Purge,
    Transform,
    TransformUpgraded,
    Duplicate,
    BottleFlame,
    BottleLightning,
    BottleTornado,
    Exhaust,
    Discard,
    Retain,
    PutOnDrawPile,
    PutToBottomOfDraw,
    Setup,
    Copy,
    Nightmare,
    GamblingChip,
    MoveToDrawPile,
    Exhume,
    DrawPileToHand,
    SkillFromDeckToHand,
    AttackFromDeckToHand,
    DiscardToHand,
    DiscardToHandNoCostChange,
    DiscardToHandRetain,
    Omniscience,
    Recycle,
    Discovery,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum SelectionConstraint {
    Exactly(usize),
    Between { min: usize, max: usize },
    UpToAvailable,
    OptionalUpToAvailable,
}

impl SelectionConstraint {
    pub fn from_bounds(min: usize, max: usize, available: usize) -> Self {
        if available == 0 {
            return if min == 0 {
                SelectionConstraint::OptionalUpToAvailable
            } else {
                SelectionConstraint::Exactly(min)
            };
        }

        if min == 0 && max >= available {
            SelectionConstraint::OptionalUpToAvailable
        } else if min == 1 && max >= available {
            SelectionConstraint::UpToAvailable
        } else if min == max {
            SelectionConstraint::Exactly(min)
        } else {
            SelectionConstraint::Between { min, max }
        }
    }

    pub fn describe(self, available: usize) -> String {
        match self {
            SelectionConstraint::Exactly(n) => format!("choose exactly {n}"),
            SelectionConstraint::Between { min, max } => format!("choose {min}-{max}"),
            SelectionConstraint::UpToAvailable => {
                format!("choose up to {}", available.max(1))
            }
            SelectionConstraint::OptionalUpToAvailable => {
                format!("choose 0-{}", available)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum SelectionTargetRef {
    CardLineage(CardLineageId),
}

impl SelectionTargetRef {
    pub fn card_lineage(self) -> CardLineageId {
        match self {
            SelectionTargetRef::CardLineage(lineage) => lineage,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SelectionRequest {
    pub scope: SelectionScope,
    pub reason: SelectionReason,
    pub constraint: SelectionConstraint,
    pub can_cancel: bool,
    pub targets: Vec<SelectionTargetRef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SelectionResolution {
    pub scope: SelectionScope,
    pub selected: Vec<SelectionTargetRef>,
}

impl SelectionResolution {
    pub fn card_lineages(
        scope: SelectionScope,
        lineages: impl IntoIterator<Item = CardLineageId>,
    ) -> Self {
        Self {
            scope,
            selected: lineages
                .into_iter()
                .map(SelectionTargetRef::CardLineage)
                .collect(),
        }
    }

    pub fn selected_card_lineages(&self) -> Vec<CardLineageId> {
        self.selected
            .iter()
            .copied()
            .map(SelectionTargetRef::card_lineage)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum DomainEventSource {
    Event(EventId),
    Relic(RelicId),
    Potion(PotionId),
    CampfireSmith,
    CampfireToke,
    Selection(SelectionReason),
    DeckMutation,
    RewardScreen,
    Shop,
    BossRelicChoice,
    CombatDraw,
    CombatMonsterGrant {
        monster_id: crate::content::monsters::EnemyId,
        monster_slot: u32,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DomainCardSnapshot {
    pub id: CardId,
    pub upgrades: u8,
    pub lineage: CardLineageId,
}

impl DomainCardSnapshot {
    pub fn upgraded(mut self) -> Self {
        self.upgrades += 1;
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum DomainEvent {
    RelicObtained {
        relic_id: RelicId,
        source: DomainEventSource,
    },
    RelicLost {
        relic_id: RelicId,
        source: DomainEventSource,
    },
    GoldChanged {
        delta: i32,
        new_total: i32,
        source: DomainEventSource,
    },
    HpChanged {
        delta: i32,
        current_hp: i32,
        max_hp: i32,
        source: DomainEventSource,
    },
    MaxHpChanged {
        delta: i32,
        current_hp: i32,
        max_hp: i32,
        source: DomainEventSource,
    },
    PotionObtained {
        potion_id: PotionId,
        slot: usize,
        source: DomainEventSource,
    },
    PotionLost {
        potion_id: PotionId,
        slot: usize,
        source: DomainEventSource,
    },
    SelectionResolved {
        scope: SelectionScope,
        reason: SelectionReason,
        selected: Vec<SelectionTargetRef>,
        source: DomainEventSource,
    },
    CardObtained {
        card: DomainCardSnapshot,
        source: DomainEventSource,
    },
    CardRemoved {
        card: DomainCardSnapshot,
        source: DomainEventSource,
    },
    CardUpgraded {
        before: DomainCardSnapshot,
        after: DomainCardSnapshot,
        source: DomainEventSource,
    },
    CardTransformed {
        before: DomainCardSnapshot,
        after: DomainCardSnapshot,
        source: DomainEventSource,
    },
    CardDrawn {
        card: DomainCardSnapshot,
        source: DomainEventSource,
    },
    CardsExhausted {
        cards: Vec<DomainCardSnapshot>,
        source: DomainEventSource,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum EngineDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum EngineDiagnosticClass {
    Normalization,
    Suspicious,
    Broken,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct EngineDiagnostic {
    pub severity: EngineDiagnosticSeverity,
    pub class: EngineDiagnosticClass,
    pub message: String,
}
