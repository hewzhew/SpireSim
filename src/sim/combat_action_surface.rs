use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::content::cards::{CardId, CardType};
use crate::content::potions::PotionId;
use crate::runtime::action::CardDestination;
use crate::runtime::combat::{CardLineageId, CardPileView, CombatState, StanceId};
use crate::state::core::{
    ClientInput, EngineState, GridSelectReason, HandSelectReason, PendingChoice, PileType,
};
use crate::state::selection::SelectionScope;

/// A linear-size description of every input accepted at one combat boundary.
///
/// Atomic inputs remain explicit. Combinatorial pending choices are described
/// as an ordered-input language instead of materializing every payload.
/// Indexed one-of choices keep their exact atomic inputs and carry one aligned
/// typed candidate list instead of duplicating candidate semantics elsewhere.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLegalActionSurfaceV2 {
    pub atomic_actions: Vec<ClientInput>,
    pub selection_families: Vec<CombatSelectionActionFamilyV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed_choice: Option<CombatIndexedChoiceSurfaceV2>,
}

impl CombatLegalActionSurfaceV2 {
    /// Narrows the exact engine surface to actions worth materializing in an
    /// automatic producer.
    ///
    /// The engine intentionally preserves Java's legal zero-card submission
    /// and cancel inputs for Elixir.  Once the potion slot has been consumed,
    /// those inputs are a pure no-op unless another on-use effect is still
    /// pending.  Automatic producers must not publish that no-op as a turn
    /// option, while exact/manual callers continue to receive it from
    /// [`combat_legal_action_surface_v2`].
    pub fn apply_automatic_action_admission(&mut self, combat: &CombatState) {
        self.atomic_actions
            .retain(|input| automatic_client_input_is_admitted(combat, input));
        let mut exclude_cancel = false;
        for family in &mut self.selection_families {
            let CombatSelectionReasonV2::Hand(reason) = family.reason else {
                continue;
            };
            if !hand_selection_requires_material_automatic_action(reason) {
                continue;
            }

            exclude_cancel = true;
            family.declared_min = family.declared_min.max(1);
            if family.declared_min > family.effective_max {
                family.selection_status = CombatSelectionStatusV2::Disabled(
                    CombatSelectionDisabledReasonV2::InfeasibleBounds,
                );
            }
        }

        if exclude_cancel {
            self.atomic_actions
                .retain(|input| !matches!(input, ClientInput::Cancel));
        }
    }
}

/// Returns whether an exact legal input has a material effect worth exposing
/// to an automatic producer at the current state.
pub fn automatic_client_input_is_admitted(combat: &CombatState, input: &ClientInput) -> bool {
    let ClientInput::UsePotion { potion_index, .. } = input else {
        return true;
    };
    let Some(potion) = combat
        .entities
        .potions
        .get(*potion_index)
        .and_then(Option::as_ref)
    else {
        return true;
    };
    if potion.id != PotionId::Elixir {
        return true;
    }

    !combat.zones.hand.is_empty()
        || crate::content::potions::potion_effects::elixir_has_pending_use_effect(combat)
}

/// Returns whether an automatic producer must select at least one hand card
/// for this choice to have any material effect.
pub fn hand_selection_requires_material_automatic_action(reason: HandSelectReason) -> bool {
    matches!(
        reason,
        HandSelectReason::PotionExhaust {
            has_pending_use_effect: false
        }
    )
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatIndexedChoiceSurfaceV2 {
    pub input_encoding: CombatIndexedChoiceInputEncodingV2,
    pub reason: CombatIndexedChoiceReasonV2,
    /// Position in this vector is the exact submitted choice index.
    pub candidates: Vec<CombatIndexedChoiceCandidateV2>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatIndexedChoiceInputEncodingV2 {
    SubmitDiscoverChoiceIndex,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatIndexedChoiceReasonV2 {
    Discovery {
        colorless: bool,
        card_type: Option<CardType>,
        amount: u8,
    },
    CardReward {
        destination: CardDestination,
    },
    ForeignInfluence {
        upgraded: bool,
    },
    ChooseOne,
    Stance,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatIndexedChoiceCandidateV2 {
    Card { card_id: CardId, upgrades: u8 },
    Stance { stance: StanceId },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatSelectionActionFamilyV2 {
    pub input_encoding: CombatSelectionInputEncodingV2,
    pub reason: CombatSelectionReasonV2,
    pub source_pile: Option<PileType>,
    pub raw_domain: Vec<CombatSelectionDomainCandidateV2>,
    pub raw_domain_count: u64,
    pub eligible_domain_count: u64,
    pub max_distinct_selection_count: u64,
    pub declared_min: u64,
    pub declared_max: u64,
    pub effective_max: u64,
    pub selection_status: CombatSelectionStatusV2,
    pub payload_language: CombatSelectionPayloadLanguageV2,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSelectionInputEncodingV2 {
    SubmitSelectionHandCardLineages,
    SubmitSelectionGridCardLineages,
    SubmitScryDiscardIndices,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CombatSelectionReasonV2 {
    Hand(HandSelectReason),
    Grid(GridSelectReason),
    ScryDiscard,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "address_kind", rename_all = "snake_case")]
pub enum CombatSelectionDomainCandidateV2 {
    CardLineage {
        ordinal: u64,
        lineage: CardLineageId,
        card_id: Option<CardId>,
        upgrades: Option<u8>,
        eligible: bool,
    },
    ScryIndex {
        index: u64,
        card_id: Option<CardId>,
        card_lineage: Option<CardLineageId>,
        currently_present: bool,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "reason", rename_all = "snake_case")]
pub enum CombatSelectionStatusV2 {
    Enabled,
    Disabled(CombatSelectionDisabledReasonV2),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSelectionDisabledReasonV2 {
    InvalidGridSource,
    MalformedScryDomain,
    InfeasibleBounds,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "language", content = "distinct_by", rename_all = "snake_case")]
pub enum CombatSelectionPayloadLanguageV2 {
    OrderedDistinctSequence(CombatSelectionDistinctByV2),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSelectionDistinctByV2 {
    CardLineage,
    ScryIndexAndCardLineage,
}

pub fn combat_legal_action_surface_v2(
    engine: &EngineState,
    combat: &CombatState,
) -> CombatLegalActionSurfaceV2 {
    match engine {
        EngineState::PendingChoice(choice) => pending_choice_surface(choice, combat),
        _ => CombatLegalActionSurfaceV2 {
            atomic_actions: crate::sim::combat_legal_actions::engine_atomic_actions(engine, combat),
            selection_families: Vec::new(),
            indexed_choice: None,
        },
    }
}

/// Exact pending-choice membership without constructing an eager candidate
/// list.  The symbolic surface projector and the simulator membership gate
/// share this semantic owner.
pub fn pending_choice_input_is_legal(
    choice: &PendingChoice,
    combat: &CombatState,
    input: &ClientInput,
) -> bool {
    if let ClientInput::DiscardPotion(slot) = input {
        return combat
            .entities
            .potions
            .get(*slot)
            .and_then(Option::as_ref)
            .is_some_and(|potion| potion.can_discard);
    }
    match (choice, input) {
        (
            PendingChoice::HandSelect {
                min_cards,
                max_cards,
                candidate_lineages,
                ..
            },
            ClientInput::SubmitSelection(resolution),
        ) if resolution.scope == SelectionScope::Hand => selection_is_legal(
            &resolution.selected_card_lineages(),
            candidate_lineages,
            *min_cards as usize,
            *max_cards as usize,
            CardPileView::Contiguous(&combat.zones.hand),
        ),
        (
            PendingChoice::GridSelect {
                source_pile,
                min_cards,
                max_cards,
                candidate_lineages,
                reason,
                ..
            },
            ClientInput::SubmitSelection(resolution),
        ) if resolution.scope == SelectionScope::Grid => {
            grid_source_is_legal(*reason, *source_pile)
                && selection_is_legal(
                    &resolution.selected_card_lineages(),
                    candidate_lineages,
                    *min_cards as usize,
                    *max_cards as usize,
                    cards_for_pile(combat, *source_pile),
                )
        }
        (
            PendingChoice::HandSelect { can_cancel, .. }
            | PendingChoice::GridSelect { can_cancel, .. },
            ClientInput::Cancel,
        ) => *can_cancel,
        (
            PendingChoice::ScrySelect {
                cards,
                card_lineages,
            },
            ClientInput::SubmitScryDiscard(indices),
        ) => {
            let selected_lineages = indices
                .iter()
                .filter_map(|index| card_lineages.get(*index).copied())
                .collect::<Vec<_>>();
            cards.len() == card_lineages.len()
                && indices.iter().all(|index| *index < cards.len())
                && all_unique(indices)
                && selected_lineages.len() == indices.len()
                && all_unique(&selected_lineages)
                && indices.iter().all(|index| {
                    combat
                        .zones
                        .draw_pile
                        .iter()
                        .any(|card| card.lineage == card_lineages[*index])
                })
        }
        (PendingChoice::DiscoverySelect(choice), ClientInput::SubmitDiscoverChoice(index)) => {
            *index < choice.cards.len()
        }
        (PendingChoice::DiscoverySelect(choice), ClientInput::Cancel) => choice.can_skip,
        (
            PendingChoice::CardRewardSelect { cards, .. },
            ClientInput::SubmitDiscoverChoice(index),
        ) => *index < cards.len(),
        (PendingChoice::CardRewardSelect { can_skip, .. }, ClientInput::Cancel) => *can_skip,
        (
            PendingChoice::ForeignInfluenceSelect { cards, .. },
            ClientInput::SubmitDiscoverChoice(index),
        ) => *index < cards.len(),
        (PendingChoice::ChooseOneSelect { choices }, ClientInput::SubmitDiscoverChoice(index)) => {
            *index < choices.len()
        }
        (PendingChoice::StanceChoice, ClientInput::SubmitDiscoverChoice(index)) => *index < 2,
        _ => false,
    }
}

fn pending_choice_surface(
    choice: &PendingChoice,
    combat: &CombatState,
) -> CombatLegalActionSurfaceV2 {
    let mut surface = match choice {
        PendingChoice::HandSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        } => {
            let family = card_lineage_family(
                CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages,
                CombatSelectionReasonV2::Hand(*reason),
                None,
                candidate_lineages,
                *min_cards as usize,
                *max_cards as usize,
                CardPileView::Contiguous(&combat.zones.hand),
                true,
            );
            symbolic_surface(family, *can_cancel)
        }
        PendingChoice::GridSelect {
            source_pile,
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        } => {
            let source_is_valid = grid_source_is_legal(*reason, *source_pile);
            let family = card_lineage_family(
                CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages,
                CombatSelectionReasonV2::Grid(*reason),
                Some(*source_pile),
                candidate_lineages,
                *min_cards as usize,
                *max_cards as usize,
                cards_for_pile(combat, *source_pile),
                source_is_valid,
            );
            symbolic_surface(family, *can_cancel)
        }
        PendingChoice::ScrySelect {
            cards,
            card_lineages,
        } => CombatLegalActionSurfaceV2 {
            atomic_actions: Vec::new(),
            selection_families: vec![scry_family(cards, card_lineages, combat)],
            indexed_choice: None,
        },
        PendingChoice::DiscoverySelect(choice) => indexed_choice_surface(
            CombatIndexedChoiceReasonV2::Discovery {
                colorless: choice.colorless,
                card_type: choice.card_type,
                amount: choice.amount,
            },
            choice
                .cards
                .iter()
                .copied()
                .map(|card_id| CombatIndexedChoiceCandidateV2::Card {
                    card_id,
                    upgrades: 0,
                })
                .collect(),
            choice.can_skip,
        ),
        PendingChoice::CardRewardSelect {
            cards,
            destination,
            can_skip,
        } => indexed_choice_surface(
            CombatIndexedChoiceReasonV2::CardReward {
                destination: *destination,
            },
            cards
                .iter()
                .copied()
                .map(|card_id| CombatIndexedChoiceCandidateV2::Card {
                    card_id,
                    upgrades: 0,
                })
                .collect(),
            *can_skip,
        ),
        PendingChoice::ForeignInfluenceSelect { cards, upgraded } => indexed_choice_surface(
            CombatIndexedChoiceReasonV2::ForeignInfluence {
                upgraded: *upgraded,
            },
            cards
                .iter()
                .copied()
                .map(|card_id| CombatIndexedChoiceCandidateV2::Card {
                    card_id,
                    upgrades: u8::from(*upgraded),
                })
                .collect(),
            false,
        ),
        PendingChoice::ChooseOneSelect { choices } => indexed_choice_surface(
            CombatIndexedChoiceReasonV2::ChooseOne,
            choices
                .iter()
                .map(|choice| CombatIndexedChoiceCandidateV2::Card {
                    card_id: choice.card_id,
                    upgrades: choice.upgrades,
                })
                .collect(),
            false,
        ),
        PendingChoice::StanceChoice => indexed_choice_surface(
            CombatIndexedChoiceReasonV2::Stance,
            vec![
                CombatIndexedChoiceCandidateV2::Stance {
                    stance: StanceId::Wrath,
                },
                CombatIndexedChoiceCandidateV2::Stance {
                    stance: StanceId::Calm,
                },
            ],
            false,
        ),
    };
    surface
        .atomic_actions
        .extend(
            combat
                .entities
                .potions
                .iter()
                .enumerate()
                .filter_map(|(slot, potion)| {
                    potion
                        .as_ref()
                        .is_some_and(|potion| potion.can_discard)
                        .then_some(ClientInput::DiscardPotion(slot))
                }),
        );
    surface
}

fn indexed_choice_surface(
    reason: CombatIndexedChoiceReasonV2,
    candidates: Vec<CombatIndexedChoiceCandidateV2>,
    can_cancel: bool,
) -> CombatLegalActionSurfaceV2 {
    let mut atomic_actions = (0..candidates.len())
        .map(ClientInput::SubmitDiscoverChoice)
        .collect::<Vec<_>>();
    if can_cancel {
        atomic_actions.push(ClientInput::Cancel);
    }
    CombatLegalActionSurfaceV2 {
        atomic_actions,
        selection_families: Vec::new(),
        indexed_choice: Some(CombatIndexedChoiceSurfaceV2 {
            input_encoding: CombatIndexedChoiceInputEncodingV2::SubmitDiscoverChoiceIndex,
            reason,
            candidates,
        }),
    }
}

fn symbolic_surface(
    family: CombatSelectionActionFamilyV2,
    can_cancel: bool,
) -> CombatLegalActionSurfaceV2 {
    CombatLegalActionSurfaceV2 {
        atomic_actions: if can_cancel {
            vec![ClientInput::Cancel]
        } else {
            Vec::new()
        },
        selection_families: vec![family],
        indexed_choice: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn card_lineage_family(
    input_encoding: CombatSelectionInputEncodingV2,
    reason: CombatSelectionReasonV2,
    source_pile: Option<PileType>,
    candidate_lineages: &[CardLineageId],
    declared_min: usize,
    declared_max: usize,
    source_cards: CardPileView<'_>,
    source_is_valid: bool,
) -> CombatSelectionActionFamilyV2 {
    let mut seen = HashSet::new();
    let mut eligible_count = 0usize;
    let raw_domain = candidate_lineages
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, lineage)| {
            let card = source_cards.iter().find(|card| card.lineage == lineage);
            let eligible = source_is_valid && card.is_some() && seen.insert(lineage);
            eligible_count += usize::from(eligible);
            CombatSelectionDomainCandidateV2::CardLineage {
                ordinal: to_u64(ordinal),
                lineage,
                card_id: card.map(|card| card.id),
                upgrades: card.map(|card| card.upgrades),
                eligible,
            }
        })
        .collect::<Vec<_>>();
    let effective_max = declared_max.min(eligible_count);
    let selection_status = if !source_is_valid {
        CombatSelectionStatusV2::Disabled(CombatSelectionDisabledReasonV2::InvalidGridSource)
    } else if declared_min > effective_max {
        CombatSelectionStatusV2::Disabled(CombatSelectionDisabledReasonV2::InfeasibleBounds)
    } else {
        CombatSelectionStatusV2::Enabled
    };

    CombatSelectionActionFamilyV2 {
        input_encoding,
        reason,
        source_pile,
        raw_domain_count: to_u64(candidate_lineages.len()),
        eligible_domain_count: to_u64(eligible_count),
        max_distinct_selection_count: to_u64(eligible_count),
        raw_domain,
        declared_min: to_u64(declared_min),
        declared_max: to_u64(declared_max),
        effective_max: to_u64(effective_max),
        selection_status,
        payload_language: CombatSelectionPayloadLanguageV2::OrderedDistinctSequence(
            CombatSelectionDistinctByV2::CardLineage,
        ),
    }
}

fn scry_family(
    cards: &[CardId],
    card_lineages: &[CardLineageId],
    combat: &CombatState,
) -> CombatSelectionActionFamilyV2 {
    let raw_domain_count = cards.len().max(card_lineages.len());
    let raw_domain = (0..raw_domain_count)
        .map(|index| {
            let card_lineage = card_lineages.get(index).copied();
            let currently_present = card_lineage.is_some_and(|lineage| {
                combat
                    .zones
                    .draw_pile
                    .iter()
                    .any(|card| card.lineage == lineage)
            });
            CombatSelectionDomainCandidateV2::ScryIndex {
                index: to_u64(index),
                card_id: cards.get(index).copied(),
                card_lineage,
                currently_present,
            }
        })
        .collect::<Vec<_>>();
    let lengths_match = cards.len() == card_lineages.len();
    let present_indices = card_lineages
        .iter()
        .filter(|lineage| {
            combat
                .zones
                .draw_pile
                .iter()
                .any(|card| card.lineage == **lineage)
        })
        .count();
    let unique_present = card_lineages
        .iter()
        .copied()
        .filter(|lineage| {
            combat
                .zones
                .draw_pile
                .iter()
                .any(|card| card.lineage == *lineage)
        })
        .collect::<HashSet<_>>()
        .len();
    let selection_status = if lengths_match {
        CombatSelectionStatusV2::Enabled
    } else {
        CombatSelectionStatusV2::Disabled(CombatSelectionDisabledReasonV2::MalformedScryDomain)
    };

    CombatSelectionActionFamilyV2 {
        input_encoding: CombatSelectionInputEncodingV2::SubmitScryDiscardIndices,
        reason: CombatSelectionReasonV2::ScryDiscard,
        source_pile: Some(PileType::Draw),
        raw_domain,
        raw_domain_count: to_u64(raw_domain_count),
        eligible_domain_count: if lengths_match {
            to_u64(present_indices)
        } else {
            0
        },
        max_distinct_selection_count: if lengths_match {
            to_u64(unique_present)
        } else {
            0
        },
        declared_min: 0,
        declared_max: to_u64(cards.len()),
        effective_max: if lengths_match {
            to_u64(unique_present)
        } else {
            0
        },
        selection_status,
        payload_language: CombatSelectionPayloadLanguageV2::OrderedDistinctSequence(
            CombatSelectionDistinctByV2::ScryIndexAndCardLineage,
        ),
    }
}

fn cards_for_pile(combat: &CombatState, pile: PileType) -> CardPileView<'_> {
    match pile {
        PileType::Draw => CardPileView::Contiguous(combat.zones.draw_pile.as_ref()),
        PileType::Discard => CardPileView::Discard(&combat.zones.discard_pile),
        PileType::Exhaust => CardPileView::Contiguous(combat.zones.exhaust_pile.as_slice()),
        PileType::Hand => CardPileView::Contiguous(&combat.zones.hand),
        PileType::Limbo => CardPileView::Contiguous(&combat.zones.limbo),
        PileType::MasterDeck => CardPileView::Contiguous(&combat.meta.master_deck_snapshot),
    }
}

fn grid_source_is_legal(reason: GridSelectReason, source_pile: PileType) -> bool {
    match reason {
        GridSelectReason::DiscardToHand
        | GridSelectReason::DiscardToHandNoCostChange
        | GridSelectReason::DiscardToHandRetain => source_pile == PileType::Discard,
        GridSelectReason::MoveToDrawPile => {
            matches!(source_pile, PileType::Discard | PileType::Exhaust)
        }
        GridSelectReason::Exhume { .. } => source_pile == PileType::Exhaust,
        GridSelectReason::DrawPileToHand
        | GridSelectReason::SkillFromDeckToHand
        | GridSelectReason::AttackFromDeckToHand
        | GridSelectReason::Omniscience { .. } => source_pile == PileType::Draw,
    }
}

fn selection_is_legal(
    selected: &[CardLineageId],
    candidates: &[CardLineageId],
    min_cards: usize,
    max_cards: usize,
    source_cards: CardPileView<'_>,
) -> bool {
    selected.len() >= min_cards
        && selected.len() <= max_cards.min(candidates.len())
        && all_unique(selected)
        && selected.iter().all(|lineage| candidates.contains(lineage))
        && selected
            .iter()
            .all(|lineage| source_cards.iter().any(|card| card.lineage == *lineage))
}

fn all_unique<T: Ord + Copy>(values: &[T]) -> bool {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    !sorted.windows(2).any(|pair| pair[0] == pair[1])
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::potions::Potion;
    use crate::runtime::combat::CombatCard;
    use crate::sim::combat::{CombatPosition, CombatStepper, EngineCombatStepper};
    use crate::state::selection::SelectionResolution;

    #[test]
    fn sixty_four_card_scry_is_one_linear_symbolic_family() {
        let mut combat = crate::test_support::blank_test_combat();
        let cards = vec![CardId::Strike; 64];
        let card_lineages = (1..=64)
            .map(crate::test_support::scenario_lineage)
            .collect::<Vec<_>>();
        combat.zones.draw_pile = (card_lineages
            .iter()
            .map(|lineage| CombatCard::with_lineage(CardId::Strike, *lineage))
            .collect::<Vec<_>>())
        .into();
        let surface = combat_legal_action_surface_v2(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards,
                card_lineages,
            }),
            &combat,
        );

        assert!(surface.atomic_actions.is_empty());
        assert_eq!(surface.selection_families.len(), 1);
        let family = &surface.selection_families[0];
        assert_eq!(family.raw_domain_count, 64);
        assert_eq!(family.raw_domain.len(), 64);
        assert_eq!(family.declared_min, 0);
        assert_eq!(family.declared_max, 64);
        assert_eq!(family.selection_status, CombatSelectionStatusV2::Enabled);
    }

    #[test]
    fn engine_stepper_separates_atomic_actions_from_exact_structured_membership() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = (0..24)
            .map(|index| CombatCard::for_scenario(CardId::Strike, 100 + index))
            .collect();
        let choice = PendingChoice::HandSelect {
            candidate_lineages: (100..124)
                .map(crate::test_support::scenario_lineage)
                .collect(),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: HandSelectReason::Discard,
        };
        let position = CombatPosition::new(EngineState::PendingChoice(choice), combat);
        let stepper = EngineCombatStepper;
        let surface = stepper.legal_action_surface(&position);
        let last_candidate = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Hand,
            [crate::test_support::scenario_lineage(123)],
        ));

        assert!(surface.atomic_actions.is_empty());
        assert_eq!(surface.selection_families.len(), 1);
        assert!(stepper.is_legal_action(&position, &last_candidate));
        assert!(stepper
            .choice_for_legal_input(&position, &last_candidate)
            .is_some());
    }

    #[test]
    fn empty_hand_submission_and_cancel_remain_distinct() {
        let combat = crate::test_support::blank_test_combat();
        let choice = PendingChoice::HandSelect {
            candidate_lineages: Vec::new(),
            min_cards: 0,
            max_cards: 0,
            can_cancel: true,
            reason: crate::state::core::HandSelectReason::Discard,
        };
        let surface =
            combat_legal_action_surface_v2(&EngineState::PendingChoice(choice.clone()), &combat);

        assert_eq!(surface.atomic_actions, vec![ClientInput::Cancel]);
        assert_eq!(surface.selection_families.len(), 1);
        assert_eq!(
            surface.selection_families[0].selection_status,
            CombatSelectionStatusV2::Enabled
        );
        assert!(pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::Cancel
        ));
        assert!(pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    SelectionScope::Hand,
                    [],
                )
            )
        ));
    }

    #[test]
    fn pending_hand_selection_composes_with_discardable_potions_only() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 10)];
        combat.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                crate::content::potions::PotionId::GamblersBrew,
                20,
                true,
                false,
                false,
            )),
            Some(Potion::with_affordance_truth(
                crate::content::potions::PotionId::EnergyPotion,
                30,
                true,
                true,
                false,
            )),
        ];
        let choice = PendingChoice::HandSelect {
            candidate_lineages: vec![crate::test_support::scenario_lineage(10)],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: HandSelectReason::Exhaust,
        };

        let surface =
            combat_legal_action_surface_v2(&EngineState::PendingChoice(choice.clone()), &combat);

        assert_eq!(surface.atomic_actions, vec![ClientInput::DiscardPotion(1)]);
        assert!(pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::DiscardPotion(1)
        ));
        assert!(!pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::DiscardPotion(0)
        ));
        assert!(!pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::UsePotion {
                potion_index: 1,
                target: None,
            }
        ));
    }

    #[test]
    fn automatic_admission_excludes_only_the_effectless_elixir_no_op() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let choice = PendingChoice::HandSelect {
            candidate_lineages: [10, 11].map(crate::test_support::scenario_lineage).to_vec(),
            min_cards: 0,
            max_cards: 99,
            can_cancel: true,
            reason: HandSelectReason::PotionExhaust {
                has_pending_use_effect: false,
            },
        };
        let exact = combat_legal_action_surface_v2(&EngineState::PendingChoice(choice), &combat);

        assert_eq!(exact.atomic_actions, vec![ClientInput::Cancel]);
        assert_eq!(exact.selection_families[0].declared_min, 0);

        let mut automatic = exact.clone();
        automatic.apply_automatic_action_admission(&combat);

        assert!(automatic.atomic_actions.is_empty());
        assert_eq!(automatic.selection_families[0].declared_min, 1);
        assert_eq!(
            automatic.selection_families[0].selection_status,
            CombatSelectionStatusV2::Enabled
        );
    }

    #[test]
    fn automatic_admission_keeps_empty_elixir_when_an_on_use_effect_is_pending() {
        let combat = crate::test_support::blank_test_combat();
        let choice = PendingChoice::HandSelect {
            candidate_lineages: Vec::new(),
            min_cards: 0,
            max_cards: 99,
            can_cancel: true,
            reason: HandSelectReason::PotionExhaust {
                has_pending_use_effect: true,
            },
        };
        let exact = combat_legal_action_surface_v2(&EngineState::PendingChoice(choice), &combat);
        let mut automatic = exact.clone();

        automatic.apply_automatic_action_admission(&combat);

        assert_eq!(automatic, exact);
    }

    #[test]
    fn automatic_admission_excludes_empty_hand_elixir_before_it_can_fizzle() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters = vec![crate::test_support::planned_monster(
            crate::content::monsters::EnemyId::JawWorm,
            1,
        )];
        combat.entities.potions = vec![Some(Potion::new(PotionId::Elixir, 7))];
        combat.zones.hand.clear();
        let exact = combat_legal_action_surface_v2(&EngineState::CombatPlayerTurn, &combat);
        let use_elixir = ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        };

        assert!(exact.atomic_actions.contains(&use_elixir));

        let mut automatic = exact;
        automatic.apply_automatic_action_admission(&combat);

        assert!(!automatic.atomic_actions.contains(&use_elixir));
    }

    #[test]
    fn hand_family_preserves_candidates_beyond_legacy_preview_cap() {
        let mut combat = crate::test_support::blank_test_combat();
        let candidate_lineages = (100..124)
            .map(crate::test_support::scenario_lineage)
            .collect::<Vec<_>>();
        combat.zones.hand = candidate_lineages
            .iter()
            .map(|lineage| CombatCard::with_lineage(CardId::Strike, *lineage))
            .collect();
        let surface = combat_legal_action_surface_v2(
            &EngineState::PendingChoice(PendingChoice::HandSelect {
                candidate_lineages,
                min_cards: 1,
                max_cards: 3,
                can_cancel: false,
                reason: crate::state::core::HandSelectReason::Discard,
            }),
            &combat,
        );

        let family = &surface.selection_families[0];
        assert_eq!(family.raw_domain_count, 24);
        assert_eq!(family.raw_domain.len(), 24);
        assert_eq!(family.eligible_domain_count, 24);
        assert_eq!(family.effective_max, 3);
    }

    #[test]
    fn invalid_grid_source_disables_submit_but_preserves_cancel() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 7)]).into();
        let choice = PendingChoice::GridSelect {
            source_pile: PileType::Draw,
            candidate_lineages: vec![crate::runtime::combat::CardLineageId::constructed_scenario(
                7,
            )],
            min_cards: 1,
            max_cards: 1,
            can_cancel: true,
            reason: GridSelectReason::DiscardToHand,
        };
        let surface =
            combat_legal_action_surface_v2(&EngineState::PendingChoice(choice.clone()), &combat);

        assert_eq!(surface.atomic_actions, vec![ClientInput::Cancel]);
        let family = &surface.selection_families[0];
        assert_eq!(family.eligible_domain_count, 0);
        assert_eq!(family.effective_max, 0);
        assert_eq!(
            family.selection_status,
            CombatSelectionStatusV2::Disabled(CombatSelectionDisabledReasonV2::InvalidGridSource)
        );
        assert!(pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::Cancel
        ));
        assert!(!pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::SubmitSelection(
                crate::state::selection::SelectionResolution::card_lineages(
                    SelectionScope::Grid,
                    [crate::test_support::scenario_lineage(7)],
                )
            )
        ));
    }

    #[test]
    fn duplicate_scry_uuids_keep_both_addresses_but_limit_distinct_payloads() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 7)]).into();
        let choice = PendingChoice::ScrySelect {
            cards: vec![CardId::Strike, CardId::Strike],
            card_lineages: vec![crate::test_support::scenario_lineage(7); 2],
        };
        let surface =
            combat_legal_action_surface_v2(&EngineState::PendingChoice(choice.clone()), &combat);

        let family = &surface.selection_families[0];
        assert_eq!(family.eligible_domain_count, 2);
        assert_eq!(family.max_distinct_selection_count, 1);
        assert_eq!(family.effective_max, 1);
        assert_eq!(
            family.payload_language,
            CombatSelectionPayloadLanguageV2::OrderedDistinctSequence(
                CombatSelectionDistinctByV2::ScryIndexAndCardLineage
            )
        );
        assert!(pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::SubmitScryDiscard(vec![0])
        ));
        assert!(pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::SubmitScryDiscard(vec![1])
        ));
        assert!(!pending_choice_input_is_legal(
            &choice,
            &combat,
            &ClientInput::SubmitScryDiscard(vec![0, 1])
        ));
    }

    #[test]
    fn malformed_scry_is_disabled_without_eager_fallback() {
        let combat = crate::test_support::blank_test_combat();
        let surface = combat_legal_action_surface_v2(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Strike, CardId::Defend],
                card_lineages: vec![crate::runtime::combat::CardLineageId::constructed_scenario(
                    1,
                )],
            }),
            &combat,
        );

        assert_eq!(surface.selection_families.len(), 1);
        assert_eq!(
            surface.selection_families[0].selection_status,
            CombatSelectionStatusV2::Disabled(CombatSelectionDisabledReasonV2::MalformedScryDomain)
        );
    }

    #[test]
    fn discovery_indices_carry_one_aligned_typed_candidate_list() {
        let combat = crate::test_support::blank_test_combat();
        let surface = combat_legal_action_surface_v2(
            &EngineState::PendingChoice(PendingChoice::DiscoverySelect(
                crate::state::DiscoveryChoiceState {
                    cards: vec![CardId::Bash, CardId::FiendFire],
                    colorless: false,
                    card_type: Some(CardType::Attack),
                    amount: 2,
                    can_skip: true,
                },
            )),
            &combat,
        );

        assert_eq!(
            surface.atomic_actions,
            vec![
                ClientInput::SubmitDiscoverChoice(0),
                ClientInput::SubmitDiscoverChoice(1),
                ClientInput::Cancel,
            ]
        );
        assert!(surface.selection_families.is_empty());
        assert_eq!(
            surface.indexed_choice,
            Some(CombatIndexedChoiceSurfaceV2 {
                input_encoding: CombatIndexedChoiceInputEncodingV2::SubmitDiscoverChoiceIndex,
                reason: CombatIndexedChoiceReasonV2::Discovery {
                    colorless: false,
                    card_type: Some(CardType::Attack),
                    amount: 2,
                },
                candidates: vec![
                    CombatIndexedChoiceCandidateV2::Card {
                        card_id: CardId::Bash,
                        upgrades: 0,
                    },
                    CombatIndexedChoiceCandidateV2::Card {
                        card_id: CardId::FiendFire,
                        upgrades: 0,
                    },
                ],
            })
        );
    }

    #[test]
    fn choose_one_and_stance_indices_preserve_typed_option_semantics() {
        let combat = crate::test_support::blank_test_combat();
        let choose_one = combat_legal_action_surface_v2(
            &EngineState::PendingChoice(PendingChoice::ChooseOneSelect {
                choices: vec![
                    crate::state::ChooseOneCardChoice {
                        card_id: CardId::BecomeAlmighty,
                        upgrades: 1,
                    },
                    crate::state::ChooseOneCardChoice {
                        card_id: CardId::ReachHeaven,
                        upgrades: 0,
                    },
                ],
            }),
            &combat,
        );
        let stance = combat_legal_action_surface_v2(
            &EngineState::PendingChoice(PendingChoice::StanceChoice),
            &combat,
        );

        assert_eq!(
            choose_one
                .indexed_choice
                .expect("choose-one semantics")
                .candidates,
            vec![
                CombatIndexedChoiceCandidateV2::Card {
                    card_id: CardId::BecomeAlmighty,
                    upgrades: 1,
                },
                CombatIndexedChoiceCandidateV2::Card {
                    card_id: CardId::ReachHeaven,
                    upgrades: 0,
                },
            ]
        );
        assert_eq!(
            stance.indexed_choice.expect("stance semantics").candidates,
            vec![
                CombatIndexedChoiceCandidateV2::Stance {
                    stance: StanceId::Wrath,
                },
                CombatIndexedChoiceCandidateV2::Stance {
                    stance: StanceId::Calm,
                },
            ]
        );
    }
}
