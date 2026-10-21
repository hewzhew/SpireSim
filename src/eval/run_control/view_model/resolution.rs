use serde::{Deserialize, Serialize};

use crate::content::cards::{get_card_definition, CardId};
use crate::content::identity::gameplay_display_label_v1;
use crate::content::potions::{get_potion_definition, PotionId};
use crate::content::relics::RelicId;
use crate::state::events::{
    EventCardKind, EventEffect, EventOption, EventOptionTransition, EventRandomOutcomeKind,
    EventRelicKind, EventSelectionKind,
};
use crate::state::rewards::{RewardCard, RewardItem, RewardScreenContext, RewardState};
use crate::state::run::RunState;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CandidateResolution {
    pub status: ResolutionStatus,
    pub known_effects: Vec<KnownEffect>,
    pub unresolved_effects: Vec<UnresolvedEffect>,
    pub followup: Option<FollowupBoundary>,
    pub evidence: ResolutionEvidence,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum ResolutionStatus {
    Known,
    Partial,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum ResolutionEvidence {
    PublicScreenSemantics,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum KnownEffect {
    GainGold(i32),
    LoseGold(i32),
    LoseHp(i32),
    LoseMaxHp(i32),
    Heal(i32),
    GainMaxHp(i32),
    ObtainSpecificRelic { count: usize, relic: RelicId },
    ObtainSpecificPotion { count: usize, potion: PotionId },
    ObtainSpecificCard { count: usize, card: CardId },
    ObtainSpecificColorlessCard { count: usize, card: CardId },
    ObtainSpecificCurse { count: usize, card: CardId },
    ObtainKey(RunKey),
    RemoveCard { count: usize },
    RemoveSpecificCard { count: usize, card: CardId },
    UpgradeCard { count: usize },
    UpgradeAllCards,
    TransformCard { count: usize },
    DuplicateCard { count: usize },
    LoseSpecificRelic { relic: RelicId, starter_only: bool },
    LoseRelic { starter_only: bool },
    NoVisibleChange { reason: String },
    StartCombat,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum UnresolvedEffect {
    RandomGoldRange {
        min: i32,
        max: i32,
        visibility: HiddenInfoVisibility,
    },
    RandomRelic {
        count: usize,
        pool: RelicPoolBoundary,
        visibility: HiddenInfoVisibility,
    },
    RandomPotion {
        count: usize,
        visibility: HiddenInfoVisibility,
    },
    RandomCard {
        count: usize,
        pool: CardPoolBoundary,
        visibility: HiddenInfoVisibility,
    },
    CardRewardChoices {
        count: usize,
        pool: CardPoolBoundary,
        visibility: HiddenInfoVisibility,
    },
    VisibleCardRewardChoices {
        cards: Vec<VisibleCardChoice>,
    },
    RandomCurse {
        count: usize,
        pool: CardPoolBoundary,
        visibility: HiddenInfoVisibility,
    },
    PlayerSelection {
        kind: SelectionBoundary,
    },
    TransformOutput {
        count: usize,
        visibility: HiddenInfoVisibility,
    },
    RandomEventOutcome {
        kind: RandomEventOutcomeBoundary,
        chance_percent: Option<u8>,
        visibility: HiddenInfoVisibility,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum HiddenInfoVisibility {
    DistributionKnownResultHiddenUntilResolved,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct VisibleCardChoice {
    pub card: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum RelicPoolBoundary {
    AnyRelic,
    Common,
    Uncommon,
    Rare,
    Shop,
    Boss,
    Book,
    Face,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum CardPoolBoundary {
    ClassCard,
    ClassCommonOrUncommon,
    ClassRare,
    Colorless,
    ColorlessUncommon,
    ColorlessRare,
    Unknown,
    Curse,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum FollowupBoundary {
    EventScreenAdvance,
    EventComplete,
    RewardScreen,
    RewardCardChoice,
    Selection(SelectionBoundary),
    CombatStart,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum RunKey {
    Ruby,
    Sapphire,
    Emerald,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum SelectionBoundary {
    RemoveCard,
    UpgradeCard,
    TransformCard,
    DuplicateCard,
    OfferCard,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum RandomEventOutcomeBoundary {
    GremlinWheel,
    JoustOwnerWin,
    DeadAdventurerSearch,
    MatchAndKeepFlip,
    ScrapOozeRelicRoll,
}

impl CandidateResolution {
    pub fn known(known_effects: Vec<KnownEffect>) -> Option<Self> {
        if known_effects.is_empty() {
            return None;
        }
        Some(Self {
            status: ResolutionStatus::Known,
            known_effects,
            unresolved_effects: Vec::new(),
            followup: None,
            evidence: ResolutionEvidence::PublicScreenSemantics,
        })
    }

    pub fn from_event_option(option: &EventOption) -> Option<Self> {
        if option.ui.disabled {
            return None;
        }

        let mut known_effects = Vec::new();
        let mut unresolved_effects = Vec::new();
        for effect in &option.semantics.effects {
            push_effect_resolution(effect, &mut known_effects, &mut unresolved_effects);
        }

        let followup = followup_boundary(option.semantics.transition);
        if known_effects.is_empty() && unresolved_effects.is_empty() && followup.is_none() {
            return None;
        }

        let status = if !unresolved_effects.is_empty()
            || matches!(
                followup,
                Some(FollowupBoundary::RewardScreen | FollowupBoundary::Selection(_))
            ) {
            ResolutionStatus::Partial
        } else {
            ResolutionStatus::Known
        };

        Some(Self {
            status,
            known_effects,
            unresolved_effects,
            followup,
            evidence: ResolutionEvidence::PublicScreenSemantics,
        })
    }

    pub fn from_reward_card(card: &RewardCard) -> Self {
        Self {
            status: ResolutionStatus::Known,
            known_effects: vec![KnownEffect::ObtainSpecificCard {
                count: 1,
                card: card.id,
            }],
            unresolved_effects: Vec::new(),
            followup: None,
            evidence: ResolutionEvidence::PublicScreenSemantics,
        }
    }

    pub fn from_reward_item(
        item: &RewardItem,
        reward_state: &RewardState,
        run_state: &RunState,
    ) -> Option<Self> {
        match item {
            RewardItem::Gold { amount } => {
                let amount = visible_reward_gold_amount(*amount, reward_state, run_state);
                Self::known(vec![KnownEffect::GainGold(amount)])
            }
            RewardItem::StolenGold { amount } => Self::known(vec![KnownEffect::GainGold(*amount)]),
            RewardItem::Relic { relic_id } => Self::known(vec![KnownEffect::ObtainSpecificRelic {
                count: 1,
                relic: *relic_id,
            }]),
            RewardItem::Potion { potion_id } => potion_reward_resolution(*potion_id, run_state),
            RewardItem::Card { cards } => Some(Self {
                status: ResolutionStatus::Partial,
                known_effects: Vec::new(),
                unresolved_effects: vec![UnresolvedEffect::VisibleCardRewardChoices {
                    cards: cards
                        .iter()
                        .map(|card| VisibleCardChoice {
                            card: card.id,
                            upgrades: card.upgrades,
                        })
                        .collect(),
                }],
                followup: Some(FollowupBoundary::RewardCardChoice),
                evidence: ResolutionEvidence::PublicScreenSemantics,
            }),
            RewardItem::EmeraldKey => Self::known(vec![KnownEffect::ObtainKey(RunKey::Emerald)]),
            RewardItem::SapphireKey => Self::known(vec![KnownEffect::ObtainKey(RunKey::Sapphire)]),
        }
    }

    pub fn from_boss_relic(relic: RelicId) -> Self {
        Self {
            status: ResolutionStatus::Known,
            known_effects: vec![KnownEffect::ObtainSpecificRelic { count: 1, relic }],
            unresolved_effects: Vec::new(),
            followup: None,
            evidence: ResolutionEvidence::PublicScreenSemantics,
        }
    }

    pub fn main_note(&self) -> Option<String> {
        let mut parts = self
            .known_effects
            .iter()
            .map(KnownEffect::brief)
            .chain(self.unresolved_effects.iter().map(UnresolvedEffect::brief))
            .collect::<Vec<_>>();
        if let Some(followup) = self.followup.and_then(FollowupBoundary::main_note) {
            if !parts.iter().any(|part| part == &followup) {
                parts.push(followup);
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }

    /// Compact public mechanics without the successor boundary. Consumers
    /// that render the follow-up as a typed field use this to avoid repeating
    /// the same transition in prose.
    pub fn effect_note(&self) -> Option<String> {
        let parts = self
            .known_effects
            .iter()
            .map(KnownEffect::brief)
            .chain(self.unresolved_effects.iter().map(UnresolvedEffect::brief))
            .collect::<Vec<_>>();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }

    pub fn detail_lines(&self) -> Vec<String> {
        let mut lines = vec![format!("resolution: {:?}", self.status)];
        if !self.known_effects.is_empty() {
            lines.push("known_effects:".to_string());
            lines.extend(
                self.known_effects
                    .iter()
                    .map(|effect| format!("  - {}", effect.brief())),
            );
        }
        if !self.unresolved_effects.is_empty() {
            lines.push("unresolved_effects:".to_string());
            lines.extend(
                self.unresolved_effects
                    .iter()
                    .map(|effect| format!("  - {}", effect.detail())),
            );
        }
        if let Some(followup) = self.followup {
            lines.push(format!("followup: {}", followup.detail()));
        }
        lines.push(
            "evidence: public screen semantics; hidden random results are not sampled".to_string(),
        );
        lines
    }
}

impl KnownEffect {
    fn brief(&self) -> String {
        match self {
            KnownEffect::GainGold(amount) => format!("gain {amount} gold"),
            KnownEffect::LoseGold(amount) => format!("lose {amount} gold"),
            KnownEffect::LoseHp(amount) => format!("lose {amount} hp"),
            KnownEffect::LoseMaxHp(amount) => format!("lose {amount} max hp"),
            KnownEffect::Heal(amount) => format!("heal {amount} hp"),
            KnownEffect::GainMaxHp(amount) => format!("gain {amount} max hp"),
            KnownEffect::ObtainSpecificRelic { count, relic } => {
                format!("obtain {count} specific relic {}", relic_label(*relic))
            }
            KnownEffect::ObtainSpecificPotion { count, potion } => {
                format!("obtain {count} specific potion {}", potion_label(*potion))
            }
            KnownEffect::ObtainSpecificCard { count, card } => {
                format!("obtain {count} specific card {}", card_label(*card, 0))
            }
            KnownEffect::ObtainSpecificColorlessCard { count, card } => {
                format!(
                    "obtain {count} specific colorless card {}",
                    card_label(*card, 0)
                )
            }
            KnownEffect::ObtainSpecificCurse { count, card } => {
                format!("obtain {count} specific curse {}", card_label(*card, 0))
            }
            KnownEffect::ObtainKey(key) => format!("obtain {} key", key.brief()),
            KnownEffect::RemoveCard { count } => format!("remove {count} card"),
            KnownEffect::RemoveSpecificCard { count, card } => {
                if *count == 1 {
                    format!("remove specific card {}", card_label(*card, 0))
                } else {
                    format!("remove {count} specific card {}", card_label(*card, 0))
                }
            }
            KnownEffect::UpgradeCard { count } => format!("upgrade {count} card"),
            KnownEffect::UpgradeAllCards => "upgrade all upgradeable cards".to_string(),
            KnownEffect::TransformCard { count } => format!("transform {count} card"),
            KnownEffect::DuplicateCard { count } => format!("duplicate {count} card"),
            KnownEffect::LoseSpecificRelic {
                relic,
                starter_only,
            } => {
                if *starter_only {
                    format!("lose starter relic {}", relic_label(*relic))
                } else {
                    format!("lose relic {}", relic_label(*relic))
                }
            }
            KnownEffect::LoseRelic { starter_only } => {
                if *starter_only {
                    "lose starter relic".to_string()
                } else {
                    "lose relic".to_string()
                }
            }
            KnownEffect::NoVisibleChange { reason } => format!("no visible change: {reason}"),
            KnownEffect::StartCombat => "starts combat".to_string(),
        }
    }
}

impl UnresolvedEffect {
    fn brief(&self) -> String {
        match self {
            UnresolvedEffect::RandomGoldRange { min, max, .. } => {
                format!("random gold {min}-{max} outcome")
            }
            UnresolvedEffect::RandomRelic { pool, .. } => {
                format!("{} outcome", pool.brief())
            }
            UnresolvedEffect::RandomPotion { .. } => "random potion outcome".to_string(),
            UnresolvedEffect::RandomCard { pool, .. } => {
                format!("{} outcome", pool.brief())
            }
            UnresolvedEffect::CardRewardChoices { count, pool, .. } => {
                format!("{count} {} choices", pool.brief())
            }
            UnresolvedEffect::VisibleCardRewardChoices { cards } => {
                format!("{} visible card choices", cards.len())
            }
            UnresolvedEffect::RandomCurse { pool, .. } => {
                format!("{} outcome", pool.brief())
            }
            UnresolvedEffect::PlayerSelection { kind } => {
                format!("{} target selected later", kind.brief())
            }
            UnresolvedEffect::TransformOutput { .. } => {
                "transform result hidden until resolved".to_string()
            }
            UnresolvedEffect::RandomEventOutcome { kind, .. } => {
                format!("random {} outcome", kind.brief())
            }
        }
    }

    fn detail(&self) -> String {
        match self {
            UnresolvedEffect::RandomGoldRange { min, max, .. } => {
                format!("gain {min}-{max} gold; distribution known, result hidden")
            }
            UnresolvedEffect::RandomRelic { count, pool, .. } => {
                format!(
                    "{count} {}; distribution known, result hidden",
                    pool.brief()
                )
            }
            UnresolvedEffect::RandomPotion { count, .. } => {
                format!("{count} random potion; distribution known, result hidden")
            }
            UnresolvedEffect::RandomCard { count, pool, .. } => {
                format!(
                    "{count} {}; distribution known, result hidden",
                    pool.brief()
                )
            }
            UnresolvedEffect::CardRewardChoices { count, pool, .. } => {
                format!(
                    "{count} {} reward choices; distribution known, exact candidates hidden until reward screen",
                    pool.brief()
                )
            }
            UnresolvedEffect::VisibleCardRewardChoices { cards } => {
                let labels = cards
                    .iter()
                    .map(|choice| card_label(choice.card, choice.upgrades))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{} visible card reward choices: {labels}", cards.len())
            }
            UnresolvedEffect::RandomCurse { count, pool, .. } => {
                format!(
                    "{count} {}; distribution known, result hidden",
                    pool.brief()
                )
            }
            UnresolvedEffect::PlayerSelection { kind } => {
                format!("player selection required: {}", kind.brief())
            }
            UnresolvedEffect::TransformOutput { count, .. } => {
                format!("{count} transform result; distribution known, result hidden")
            }
            UnresolvedEffect::RandomEventOutcome {
                kind,
                chance_percent,
                ..
            } => match chance_percent {
                Some(chance_percent) => format!(
                    "random {} result ({chance_percent}% success); outcome hidden until resolved",
                    kind.brief()
                ),
                None => format!(
                    "random {} result; outcome hidden until resolved",
                    kind.brief()
                ),
            },
        }
    }
}

impl RandomEventOutcomeBoundary {
    fn brief(self) -> &'static str {
        match self {
            RandomEventOutcomeBoundary::GremlinWheel => "Gremlin Wheel",
            RandomEventOutcomeBoundary::JoustOwnerWin => "Joust: Owner wins",
            RandomEventOutcomeBoundary::DeadAdventurerSearch => "Dead Adventurer search",
            RandomEventOutcomeBoundary::MatchAndKeepFlip => "Match and Keep flip",
            RandomEventOutcomeBoundary::ScrapOozeRelicRoll => "Scrap Ooze relic roll",
        }
    }
}

impl RelicPoolBoundary {
    fn brief(self) -> &'static str {
        match self {
            RelicPoolBoundary::AnyRelic => "random relic",
            RelicPoolBoundary::Common => "random common relic",
            RelicPoolBoundary::Uncommon => "random uncommon relic",
            RelicPoolBoundary::Rare => "random rare relic",
            RelicPoolBoundary::Shop => "random shop relic",
            RelicPoolBoundary::Boss => "random boss relic",
            RelicPoolBoundary::Book => "random book relic",
            RelicPoolBoundary::Face => "random face relic",
            RelicPoolBoundary::Unknown => "unknown relic",
        }
    }
}

impl CardPoolBoundary {
    fn brief(self) -> &'static str {
        match self {
            CardPoolBoundary::ClassCard => "random class card",
            CardPoolBoundary::Curse => "random ordinary curse",
            CardPoolBoundary::ClassCommonOrUncommon => "random common/uncommon class card",
            CardPoolBoundary::ClassRare => "random rare class card",
            CardPoolBoundary::Colorless => "random colorless card",
            CardPoolBoundary::ColorlessUncommon => "random uncommon colorless card",
            CardPoolBoundary::ColorlessRare => "random rare colorless card",
            CardPoolBoundary::Unknown => "unknown card",
        }
    }
}

impl FollowupBoundary {
    fn main_note(self) -> Option<String> {
        match self {
            FollowupBoundary::RewardScreen => Some("opens follow-up reward".to_string()),
            FollowupBoundary::RewardCardChoice => Some("opens card choice".to_string()),
            FollowupBoundary::Selection(kind) => Some(format!("opens {} selection", kind.brief())),
            FollowupBoundary::CombatStart => Some("starts combat".to_string()),
            FollowupBoundary::EventScreenAdvance | FollowupBoundary::EventComplete => None,
        }
    }

    fn detail(self) -> String {
        match self {
            FollowupBoundary::EventScreenAdvance => "event screen advances".to_string(),
            FollowupBoundary::EventComplete => "event completes".to_string(),
            FollowupBoundary::RewardScreen => {
                "reward screen; contents resolved by engine".to_string()
            }
            FollowupBoundary::RewardCardChoice => {
                "reward card choice; exact candidates are already visible".to_string()
            }
            FollowupBoundary::Selection(kind) => format!("selection: {}", kind.brief()),
            FollowupBoundary::CombatStart => "combat start boundary".to_string(),
        }
    }
}

impl RunKey {
    fn brief(self) -> &'static str {
        match self {
            RunKey::Ruby => "ruby",
            RunKey::Sapphire => "sapphire",
            RunKey::Emerald => "emerald",
        }
    }
}

impl SelectionBoundary {
    fn brief(self) -> &'static str {
        match self {
            SelectionBoundary::RemoveCard => "remove card",
            SelectionBoundary::UpgradeCard => "upgrade card",
            SelectionBoundary::TransformCard => "transform card",
            SelectionBoundary::DuplicateCard => "duplicate card",
            SelectionBoundary::OfferCard => "offer card",
            SelectionBoundary::Unknown => "unknown",
        }
    }
}

fn push_effect_resolution(
    effect: &EventEffect,
    known_effects: &mut Vec<KnownEffect>,
    unresolved_effects: &mut Vec<UnresolvedEffect>,
) {
    match effect {
        EventEffect::GainGold(amount) => known_effects.push(KnownEffect::GainGold(*amount)),
        EventEffect::GainGoldRange { min, max } => {
            unresolved_effects.push(UnresolvedEffect::RandomGoldRange {
                min: *min,
                max: *max,
                visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
            });
        }
        EventEffect::LoseGold(amount) => known_effects.push(KnownEffect::LoseGold(*amount)),
        EventEffect::LoseHp(amount) => known_effects.push(KnownEffect::LoseHp(*amount)),
        EventEffect::LoseMaxHp(amount) => known_effects.push(KnownEffect::LoseMaxHp(*amount)),
        EventEffect::Heal(amount) => known_effects.push(KnownEffect::Heal(*amount)),
        EventEffect::GainMaxHp(amount) => known_effects.push(KnownEffect::GainMaxHp(*amount)),
        EventEffect::ObtainRelic { count, kind } => match kind {
            EventRelicKind::Specific(relic) => {
                known_effects.push(KnownEffect::ObtainSpecificRelic {
                    count: *count,
                    relic: *relic,
                });
            }
            EventRelicKind::RandomRelic => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::AnyRelic)
            }
            EventRelicKind::RandomCommonRelic => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::Common)
            }
            EventRelicKind::RandomUncommonRelic => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::Uncommon)
            }
            EventRelicKind::RandomRareRelic => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::Rare)
            }
            EventRelicKind::RandomShopRelic => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::Shop)
            }
            EventRelicKind::RandomBossRelic => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::Boss)
            }
            EventRelicKind::RandomBook => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::Book)
            }
            EventRelicKind::RandomFace => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::Face)
            }
            EventRelicKind::Unknown => {
                push_random_relic(unresolved_effects, *count, RelicPoolBoundary::Unknown)
            }
        },
        EventEffect::ObtainPotion { count } => {
            unresolved_effects.push(UnresolvedEffect::RandomPotion {
                count: *count,
                visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
            })
        }
        EventEffect::ObtainCard { count, kind } => push_card_effect(
            known_effects,
            unresolved_effects,
            *count,
            *kind,
            CardFactKind::Class,
        ),
        EventEffect::ObtainColorlessCard { count, kind } => push_card_effect(
            known_effects,
            unresolved_effects,
            *count,
            *kind,
            CardFactKind::Colorless,
        ),
        EventEffect::OfferCards { count, kind } => push_card_choices(
            unresolved_effects,
            *count,
            card_pool_boundary(*kind, CardFactKind::Class),
        ),
        EventEffect::ObtainCurse { count, kind } => match kind {
            EventCardKind::Specific(card) => {
                known_effects.push(KnownEffect::ObtainSpecificCurse {
                    count: *count,
                    card: *card,
                });
            }
            EventCardKind::RandomClassCard
            | EventCardKind::RandomClassCommonOrUncommon
            | EventCardKind::RandomClassRare => {
                unresolved_effects.push(UnresolvedEffect::RandomCurse {
                    count: *count,
                    pool: card_pool_boundary(*kind, CardFactKind::Class),
                    visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
                })
            }
            EventCardKind::RandomCurse => unresolved_effects.push(UnresolvedEffect::RandomCurse {
                count: *count,
                pool: CardPoolBoundary::Curse,
                visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
            }),
            EventCardKind::RandomColorless
            | EventCardKind::RandomColorlessUncommon
            | EventCardKind::RandomColorlessRare => {
                unresolved_effects.push(UnresolvedEffect::RandomCurse {
                    count: *count,
                    pool: card_pool_boundary(*kind, CardFactKind::Colorless),
                    visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
                })
            }
            EventCardKind::Unknown => unresolved_effects.push(UnresolvedEffect::RandomCurse {
                count: *count,
                pool: CardPoolBoundary::Unknown,
                visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
            }),
        },
        EventEffect::RemoveCard { count, kind, .. } => {
            if let EventCardKind::Specific(card) = kind {
                known_effects.push(KnownEffect::RemoveSpecificCard {
                    count: *count,
                    card: *card,
                });
            } else {
                known_effects.push(KnownEffect::RemoveCard { count: *count });
            }
        }
        EventEffect::UpgradeAllCards => known_effects.push(KnownEffect::UpgradeAllCards),
        EventEffect::UpgradeCard { count } => {
            known_effects.push(KnownEffect::UpgradeCard { count: *count });
        }
        EventEffect::TransformCard { count } => {
            known_effects.push(KnownEffect::TransformCard { count: *count });
            unresolved_effects.push(UnresolvedEffect::TransformOutput {
                count: *count,
                visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
            });
        }
        EventEffect::DuplicateCard { count } => {
            known_effects.push(KnownEffect::DuplicateCard { count: *count });
        }
        EventEffect::LoseRelic {
            specific,
            starter_only,
        } => {
            if let Some(relic) = specific {
                known_effects.push(KnownEffect::LoseSpecificRelic {
                    relic: *relic,
                    starter_only: *starter_only,
                });
            } else {
                known_effects.push(KnownEffect::LoseRelic {
                    starter_only: *starter_only,
                });
            }
        }
        EventEffect::LoseStarterRelic { specific } => {
            if let Some(relic) = specific {
                known_effects.push(KnownEffect::LoseSpecificRelic {
                    relic: *relic,
                    starter_only: true,
                });
            } else {
                known_effects.push(KnownEffect::LoseRelic { starter_only: true });
            }
        }
        EventEffect::RandomOutcome {
            kind,
            chance_percent,
        } => {
            unresolved_effects.push(UnresolvedEffect::RandomEventOutcome {
                kind: random_event_outcome_boundary(*kind),
                chance_percent: *chance_percent,
                visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
            });
        }
        EventEffect::StartCombat => known_effects.push(KnownEffect::StartCombat),
    }
}

fn random_event_outcome_boundary(kind: EventRandomOutcomeKind) -> RandomEventOutcomeBoundary {
    match kind {
        EventRandomOutcomeKind::GremlinWheel => RandomEventOutcomeBoundary::GremlinWheel,
        EventRandomOutcomeKind::JoustOwnerWin => RandomEventOutcomeBoundary::JoustOwnerWin,
        EventRandomOutcomeKind::DeadAdventurerSearch => {
            RandomEventOutcomeBoundary::DeadAdventurerSearch
        }
        EventRandomOutcomeKind::MatchAndKeepFlip => RandomEventOutcomeBoundary::MatchAndKeepFlip,
        EventRandomOutcomeKind::ScrapOozeRelicRoll => {
            RandomEventOutcomeBoundary::ScrapOozeRelicRoll
        }
    }
}

#[derive(Clone, Copy)]
enum CardFactKind {
    Class,
    Colorless,
}

fn push_card_effect(
    known_effects: &mut Vec<KnownEffect>,
    unresolved_effects: &mut Vec<UnresolvedEffect>,
    count: usize,
    kind: EventCardKind,
    fact_kind: CardFactKind,
) {
    match kind {
        EventCardKind::Specific(card) => match fact_kind {
            CardFactKind::Class => {
                known_effects.push(KnownEffect::ObtainSpecificCard { count, card })
            }
            CardFactKind::Colorless => {
                known_effects.push(KnownEffect::ObtainSpecificColorlessCard { count, card })
            }
        },
        EventCardKind::RandomClassCard
        | EventCardKind::RandomClassCommonOrUncommon
        | EventCardKind::RandomClassRare
        | EventCardKind::RandomColorless
        | EventCardKind::RandomColorlessUncommon
        | EventCardKind::RandomColorlessRare => {
            unresolved_effects.push(UnresolvedEffect::RandomCard {
                count,
                pool: card_pool_boundary(kind, fact_kind),
                visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
            })
        }
        EventCardKind::RandomCurse => unresolved_effects.push(UnresolvedEffect::RandomCard {
            count,
            pool: CardPoolBoundary::Curse,
            visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
        }),
        EventCardKind::Unknown => unresolved_effects.push(UnresolvedEffect::RandomCard {
            count,
            pool: match fact_kind {
                CardFactKind::Class => CardPoolBoundary::Unknown,
                CardFactKind::Colorless => CardPoolBoundary::Colorless,
            },
            visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
        }),
    }
}

fn push_card_choices(
    unresolved_effects: &mut Vec<UnresolvedEffect>,
    count: usize,
    pool: CardPoolBoundary,
) {
    unresolved_effects.push(UnresolvedEffect::CardRewardChoices {
        count,
        pool,
        visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
    });
}

fn card_pool_boundary(kind: EventCardKind, fact_kind: CardFactKind) -> CardPoolBoundary {
    match kind {
        EventCardKind::RandomClassCard => CardPoolBoundary::ClassCard,
        EventCardKind::RandomClassCommonOrUncommon => CardPoolBoundary::ClassCommonOrUncommon,
        EventCardKind::RandomClassRare => CardPoolBoundary::ClassRare,
        EventCardKind::RandomColorless => CardPoolBoundary::Colorless,
        EventCardKind::RandomColorlessUncommon => CardPoolBoundary::ColorlessUncommon,
        EventCardKind::RandomColorlessRare => CardPoolBoundary::ColorlessRare,
        EventCardKind::RandomCurse => CardPoolBoundary::Curse,
        EventCardKind::Specific(_) | EventCardKind::Unknown => match fact_kind {
            CardFactKind::Class => CardPoolBoundary::Unknown,
            CardFactKind::Colorless => CardPoolBoundary::Colorless,
        },
    }
}

fn push_random_relic(
    unresolved_effects: &mut Vec<UnresolvedEffect>,
    count: usize,
    pool: RelicPoolBoundary,
) {
    unresolved_effects.push(UnresolvedEffect::RandomRelic {
        count,
        pool,
        visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
    });
}

fn relic_label(id: RelicId) -> String {
    gameplay_display_label_v1(id).to_string()
}

fn potion_label(id: PotionId) -> &'static str {
    get_potion_definition(id).name
}

fn card_label(id: CardId, upgrades: u8) -> String {
    let name = get_card_definition(id).name;
    if upgrades == 0 {
        name.to_string()
    } else {
        format!("{name}+{upgrades}")
    }
}

fn visible_reward_gold_amount(
    amount: i32,
    reward_state: &RewardState,
    run_state: &RunState,
) -> i32 {
    let golden_idol_bonus = reward_state.screen_context != RewardScreenContext::TreasureRoom
        && run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::GoldenIdol);
    if golden_idol_bonus {
        amount + crate::content::relics::golden_idol::reward_gold_bonus(amount)
    } else {
        amount
    }
}

fn potion_reward_resolution(potion: PotionId, run_state: &RunState) -> Option<CandidateResolution> {
    if run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Sozu)
    {
        return CandidateResolution::known(vec![KnownEffect::NoVisibleChange {
            reason: "Sozu blocks potion rewards".to_string(),
        }]);
    }
    if run_state.find_empty_potion_slot().is_none() {
        return CandidateResolution::known(vec![KnownEffect::NoVisibleChange {
            reason: "no empty potion slot".to_string(),
        }]);
    }
    CandidateResolution::known(vec![KnownEffect::ObtainSpecificPotion { count: 1, potion }])
}

fn followup_boundary(transition: EventOptionTransition) -> Option<FollowupBoundary> {
    match transition {
        EventOptionTransition::None => None,
        EventOptionTransition::AdvanceScreen => Some(FollowupBoundary::EventScreenAdvance),
        EventOptionTransition::Complete => Some(FollowupBoundary::EventComplete),
        EventOptionTransition::OpenSelection(kind) => {
            Some(FollowupBoundary::Selection(selection_boundary(kind)))
        }
        EventOptionTransition::OpenReward => Some(FollowupBoundary::RewardScreen),
        EventOptionTransition::StartCombat => Some(FollowupBoundary::CombatStart),
    }
}

fn selection_boundary(kind: EventSelectionKind) -> SelectionBoundary {
    match kind {
        EventSelectionKind::None => SelectionBoundary::Unknown,
        EventSelectionKind::RemoveCard => SelectionBoundary::RemoveCard,
        EventSelectionKind::UpgradeCard => SelectionBoundary::UpgradeCard,
        EventSelectionKind::TransformCard => SelectionBoundary::TransformCard,
        EventSelectionKind::DuplicateCard => SelectionBoundary::DuplicateCard,
        EventSelectionKind::OfferCard => SelectionBoundary::OfferCard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::events::{EventActionKind, EventChoiceMeta, EventOptionSemantics};

    #[test]
    fn all_card_upgrade_effect_does_not_leak_usize_sentinel_to_ui() {
        let option = EventOption::new(
            EventChoiceMeta::new("[Remember] Upgrade all cards. Obtain Mark of the Bloom."),
            EventOptionSemantics {
                action: EventActionKind::Accept,
                effects: vec![EventEffect::UpgradeAllCards],
                constraints: Vec::new(),
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        );

        let resolution =
            CandidateResolution::from_event_option(&option).expect("effect should resolve");
        let note = resolution
            .main_note()
            .expect("all-card upgrade should produce a visible note");

        assert_eq!(note, "upgrade all upgradeable cards");
        assert!(!note.contains(&usize::MAX.to_string()));
    }

    #[test]
    fn specific_remove_card_effect_names_the_card() {
        let option = EventOption::new(
            EventChoiceMeta::new("[Give Card] Give Shrug It Off. Obtain a Relic."),
            EventOptionSemantics {
                action: EventActionKind::Trade,
                effects: vec![EventEffect::RemoveCard {
                    count: 1,
                    target_lineage: Some(crate::test_support::scenario_lineage(11)),
                    kind: EventCardKind::Specific(CardId::ShrugItOff),
                }],
                constraints: Vec::new(),
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        );

        let resolution =
            CandidateResolution::from_event_option(&option).expect("effect should resolve");
        let note = resolution
            .main_note()
            .expect("specific remove should produce a visible note");

        assert_eq!(note, "remove specific card Shrug It Off");
    }

    #[test]
    fn random_event_outcome_effect_is_unresolved_not_flattened() {
        let option = EventOption::new(
            EventChoiceMeta::new("[Spin] Spin the wheel!"),
            EventOptionSemantics {
                action: EventActionKind::Special,
                effects: vec![EventEffect::RandomOutcome {
                    kind: crate::state::events::EventRandomOutcomeKind::GremlinWheel,
                    chance_percent: None,
                }],
                constraints: Vec::new(),
                transition: EventOptionTransition::None,
                repeatable: false,
                terminal: false,
            },
        );

        let resolution =
            CandidateResolution::from_event_option(&option).expect("random outcome should resolve");

        assert_eq!(resolution.status, ResolutionStatus::Partial);
        assert_eq!(
            resolution.main_note().as_deref(),
            Some("random Gremlin Wheel outcome")
        );
        assert!(resolution.known_effects.is_empty());
    }

    #[test]
    fn random_curse_effect_names_the_known_pool_without_promising_a_result() {
        let option = EventOption::new(
            EventChoiceMeta::new("Obtain a random Curse."),
            EventOptionSemantics {
                action: EventActionKind::Accept,
                effects: vec![EventEffect::ObtainCurse {
                    count: 1,
                    kind: EventCardKind::RandomCurse,
                }],
                constraints: Vec::new(),
                transition: EventOptionTransition::AdvanceScreen,
                repeatable: false,
                terminal: false,
            },
        );

        let resolution =
            CandidateResolution::from_event_option(&option).expect("curse should resolve");

        assert_eq!(resolution.status, ResolutionStatus::Partial);
        assert_eq!(
            resolution.main_note().as_deref(),
            Some("random ordinary curse outcome")
        );
        assert!(resolution.known_effects.is_empty());
        assert_eq!(
            resolution.unresolved_effects,
            vec![UnresolvedEffect::RandomCurse {
                count: 1,
                pool: CardPoolBoundary::Curse,
                visibility: HiddenInfoVisibility::DistributionKnownResultHiddenUntilResolved,
            }]
        );
    }
}
